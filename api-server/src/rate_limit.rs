use std::sync::Arc;
use std::net::SocketAddr;
use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use once_cell::sync::Lazy;

/// Per-IP rate limit: 100 requests per minute.
static IP_LIMITERS: Lazy<DashMap<String, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>> =
    Lazy::new(DashMap::new);

/// Per-API-key rate limit: 1000 requests per minute.
static KEY_LIMITERS: Lazy<DashMap<String, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>> =
    Lazy::new(DashMap::new);

/// Custom rate-limit-exceeded response.
#[derive(Debug)]
pub struct RateLimitExceeded;

impl IntoResponse for RateLimitExceeded {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": "Rate limit exceeded" });
        (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response()
    }
}

/// Axum middleware that enforces token-bucket rate limits.
/// Checks per-API-key first (if `x-api-key` header present), then falls back to per-IP.
pub async fn layer(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, RateLimitExceeded> {
    // Prefer API-key based limiting
    if let Some(api_key) = req.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
        let limiter = KEY_LIMITERS
            .entry(api_key.to_string())
            .or_insert_with(|| {
                Arc::new(RateLimiter::direct(Quota::per_minute(
                    std::num::NonZeroU32::new(1000).unwrap(),
                )))
            })
            .clone();

        if limiter.check().is_err() {
            return Err(RateLimitExceeded);
        }
    } else {
        // Fall back to IP-based limiting
        let ip = addr.ip().to_string();
        let limiter = IP_LIMITERS
            .entry(ip)
            .or_insert_with(|| {
                Arc::new(RateLimiter::direct(Quota::per_minute(
                    std::num::NonZeroU32::new(100).unwrap(),
                )))
            })
            .clone();

        if limiter.check().is_err() {
            return Err(RateLimitExceeded);
        }
    }

    Ok(next.run(req).await)
}

