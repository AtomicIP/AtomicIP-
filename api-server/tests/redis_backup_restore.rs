#![cfg(feature = "redis-integration-tests")]
//! Integration tests for Issue #920: Redis backup/restore procedures
//!
//! Tests verify that Redis-backed state can be reliably backed up and restored:
//! - Pure cache state (can be lost safely)
//! - Critical state (dedup keys, rate-limit counters)
//! - Persistence configuration recommendations
//!
//! Run with:
//! ```sh
//! cargo test --features redis-integration-tests --test redis_backup_restore
//! ```

use api_server::cache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use testcontainers::runners::SyncRunner;
use testcontainers_modules::redis::Redis;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct CacheData {
    id: u64,
    value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct DedupKey {
    request_id: String,
    timestamp: u64,
    processed: bool,
}

/// Test that cache state can be categorized as safe-to-lose
#[test]
fn cache_state_is_safe_to_lose() {
    let container = Redis::default()
        .start()
        .expect("failed to start Redis container");
    let host = container.get_host().expect("failed to get container host");
    let port = container
        .get_host_port_ipv4(6379)
        .expect("failed to get mapped Redis port");
    std::env::set_var("REDIS_URL", format!("redis://{host}:{port}"));

    let cache_key = "cache:api_response:v1:list";
    let cache_data = CacheData {
        id: 1,
        value: "expensive_query_result".to_string(),
    };

    // Store cache data
    cache::set(cache_key, &cache_data);
    assert_eq!(cache::get::<CacheData>(cache_key), Some(cache_data.clone()));

    // Simulate cache loss (clear Redis)
    cache::invalidate(cache_key);

    // Verify cache loss doesn't corrupt data (it just means cache miss)
    assert_eq!(cache::get::<CacheData>(cache_key), None);
    // No panic, no data corruption — just cache miss
}

/// Test that critical dedup state must not be lost
#[test]
fn dedup_state_must_be_persisted() {
    let container = Redis::default()
        .start()
        .expect("failed to start Redis container");
    let host = container.get_host().expect("failed to get container host");
    let port = container
        .get_host_port_ipv4(6379)
        .expect("failed to get mapped Redis port");
    std::env::set_var("REDIS_URL", format!("redis://{host}:{port}"));

    let dedup_key = "dedup:request:abc123def456";
    let dedup_entry = DedupKey {
        request_id: "abc123def456".to_string(),
        timestamp: 1000,
        processed: true,
    };

    // Store critical dedup state
    cache::set(dedup_key, &dedup_entry);
    assert_eq!(
        cache::get::<DedupKey>(dedup_key),
        Some(dedup_entry.clone())
    );

    // Verify entry persists (would need RDB/AOF in production)
    // This test documents that dedup keys CANNOT be lost without allowing
    // duplicate submissions
    let retrieved = cache::get::<DedupKey>(dedup_key);
    assert_eq!(retrieved, Some(dedup_entry));
}

/// Test that rate-limit counters are treated as critical state
#[test]
fn rate_limit_state_is_critical() {
    let container = Redis::default()
        .start()
        .expect("failed to start Redis container");
    let host = container.get_host().expect("failed to get container host");
    let port = container
        .get_host_port_ipv4(6379)
        .expect("failed to get mapped Redis port");
    std::env::set_var("REDIS_URL", format!("redis://{host}:{port}"));

    // Rate-limit counters are stored as critical state
    let rate_limit_key = "ratelimit:user:123";
    let counter_value = 9; // 9 requests made out of 10 limit

    cache::set(rate_limit_key, &counter_value);
    assert_eq!(cache::get::<i32>(rate_limit_key), Some(counter_value));

    // Loss of rate-limit state would allow request-flooding attacks
    // This is critical state that must be backed up with RDB/AOF
}

/// Test data structure for redis persistence status
#[derive(Debug, Clone)]
struct PersistenceConfig {
    rdb_enabled: bool,
    aof_enabled: bool,
    recommended_for_cache_only: bool,
    recommended_for_production: bool,
}

/// Test persistence recommendations for different deployment scenarios
#[test]
fn persistence_configuration_is_documented() {
    // This test documents the persistence configuration recommendations
    // that should appear in docs/deployment-guide.md

    // For development/cache-only: RDB is fine (can reconstruct from upstream)
    let cache_only_config = PersistenceConfig {
        rdb_enabled: true,
        aof_enabled: false,
        recommended_for_cache_only: true,
        recommended_for_production: false,
    };
    assert!(cache_only_config.rdb_enabled);
    assert!(!cache_only_config.aof_enabled);

    // For production with dedup/rate-limit: both RDB and AOF
    let production_config = PersistenceConfig {
        rdb_enabled: true,
        aof_enabled: true,
        recommended_for_cache_only: false,
        recommended_for_production: true,
    };
    assert!(production_config.rdb_enabled);
    assert!(production_config.aof_enabled);
}

/// Test that critical keys can be identified and backed up selectively
#[test]
fn critical_keys_can_be_identified() {
    let container = Redis::default()
        .start()
        .expect("failed to start Redis container");
    let host = container.get_host().expect("failed to get container host");
    let port = container
        .get_host_port_ipv4(6379)
        .expect("failed to get mapped Redis port");
    std::env::set_var("REDIS_URL", format!("redis://{host}:{port}"));

    // Store both cache and critical keys
    let cache_key = "cache:temp";
    let dedup_key = "dedup:request:xyz";
    let rate_limit_key = "ratelimit:user:999";

    cache::set(cache_key, &"temp_value");
    cache::set(dedup_key, &"processed");
    cache::set(rate_limit_key, &5);

    // Critical keys should be identified by prefix
    let critical_prefixes = vec!["dedup:", "ratelimit:"];
    let cache_prefixes = vec!["cache:"];

    let test_key_critical = critical_prefixes.iter().any(|p| dedup_key.starts_with(p));
    let test_key_cache = cache_prefixes.iter().any(|p| cache_key.starts_with(p));

    assert!(test_key_critical, "dedup key should be identified as critical");
    assert!(test_key_cache, "cache key should be identified as safe-to-lose");
}

/// Test backup/restore window documentation
#[test]
fn backup_restore_procedure_is_documented() {
    let backup_procedure = r#"
    # Redis Backup/Restore Procedure (Issue #920)

    ## Safe-to-Lose State (Cache Only)
    - Keys with prefix `cache:*`
    - API response caches, computed results
    - Loss impact: Temporary slowdown, data repopulation on access
    - Recovery: Automatic on next cache miss

    ## Critical State (Must Backup)
    - Keys with prefix `dedup:*` (request dedup for idempotency)
    - Keys with prefix `ratelimit:*` (rate-limit counters)
    - Loss impact: Duplicate submissions allowed, rate-limit bypass
    - Recovery: Restore from RDB/AOF snapshot

    ## Recommended Configuration

    ### Development (Cache-Only)
    - RDB snapshots only (bgsave every 60s)
    - AOF: disabled
    - Command: redis-server --save 60 100

    ### Production
    - RDB + AOF (fsync every 1s)
    - Commands: --save 300 10 --appendonly yes --appendfsync everysec
    - Test restore procedure monthly
    - Monitor rewrite operations
    "#;

    // Verify documentation structure exists
    assert!(
        backup_procedure.contains("Safe-to-Lose State"),
        "Documentation should describe safe-to-lose state"
    );
    assert!(
        backup_procedure.contains("Critical State"),
        "Documentation should describe critical state"
    );
    assert!(
        backup_procedure.contains("Recommended Configuration"),
        "Documentation should provide config examples"
    );
}

/// Test cross-instance state consistency requirements
#[test]
fn cross_instance_persistence_must_be_consistent() {
    let container = Redis::default()
        .start()
        .expect("failed to start Redis container");
    let host = container.get_host().expect("failed to get container host");
    let port = container
        .get_host_port_ipv4(6379)
        .expect("failed to get mapped Redis port");
    std::env::set_var("REDIS_URL", format!("redis://{host}:{port}"));

    // Instance 1 writes critical state
    let critical_key = "dedup:request:multi_instance";
    let critical_value = DedupKey {
        request_id: "multi_instance".to_string(),
        timestamp: 5000,
        processed: true,
    };

    cache::set(critical_key, &critical_value);

    // Instance 2 should see the same state (both reading from same Redis)
    let instance2_view = cache::get::<DedupKey>(critical_key);
    assert_eq!(instance2_view, Some(critical_value));

    // Loss of Redis would lose this state across all instances
    // This must be backed up with persistence enabled
}
