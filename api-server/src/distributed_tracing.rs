use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use uuid::Uuid;

/// Distributed trace context
#[derive(Clone, Debug)]
pub struct DistributedTraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub start_time: Instant,
}

/// Trace ID header name (W3C standard)
pub const TRACE_ID_HEADER: &str = "traceparent";

/// Custom trace ID header
pub const CUSTOM_TRACE_ID_HEADER: &str = "X-Trace-ID";

/// Custom span ID header
pub const SPAN_ID_HEADER: &str = "X-Span-ID";

/// Extract or generate distributed trace context
fn extract_or_generate_trace_context(headers: &HeaderMap) -> DistributedTraceContext {
    let trace_id = headers
        .get(CUSTOM_TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let parent_span_id = headers
        .get(SPAN_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let span_id = Uuid::new_v4().to_string();

    DistributedTraceContext {
        trace_id,
        span_id,
        parent_span_id,
        start_time: Instant::now(),
    }
}

/// Middleware for distributed request tracing
pub async fn distributed_tracing_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    let trace_context = extract_or_generate_trace_context(req.headers());
    let method = req.method().clone();
    let uri = req.uri().clone();

    // Store trace context in extensions
    req.extensions_mut().insert(trace_context.clone());

    // Log request start
    tracing::info!(
        trace_id = %trace_context.trace_id,
        span_id = %trace_context.span_id,
        parent_span_id = ?trace_context.parent_span_id,
        method = %method,
        uri = %uri,
        "Distributed trace: request started"
    );

    let mut response = next.run(req).await;
    let duration = trace_context.start_time.elapsed();

    // Add trace headers to response
    response.headers_mut().insert(
        CUSTOM_TRACE_ID_HEADER,
        trace_context.trace_id.parse().unwrap(),
    );
    response.headers_mut().insert(
        SPAN_ID_HEADER,
        trace_context.span_id.parse().unwrap(),
    );

    // Log request completion
    tracing::info!(
        trace_id = %trace_context.trace_id,
        span_id = %trace_context.span_id,
        method = %method,
        uri = %uri,
        status = response.status().as_u16(),
        duration_ms = duration.as_millis(),
        "Distributed trace: request completed"
    );

    response
}

/// Helper to get trace context from request extensions
pub fn get_trace_context(headers: &HeaderMap) -> DistributedTraceContext {
    extract_or_generate_trace_context(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_generation() {
        let headers = HeaderMap::new();
        let context = extract_or_generate_trace_context(&headers);
        
        assert!(!context.trace_id.is_empty());
        assert!(!context.span_id.is_empty());
        assert!(Uuid::parse_str(&context.trace_id).is_ok());
        assert!(Uuid::parse_str(&context.span_id).is_ok());
    }

    #[test]
    fn test_trace_context_extraction() {
        let mut headers = HeaderMap::new();
        let trace_id = "550e8400-e29b-41d4-a716-446655440000";
        let span_id = "660e8400-e29b-41d4-a716-446655440001";
        
        headers.insert(CUSTOM_TRACE_ID_HEADER, trace_id.parse().unwrap());
        headers.insert(SPAN_ID_HEADER, span_id.parse().unwrap());
        
        let context = extract_or_generate_trace_context(&headers);
        
        assert_eq!(context.trace_id, trace_id);
        assert_eq!(context.span_id, span_id);
    }

    #[test]
    fn test_parent_span_extraction() {
        let mut headers = HeaderMap::new();
        let parent_span = "770e8400-e29b-41d4-a716-446655440002";
        
        headers.insert(SPAN_ID_HEADER, parent_span.parse().unwrap());
        
        let context = extract_or_generate_trace_context(&headers);
        
        assert_eq!(context.parent_span_id, Some(parent_span.to_string()));
    }

    #[test]
    fn test_trace_context_clone() {
        let context = DistributedTraceContext {
            trace_id: "test-trace".to_string(),
            span_id: "test-span".to_string(),
            parent_span_id: Some("parent-span".to_string()),
            start_time: Instant::now(),
        };

        let cloned = context.clone();
        assert_eq!(cloned.trace_id, context.trace_id);
        assert_eq!(cloned.span_id, context.span_id);
    }
}
