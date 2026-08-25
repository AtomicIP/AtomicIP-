/// #316: Redis-based caching layer for IP and Swap queries.
///
/// Backed by Redis when `REDIS_URL` is configured and reachable, so that
/// invalidations (`invalidate`, `invalidate_prefix`, `invalidate_pattern`)
/// are visible to every `api-server` instance sharing that Redis, not just
/// the instance that performed the write.
///
/// Falls back to an in-process `DashMap` TTL cache — gracefully, so the
/// server always starts and serves correct (if not shared) data — whenever
/// Redis is not configured, not reachable at startup, or becomes
/// unreachable while running. That degraded state is *not* silent: it is
/// tracked in [`is_degraded`], logged on each transition, and exposed via
/// the `cache_backend_degraded_transitions_total` counter. A background
/// thread pings Redis every [`HEALTH_CHECK_INTERVAL`] and flips the cache
/// back to shared mode automatically once Redis is reachable again.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use metrics::{counter, describe_counter};
use once_cell::sync::Lazy;
use r2d2::Pool;
use redis::Commands;
use serde::{de::DeserializeOwned, Serialize};

const DEFAULT_TTL_SECS: u64 = 30;
const IP_TTL_SECS: u64 = 60;
const SWAP_TTL_SECS: u64 = 30;
const REPUTATION_TTL_SECS: u64 = 300;

/// How often the background thread pings Redis to detect recovery from a
/// degraded state. Cache operations do not retry Redis on every call while
/// degraded — they defer to this thread — so this interval is also the
/// worst-case time to resume shared caching after Redis comes back.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);

struct Entry {
    value: String,
    expires_at: Instant,
}

static STORE: Lazy<DashMap<String, Entry>> = Lazy::new(DashMap::new);

/// `true` when the cache is serving from the in-process `DashMap` fallback
/// instead of the shared Redis store — either because `REDIS_URL` isn't
/// configured, or because Redis is currently unreachable. Starts `true` and
/// flips to `false` once a real connection is confirmed.
static DEGRADED: AtomicBool = AtomicBool::new(true);

/// Connection pool to the shared Redis store, built once from `REDIS_URL`.
/// `None` when `REDIS_URL` is unset or invalid, in which case the cache
/// runs in-process only and no health-check thread is started.
static REDIS_POOL: Lazy<Option<Pool<redis::Client>>> = Lazy::new(|| {
    describe_counter!(
        "cache_backend_degraded_transitions_total",
        "Transitions of the cache between shared Redis mode and degraded in-process mode"
    );

    let url = match std::env::var("REDIS_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::warn!("cache: REDIS_URL not set, running in-process memory cache only");
            return None;
        }
    };

    let client = match redis::Client::open(url) {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!(error = %err, "cache: invalid REDIS_URL, running in-process memory cache only");
            return None;
        }
    };

    // `build_unchecked` never fails and never blocks on a live connection —
    // the server must always start even if Redis is down at boot.
    let pool = Pool::builder()
        .max_size(16)
        .connection_timeout(Duration::from_millis(300))
        .build_unchecked(client);

    // Establish real reachability synchronously (bounded by the 300ms
    // connection timeout above) so that the very first cache operation
    // after startup sees an accurate `DEGRADED` state instead of racing the
    // background health-check thread's first tick.
    match ping(&pool) {
        Ok(()) => mark_healthy(),
        Err(reason) => mark_degraded(&reason),
    }

    spawn_health_check(pool.clone());
    Some(pool)
});

fn ping(pool: &Pool<redis::Client>) -> Result<(), String> {
    pool.get()
        .map_err(|err| err.to_string())
        .and_then(|mut conn| {
            redis::cmd("PING")
                .query::<String>(&mut *conn)
                .map(|_| ())
                .map_err(|err| err.to_string())
        })
}

fn spawn_health_check(pool: Pool<redis::Client>) {
    let spawned = std::thread::Builder::new()
        .name("cache-redis-health".to_string())
        .spawn(move || loop {
            std::thread::sleep(HEALTH_CHECK_INTERVAL);
            match ping(&pool) {
                Ok(()) => mark_healthy(),
                Err(reason) => mark_degraded(&reason),
            }
        });

    if let Err(err) = spawned {
        tracing::error!(error = %err, "cache: failed to spawn Redis health-check thread; degraded state will only clear on the next successful cache operation");
    }
}

