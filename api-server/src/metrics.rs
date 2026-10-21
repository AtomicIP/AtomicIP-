use std::time::Instant;
use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram};
use tracing::info;

use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::env;

use crate::lock_utils::lock_or_recover;

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
    describe_counter!(
        "audit_fallback_active",
        "Number of times the in-memory fallback audit store is active"
    );
    describe_gauge!(
        "open_connections",
        "Currently open WebSocket/SSE connections"
    );
    describe_gauge!(
        "cache_entries",
        "Current number of entries stored in the response cache"
    );
    
    Mutex::new(handle)
});

/// Record the current number of open WebSocket/SSE connections.
///
/// Called by the connection limiter in `websocket.rs`/`events.rs` whenever a
/// connection is acquired or released so the gauge reflects the live count.
pub fn set_open_connections(count: u64) {
    gauge!("open_connections").set(count as f64);
}

/// Record the current number of open connections for a specific protocol
/// (e.g. "websocket" or "sse").
pub fn set_open_connections_for(protocol: &'static str, count: u64) {
    gauge!("open_connections", "protocol" => protocol).set(count as f64);
}

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
    let handle = lock_or_recover(&PROMETHEUS_HANDLE);
    Ok(handle.render())
}

/// Record that the in-memory fallback audit store is active.
pub fn record_audit_fallback_active() {
    counter!("audit_fallback_active").increment(1);
}
