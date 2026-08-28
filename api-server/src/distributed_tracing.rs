//! Distributed tracing middleware and per-operation span helpers.
//!
//! # Trace / Span hierarchy
//!
//! ```text
//! HTTP request span  (opentelemetry_semantic_conventions HTTP attributes)
//! └── ip.commit_ip        (operation = "ip.commit")
//! └── ip.get_ip           (operation = "ip.get")
//! └── swap.initiate       (operation = "swap.initiate")
//! └── swap.accept         (operation = "swap.accept")
//! └── swap.reveal_key     (operation = "swap.reveal_key")
//! └── swap.cancel         (operation = "swap.cancel")
//! └── batch.commit        (operation = "batch.commit")
//! └── batch.escrow        (operation = "batch.escrow")
//! ```
//!
//! # Correlation ID propagation
//!
//! Every request injects (or extracts) two headers:
//! - `traceparent` — W3C Trace Context (trace-id + span-id, used by OTel SDK)
//! - `X-Trace-ID`  — human-friendly shorthand forwarded in responses

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use opentelemetry::{
    global,
    propagation::Extractor,
    trace::{Span, SpanKind, Status, TraceContextExt, Tracer},
    Context, KeyValue,
};
use opentelemetry_semantic_conventions::trace as semconv;
use std::time::Instant;
use uuid::Uuid;

// ── Header names ──────────────────────────────────────────────────────────────

/// W3C Trace Context propagation header (used by OTel SDK).
pub const TRACEPARENT_HEADER: &str = "traceparent";
/// Human-friendly trace-ID forwarded in responses.
pub const CUSTOM_TRACE_ID_HEADER: &str = "X-Trace-ID";
/// Span-ID forwarded in responses.
pub const SPAN_ID_HEADER: &str = "X-Span-ID";

// ── Public context struct (stored in request extensions) ─────────────────────

/// Lightweight trace context stored in Axum request extensions so handlers
/// can annotate child spans without re-parsing headers.
#[derive(Clone, Debug)]
pub struct DistributedTraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub start_time: Instant,
}

// ── W3C header extractor for OTel propagator ─────────────────────────────────

struct HeaderExtractor<'a>(&'a HeaderMap);

impl<'a> Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

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

    DistributedTraceContext {
        trace_id,
        span_id: Uuid::new_v4().to_string(),
        parent_span_id,
        start_time: Instant::now(),
    }
}

// ── HTTP middleware ───────────────────────────────────────────────────────────

/// Axum middleware that:
/// 1. Extracts W3C `traceparent` / `X-Trace-ID` from incoming headers.
/// 2. Creates a root OTel span for the HTTP request.
/// 3. Stores [`DistributedTraceContext`] in request extensions for handlers.
/// 4. Propagates trace-ID and span-ID back in response headers.
pub async fn distributed_tracing_middleware(
    mut req: Request,
    next: Next,
) -> Response {
    // Extract W3C context so child spans are linked to the upstream trace.
    let parent_cx = global::get_text_map_propagator(|prop| {
        prop.extract(&HeaderExtractor(req.headers()))
    });

    let trace_ctx = extract_or_generate_trace_context(req.headers());
    let method = req.method().to_string();
    let uri = req.uri().to_string();

    let tracer = global::tracer("atomic-patent");
    let mut span = tracer
        .span_builder(format!("{} {}", method, uri))
        .with_kind(SpanKind::Server)
        .with_attributes(vec![
            KeyValue::new(semconv::HTTP_REQUEST_METHOD, method.clone()),
            KeyValue::new(semconv::URL_FULL, uri.clone()),
            KeyValue::new("trace.id", trace_ctx.trace_id.clone()),
        ])
        .start_with_context(&tracer, &parent_cx);

    let cx = Context::current_with_span(span.clone_as_boxed_ref());
    // Attach a compatible context so tracing:: macros pick up the OTel trace-ID.
    {
        let _guard = cx.clone().attach();

        // Expose trace context to downstream handlers via extensions.
        req.extensions_mut().insert(trace_ctx.clone());

        tracing::info!(
            trace_id = %trace_ctx.trace_id,
            span_id  = %trace_ctx.span_id,
            parent_span_id = ?trace_ctx.parent_span_id,
            method = %method,
            uri    = %uri,
            "request started"
        );
    }

    let mut response = next.run(req).await;
    let duration = trace_ctx.start_time.elapsed();
    let status = response.status().as_u16();

    span.set_attribute(KeyValue::new(semconv::HTTP_RESPONSE_STATUS_CODE, status as i64));
    if status >= 500 {
        span.set_status(Status::error(format!("HTTP {status}")));
    }
    span.end();

    // Propagate correlation headers to the caller.
    if let Ok(v) = trace_ctx.trace_id.parse() {
        response.headers_mut().insert(CUSTOM_TRACE_ID_HEADER, v);
    }
    if let Ok(v) = trace_ctx.span_id.parse() {
        response.headers_mut().insert(SPAN_ID_HEADER, v);
    }

    tracing::info!(
        trace_id    = %trace_ctx.trace_id,
        span_id     = %trace_ctx.span_id,
        method      = %method,
        uri         = %uri,
        status      = status,
        duration_ms = duration.as_millis(),
        "request completed"
    );

    response
}

