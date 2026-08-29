use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use metrics::{gauge, histogram};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Queue configuration
#[derive(Clone, Debug)]
pub struct QueueConfig {
    pub max_queue_size: usize,
    pub max_concurrent_requests: usize,
    pub request_timeout: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        QueueConfig {
            max_queue_size: 1000,
            max_concurrent_requests: 100,
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// Request queue entry
#[derive(Clone, Debug)]
pub struct QueueEntry {
    pub request_id: String,
    pub enqueued_at: Instant,
    pub priority: u32,
}

/// Request queue manager
pub struct RequestQueue {
    config: QueueConfig,
    semaphore: Arc<Semaphore>,
    queue: Arc<DashMap<String, QueueEntry>>,
    queue_size: Arc<std::sync::atomic::AtomicUsize>,
}

impl RequestQueue {
    pub fn new(config: QueueConfig) -> Self {
        RequestQueue {
            config: config.clone(),
            semaphore: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            queue: Arc::new(DashMap::new()),
            queue_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Try to acquire a slot in the queue
    pub async fn acquire(&self, request_id: String) -> Result<QueueGuard, StatusCode> {
        // Reserve a slot immediately (before waiting for a semaphore permit) so that
        // queue_size reflects all pending + active requests.  This is the correct
        // backpressure signal: a request is "in the queue" from the moment it
        // attempts to enter, not only after it wins a concurrency permit.
        let prev_size = self.queue_size.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if prev_size >= self.config.max_queue_size {
            // Over capacity — undo the reservation and reject immediately.
            self.queue_size.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            tracing::warn!(
                queue_size = prev_size,
                max_size = self.config.max_queue_size,
                "Queue is full — request rejected"
            );
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }

        // Register the entry so get_stats() can compute wait times.
        let entry = QueueEntry {
            request_id: request_id.clone(),
            enqueued_at: Instant::now(),
            priority: 0,
        };
        self.queue.insert(request_id.clone(), entry);

        // Wait for a concurrency slot (bounded by request_timeout).
        let permit = match tokio::time::timeout(
            self.config.request_timeout,
            Arc::clone(&self.semaphore).acquire_owned(),
        )
        .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(_)) => {
                // Semaphore closed — clean up.
                self.queue.remove(&request_id);
                self.queue_size.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            Err(_) => {
                // Timeout waiting for a slot.
                self.queue.remove(&request_id);
                self.queue_size.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(request_id = %request_id, "Request timeout waiting for queue slot");
                return Err(StatusCode::REQUEST_TIMEOUT);
            }
        };

        tracing::debug!(
            request_id = %request_id,
            queue_size = prev_size + 1,
            "Request acquired concurrency slot"
        );

        Ok(QueueGuard {
            request_id,
            queue: self.queue.clone(),
            queue_size: self.queue_size.clone(),
            _permit: permit,
        })
    }

    /// Get current queue size
    pub fn get_queue_size(&self) -> usize {
        self.queue_size.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> QueueStats {
        let entries: Vec<_> = self.queue.iter().collect();
        let wait_times: Vec<Duration> = entries
            .iter()
            .map(|e| e.value().enqueued_at.elapsed())
            .collect();

        let avg_wait_time = if !wait_times.is_empty() {
            let total: Duration = wait_times.iter().sum();
            total / wait_times.len() as u32
        } else {
            Duration::from_secs(0)
        };

        QueueStats {
            queue_size: self.queue_size.load(std::sync::atomic::Ordering::Relaxed),
            max_queue_size: self.config.max_queue_size,
            max_concurrent_requests: self.config.max_concurrent_requests,
            avg_wait_time,
        }
    }
}

/// Guard that removes request from queue when dropped
pub struct QueueGuard {
    request_id: String,
    queue: Arc<DashMap<String, QueueEntry>>,
    queue_size: Arc<std::sync::atomic::AtomicUsize>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl std::fmt::Debug for QueueGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueGuard")
            .field("request_id", &self.request_id)
            .finish()
    }
}

impl Drop for QueueGuard {
    fn drop(&mut self) {
        self.queue.remove(&self.request_id);
        self.queue_size
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!(request_id = %self.request_id, "Request dequeued");
    }
}

/// Queue statistics
#[derive(Clone, Debug)]
pub struct QueueStats {
    pub queue_size: usize,
    pub max_queue_size: usize,
    pub max_concurrent_requests: usize,
    pub avg_wait_time: Duration,
}

/// Middleware for request queuing/backpressure, backed by a real `RequestQueue`.
///
/// Every request must acquire a queue slot before reaching its handler. If the
/// queue is already at `max_queue_size`, the request is rejected immediately
/// with `503 Service Unavailable`. Otherwise it waits (FIFO, via the
/// underlying semaphore) for one of `max_concurrent_requests` slots to free
/// up, up to `request_timeout`; a wait that exceeds the timeout is rejected
/// with `408 Request Timeout`. Queue depth and wait time are published as
/// metrics so operators can observe backpressure occurring.
pub async fn request_queue_middleware(
    State(queue): State<Arc<RequestQueue>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let wait_start = Instant::now();

    let guard = queue.acquire(request_id).await?;

    histogram!("request_queue_wait_seconds").record(wait_start.elapsed().as_secs_f64());
    gauge!("request_queue_depth").set(queue.get_queue_size() as f64);

    let response = next.run(req).await;
    drop(guard);
    gauge!("request_queue_depth").set(queue.get_queue_size() as f64);

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest, routing::get, Router};
    use tower::ServiceExt;

    fn app_with_queue(config: QueueConfig) -> Router {
        let queue = Arc::new(RequestQueue::new(config));
        Router::new()
            .route("/ok", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                queue,
                request_queue_middleware,
            ))
    }

    /// Issue #792: requests within capacity are actually served through the
    /// real middleware path, not just through direct `RequestQueue` calls.
    #[tokio::test]
    async fn test_middleware_serves_requests_within_capacity() {
        let app = app_with_queue(QueueConfig {
            max_queue_size: 10,
            max_concurrent_requests: 10,
            request_timeout: Duration::from_secs(5),
        });

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/ok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Issue #792: once the queue is at capacity, excess requests are
    /// rejected through the real middleware path rather than passing through
    /// unthrottled.
    #[tokio::test]
    async fn test_middleware_rejects_when_queue_full() {
        let config = QueueConfig {
            max_queue_size: 1,
            max_concurrent_requests: 100,
            request_timeout: Duration::from_secs(5),
        };
        let queue = Arc::new(RequestQueue::new(config));
        // Occupy the only queue slot directly so the middleware call below
        // must observe the queue as full.
        let _held = queue.acquire("holder".to_string()).await.unwrap();

        let app = Router::new()
            .route("/ok", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                queue,
                request_queue_middleware,
            ));

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/ok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_queue_creation() {
        let config = QueueConfig::default();
        let queue = RequestQueue::new(config);
        
        assert_eq!(queue.get_queue_size(), 0);
    }

    #[tokio::test]
    async fn test_queue_acquire() {
        let config = QueueConfig {
            max_queue_size: 10,
            max_concurrent_requests: 2,
            request_timeout: Duration::from_secs(5),
        };
        let queue = RequestQueue::new(config);
        
        let guard = queue.acquire("req-1".to_string()).await;
        assert!(guard.is_ok());
        assert_eq!(queue.get_queue_size(), 1);
    }

    #[tokio::test]
    async fn test_queue_full() {
        let config = QueueConfig {
            max_queue_size: 1,
            max_concurrent_requests: 100,
            request_timeout: Duration::from_secs(5),
        };
        let queue = Arc::new(RequestQueue::new(config));
        
        let _guard1 = queue.acquire("req-1".to_string()).await.unwrap();
        let result = queue.acquire("req-2".to_string()).await;
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_queue_guard_cleanup() {
        let config = QueueConfig::default();
        let queue = RequestQueue::new(config);
        
        {
            let _guard = queue.acquire("req-1".to_string()).await.unwrap();
            assert_eq!(queue.get_queue_size(), 1);
        }
        
        // Guard dropped, queue should be cleaned up
        assert_eq!(queue.get_queue_size(), 0);
    }

    #[tokio::test]
    async fn test_queue_stats() {
        let config = QueueConfig::default();
        let queue = RequestQueue::new(config);
        
        let _guard = queue.acquire("req-1".to_string()).await.unwrap();
        let stats = queue.get_stats();
        
        assert_eq!(stats.queue_size, 1);
        assert_eq!(stats.max_queue_size, 1000);
    }

    #[tokio::test]
    async fn test_concurrent_requests() {
        let config = QueueConfig {
            max_queue_size: 100,
            max_concurrent_requests: 5,
            request_timeout: Duration::from_secs(5),
        };
        let queue = Arc::new(RequestQueue::new(config));
        
        let mut handles = vec![];
        for i in 0..5 {
            let queue_clone = queue.clone();
            let handle = tokio::spawn(async move {
                queue_clone
                    .acquire(format!("req-{}", i))
                    .await
                    .is_ok()
            });
            handles.push(handle);
        }
        
        for handle in handles {
            assert!(handle.await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_backpressure_under_simulated_rpc_latency() {
        // Simulates RPC latency in the 200ms–2s range with realistic queue backpressure.
        // Design: max_queue_size=5, max_concurrent_requests=2.
        // Two permits are held directly (simulating in-flight Soroban RPC calls).
        // Three more slots are occupied by waiting requests (total queue depth = 5).
        // A 6th request must be rejected immediately with 503 Service Unavailable.
        let config = QueueConfig {
            max_queue_size: 5,
            max_concurrent_requests: 2,
            request_timeout: Duration::from_millis(1500),
        };
        let queue = Arc::new(RequestQueue::new(config));

        // Acquire both concurrency permits on the main task — guaranteed before any assertion.
        let guard_rpc1 = queue.acquire("req-rpc-1".to_string()).await.unwrap();
        let guard_rpc2 = queue.acquire("req-rpc-2".to_string()).await.unwrap();
        assert_eq!(queue.get_queue_size(), 2);

        // Spawn 3 additional requests; they block on the semaphore but increment queue_size
        // as soon as they enter acquire().
        let q3 = queue.clone();
        let h3 = tokio::spawn(async move {
            let res = q3.acquire("req-rpc-3".to_string()).await;
            assert!(res.is_ok());
        });

        let q4 = queue.clone();
        let h4 = tokio::spawn(async move {
            let res = q4.acquire("req-rpc-4".to_string()).await;
            assert!(res.is_ok());
        });

        let q5 = queue.clone();
        let h5 = tokio::spawn(async move {
            let res = q5.acquire("req-rpc-5".to_string()).await;
            assert!(res.is_ok());
        });

        // Retry loop: wait until all 3 spawned tasks have entered acquire() and
        // registered in the queue (they increment queue_size before blocking on
        // the semaphore). Avoids fixed-sleep flakiness.
        for _ in 0..50 {
            if queue.get_queue_size() == 5 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(queue.get_queue_size(), 5, "queue should be full (5/5)");

        // 6th request exceeds max_queue_size -> rejected immediately with 503.
        let rej = queue.acquire("req-rpc-rejected".to_string()).await;
        assert_eq!(rej.unwrap_err(), StatusCode::SERVICE_UNAVAILABLE);

        // Release held permits so spawned tasks can complete.
        drop(guard_rpc1);
        drop(guard_rpc2);
        h3.await.unwrap();
        h4.await.unwrap();
        h5.await.unwrap();

        // After all tasks finish, queue drains back to 0.
        assert_eq!(queue.get_queue_size(), 0);
    }

    #[tokio::test]
    async fn test_queue_depth_and_rejection_limits_under_rpc_latency() {
        // Verifies queue depth and rejection behaviour under a single-concurrency config
        // that simulates Soroban RPC latency holding the one available permit.
        let config = QueueConfig {
            max_queue_size: 3,
            max_concurrent_requests: 1,
            request_timeout: Duration::from_millis(1000),
        };
        let queue = Arc::new(RequestQueue::new(config));

        // First request acquires the single permit (simulates in-flight RPC call).
        let guard1 = queue.acquire("req-1".to_string()).await.unwrap();
        assert_eq!(queue.get_queue_size(), 1);

        // Spawn two more requests; they block on the semaphore but count in queue_size.
        let q_clone = queue.clone();
        let h2 = tokio::spawn(async move {
            let _g = q_clone.acquire("req-2".to_string()).await;
        });
        let q_clone2 = queue.clone();
        let h3 = tokio::spawn(async move {
            let _g = q_clone2.acquire("req-3".to_string()).await;
        });

        // Retry loop — wait until queue reaches expected depth.
        for _ in 0..50 {
            if queue.get_queue_size() == 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(queue.get_queue_size(), 3, "queue should be full (3/3)");

        // Confirm rejection when the queue limit is hit.
        let reject_res = queue.acquire("req-overflow".to_string()).await;
        assert!(reject_res.is_err());
        assert_eq!(reject_res.unwrap_err(), StatusCode::SERVICE_UNAVAILABLE);

        // Release the first permit so waiting tasks can complete.
        drop(guard1);
        h2.await.unwrap();
        h3.await.unwrap();
    }

    #[tokio::test]
    async fn test_request_timeout_under_high_rpc_latency() {
        let config = QueueConfig {
            max_queue_size: 10,
            max_concurrent_requests: 1,
            request_timeout: Duration::from_millis(200), // Short queue wait timeout
        };
        let queue = Arc::new(RequestQueue::new(config));

        // Holder simulates high RPC latency (e.g. 2000ms / 2s)
        let q_holder = queue.clone();
        let holder = tokio::spawn(async move {
            let guard = q_holder.acquire("slow-rpc-call".to_string()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(1000)).await;
            drop(guard);
        });

        tokio::time::sleep(Duration::from_millis(20)).await;

        // Waiting request should time out waiting for semaphore permit
        let timed_out_req = queue.acquire("waiting-req".to_string()).await;
        assert_eq!(timed_out_req.unwrap_err(), StatusCode::REQUEST_TIMEOUT);

        holder.await.unwrap();
    }
}

