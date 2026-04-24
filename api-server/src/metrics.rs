use std::time::Instant;
use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use metrics::{counter, histogram, describe_counter, describe_histogram};
use tracing::info;

use once_cell::sync::Lazy;
use std::sync::Mutex;

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