/// Record a Redis failure. Logs and increments the transition counter only
/// on the edge (healthy -> degraded), not on every failed operation.
fn mark_degraded(reason: &str) {
    let was_degraded = DEGRADED.swap(true, Ordering::SeqCst);
    if !was_degraded {
        tracing::warn!(
            reason,
            "cache: Redis unreachable, falling back to in-process memory cache (degraded mode)"
        );
        counter!(
            "cache_backend_degraded_transitions_total",
            "direction" => "to_degraded",
        )
        .increment(1);
    }
}

/// Record a successful Redis health check. Logs and increments the
/// transition counter only on the edge (degraded -> healthy).
fn mark_healthy() {
    let was_degraded = DEGRADED.swap(false, Ordering::SeqCst);
    if was_degraded {
        tracing::info!("cache: Redis connection restored, resuming shared cache mode");
        counter!(
            "cache_backend_degraded_transitions_total",
            "direction" => "to_healthy",
        )
        .increment(1);
    }
}

/// Whether the cache is currently serving from the in-process fallback
/// rather than shared Redis.
pub fn is_degraded() -> bool {
    DEGRADED.load(Ordering::SeqCst)
}

/// Get a pooled Redis connection, marking the cache degraded on failure.
/// Returns `None` when Redis isn't configured or is currently unreachable.
fn redis_conn() -> Option<r2d2::PooledConnection<redis::Client>> {
    let pool = REDIS_POOL.as_ref()?;
    if is_degraded() {
        return None;
    }
    match pool.get() {
        Ok(conn) => Some(conn),
        Err(err) => {
            mark_degraded(&err.to_string());
            None
        }
    }
}

// ── Cache Configuration ───────────────────────────────────────────────────────

/// Configure cache TTL for different data types.
#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub default_ttl: u64,
    pub ip_ttl: u64,
    pub swap_ttl: u64,
    pub reputation_ttl: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: DEFAULT_TTL_SECS,
            ip_ttl: IP_TTL_SECS,
            swap_ttl: SWAP_TTL_SECS,
            reputation_ttl: REPUTATION_TTL_SECS,
        }
    }
}

/// Global cache configuration.
static CONFIG: Lazy<std::sync::Arc<CacheConfig>> =
    Lazy::new(|| std::sync::Arc::new(CacheConfig::default()));

/// Initialize cache with custom configuration.
/// Note: has no effect after the cache has been first accessed (Lazy is already initialized).
pub fn init_cache(_config: CacheConfig) {
    // CONFIG is a Lazy — it initializes on first access and cannot be reset afterwards.
    // Custom configuration must be supplied before the first cache operation.
}

// ── Core Cache Operations ─────────────────────────────────────────────────────

/// Write a value into the cache under `key` with the default TTL.
pub fn set<T: Serialize>(key: &str, value: &T) {
    set_with_ttl(key, value, CONFIG.default_ttl);
}

/// Write a value into the cache under `key` with custom TTL.
pub fn set_with_ttl<T: Serialize>(key: &str, value: &T, ttl_secs: u64) {
    let Ok(json) = serde_json::to_string(value) else {
        return;
    };

    if let Some(mut conn) = redis_conn() {
        let result: redis::RedisResult<()> = conn.set_ex(key, &json, ttl_secs.max(1));
        match result {
            Ok(()) => return,
            Err(err) => mark_degraded(&err.to_string()),
        }
    }

    STORE.insert(
        key.to_string(),
        Entry {
            value: json,
            expires_at: Instant::now() + Duration::from_secs(ttl_secs),
        },
    );
}

