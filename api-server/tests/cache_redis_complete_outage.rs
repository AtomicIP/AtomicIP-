#![cfg(feature = "redis-integration-tests")]
//! Integration test for #889: the API gracefully handles complete Redis outage
//! (distinct from degraded performance or single-node failure).
//!
//! Gated behind the `redis-integration-tests` feature; run with:
//! ```sh
//! cargo test --features redis-integration-tests --test cache_redis_complete_outage
//! ```

use api_server::cache;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct TestData {
    id: u64,
    value: String,
}

/// When Redis is entirely unreachable (not listening on expected port),
/// the cache layer must fall back gracefully without panicking.
#[test]
fn complete_redis_outage_does_not_panic() {
    // Point to a port that's definitely not running Redis.
    // Using 127.0.0.1:1 ensures fast failure (connection refused).
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:1");

    let key = "test:outage:1";
    let data = TestData {
        id: 42,
        value: "test_value".to_string(),
    };

    // Even with Redis down, set() must not panic.
    cache::set(key, &data);

    // Verify fallback layer serves the data (in-memory).
    let result = cache::get::<TestData>(key);
    assert_eq!(result, Some(data), "Fallback must return set value");
}

/// Read operations must work when Redis is completely offline.
#[test]
fn read_operations_work_during_redis_outage() {
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:1");

    let keys = vec!["test:outage:read:1", "test:outage:read:2"];
    let values: Vec<TestData> = vec![
        TestData {
            id: 1,
            value: "first".to_string(),
        },
        TestData {
            id: 2,
            value: "second".to_string(),
        },
    ];

    for (key, value) in keys.iter().zip(values.iter()) {
        cache::set(key, value);
    }

    for (key, expected) in keys.iter().zip(values.iter()) {
        let result = cache::get::<TestData>(key);
        assert_eq!(
            result, Some(expected.clone()),
            "Read must succeed with degraded cache"
        );
    }
}

/// Degraded mode flag must be set when Redis is unreachable.
#[test]
fn degraded_mode_is_signaled_during_outage() {
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:1");

    // Trigger cache access to detect Redis unavailability.
    let key = "test:outage:degrade";
    let data = TestData {
        id: 99,
        value: "signal_test".to_string(),
    };
    cache::set(key, &data);

    // When Redis is unreachable, the cache should signal degraded mode.
    assert!(
        cache::is_degraded(),
        "Cache must indicate degraded mode during Redis outage"
    );
}

/// Multiple concurrent reads during outage must not deadlock.
#[test]
fn concurrent_reads_during_outage_do_not_deadlock() {
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:1");

    let key = "test:outage:concurrent";
    let data = TestData {
        id: 77,
        value: "concurrent_test".to_string(),
    };
    cache::set(key, &data);

    // Simulate concurrent reads by performing multiple sequential reads.
    // (In a real scenario, these would be concurrent; this test at least
    // ensures no panics or deadlocks during repeated access.)
    for _ in 0..5 {
        let result = cache::get::<TestData>(key);
        assert_eq!(
            result, Some(data.clone()),
            "Concurrent-style reads must all succeed"
        );
    }
}