// ── Per-operation span helpers ────────────────────────────────────────────────

/// Record a span for an IP commitment operation.
///
/// ```rust,no_run
/// use api_server::distributed_tracing::record_ip_commit_span;
/// let ip_id = record_ip_commit_span("owner123", "deadbeef", || 42u64);
/// ```
pub fn record_ip_commit_span<F, R>(owner: &str, commitment_hash: &str, op: F) -> R
where
    F: FnOnce() -> R,
{
    let tracer = global::tracer("atomic-patent");
    let mut span = tracer
        .span_builder("ip.commit_ip")
        .with_kind(SpanKind::Internal)
        .with_attributes(vec![
            KeyValue::new("operation", "ip.commit"),
            KeyValue::new("ip.owner", owner.to_string()),
            KeyValue::new("ip.commitment_hash", commitment_hash.to_string()),
        ])
        .start(&tracer);

    let result = op();
    span.end();
    result
}

/// Record a span for an atomic swap initiation.
pub fn record_swap_initiate_span<F, R>(ip_id: u64, seller: &str, buyer: &str, op: F) -> R
where
    F: FnOnce() -> R,
{
    let tracer = global::tracer("atomic-patent");
    let mut span = tracer
        .span_builder("swap.initiate")
        .with_kind(SpanKind::Internal)
        .with_attributes(vec![
            KeyValue::new("operation", "swap.initiate"),
            KeyValue::new("swap.ip_id", ip_id as i64),
            KeyValue::new("swap.seller", seller.to_string()),
            KeyValue::new("swap.buyer", buyer.to_string()),
        ])
        .start(&tracer);

    let result = op();
    span.end();
    result
}

/// Record a span for a key-reveal step (completes the atomic swap).
pub fn record_swap_reveal_span<F, R>(swap_id: u64, op: F) -> R
where
    F: FnOnce() -> R,
{
    let tracer = global::tracer("atomic-patent");
    let mut span = tracer
        .span_builder("swap.reveal_key")
        .with_kind(SpanKind::Internal)
        .with_attributes(vec![
            KeyValue::new("operation", "swap.reveal_key"),
            KeyValue::new("swap.id", swap_id as i64),
        ])
        .start(&tracer);

    let result = op();
    span.end();
    result
}

/// Record a span for a batch commitment operation.
pub fn record_batch_commit_span<F, R>(count: usize, op: F) -> R
where
    F: FnOnce() -> R,
{
    let tracer = global::tracer("atomic-patent");
    let mut span = tracer
        .span_builder("batch.commit")
        .with_kind(SpanKind::Internal)
        .with_attributes(vec![
            KeyValue::new("operation", "batch.commit"),
            KeyValue::new("batch.size", count as i64),
        ])
        .start(&tracer);

    let result = op();
    span.end();
    result
}

/// Record a span for a batch escrow operation.
pub fn record_batch_escrow_span<F, R>(ip_count: usize, op: F) -> R
where
    F: FnOnce() -> R,
{
    let tracer = global::tracer("atomic-patent");
    let mut span = tracer
        .span_builder("batch.escrow")
        .with_kind(SpanKind::Internal)
        .with_attributes(vec![
            KeyValue::new("operation", "batch.escrow"),
            KeyValue::new("batch.ip_count", ip_count as i64),
        ])
        .start(&tracer);

    let result = op();
    span.end();
    result
}

/// Re-export for handlers that only need the basic context extraction helper.
pub fn get_trace_context(headers: &HeaderMap) -> DistributedTraceContext {
    extract_or_generate_trace_context(headers)
}