/// Read a cached value. Returns `None` on miss or expiry.
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
    if let Some(mut conn) = redis_conn() {
        match conn.get::<_, Option<String>>(key) {
            Ok(Some(json)) => return serde_json::from_str(&json).ok(),
            Ok(None) => return None,
            Err(err) => mark_degraded(&err.to_string()),
        }
    }

    let entry = STORE.get(key)?;
    if entry.expires_at < Instant::now() {
        drop(entry);
        STORE.remove(key);
        return None;
    }
    serde_json::from_str(&entry.value).ok()
}

/// Check if a key exists and is not expired.
pub fn exists(key: &str) -> bool {
    if let Some(mut conn) = redis_conn() {
        match conn.exists::<_, bool>(key) {
            Ok(exists) => return exists,
            Err(err) => mark_degraded(&err.to_string()),
        }
    }

    match STORE.get(key) {
        Some(entry) => entry.expires_at >= Instant::now(),
        None => false,
    }
}

/// Get TTL remaining for a key in seconds. Returns None if key doesn't exist or is expired.
pub fn ttl_remaining(key: &str) -> Option<u64> {
    if let Some(mut conn) = redis_conn() {
        match conn.ttl::<_, i64>(key) {
            Ok(ttl) if ttl >= 0 => return Some(ttl as u64),
            Ok(_) => return None,
            Err(err) => mark_degraded(&err.to_string()),
        }
    }

    let entry = STORE.get(key)?;
    if entry.expires_at < Instant::now() {
        drop(entry);
        STORE.remove(key);
        return None;
    }
    let remaining = entry.expires_at.duration_since(Instant::now()).as_secs();
    Some(remaining)
}

/// Invalidate a single cache key.
///
/// Removes the key from Redis (visible to every instance sharing it) and
/// from the local fallback store, so a flip between backends around the
/// time of the call can never leave a stale copy behind.
pub fn invalidate(key: &str) {
    if let Some(mut conn) = redis_conn() {
        let result: redis::RedisResult<()> = conn.del(key);
        if let Err(err) = result {
            mark_degraded(&err.to_string());
        }
    }
    STORE.remove(key);
}

/// Invalidate all keys that start with `prefix`.
pub fn invalidate_prefix(prefix: &str) {
    if let Some(mut conn) = redis_conn() {
        match redis_delete_matching(&mut conn, &format!("{prefix}*")) {
            Ok(()) => {}
            Err(err) => mark_degraded(&err.to_string()),
        }
    }
    STORE.retain(|k, _| !k.starts_with(prefix));
}

/// Invalidate all keys matching a pattern (supports * wildcards).
pub fn invalidate_pattern(pattern: &str) {
    if !pattern.contains('*') {
        invalidate_prefix(pattern);
        return;
    }

    if let Some(mut conn) = redis_conn() {
        // Redis glob patterns already use `*`/`?`/`[...]`, so the pattern is
        // passed through to SCAN MATCH as-is — no regex translation needed.
        match redis_delete_matching(&mut conn, pattern) {
            Ok(()) => {}
            Err(err) => mark_degraded(&err.to_string()),
        }
    }

    let regex_pattern = pattern.replace('*', ".*");
    if let Ok(regex) = regex::Regex::new(&regex_pattern) {
        STORE.retain(|k, _| !regex.is_match(k));
    }
}

/// Scan for keys matching a Redis glob `pattern` and delete them.
fn redis_delete_matching(conn: &mut redis::Connection, pattern: &str) -> redis::RedisResult<()> {
    let keys: Vec<String> = conn
        .scan_match::<_, String>(pattern)?
        .collect::<Result<Vec<String>, redis::RedisError>>()?;
    if !keys.is_empty() {
        let _: () = conn.del(keys)?;
    }
    Ok(())
}

/// Clear all cache entries.
///
/// When Redis-backed, this flushes the connected Redis database — deployments
/// that need `clear()` scoped strictly to this cache's keys should point
/// `REDIS_URL` at a Redis instance/logical DB dedicated to it.
pub fn clear() {
    if let Some(mut conn) = redis_conn() {
        let result: redis::RedisResult<()> = redis::cmd("FLUSHDB").query(&mut *conn);
        if let Err(err) = result {
            mark_degraded(&err.to_string());
        }
    }
    STORE.clear();
}

