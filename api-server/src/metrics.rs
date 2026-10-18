use std::time::Instant;
use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
    routing::get,
    Router,
};
use metrics::{counter, histogram, describe_counter, describe_histogram};
use tracing::info;

use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::env;

const DEFAULT_SLOW_REQUEST_THRESHOLD_MS: u64 = 1_000;

fn slow_request_threshold_ms() -> u64 {
    env::var("SLOW_REQUEST_THRESHOLD_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &u64| *value > 0)
        .unwrap_or(DEFAULT_SLOW_REQUEST_THRESHOLD_MS)
}

static PROMETHEUS_HANDLE: Lazy<Mutex<metrics_exporter_prometheus::PrometheusHandle>> = Lazy::new(|| {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder();
    let handle = recorder.handle();
    metrics::set_global_recorder(recorder)
        .expect("Failed to set global metrics recorder");
    
    // Describe metrics
    describe_counter!(
        "http_requests_total",
        "Total number of HTTP requests"
    );
    describe_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    );
    describe_counter!(
        "http_errors_total",
        "Total number of HTTP error responses"
    );
    describe_counter!(
        "http_slow_requests_total",
        "HTTP requests slower than SLOW_REQUEST_THRESHOLD_MS"
    );
    
    Mutex::new(handle)
});

/// Initialize the Prometheus metrics recorder and JSON logging subscriber.
pub fn init() {
    // Ensure the lazy static is initialized
    let _ = &*PROMETHEUS_HANDLE;

    // Initialize JSON subscriber
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .with_current_span(false)
        .with_target(false)
        .init();
}

/// Middleware that records structured logs and Prometheus metrics for every request.
pub async fn track(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let client_ip = extract_client_ip(&req);

    // Execute the request
    let response = next.run(req).await;

    let latency = start.elapsed();
    let status = response.status().as_u16();

    // Prometheus metrics
    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status.to_string(),
    )
    .increment(1);

    histogram!(
        "http_request_duration_seconds",
        "method" => method.clone(),
        "path" => path.clone(),
    )
    .record(latency.as_secs_f64());

    if status >= 400 {
        counter!(
            "http_errors_total",
            "path" => path.clone(),
            "status" => status.to_string(),
        )
        .increment(1);
    }

    // Structured JSON log
    info!(
        method = %method,
        path = %path,
        status = status,
        latency_ms = latency.as_millis() as u64,
        client_ip = %client_ip,
        "request completed"
    );

    let latency_ms = latency.as_millis() as u64;
    if latency_ms >= slow_request_threshold_ms() {
        counter!(
            "http_slow_requests_total",
            "method" => method.clone(),
            "path" => path.clone(),
        )
        .increment(1);
        tracing::warn!(
            method = %method,
            path = %path,
            status = status,
            latency_ms,
            threshold_ms = slow_request_threshold_ms(),
            "slow request detected"
        );
    }

    response
}

/// Extract client IP from X-Forwarded-For header or connection info.
fn extract_client_ip(req: &Request) -> String {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Handler for the /metrics endpoint that returns Prometheus exposition format.
pub async fn metrics_handler() -> Result<String, StatusCode> {
    let handle = PROMETHEUS_HANDLE.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(handle.render())
}

/// Environment variable holding the bearer token required to scrape /metrics.
pub const METRICS_AUTH_TOKEN_ENV: &str = "METRICS_AUTH_TOKEN";

/// Environment variable holding the port for the internal metrics listener.
pub const METRICS_PORT_ENV: &str = "METRICS_PORT";

/// Default port for the internal metrics listener when METRICS_PORT is unset.
pub const DEFAULT_METRICS_PORT: u16 = 9091;

/// Read the configured metrics auth token, if any.
fn metrics_auth_token() -> Option<String> {
    env::var(METRICS_AUTH_TOKEN_ENV)
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// Read the configured internal metrics port, falling back to the default.
pub fn metrics_port() -> u16 {
    env::var(METRICS_PORT_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(DEFAULT_METRICS_PORT)
}

/// Constant-time-ish comparison of the supplied bearer token against the expected token.
fn token_matches(expected: &str, supplied: &str) -> bool {
    let expected = expected.as_bytes();
    let supplied = supplied.as_bytes();
    if expected.len() != supplied.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.iter().zip(supplied.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Middleware that requires a valid bearer token before serving /metrics.
///
/// If `METRICS_AUTH_TOKEN` is unset, the endpoint is treated as unauthenticated
/// (intended for deployments that bind the metrics listener to a private port).
pub async fn require_metrics_token(req: Request, next: Next) -> Response {
    let Some(expected) = metrics_auth_token() else {
        return next.run(req).await;
    };

    let supplied = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value.trim());

    match supplied {
        Some(token) if token_matches(&expected, token) => next.run(req).await,
        _ => {
            let mut response = Response::new(Body::from("unauthorized"));
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
            response
        }
    }
}

/// Build the internal metrics router, guarded by the bearer-token middleware.
///
/// Mount this on a separate listener (see [`metrics_port`]) so that /metrics is
/// not exposed on the public API port.
pub fn metrics_router() -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .layer(axum::middleware::from_fn(require_metrics_token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    fn request_with_auth(auth: Option<&str>) -> HttpRequest<Body> {
        let mut builder = HttpRequest::builder().uri("/metrics");
        if let Some(value) = auth {
            builder = builder.header(axum::http::header::AUTHORIZATION, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn metrics_refused_without_token() {
        env::set_var(METRICS_AUTH_TOKEN_ENV, "secret-token");
        let app = metrics_router();

        let response = app
            .clone()
            .oneshot(request_with_auth(None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .clone()
            .oneshot(request_with_auth(Some("Bearer wrong-token")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        env::remove_var(METRICS_AUTH_TOKEN_ENV);
    }

    #[tokio::test]
    async fn metrics_allowed_with_correct_token() {
        env::set_var(METRICS_AUTH_TOKEN_ENV, "secret-token");
        let app = metrics_router();

        let response = app
            .oneshot(request_with_auth(Some("Bearer secret-token")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let _ = body;

        env::remove_var(METRICS_AUTH_TOKEN_ENV);
    }
}
