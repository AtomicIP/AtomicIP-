//! Idempotency-key deduplication for write requests.
//!
//! Backs the `x-idempotency-key` middleware (#523): a client that retries a
//! `POST`/`PUT`/`PATCH` with the same idempotency key gets back the cached
//! result of the original request instead of re-executing it.
//!
//! Both halves of that guarantee — the completed-response cache
//! ([`DeduplicationStore`]) and the in-flight marker that stops a duplicate
//! from executing concurrently with the original ([`ConcurrentDeduplicator`])
//! — support a [`DeduplicationBackend::Redis`] backend so the guarantee holds
//! behind a load balancer, where a retry has no guarantee of landing on the
//! instance that handled the original request (#800). The default
//! [`DeduplicationBackend::InProcess`] backend keeps state in that process's
//! memory only and is safe for a single-instance deployment and for tests,
//! but two instances each running it enforce independent state — see the
//! `redis_backend_*` tests below for the cross-instance guarantee the Redis
//! backend provides instead.
use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::mapref::entry::Entry as DashEntry;
use dashmap::DashMap;
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

/// Idempotency-key cache TTL (#523): matches the client-facing guarantee
/// that a replayed request within one hour of the original gets the cached
/// result.
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Upper bound on how long a duplicate request waits for the in-flight
/// original to finish before giving up and treating the key as free again.
/// Bounds the damage if the original crashed without releasing its marker.
const PENDING_TTL: Duration = Duration::from_secs(30);

/// How often a `Redis`-backed waiter re-checks whether the in-flight marker
/// has cleared. The in-process backend instead wakes instantly via
/// `tokio::sync::Notify`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Where deduplication state (the completed-response cache and the
/// in-flight marker) is stored. See the module docs for why this matters
/// behind a load balancer.
#[derive(Clone, Default)]
pub enum DeduplicationBackend {
    /// State lives in this process's memory only. Two instances each
    /// running this backend do **not** share state — see the module docs.
    #[default]
    InProcess,
    /// State lives in Redis at the given connection URL, shared by every
    /// instance pointed at the same Redis. Required for correct idempotency
    /// behind a load balancer running more than one instance.
    Redis(String),
}

/// Storage behind both [`DeduplicationStore`] and [`ConcurrentDeduplicator`].
/// Implementations must make each operation atomic: `try_begin` in
/// particular must never let two callers both observe "first" for the same
/// key.
#[async_trait::async_trait]
trait StoreBackend: Send + Sync + fmt::Debug {
    async fn get_completed(&self, key: &str) -> Option<Value>;
    async fn set_completed(&self, key: &str, value: Value, ttl: Duration);
    async fn remove_completed(&self, key: &str);

    /// Attempt to mark `key` in-flight. Returns `true` if this call marked
    /// it (the caller is "first" and should execute, then call `end`),
    /// `false` if another caller already holds it.
    async fn try_begin(&self, key: &str, ttl: Duration) -> bool;
    /// Wait for `key`'s in-flight marker to clear, bounded by `max_wait`.
    /// Returns as soon as the marker is gone (released, or expired).
    async fn wait_for_release(&self, key: &str, max_wait: Duration);
    /// Clear the in-flight marker for `key`, releasing any waiters.
    async fn end(&self, key: &str);
}

#[derive(Debug, Default)]
struct InProcessBackend {
    completed: DashMap<String, (Value, Instant)>,
    pending: DashMap<String, Arc<tokio::sync::Notify>>,
}

#[async_trait::async_trait]
impl StoreBackend for InProcessBackend {
    async fn get_completed(&self, key: &str) -> Option<Value> {
        let entry = self.completed.get(key)?;
        if entry.1 > Instant::now() {
            Some(entry.0.clone())
        } else {
            drop(entry);
            self.completed.remove(key);
            None
        }
    }

    async fn set_completed(&self, key: &str, value: Value, ttl: Duration) {
        self.completed
            .insert(key.to_string(), (value, Instant::now() + ttl));
    }

    async fn remove_completed(&self, key: &str) {
        self.completed.remove(key);
    }

    async fn try_begin(&self, key: &str, _ttl: Duration) -> bool {
        match self.pending.entry(key.to_string()) {
            DashEntry::Occupied(_) => false,
            DashEntry::Vacant(v) => {
                v.insert(Arc::new(tokio::sync::Notify::new()));
                true
            }
        }
    }