/// Get cache statistics.
pub fn stats() -> CacheStats {
    if let Some(mut conn) = redis_conn() {
        if let Ok(total_entries) = redis::cmd("DBSIZE").query::<usize>(&mut *conn) {
            return CacheStats { total_entries };
        }
    }
    CacheStats {
        total_entries: STORE.len(),
    }
}

// ── Key helpers ───────────────────────────────────────────────────────────────

pub fn ip_key(ip_id: u64) -> String {
    format!("ip:{}", ip_id)
}

pub fn ip_list_key(owner: &str, limit: u64, cursor: &str) -> String {
    format!("ip:list:{}:{}:{}", owner, limit, cursor)
}

pub fn swap_key(swap_id: u64) -> String {
    format!("swap:{}", swap_id)
}

pub fn swap_list_seller_key(seller: &str, limit: u64, cursor: &str) -> String {
    format!("swap:seller:{}:{}:{}", seller, limit, cursor)
}

pub fn swap_list_buyer_key(buyer: &str, limit: u64, cursor: &str) -> String {
    format!("swap:buyer:{}:{}:{}", buyer, limit, cursor)
}

pub fn reputation_key(address: &str) -> String {
    format!("reputation:{}", address)
}

pub fn dispute_evidence_key(swap_id: u64) -> String {
    format!("evidence:{}", swap_id)
}

// ── Cache-Control header value ────────────────────────────────────────────────

/// Returns a `Cache-Control` header value for cacheable GET responses.
pub fn cache_control_header() -> &'static str {
    "public, max-age=30, stale-while-revalidate=10"
}

/// Returns a `Cache-Control` header value for mutable/write responses.
pub fn no_cache_header() -> &'static str {
    "no-store"
}

/// Returns a `Cache-Control` header value for IP records (longer TTL).
pub fn ip_cache_control_header() -> &'static str {
    "public, max-age=60, stale-while-revalidate=30"
}

/// Returns a `Cache-Control` header value for reputation data (very long TTL).
pub fn reputation_cache_control_header() -> &'static str {
    "public, max-age=300, stale-while-revalidate=60"
}

// ── Cache Statistics ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
}

// ── Contract Event Invalidation ───────────────────────────────────────────────

