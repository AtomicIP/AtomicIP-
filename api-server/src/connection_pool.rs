/// Database Connection Pool Management
///
/// Provides comprehensive connection pooling with metrics tracking for the API server.
/// Monitors active/idle connections and waiting requests to ensure optimal resource utilization.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use metrics::{counter, gauge, describe_counter, describe_gauge};
use once_cell::sync::Lazy;
use r2d2::Pool;
use redis::Client;

/// Pool metrics tracked for monitoring and observability
#[derive(Clone, Debug)]
pub struct PoolMetrics {
    /// Number of connections currently in use
    active_connections: Arc<AtomicU64>,
    /// Number of idle connections available in the pool
    idle_connections: Arc<AtomicU64>,
    /// Number of requests waiting for a connection
    waiting_requests: Arc<AtomicU64>,
    /// Total connections created since startup
    total_created: Arc<AtomicU64>,
    /// Total connections reused since startup
    total_reused: Arc<AtomicU64>,
    /// Total timeout errors
    timeout_errors: Arc<AtomicU64>,
}

impl PoolMetrics {
    fn new() -> Self {
        describe_gauge!(
            "db_pool_active_connections",
            "Number of connections currently in use"
        );
        describe_gauge!(
            "db_pool_idle_connections",
            "Number of idle connections available in the pool"
        );
        describe_gauge!(
            "db_pool_waiting_requests",
            "Number of requests waiting for a connection"
        );
        describe_counter!(
            "db_pool_connections_created_total",
            "Total number of connections created"
        );
        describe_counter!(
            "db_pool_connections_reused_total",
            "Total number of connections reused"
        );
        describe_counter!(
            "db_pool_timeout_errors_total",
            "Total number of connection timeout errors"
        );

        Self {
            active_connections: Arc::new(AtomicU64::new(0)),
            idle_connections: Arc::new(AtomicU64::new(0)),
            waiting_requests: Arc::new(AtomicU64::new(0)),
            total_created: Arc::new(AtomicU64::new(0)),
            total_reused: Arc::new(AtomicU64::new(0)),
            timeout_errors: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a new connection creation
    pub fn record_connection_created(&self) {
        self.total_created.fetch_add(1, Ordering::Relaxed);
        counter!("db_pool_connections_created_total").increment(1);
        self.update_metrics();
    }

    /// Record a connection reuse
    pub fn record_connection_reused(&self) {
        self.total_reused.fetch_add(1, Ordering::Relaxed);
        counter!("db_pool_connections_reused_total").increment(1);
        self.update_metrics();
    }

    /// Record an active connection
    pub fn set_active_connections(&self, count: u64) {
        self.active_connections.store(count, Ordering::Relaxed);
        self.update_metrics();
    }

    /// Record idle connections
    pub fn set_idle_connections(&self, count: u64) {
        self.idle_connections.store(count, Ordering::Relaxed);
        self.update_metrics();
    }

    /// Record waiting requests
    pub fn set_waiting_requests(&self, count: u64) {
        self.waiting_requests.store(count, Ordering::Relaxed);
        self.update_metrics();
    }

    /// Record a timeout error
    pub fn record_timeout_error(&self) {
        self.timeout_errors.fetch_add(1, Ordering::Relaxed);
        counter!("db_pool_timeout_errors_total").increment(1);
        self.update_metrics();
    }

    /// Update gauge metrics
    fn update_metrics(&self) {
        gauge!("db_pool_active_connections")
            .set(self.active_connections.load(Ordering::Relaxed) as f64);
        gauge!("db_pool_idle_connections")
            .set(self.idle_connections.load(Ordering::Relaxed) as f64);
        gauge!("db_pool_waiting_requests")
            .set(self.waiting_requests.load(Ordering::Relaxed) as f64);
    }

    /// Get snapshot of current metrics
    pub fn snapshot(&self) -> PoolMetricsSnapshot {
        PoolMetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
            waiting_requests: self.waiting_requests.load(Ordering::Relaxed),
            total_created: self.total_created.load(Ordering::Relaxed),
            total_reused: self.total_reused.load(Ordering::Relaxed),
            timeout_errors: self.timeout_errors.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of pool metrics at a point in time
#[derive(Clone, Debug)]
pub struct PoolMetricsSnapshot {
    pub active_connections: u64,
    pub idle_connections: u64,
    pub waiting_requests: u64,
    pub total_created: u64,
    pub total_reused: u64,
    pub timeout_errors: u64,
}

/// Connection pool configuration
#[derive(Clone, Debug)]
pub struct PoolConfig {
    /// Minimum number of connections
    pub min_size: u32,
    /// Maximum number of connections
    pub max_size: u32,
    /// Timeout for acquiring a connection
    pub connection_timeout: Duration,
    /// Timeout for idle connections
    pub idle_timeout: Duration,
    /// Maximum lifetime of a connection
    pub max_lifetime: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 5,
            max_size: 20,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(1800),
        }
    }
}

/// Global connection pool metrics
static METRICS: Lazy<PoolMetrics> = Lazy::new(PoolMetrics::new);

/// Initialize and return the connection pool metrics
pub fn get_metrics() -> PoolMetrics {
    METRICS.clone()
}

/// Get current pool metrics snapshot
pub fn get_metrics_snapshot() -> PoolMetricsSnapshot {
    METRICS.snapshot()
}

/// Create a configured Redis connection pool
pub fn create_redis_pool(
    redis_url: &str,
    config: PoolConfig,
) -> Result<Pool<Client>, Box<dyn std::error::Error>> {
    let client = Client::open(redis_url)?;

    let metrics = get_metrics();
    metrics.record_connection_created();

    let pool = Pool::builder()
        .min_size(config.min_size)
        .max_size(config.max_size)
        .connection_timeout(config.connection_timeout)
        .build(client)?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_metrics_creation() {
        let metrics = PoolMetrics::new();
        assert_eq!(metrics.total_created.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.total_reused.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_pool_metrics_record_creation() {
        let metrics = PoolMetrics::new();
        metrics.record_connection_created();
        assert_eq!(metrics.total_created.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_pool_metrics_record_reuse() {
        let metrics = PoolMetrics::new();
        metrics.record_connection_reused();
        assert_eq!(metrics.total_reused.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_pool_metrics_active_connections() {
        let metrics = PoolMetrics::new();
        metrics.set_active_connections(5);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_pool_metrics_idle_connections() {
        let metrics = PoolMetrics::new();
        metrics.set_idle_connections(10);
        assert_eq!(metrics.idle_connections.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn test_pool_metrics_waiting_requests() {
        let metrics = PoolMetrics::new();
        metrics.set_waiting_requests(3);
        assert_eq!(metrics.waiting_requests.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_pool_metrics_timeout_errors() {
        let metrics = PoolMetrics::new();
        metrics.record_timeout_error();
        assert_eq!(metrics.timeout_errors.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_pool_metrics_snapshot() {
        let metrics = PoolMetrics::new();
        metrics.record_connection_created();
        metrics.record_connection_reused();
        metrics.set_active_connections(5);
        metrics.set_idle_connections(10);
        metrics.set_waiting_requests(2);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_created, 1);
        assert_eq!(snapshot.total_reused, 1);
        assert_eq!(snapshot.active_connections, 5);
        assert_eq!(snapshot.idle_connections, 10);
        assert_eq!(snapshot.waiting_requests, 2);
    }

    #[test]
    fn test_pool_config_defaults() {
        let config = PoolConfig::default();
        assert_eq!(config.min_size, 5);
        assert_eq!(config.max_size, 20);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_pool_config_custom() {
        let config = PoolConfig {
            min_size: 3,
            max_size: 15,
            connection_timeout: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(3600),
        };
        assert_eq!(config.min_size, 3);
        assert_eq!(config.max_size, 15);
        assert_eq!(config.connection_timeout, Duration::from_secs(60));
    }
}