    async fn wait_for_release(&self, key: &str, max_wait: Duration) {
        let Some(notify) = self.pending.get(key).map(|n| n.clone()) else {
            return;
        };
        let _ = tokio::time::timeout(max_wait, notify.notified()).await;
    }

    async fn end(&self, key: &str) {
        if let Some((_, notify)) = self.pending.remove(key) {
            notify.notify_waiters();
        }
    }
}

/// Shared state backed by Redis. See [`DeduplicationBackend::Redis`].
///
/// A connection is established lazily on first use. Every operation fails
/// open on a Redis error or timeout — deduplication degrades to "treat as
/// new request" rather than making the API unavailable when Redis is down,
/// mirroring the fallback philosophy in `cache.rs` and `rate_limit.rs`.
#[derive(Debug)]
struct RedisBackend {
    client: redis::Client,
    conn: tokio::sync::OnceCell<redis::aio::ConnectionManager>,
}

impl RedisBackend {
    fn new(url: &str) -> redis::RedisResult<Self> {
        Ok(Self {
            client: redis::Client::open(url)?,
            conn: tokio::sync::OnceCell::new(),
        })
    }

    async fn connection(&self) -> redis::RedisResult<redis::aio::ConnectionManager> {
        self.conn
            .get_or_try_init(|| async { redis::aio::ConnectionManager::new(self.client.clone()).await })
            .await
            .cloned()
    }

    fn done_key(key: &str) -> String {
        format!("dedupe:done:{key}")
    }

    fn pending_key(key: &str) -> String {
        format!("dedupe:pending:{key}")
    }
}

