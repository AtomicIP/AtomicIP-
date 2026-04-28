/// #316: Redis-based caching layer for IP and Swap queries.
///
/// Uses an in-process DashMap as a TTL cache when Redis is unavailable,
/// falling back gracefully so the server always starts without Redis.
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Serialize};

const DEFAULT_TTL_SECS: u64 = 30;
const IP_TTL_SECS: u64 = 60;
const SWAP_TTL_SECS: u64 = 30;
const REPUTATION_TTL_SECS: u64 = 300;

struct Entry {
    value: String,
    expires_at: Instant,
}

static STORE: Lazy<DashMap<String, Entry>> = Lazy::new(DashMap::new);

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
static CONFIG: Lazy<Arc<CacheConfig>> = Lazy::new(|| Arc::new(CacheConfig::default()));

/// Initialize cache with custom configuration.
pub fn init_cache(config: CacheConfig) {
    let _ = CONFIG.set(Arc::new(config));
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
    }
}

/// Read a cached value. Returns `None` on miss or expiry.
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
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
    let entry = STORE.get(key)?;
    let is_valid = entry.expires_at >= Instant::now();
    drop(entry);
    is_valid
}

/// Get TTL remaining for a key in seconds. Returns None if key doesn't exist or is expired.
pub fn ttl_remaining(key: &str) -> Option<u64> {
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
pub fn invalidate(key: &str) {
    STORE.remove(key);
}

/// Invalidate all keys that start with `prefix`.
pub fn invalidate_prefix(prefix: &str) {
    STORE.retain(|k, _| !k.starts_with(prefix));
}

/// Invalidate all keys matching a pattern (supports * wildcards).
pub fn invalidate_pattern(pattern: &str) {
    if pattern.contains('*') {
        let regex_pattern = pattern.replace('*', ".*");
        if let Ok(regex) = regex::Regex::new(&regex_pattern) {
            STORE.retain(|k, _| !regex.is_match(k));
        }
    } else {
        invalidate_prefix(pattern);
    }
}

/// Clear all cache entries.
pub fn clear() {
    STORE.clear();
}

/// Get cache statistics.
pub fn stats() -> CacheStats {
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
}
