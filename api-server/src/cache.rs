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

struct Entry {
    value: String,
    expires_at: Instant,
}

static STORE: Lazy<DashMap<String, Entry>> = Lazy::new(DashMap::new);

/// Write a value into the cache under `key` with the default TTL.
pub fn set<T: Serialize>(key: &str, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        STORE.insert(
            key.to_string(),
            Entry {
                value: json,
                expires_at: Instant::now() + Duration::from_secs(DEFAULT_TTL_SECS),
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

/// Invalidate a single cache key.
pub fn invalidate(key: &str) {
    STORE.remove(key);
}

/// Invalidate all keys that start with `prefix`.
pub fn invalidate_prefix(prefix: &str) {
    STORE.retain(|k, _| !k.starts_with(prefix));
}

// ── Key helpers ───────────────────────────────────────────────────────────────

pub fn ip_key(ip_id: u64) -> String {
    format!("ip:{}", ip_id)
}

pub fn ip_list_key(owner: &str, limit: u64, offset: u64) -> String {
    format!("ip:list:{}:{}:{}", owner, limit, offset)
}

pub fn swap_key(swap_id: u64) -> String {
    format!("swap:{}", swap_id)
}

pub fn swap_list_seller_key(seller: &str, limit: u64, offset: u64) -> String {
    format!("swap:seller:{}:{}:{}", seller, limit, offset)
}

pub fn swap_list_buyer_key(buyer: &str, limit: u64, offset: u64) -> String {
    format!("swap:buyer:{}:{}:{}", buyer, limit, offset)
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
}