#[async_trait::async_trait]
impl StoreBackend for RedisBackend {
    async fn get_completed(&self, key: &str) -> Option<Value> {
        use redis::AsyncCommands;
        let mut conn = match self.connection().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::warn!(error = %err, "dedup redis backend unreachable (get), failing open");
                return None;
            }
        };
        match conn.get::<_, Option<String>>(Self::done_key(key)).await {
            Ok(Some(json)) => serde_json::from_str(&json).ok(),
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(error = %err, "dedup redis GET failed, failing open");
                None
            }
        }
    }

    async fn set_completed(&self, key: &str, value: Value, ttl: Duration) {
        use redis::AsyncCommands;
        let Ok(json) = serde_json::to_string(&value) else {
            return;
        };
        let mut conn = match self.connection().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::warn!(error = %err, "dedup redis backend unreachable (set), result not cached");
                return;
            }
        };
        let result: redis::RedisResult<()> = conn
            .set_ex(Self::done_key(key), json, ttl.as_secs().max(1))
            .await;
        if let Err(err) = result {
            tracing::warn!(error = %err, "dedup redis SET failed, result not cached");
        }
    }

    async fn remove_completed(&self, key: &str) {
        use redis::AsyncCommands;
        if let Ok(mut conn) = self.connection().await {
            let _: redis::RedisResult<()> = conn.del(Self::done_key(key)).await;
        }
    }

    async fn try_begin(&self, key: &str, ttl: Duration) -> bool {
        let mut conn = match self.connection().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::warn!(error = %err, "dedup redis backend unreachable (try_begin), failing open");
                return true;
            }
        };
        let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
            .arg(Self::pending_key(key))
            .arg(1)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut conn)
            .await;
        match result {
            Ok(reply) => reply.is_some(),
            Err(err) => {
                tracing::warn!(error = %err, "dedup redis try_begin failed, failing open");
                true
            }
        }
    }

    async fn wait_for_release(&self, key: &str, max_wait: Duration) {
        use redis::AsyncCommands;
        let redis_key = Self::pending_key(key);
        let deadline = Instant::now() + max_wait;
        loop {
            let mut conn = match self.connection().await {
                Ok(conn) => conn,
                Err(_) => return,
            };
            match conn.exists::<_, bool>(&redis_key).await {
                Ok(false) | Err(_) => return,
                Ok(true) => {}
            }
            if Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn end(&self, key: &str) {
        use redis::AsyncCommands;
        if let Ok(mut conn) = self.connection().await {
            let _: redis::RedisResult<()> = conn.del(Self::pending_key(key)).await;
        }
    }
}

fn build_backend(backend: DeduplicationBackend) -> Arc<dyn StoreBackend> {
    match backend {
        DeduplicationBackend::InProcess => Arc::new(InProcessBackend::default()),
        DeduplicationBackend::Redis(url) => {
            Arc::new(RedisBackend::new(&url).expect("invalid redis deduplication backend URL"))
        }
    }
}

/// Cache of completed responses, keyed by idempotency key. See the module
/// docs for the multi-instance guarantee this provides with
/// [`DeduplicationBackend::Redis`].
#[derive(Clone)]
pub struct DeduplicationStore {
    backend: Arc<dyn StoreBackend>,
}

impl DeduplicationStore {
    /// Look up the cached response for `key`, if any and unexpired.
    pub async fn get(&self, key: &str) -> Option<Value> {
        self.backend.get_completed(key).await
    }

    /// Cache `value` as the completed response for `key`, for the
    /// deduplication TTL (one hour, matching #523).
    pub async fn set(&self, key: &str, value: Value) {
        self.backend.set_completed(key, value, DEFAULT_TTL).await;
    }

    /// Remove any cached response for `key`.
    pub async fn remove(&self, key: &str) {
        self.backend.remove_completed(key).await;
    }
}

/// Create a [`DeduplicationStore`] backed by this process's memory only.
/// Safe for a single-instance deployment and for tests; **not** safe behind
/// a load balancer running more than one instance — use
/// [`create_store_with_backend`] with [`DeduplicationBackend::Redis`] there.
pub fn create_store() -> DeduplicationStore {
    DeduplicationStore {
        backend: build_backend(DeduplicationBackend::InProcess),
    }
}

/// Create a [`DeduplicationStore`] using the given backend.
pub fn create_store_with_backend(backend: DeduplicationBackend) -> DeduplicationStore {
    DeduplicationStore {
        backend: build_backend(backend),
    }
}

/// Deduplication middleware for idempotent requests.
/// Uses x-idempotency-key header to deduplicate identical concurrent requests.
pub async fn deduplication_middleware(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Only apply to write operations
    if !matches!(req.method(), &axum::http::Method::POST | &axum::http::Method::PUT | &axum::http::Method::PATCH) {
        return Ok(next.run(req).await);
    }

    let idempotency_key = headers
        .get("x-idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let store = req.extensions().get::<DeduplicationStore>().unwrap().clone();

    // Check for existing result (the store itself only returns unexpired entries).
    if let Some(cached_result) = store.get(idempotency_key).await {
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .header("x-idempotency-replayed", "true")
            .body(cached_result.to_string().into())
            .unwrap();
        return Ok(response);
    }

    let response = next.run(req).await;

    // Cache successful responses
    if response.status().is_success() {
        let status = response.status();
        if let Ok(body_bytes) = axum::body::to_bytes(response.into_body(), usize::MAX).await {
            if let Ok(json_value) = serde_json::from_slice::<Value>(&body_bytes) {
                store.set(idempotency_key, json_value.clone()).await;

                let new_response = Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(body_bytes.into())
                    .unwrap();
                return Ok(new_response);
            }
        }
    } else {
        return Ok(response);
    }

    Ok(Response::default())
}

/// Concurrent request deduplication - prevents duplicate concurrent requests
/// from hitting the backend multiple times. See the module docs for the
/// multi-instance guarantee this provides with [`DeduplicationBackend::Redis`].
#[derive(Clone)]
pub struct ConcurrentDeduplicator {
    backend: Arc<dyn StoreBackend>,
}

impl ConcurrentDeduplicator {
    /// Create a deduplicator backed by this process's memory only. Two
    /// instances each running this constructor do **not** coordinate with
    /// each other — use [`Self::with_backend`] with
    /// [`DeduplicationBackend::Redis`] for that.
    pub fn new() -> Self {
        Self {
            backend: build_backend(DeduplicationBackend::InProcess),
        }
    }

    /// Create a deduplicator using the given backend.
    pub fn with_backend(backend: DeduplicationBackend) -> Self {
        Self {
            backend: build_backend(backend),
        }
    }

    /// Check if a request is already in flight. If so, wait for it.
    /// Returns true if this is the first request, false if it's a duplicate.
    pub async fn acquire_or_wait(&self, key: &str) -> bool {
        if self.backend.try_begin(key, PENDING_TTL).await {
            return true;
        }
        self.backend.wait_for_release(key, PENDING_TTL).await;
        false
    }

    /// Mark a request as complete and notify waiters
    pub async fn release(&self, key: &str) {
        self.backend.end(key).await;
    }
}

impl Default for ConcurrentDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};

    #[tokio::test]
    async fn test_deduplication_requires_idempotency_key() {
        use axum::{middleware, routing::post, Router};
        use tower::ServiceExt;

        let app = Router::new()
            .route("/test", post(|| async { "ok" }))
            .layer(middleware::from_fn(deduplication_middleware));

        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_concurrent_deduplicator_first_request() {
        let dedup = ConcurrentDeduplicator::new();
        let is_first = dedup.acquire_or_wait("test-key").await;
        assert!(is_first);
        dedup.release("test-key").await;
    }

    #[tokio::test]
    async fn test_concurrent_deduplicator_duplicate_waits() {
        let dedup = Arc::new(ConcurrentDeduplicator::new());
        let dedup_clone = dedup.clone();

        let handle = tokio::spawn(async move {
            let is_first = dedup_clone.acquire_or_wait("test-key").await;
            assert!(is_first);
            tokio::time::sleep(Duration::from_millis(100)).await;
            dedup_clone.release("test-key").await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        let is_first = dedup.acquire_or_wait("test-key").await;
        assert!(!is_first);

        handle.await.unwrap();
    }

    /// #800: two independent `DeduplicationStore`s (standing in for two
    /// `api-server` instances behind a load balancer) pointed at the same
    /// Redis must share the completed-response cache, so a client's retry
    /// landing on either instance gets the same cached result instead of
    /// re-executing the request.
    #[tokio::test]
    async fn redis_backend_shares_completed_result_across_instances() {
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::redis::Redis;

        let container = Redis::default()
            .start()
            .await
            .expect("failed to start redis container - is Docker available?");
        let port = container
            .get_host_port_ipv4(6379)
            .await
            .expect("failed to map redis port");
        let url = format!("redis://127.0.0.1:{port}");

        let instance_a = create_store_with_backend(DeduplicationBackend::Redis(url.clone()));
        let instance_b = create_store_with_backend(DeduplicationBackend::Redis(url));

        let key = "batch-swap-key-1";
        let value = serde_json::json!({"swap_id": 42, "status": "ok"});

        // "Instance A" handles the original request and caches the result.
        assert!(instance_a.get(key).await.is_none());
        instance_a.set(key, value.clone()).await;

        // "Instance B" handles the client's retry. It must see the same
        // cached result via the shared Redis backend, not a miss.
        assert_eq!(instance_b.get(key).await, Some(value));
    }

    /// #800: a request still in flight on "instance A" must not be
    /// independently re-executed by "instance B" for the same key — the
    /// in-flight marker must be visible across instances too, not just the
    /// completed-response cache.
    #[tokio::test]
    async fn redis_backend_pending_marker_blocks_concurrent_instance() {
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::redis::Redis;

        let container = Redis::default()
            .start()
            .await
            .expect("failed to start redis container - is Docker available?");
        let port = container
            .get_host_port_ipv4(6379)
            .await
            .expect("failed to map redis port");
        let url = format!("redis://127.0.0.1:{port}");

        let instance_a = ConcurrentDeduplicator::with_backend(DeduplicationBackend::Redis(url.clone()));
        let instance_b = Arc::new(ConcurrentDeduplicator::with_backend(DeduplicationBackend::Redis(url)));

        let key = "batch-swap-inflight-1";

        // "Instance A" starts handling the request first.
        assert!(
            instance_a.acquire_or_wait(key).await,
            "instance A should be the first to acquire the key"
        );

        // "Instance B" receives the client's retry while A is still working.
        let waiter = {
            let instance_b = instance_b.clone();
            tokio::spawn(async move { instance_b.acquire_or_wait(key).await })
        };

        // Give the waiter time to observe the shared marker and start
        // waiting, rather than racing ahead of instance A's acquire.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !waiter.is_finished(),
            "instance B must not independently re-execute the still-in-flight request"
        );

        // "Instance A" finishes and releases the shared marker.
        instance_a.release(key).await;

        let is_first = waiter.await.unwrap();
        assert!(
            !is_first,
            "instance B must observe instance A's release, not become 'first' on its own"
        );
    }
}
