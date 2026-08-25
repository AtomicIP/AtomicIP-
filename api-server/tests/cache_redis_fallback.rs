#![cfg(feature = "redis-integration-tests")]
//! Integration test for #786: the documented fallback path when Redis is
//! configured but unreachable.
//!
//! Gated behind the `redis-integration-tests` feature; run with:
//! ```sh
//! cargo test --features redis-integration-tests --test cache_redis_fallback
//! ```

use api_server::cache;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Dummy {
    val: u64,
}

/// Kept in its own binary (separate process from
/// `cache_redis_cross_instance`) because `cache`'s Redis pool reads
/// `REDIS_URL` once per process on first access.
#[test]
fn fallback_serves_correct_data_when_redis_unreachable() {
    // Nothing listens on this port, so every Redis attempt fails fast and
    // the cache falls back to in-process memory rather than erroring.
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:1");

    let key = "test:fallback:1";
    let value = Dummy { val: 3 };

    cache::set(key, &value);
    assert_eq!(cache::get::<Dummy>(key), Some(Dummy { val: 3 }));
    assert!(cache::is_degraded());
}