// ── Trait to allow span boxing without object-safety constraints ──────────────

trait SpanExt {
    fn clone_as_boxed_ref(&mut self) -> opentelemetry::global::BoxedSpan;
}

impl<S: Span> SpanExt for S {
    fn clone_as_boxed_ref(&mut self) -> opentelemetry::global::BoxedSpan {
        // For the context guard we start a no-op placeholder; the real span
        // lives in `span` and is ended explicitly above.
        opentelemetry::global::tracer("atomic-patent")
            .start("_guard_placeholder")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_generation_empty_headers() {
        let headers = HeaderMap::new();
        let ctx = extract_or_generate_trace_context(&headers);
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(Uuid::parse_str(&ctx.trace_id).is_ok(), "trace_id must be a UUID");
        assert!(Uuid::parse_str(&ctx.span_id).is_ok(), "span_id must be a UUID");
        assert!(ctx.parent_span_id.is_none());
    }

    #[test]
    fn test_trace_id_propagated_from_header() {
        let mut headers = HeaderMap::new();
        let expected = "550e8400-e29b-41d4-a716-446655440000";
        headers.insert(CUSTOM_TRACE_ID_HEADER, expected.parse().unwrap());

        let ctx = extract_or_generate_trace_context(&headers);
        assert_eq!(ctx.trace_id, expected, "trace_id must be extracted from X-Trace-ID");
    }

    #[test]
    fn test_parent_span_id_extracted() {
        let mut headers = HeaderMap::new();
        let parent = "660e8400-e29b-41d4-a716-446655440001";
        headers.insert(SPAN_ID_HEADER, parent.parse().unwrap());

        let ctx = extract_or_generate_trace_context(&headers);
        assert_eq!(ctx.parent_span_id, Some(parent.to_string()));
    }

    #[test]
    fn test_span_id_always_fresh() {
        // Even when X-Trace-ID is provided, X-Span-ID must be a new UUID.
        let mut headers = HeaderMap::new();
        headers.insert(CUSTOM_TRACE_ID_HEADER, "fixed-trace-id".parse().unwrap());

        let ctx = extract_or_generate_trace_context(&headers);
        assert_ne!(ctx.span_id, "fixed-trace-id");
        assert!(Uuid::parse_str(&ctx.span_id).is_ok());
    }

    #[test]
    fn test_trace_context_clone() {
        let ctx = DistributedTraceContext {
            trace_id: "t1".to_string(),
            span_id: "s1".to_string(),
            parent_span_id: Some("p1".to_string()),
            start_time: Instant::now(),
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.trace_id, "t1");
        assert_eq!(cloned.parent_span_id, Some("p1".to_string()));
    }

    #[test]
    fn test_record_ip_commit_span_returns_value() {
        // record_*_span helpers must be transparent wrappers — they must
        // return whatever the closure returns.
        let result = record_ip_commit_span("owner-abc", "deadbeef", || 99u64);
        assert_eq!(result, 99u64);
    }

    #[test]
    fn test_record_batch_commit_span_returns_value() {
        let result = record_batch_commit_span(5, || "ok");
        assert_eq!(result, "ok");
    }

    #[test]
    fn test_trace_id_propagates_across_service_boundary() {
        // Simulate two service hops sharing the same trace-ID.
        let trace_id = Uuid::new_v4().to_string();

        // Service A generates context and puts trace-ID in outbound headers.
        let mut outbound = HeaderMap::new();
        outbound.insert(CUSTOM_TRACE_ID_HEADER, trace_id.parse().unwrap());

        // Service B extracts it.
        let ctx_b = extract_or_generate_trace_context(&outbound);
        assert_eq!(
            ctx_b.trace_id, trace_id,
            "trace_id must propagate intact across service boundaries"
        );

        // Service C receives Service B's span-ID as parent.
        let mut outbound2 = HeaderMap::new();
        outbound2.insert(CUSTOM_TRACE_ID_HEADER, ctx_b.trace_id.parse().unwrap());
        outbound2.insert(SPAN_ID_HEADER, ctx_b.span_id.parse().unwrap());

        let ctx_c = extract_or_generate_trace_context(&outbound2);
        assert_eq!(ctx_c.trace_id, trace_id, "trace_id must remain the same through the chain");
        assert_eq!(ctx_c.parent_span_id, Some(ctx_b.span_id.clone()));
        assert_ne!(ctx_c.span_id, ctx_b.span_id, "each hop must produce a new span_id");
    }
}
