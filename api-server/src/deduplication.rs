use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 24-hour TTL for cached idempotent responses as required by the spec.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Header names for request deduplication.
const HEADER_IDEMPOTENCY_KEY: &str = "x-idempotency-key";
const HEADER_REQUEST_ID: &str = "x-request-id";
const HEADER_REPLAYED: &str = "x-idempotency-replayed";

pub type DeduplicationStore = Arc<DashMap<String, CachedResponse>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedResponse {
    pub body: Value,
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub cached_at: u64,
}

pub fn create_store() -> DeduplicationStore {
    Arc::new(DashMap::new())
}

/// Extract and validate a request ID from headers.
/// Checks `x-request-id` first, then `x-idempotency-key`.
pub fn extract_request_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(HEADER_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get(HEADER_IDEMPOTENCY_KEY)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

/// Validate that a request ID meets the required format (non-empty, valid UUID or hex).
pub fn validate_request_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 256 {
        return false;
    }
    // Accept UUID format, hex strings, or alphanumeric identifiers
    id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Deduplication middleware for idempotent requests.
/// Uses x-idempotency-key header to cache and replay responses for 24 hours.
pub async fn deduplication_middleware(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !matches!(
        req.method(),
        &axum::http::Method::POST
            | &axum::http::Method::PUT
            | &axum::http::Method::PATCH
            | &axum::http::Method::DELETE
    ) {
        return Ok(next.run(req).await);
    }

    let idempotency_key = headers
        .get(HEADER_IDEMPOTENCY_KEY)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    if !validate_request_id(idempotency_key) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let store = req
        .extensions()
        .get::<DeduplicationStore>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(entry) = store.get(idempotency_key) {
        if entry.cached_at > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let age = now.saturating_sub(entry.cached_at);
            if age < CACHE_TTL.as_secs() {
                let mut response_builder = Response::builder()
                    .status(entry.status_code)
                    .header("content-type", "application/json")
                    .header(HEADER_REPLAYED, "true");
                for (k, v) in &entry.headers {
                    response_builder = response_builder.header(k.as_str(), v.as_str());
                }
                let body_bytes = serde_json::to_vec(&entry.body).unwrap_or_default();
                return response_builder
                    .body(axum::body::Body::from(body_bytes))
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
            }
            store.remove(idempotency_key);
        }
    }

    let response = next.run(req).await;
    let (parts, body) = response.into_parts();

    if parts.status.is_success() {
        match axum::body::to_bytes(body, usize::MAX).await {
            Ok(body_bytes) => {
                if let Ok(json_value) = serde_json::from_slice::<Value>(&body_bytes) {
                    let cached = CachedResponse {
                        body: json_value,
                        status_code: parts.status.as_u16(),
                        headers: parts
                            .headers
                            .iter()
                            .map(|(k, v)| {
                                (
                                    k.as_str().to_string(),
                                    v.to_str().unwrap_or("").to_string(),
                                )
                            })
                            .collect(),
                        cached_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    };
                    store.insert(idempotency_key.to_string(), cached);

                    let new_response = Response::from_parts(parts, body_bytes.into());
                    return Ok(new_response);
                }
                // JSON parsing failed, return original body
                let reconstructed = Response::from_parts(parts, body_bytes.into());
                return Ok(reconstructed);
            }
            Err(_) => {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    Ok(Response::from_parts(parts, body))
}

/// Garbage collection for expired deduplication entries.
/// Removes entries older than CACHE_TTL.
pub fn garbage_collect(store: &DeduplicationStore) -> usize {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut removed = 0;
    store.retain(|_, entry| {
        let age = now.saturating_sub(entry.cached_at);
        if age >= CACHE_TTL.as_secs() {
            removed += 1;
            false
        } else {
            true
        }
    });
    removed
}

/// Spawn a background GC task that runs periodically.
pub fn spawn_gc_task(store: DeduplicationStore, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let removed = garbage_collect(&store);
            if removed > 0 {
                tracing::info!(removed = removed, "GC removed expired dedup entries");
            }
        }
    });
}

/// Concurrent request deduplication to prevent duplicate in-flight requests.
pub struct ConcurrentDeduplicator {
    pending: Arc<DashMap<String, Arc<tokio::sync::Notify>>>,
}

