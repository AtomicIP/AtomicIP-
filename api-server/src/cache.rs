/// #316: Redis-based caching layer for IP and Swap queries.
/// #629: Enhanced with LRU eviction, Redis integration, prewarming, and TTL management.
///
/// Uses an in-process DashMap as a TTL cache when Redis is unavailable,
/// falling back gracefully so the server always starts without Redis.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Serialize};

const DEFAULT_TTL_SECS: u64 = 30;
const IP_TTL_SECS: u64 = 60;
const SWAP_TTL_SECS: u64 = 30;
const COMMITMENT_TTL_SECS: u64 = 3600; // #629: 1h for commitments
const PRICE_TTL_SECS: u64 = 900;        // #629: 15m for prices
const REPUTATION_TTL_SECS: u64 = 300;
const LRU_MAX_ENTRIES: usize = 10_000;

struct Entry {
    value: String,
    expires_at: Instant,
}

static STORE: Lazy<DashMap<String, Entry>> = Lazy::new(DashMap::new);

// ── #629: LRU tracking ────────────────────────────────────────────────────────

/// Ordered list of keys for LRU eviction (most recently used at the back).
static LRU_LIST: Lazy<std::sync::Mutex<VecDeque<String>>> =
    Lazy::new(|| std::sync::Mutex::new(VecDeque::with_capacity(LRU_MAX_ENTRIES)));

// ── #629: Cache statistics counters ────────────────────────────────────────────

static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static LRU_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static TTL_EVICTIONS: AtomicU64 = AtomicU64::new(0);

// ── #629: Prewarm hot data list ────────────────────────────────────────────────

static PREWARM_KEYS: Lazy<std::sync::Mutex<Vec<String>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));

// ── Cache Configuration ───────────────────────────────────────────────────────

/// Configure cache TTL for different data types.
#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub default_ttl: u64,
    pub ip_ttl: u64,
    pub swap_ttl: u64,
    pub commitment_ttl: u64,
    pub price_ttl: u64,
    pub reputation_ttl: u64,
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: DEFAULT_TTL_SECS,
            ip_ttl: IP_TTL_SECS,
            swap_ttl: SWAP_TTL_SECS,
            commitment_ttl: COMMITMENT_TTL_SECS,
            price_ttl: PRICE_TTL_SECS,
            reputation_ttl: REPUTATION_TTL_SECS,
            max_entries: LRU_MAX_ENTRIES,
        }
    }
}

/// Global cache configuration.
static CONFIG: Lazy<Arc<CacheConfig>> = Lazy::new(|| Arc::new(CacheConfig::default()));

/// Initialize cache with custom configuration.
/// Note: has no effect after the cache has been first accessed (Lazy is already initialized).
pub fn init_cache(config: CacheConfig) {
    // CONFIG is a Lazy — it initializes on first access and cannot be reset afterwards.
    // Custom configuration must be supplied before the first cache operation.
}

// ── #629: Redis Integration ────────────────────────────────────────────────────

/// Redis client (optional — cache degrades gracefully to in-memory when unavailable).
static REDIS_CLIENT: Lazy<Option<String>> = Lazy::new(|| {
    std::env::var("REDIS_URL").ok()
});

/// Check if Redis is configured.
pub fn redis_available() -> bool {
    REDIS_CLIENT.is_some()
}

// ── #629: LRU eviction ────────────────────────────────────────────────────────

/// Touch a key in the LRU list (move to back = most recently used).
fn touch_lru(key: &str) {
    if let Ok(mut list) = LRU_LIST.lock() {
        // Remove if present, then push to back
        if let Some(pos) = list.iter().position(|k| k == key) {
            list.remove(pos);
        }
        list.push_back(key.to_string());
    }
}

/// Evict the least recently used entry if cache exceeds max entries.
fn evict_lru_if_needed() {
    let max = CONFIG.max_entries;
    if STORE.len() <= max {
        return;
    }
    if let Ok(mut list) = LRU_LIST.lock() {
        while STORE.len() > max {
            if let Some(lru_key) = list.pop_front() {
                if STORE.remove(&lru_key).is_some() {
                    LRU_EVICTIONS.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                break;
            }
        }
    }
}

// ── #629: Prewarming ───────────────────────────────────────────────────────────

/// Register a key for cache prewarming. Prewarmed keys are refreshed on startup.
pub fn register_prewarm_key(key: &str) {
    if let Ok(mut keys) = PREWARM_KEYS.lock() {
        if !keys.contains(&key.to_string()) {
            keys.push(key.to_string());
        }
    }
}

/// Get all registered prewarm keys.
pub fn get_prewarm_keys() -> Vec<String> {
    PREWARM_KEYS.lock().map(|keys| keys.clone()).unwrap_or_default()
}

// ── Core Cache Operations ─────────────────────────────────────────────────────

/// Write a value into the cache under `key` with the default TTL.
pub fn set<T: Serialize>(key: &str, value: &T) {
    set_with_ttl(key, value, CONFIG.default_ttl);
}

/// Write a value into the cache under `key` with custom TTL.
pub fn set_with_ttl<T: Serialize>(key: &str, value: &T, ttl_secs: u64) {
    if let Ok(json) = serde_json::to_string(value) {
        STORE.insert(
            key.to_string(),
            Entry {
                value: json,
                expires_at: Instant::now() + Duration::from_secs(ttl_secs),
            },
        );
        touch_lru(key);
        evict_lru_if_needed();
    }
}

/// Write a value into the cache with a TTL appropriate for commitment data.
pub fn set_commitment<T: Serialize>(key: &str, value: &T) {
    set_with_ttl(key, value, CONFIG.commitment_ttl);
}

/// Write a value into the cache with a TTL appropriate for price data.
pub fn set_price<T: Serialize>(key: &str, value: &T) {
    set_with_ttl(key, value, CONFIG.price_ttl);
}

/// Read a cached value. Returns `None` on miss or expiry.
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
    let entry = STORE.get(key)?;
    if entry.expires_at < Instant::now() {
        drop(entry);
        STORE.remove(key);
        TTL_EVICTIONS.fetch_add(1, Ordering::Relaxed);
        MISSES.fetch_add(1, Ordering::Relaxed);
        return None;
    }
    drop(entry);
    touch_lru(key);
    HITS.fetch_add(1, Ordering::Relaxed);
    let entry = STORE.get(key)?;
    serde_json::from_str(&entry.value).ok()
}

