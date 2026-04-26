use axum::http::{Request, HeaderMap};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;
use std::time::Instant;

/// Trace ID header name
pub const TRACE_ID_HEADER: &str = "X-Trace-ID";

/// Request ID header name
pub const REQUEST_ID_HEADER: &str = "X-Request-ID";

/// Middleware to add request tracing with trace IDs
pub async fn trace_requests(
    mut req: Request,
    next: Next,
) -> Response {
    let trace_id = extract_or_generate_trace_id(req.headers());
    let request_id = Uuid::new_v4().to_string();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = Instant::now();

    // Store trace context in extensions
    req.extensions_mut().insert(TraceContext {
        trace_id: trace_id.clone(),
        request_id: request_id.clone(),
    });

    // Log request start
    tracing::info!(
        trace_id = %trace_id,
        request_id = %request_id,
        method = %method,
        uri = %uri,
        "Request started"
    );

    let mut response = next.run(req).await;
    let duration = start.elapsed();

    // Add trace headers to response
    response.headers_mut().insert(
        TRACE_ID_HEADER,
        trace_id.parse().unwrap(),
    );
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id.parse().unwrap(),
    );

    // Log request completion
    tracing::info!(
        trace_id = %trace_id,
        request_id = %request_id,
        method = %method,
        uri = %uri,
        status = response.status().as_u16(),
        duration_ms = duration.as_millis(),
        "Request completed"
    );

    response
}

/// Extract trace ID from request headers or generate a new one
fn extract_or_generate_trace_id(headers: &HeaderMap) -> String {
    headers
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// Trace context stored in request extensions
#[derive(Clone, Debug)]
pub struct TraceContext {
    pub trace_id: String,
    pub request_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_generation() {
        let headers = HeaderMap::new();
        let trace_id = extract_or_generate_trace_id(&headers);
        assert!(!trace_id.is_empty());
        // Should be a valid UUID
        assert!(Uuid::parse_str(&trace_id).is_ok());
    }

    #[test]
    fn test_trace_id_extraction() {
        let mut headers = HeaderMap::new();
        let original_id = "550e8400-e29b-41d4-a716-446655440000";
        headers.insert(TRACE_ID_HEADER, original_id.parse().unwrap());
        
        let trace_id = extract_or_generate_trace_id(&headers);
        assert_eq!(trace_id, original_id);
    }
}