impl ConcurrentDeduplicator {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
        }
    }

    /// Check if a request is already in flight. If so, wait for it.
    /// Returns true if this is the first request, false if it's a duplicate.
    pub async fn acquire_or_wait(&self, key: &str) -> bool {
        use dashmap::mapref::entry::Entry;
        match self.pending.entry(key.to_string()) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(tokio::sync::Notify::new()));
                true
            }
            Entry::Occupied(entry) => {
                let notify = entry.get().clone();
                notify.notified().await;
                false
            }
        }
    }

    /// Mark a request as complete and notify waiters.
    pub fn release(&self, key: &str) {
        if let Some((_, notify)) = self.pending.remove(key) {
            notify.notify_waiters();
        }
    }

    /// Current number of in-flight requests.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, middleware, routing::post, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_dedup_requires_idempotency_key() {
        let app = Router::new()
            .route("/test", post(|| async { "ok" }))
            .layer(middleware::from_fn(deduplication_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_dedup_returns_cached_response() {
        let store = create_store();
        let app = Router::new()
            .route("/test", post(|| async { "cached" }))
            .layer(middleware::from_fn(|headers: HeaderMap, mut req: Request, next: Next| {
                req.extensions_mut().insert(store.clone());
                deduplication_middleware(headers, req, next)
            }));

        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .header("x-idempotency-key", "key-1")
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);
        assert!(resp1.headers().get("x-idempotency-replayed").is_none());

        let req2 = Request::builder()
            .method("POST")
            .uri("/test")
            .header("x-idempotency-key", "key-1")
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        assert_eq!(
            resp2
                .headers()
                .get("x-idempotency-replayed")
                .unwrap()
                .to_str()
                .unwrap(),
            "true"
        );
    }

    #[tokio::test]
    async fn test_dedup_distinct_keys_not_replayed() {
        let store = create_store();
        let app = Router::new()
            .route("/test", post(|| async { "unique" }))
            .layer(middleware::from_fn(|headers: HeaderMap, mut req: Request, next: Next| {
                req.extensions_mut().insert(store.clone());
                deduplication_middleware(headers, req, next)
            }));

        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .header("x-idempotency-key", "key-a")
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        let req2 = Request::builder()
            .method("POST")
            .uri("/test")
            .header("x-idempotency-key", "key-b")
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        assert!(resp2.headers().get("x-idempotency-replayed").is_none());
    }

    #[tokio::test]
    async fn test_get_requests_pass_through() {
        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn(deduplication_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_extract_request_id_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "550e8400-e29b-41d4-a716-446655440000".parse().unwrap());
        assert_eq!(
            extract_request_id(&headers),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[test]
    fn test_extract_request_id_empty_returns_none() {
        let headers = HeaderMap::new();
        assert_eq!(extract_request_id(&headers), None);
    }

    #[test]
    fn test_validate_request_id_valid() {
        assert!(validate_request_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(validate_request_id("abc123"));
        assert!(validate_request_id("a-b_c"));
    }

    #[test]
    fn test_validate_request_id_invalid() {
        assert!(!validate_request_id(""));
        assert!(!validate_request_id("a b")); // spaces not allowed
    }

    #[tokio::test]
    async fn test_concurrent_dedup_first_request() {
        let dedup = ConcurrentDeduplicator::new();
        let is_first = dedup.acquire_or_wait("test-key").await;
        assert!(is_first);
        dedup.release("test-key");
    }

    #[tokio::test]
    async fn test_concurrent_dedup_duplicate_waits() {
        let dedup = Arc::new(ConcurrentDeduplicator::new());
        let dedup_clone = dedup.clone();

        let handle = tokio::spawn(async move {
            let is_first = dedup_clone.acquire_or_wait("test-key").await;
            assert!(is_first);
            tokio::time::sleep(Duration::from_millis(100)).await;
            dedup_clone.release("test-key");
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        let is_first = dedup.acquire_or_wait("test-key").await;
        assert!(!is_first);

        handle.await.unwrap();
    }

    #[test]
    fn test_garbage_collect_removes_expired() {
        let store = create_store();
        let old_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            - CACHE_TTL.as_secs()
            - 1;

        store.insert(
            "expired-key".to_string(),
            CachedResponse {
                body: Value::Null,
                status_code: 200,
                headers: vec![],
                cached_at: old_time,
            },
        );
        store.insert(
            "fresh-key".to_string(),
            CachedResponse {
                body: Value::Null,
                status_code: 200,
                headers: vec![],
                cached_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            },
        );

        let removed = garbage_collect(&store);
        assert_eq!(removed, 1);
        assert!(store.contains_key("fresh-key"));
        assert!(!store.contains_key("expired-key"));
    }

    #[tokio::test]
    async fn test_concurrent_dedup_pending_count() {
        let dedup = ConcurrentDeduplicator::new();
        dedup.acquire_or_wait("key1").await;
        dedup.acquire_or_wait("key2").await;
        assert_eq!(dedup.pending_count(), 2);
        dedup.release("key1");
        assert_eq!(dedup.pending_count(), 1);
        dedup.release("key2");
        assert_eq!(dedup.pending_count(), 0);
    }
}
