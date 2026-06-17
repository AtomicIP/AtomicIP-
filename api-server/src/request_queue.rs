use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
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
        let current_size = self.queue_size.load(std::sync::atomic::Ordering::Relaxed);
        
        if current_size >= self.config.max_queue_size {
            tracing::warn!(
                queue_size = current_size,
                max_size = self.config.max_queue_size,
                "Queue is full"
            );
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }

        // Try to acquire semaphore permit
        let permit = match tokio::time::timeout(
            self.config.request_timeout,
            self.semaphore.acquire(),
        )
        .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(_)) => return Err(StatusCode::SERVICE_UNAVAILABLE),
            Err(_) => {
                tracing::warn!("Request timeout waiting for queue slot");
                return Err(StatusCode::REQUEST_TIMEOUT);
            }
        };

        // Add to queue
        let entry = QueueEntry {
            request_id: request_id.clone(),
            enqueued_at: Instant::now(),
            priority: 0,
        };
        self.queue.insert(request_id.clone(), entry);
        self.queue_size
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        tracing::debug!(
            request_id = %request_id,
            queue_size = current_size + 1,
            "Request queued"
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
    _permit: tokio::sync::SemaphorePermit<'static>,
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

/// Middleware for request queuing
pub async fn request_queue_middleware(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // This would be integrated with the main app state
    // For now, just pass through
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