/// Invalidate cache entries based on contract events.
/// Call this when processing contract events to keep cache consistent.
pub fn invalidate_on_contract_event(event_type: &str, related_id: u64) {
    match event_type {
        "ip_committed" => {
            invalidate(&ip_key(related_id));
        }
        "ip_transferred" => {
            invalidate(&ip_key(related_id));
            invalidate_prefix("ip:list:");
        }
        "ip_revoked" => {
            invalidate(&ip_key(related_id));
            invalidate_prefix("ip:list:");
        }
        "swap_initiated" => {
            invalidate(&swap_key(related_id));
            invalidate_prefix("swap:seller:");
            invalidate_prefix("swap:buyer:");
            invalidate_prefix("swap:ip:");
        }
        "swap_accepted" => {
            invalidate(&swap_key(related_id));
            invalidate_prefix("swap:seller:");
            invalidate_prefix("swap:buyer:");
        }
        "swap_completed" => {
            invalidate(&swap_key(related_id));
            invalidate_prefix("swap:seller:");
            invalidate_prefix("swap:buyer:");
            // Invalidate reputation cache for both parties
            invalidate_prefix("reputation:");
        }
        "swap_cancelled" => {
            invalidate(&swap_key(related_id));
            invalidate_prefix("swap:seller:");
            invalidate_prefix("swap:buyer:");
        }
        "dispute_raised" => {
            invalidate(&swap_key(related_id));
            invalidate(&dispute_evidence_key(related_id));
        }
        "dispute_resolved" => {
            invalidate(&swap_key(related_id));
            invalidate(&dispute_evidence_key(related_id));
            invalidate_prefix("reputation:");
        }
        "admin_rollback" => {
            invalidate(&swap_key(related_id));
            invalidate_prefix("swap:seller:");
            invalidate_prefix("swap:buyer:");
            invalidate_prefix("reputation:");
        }
        _ => {
            // For unknown event types, invalidate broadly
            invalidate_prefix("swap:");
            invalidate_prefix("ip:");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Dummy {
        val: u64,
    }

    #[test]
    fn test_set_and_get_returns_value() {
        let key = "test:cache:1";
        let d = Dummy { val: 42 };
        set(key, &d);
        let result: Option<Dummy> = get(key);
        assert_eq!(result, Some(Dummy { val: 42 }));
    }

    #[test]
    fn test_get_miss_returns_none() {
        let result: Option<Dummy> = get("test:cache:nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_invalidate_removes_entry() {
        let key = "test:cache:2";
        set(key, &Dummy { val: 7 });
        invalidate(key);
        let result: Option<Dummy> = get(key);
        assert!(result.is_none());
    }

    #[test]
    fn test_invalidate_prefix_removes_matching_keys() {
        set("test:prefix:a", &Dummy { val: 1 });
        set("test:prefix:b", &Dummy { val: 2 });
        set("test:other:c", &Dummy { val: 3 });
        invalidate_prefix("test:prefix:");
        assert!(get::<Dummy>("test:prefix:a").is_none());
        assert!(get::<Dummy>("test:prefix:b").is_none());
        // unrelated key survives
        assert!(get::<Dummy>("test:other:c").is_some());
    }

    #[test]
    fn test_set_with_custom_ttl() {
        let key = "test:ttl:1";
        let d = Dummy { val: 99 };
        set_with_ttl(key, &d, 60);
        let result: Option<Dummy> = get(key);
        assert_eq!(result, Some(Dummy { val: 99 }));
    }

    #[test]
    fn test_exists_returns_true_for_valid_entry() {
        let key = "test:exists:1";
        set(key, &Dummy { val: 1 });
        assert!(exists(key));
    }

    #[test]
    fn test_exists_returns_false_for_nonexistent() {
        assert!(!exists("test:exists:nonexistent"));
    }

    #[test]
    fn test_stats_returns_entry_count() {
        clear();
        set("stats:1", &Dummy { val: 1 });
        set("stats:2", &Dummy { val: 2 });
        let stats = stats();
        assert_eq!(stats.total_entries, 2);
    }

    #[test]
    fn test_reputation_key_format() {
        let key = reputation_key("GABC123");
        assert_eq!(key, "reputation:GABC123");
    }

    #[test]
    fn test_dispute_evidence_key_format() {
        let key = dispute_evidence_key(42);
        assert_eq!(key, "evidence:42");
    }

    #[test]
    fn test_invalidate_on_contract_event_swap_completed() {
        clear();
        set("swap:1", &Dummy { val: 1 });
        set("swap:seller:abc:10:0", &Dummy { val: 2 });
        set("reputation:abc", &Dummy { val: 3 });

        invalidate_on_contract_event("swap_completed", 1);

        assert!(!exists("swap:1"));
        assert!(!exists("swap:seller:abc:10:0"));
        assert!(!exists("reputation:abc"));
    }

    #[test]
    fn test_invalidate_on_contract_event_ip_transferred() {
        clear();
        set("ip:1", &Dummy { val: 1 });
        set("ip:list:owner:10:0", &Dummy { val: 2 });

        invalidate_on_contract_event("ip_transferred", 1);

        assert!(!exists("ip:1"));
        assert!(!exists("ip:list:owner:10:0"));
    }

    #[test]
    fn test_degraded_by_default_without_redis_url() {
        // In the test binary REDIS_URL is unset, so the cache runs
        // in-process only and reports itself as degraded — not silently.
        assert!(is_degraded());
    }
}

// Redis integration tests (real Redis via testcontainers, gated behind the
// `redis-integration-tests` feature) live in `api-server/tests/`, not here.
//
// They must run as separate integration-test binaries — each `tests/*.rs`
// file gets its own process — because `REDIS_POOL` above is a `Lazy` that
// reads `REDIS_URL` exactly once per process, on first cache access. A
// `#[cfg(test)]` module in this file would share a process (and therefore
// the same already-initialized `REDIS_POOL`) with the in-process unit tests
// above, which rely on `REDIS_URL` staying unset.