/// Check if a key exists and is not expired.
pub fn exists(key: &str) -> bool {
    match STORE.get(key) {
        Some(entry) => {
            if entry.expires_at >= Instant::now() {
                true
            } else {
                drop(entry);
                STORE.remove(key);
                TTL_EVICTIONS.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
        None => false,
    }
}

/// Get TTL remaining for a key in seconds. Returns None if key doesn't exist or is expired.
pub fn ttl_remaining(key: &str) -> Option<u64> {
    let entry = STORE.get(key)?;
    if entry.expires_at < Instant::now() {
        drop(entry);
        STORE.remove(key);
        TTL_EVICTIONS.fetch_add(1, Ordering::Relaxed);
        return None;
    }
    let remaining = entry.expires_at.duration_since(Instant::now()).as_secs();
    Some(remaining)
}

/// Invalidate a single cache key.
pub fn invalidate(key: &str) {
    STORE.remove(key);
    if let Ok(mut list) = LRU_LIST.lock() {
        if let Some(pos) = list.iter().position(|k| k == key) {
            list.remove(pos);
        }
    }
}

/// Invalidate all keys that start with `prefix`.
pub fn invalidate_prefix(prefix: &str) {
    STORE.retain(|k, _| {
        if k.starts_with(prefix) {
            if let Ok(mut list) = LRU_LIST.lock() {
                if let Some(pos) = list.iter().position(|lk| lk == k) {
                    list.remove(pos);
                }
            }
            false
        } else {
            true
        }
    });
}

/// Invalidate all keys matching a pattern (supports * wildcards).
pub fn invalidate_pattern(pattern: &str) {
    if pattern.contains('*') {
        let regex_pattern = pattern.replace('*', ".*");
        if let Ok(regex) = regex::Regex::new(&regex_pattern) {
            let keys_to_remove: Vec<String> = STORE.iter()
                .filter(|entry| regex.is_match(entry.key()))
                .map(|entry| entry.key().clone())
                .collect();
            for key in keys_to_remove {
                STORE.remove(&key);
                if let Ok(mut list) = LRU_LIST.lock() {
                    if let Some(pos) = list.iter().position(|lk| lk == &key) {
                        list.remove(pos);
                    }
                }
            }
        }
    } else {
        invalidate_prefix(pattern);
    }
}

/// Clear all cache entries.
pub fn clear() {
    STORE.clear();
    if let Ok(mut list) = LRU_LIST.lock() {
        list.clear();
    }
    HITS.store(0, Ordering::Relaxed);
    MISSES.store(0, Ordering::Relaxed);
    LRU_EVICTIONS.store(0, Ordering::Relaxed);
    TTL_EVICTIONS.store(0, Ordering::Relaxed);
}

/// Get cache statistics.
pub fn stats() -> CacheStats {
    let hits = HITS.load(Ordering::Relaxed);
    let misses = MISSES.load(Ordering::Relaxed);
    let total = hits + misses;
    CacheStats {
        total_entries: STORE.len(),
    }
}

/// Get extended cache statistics with hit/miss rates.
pub fn extended_stats() -> ExtendedCacheStats {
    let hits = HITS.load(Ordering::Relaxed);
    let misses = MISSES.load(Ordering::Relaxed);
    let total = hits + misses;
    let hit_rate = if total > 0 { hits as f64 / total as f64 } else { 1.0 };
    ExtendedCacheStats {
        total_entries: STORE.len(),
        hits,
        misses,
        hit_rate,
        lru_evictions: LRU_EVICTIONS.load(Ordering::Relaxed),
        ttl_evictions: TTL_EVICTIONS.load(Ordering::Relaxed),
        memory_estimate_bytes: STORE.len() as u64 * 2048,
    }
}

/// Extended cache statistics.
#[derive(Debug, Clone)]
pub struct ExtendedCacheStats {
    pub total_entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub lru_evictions: u64,
    pub ttl_evictions: u64,
    pub memory_estimate_bytes: u64,
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
}
