#![cfg(feature = "redis-integration-tests")]
//! Integration test for #786: cross-instance cache invalidation via Redis.
//!
//! Requires Docker and is gated behind the `redis-integration-tests` feature
//! so a plain `cargo test` never needs a Docker daemon:
//!
//! ```sh
//! cargo test --features redis-integration-tests --test cache_redis_cross_instance
//! ```

use api_server::cache;
use serde::{Deserialize, Serialize};
use testcontainers::runners::SyncRunner;
use testcontainers_modules::redis::Redis;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Dummy {
    val: u64,
}

/// `cache`'s Redis pool is a `Lazy` read from `REDIS_URL` on first access, so
/// the env var must be set before any `cache::` call in this process — this
/// integration binary contains only Redis-backed tests, so that's safe here.
#[test]
fn cross_instance_invalidation_is_visible_immediately() {
    let container = Redis::default()
        .start()
        .expect("failed to start Redis container");
    let host = container.get_host().expect("failed to get container host");
    let port = container
        .get_host_port_ipv4(6379)
        .expect("failed to get mapped Redis port");
    std::env::set_var("REDIS_URL", format!("redis://{host}:{port}"));

    let key = "test:cross_instance:1";
    let value = Dummy { val: 7 };

    // "Instance A" writes the value.
    cache::set(key, &value);
    assert!(
        !cache::is_degraded(),
        "expected the cache to be Redis-backed against a live container"
    );
    assert_eq!(cache::get::<Dummy>(key), Some(Dummy { val: 7 }));

    // "Instance B" invalidates it. Because both "instances" share the same
    // Redis, this issues a real DEL against the shared store rather than a
    // local-only removal, so the invalidation is visible everywhere.
    cache::invalidate(key);

    // "Instance A" must no longer observe the stale value.
    assert_eq!(cache::get::<Dummy>(key), None);
}
