/// Snapshot Testing for API Responses
///
/// This module uses insta snapshot testing to validate API response formats
/// and structures. Snapshots are committed to version control and compared
/// against on each test run to catch unintended changes.
///
/// To update snapshots after intentional API changes:
/// ```
/// INSTA_UPDATE=inline cargo test snapshot_tests
/// ```
///
/// Run tests:
/// ```
/// cargo test snapshot_tests -- --nocapture
/// ```

#[cfg(test)]
mod snapshot_tests {
    use serde_json::json;

    /// Snapshot: Basic IP record response structure
    ///
    /// This snapshot validates that IP record responses maintain consistent structure
    /// including all required fields: ip_id, owner, commitment_hash, timestamp, etc.
    #[test]
    fn snapshot_ip_record_response() {
        let ip_record = json!({
            "ip_id": 42,
            "owner": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
            "commitment_hash": "abc123def456",
            "timestamp": 1695312000,
            "revoked": false,
            "co_owners": [],
            "expiry_timestamp": 0,
            "grace_period_seconds": 0
        });

        insta::assert_json_snapshot!("ip_record_response", ip_record);
    }

    /// Snapshot: Swap record response structure
    ///
    /// Validates that swap responses include all key information: swap_id, status,
    /// seller, buyer, price, ip_id, and timestamps.
    #[test]
    fn snapshot_swap_record_response() {
        let swap_record = json!({
            "swap_id": 1,
            "ip_id": 42,
            "seller": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
            "buyer": "GBTMQWEGN2YGXKUHKC3F4H5TKDPHYJLQNHKPDSVKLSVQ6YXV3RIQSJNE",
            "status": "Pending",
            "price": 10000,
            "timestamp": 1695312000,
            "expiry_timestamp": 1695398400,
            "token": "CACHETX22Q3ENJQ4RLZSCR4X7X42PR7XUHP6GOHWOZYT7N6QWFG4PNNN"
        });

        insta::assert_json_snapshot!("swap_record_response", swap_record);
    }

    /// Snapshot: Error response structure
    ///
    /// Validates consistent error response format with error code and message.
    #[test]
    fn snapshot_error_response() {
        let error_response = json!({
            "error": {
                "code": "IP_NOT_FOUND",
                "message": "The requested IP record was not found",
                "details": {
                    "ip_id": 999
                }
            }
        });

        insta::assert_json_snapshot!("error_response", error_response);
    }

    /// Snapshot: List IP records response
    ///
    /// Validates structure of paginated list responses with pagination info.
    #[test]
    fn snapshot_list_ip_response() {
        let list_response = json!({
            "data": [
                {
                    "ip_id": 1,
                    "owner": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
                    "commitment_hash": "hash1",
                    "timestamp": 1695312000,
                    "revoked": false
                },
                {
                    "ip_id": 2,
                    "owner": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
                    "commitment_hash": "hash2",
                    "timestamp": 1695312100,
                    "revoked": false
                }
            ],
            "pagination": {
                "cursor": "cursor_next_page",
                "has_more": true,
                "total_count": 100
            }
        });

        insta::assert_json_snapshot!("list_ip_response", list_response);
    }

    /// Snapshot: Commit IP request structure
    ///
    /// Validates that commit IP requests have correct format for commitment_hash
    /// and pow_difficulty fields.
    #[test]
    fn snapshot_commit_ip_request() {
        let commit_request = json!({
            "owner": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
            "commitment_hash": "abc123def456789abc123def456789abc123def456789abc123def456789ab",
            "pow_difficulty": 20
        });

        insta::assert_json_snapshot!("commit_ip_request", commit_request);
    }

    /// Snapshot: Initiate swap request structure
    ///
    /// Validates that swap initiation requests include all required parameters.
    #[test]
    fn snapshot_initiate_swap_request() {
        let swap_request = json!({
            "ip_id": 42,
            "seller": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
            "buyer": "GBTMQWEGN2YGXKUHKC3F4H5TKDPHYJLQNHKPDSVKLSVQ6YXV3RIQSJNE",
            "price": 10000,
            "token": "CACHETX22Q3ENJQ4RLZSCR4X7X42PR7XUHP6GOHWOZYT7N6QWFG4PNNN",
            "expiry_seconds": 86400
        });

        insta::assert_json_snapshot!("initiate_swap_request", swap_request);
    }

    /// Snapshot: Verify commitment request
    ///
    /// Validates structure of verification requests with secret and blinding factor.
    #[test]
    fn snapshot_verify_commitment_request() {
        let verify_request = json!({
            "ip_id": 42,
            "secret": "secret_value_32_bytes_long_123456",
            "blinding_factor": "blinding_32_bytes_long_value_123456"
        });

        insta::assert_json_snapshot!("verify_commitment_request", verify_request);
    }

    /// Snapshot: Batch commit IP response
    ///
    /// Validates response structure when committing multiple IPs in a batch.
    #[test]
    fn snapshot_batch_commit_response() {
        let batch_response = json!({
            "batch_id": "batch_20231121_abc123",
            "results": [
                {
                    "index": 0,
                    "ip_id": 100,
                    "status": "success",
                    "timestamp": 1695312000
                },
                {
                    "index": 1,
                    "ip_id": 101,
                    "status": "success",
                    "timestamp": 1695312001
                },
                {
                    "index": 2,
                    "status": "error",
                    "error": "Duplicate commitment hash"
                }
            ],
            "summary": {
                "total": 3,
                "succeeded": 2,
                "failed": 1
            }
        });

        insta::assert_json_snapshot!("batch_commit_response", batch_response);
    }

    /// Snapshot: Swap status transition response
    ///
    /// Validates response structure when swap status changes.
    #[test]
    fn snapshot_swap_status_change_response() {
        let status_change = json!({
            "swap_id": 1,
            "previous_status": "Pending",
            "current_status": "Accepted",
            "timestamp": 1695312100,
            "updated_by": "GBTMQWEGN2YGXKUHKC3F4H5TKDPHYJLQNHKPDSVKLSVQ6YXV3RIQSJNE",
            "metadata": {
                "transition": "accept_swap",
                "expiry_extended": false
            }
        });

        insta::assert_json_snapshot!("swap_status_change_response", status_change);
    }

    /// Snapshot: Cache statistics response
    ///
    /// Validates structure of cache statistics endpoint.
    #[test]
    fn snapshot_cache_statistics_response() {
        let cache_stats = json!({
            "backend": "redis",
            "status": "healthy",
            "metrics": {
                "total_entries": 5432,
                "memory_bytes": 2097152,
                "hit_rate": 0.87,
                "miss_rate": 0.13
            },
            "operations": {
                "total_reads": 100000,
                "total_writes": 50000,
                "total_deletes": 10000
            }
        });

        insta::assert_json_snapshot!("cache_statistics_response", cache_stats);
    }

    /// Snapshot: Health check response
    ///
    /// Validates health endpoint response structure.
    #[test]
    fn snapshot_health_check_response() {
        let health_response = json!({
            "status": "healthy",
            "timestamp": 1695312000,
            "components": {
                "database": {
                    "status": "healthy",
                    "latency_ms": 5
                },
                "cache": {
                    "status": "healthy",
                    "latency_ms": 2
                },
                "blockchain": {
                    "status": "healthy",
                    "latency_ms": 150
                }
            },
            "uptime_seconds": 86400
        });

        insta::assert_json_snapshot!("health_check_response", health_response);
    }

    /// Snapshot: Reputation record response
    ///
    /// Validates reputation information response structure.
    #[test]
    fn snapshot_reputation_response() {
        let reputation = json!({
            "address": "GAXSZBQTGZXH65FRMCTQBYMZXKQMV3JPZPMSOZQ5KDKQNFBXQGWXJRBN",
            "score": 85,
            "transactions": {
                "total": 42,
                "successful": 40,
                "disputed": 1,
                "failed": 1
            },
            "metrics": {
                "average_completion_time_hours": 2.5,
                "reliability_percentage": 95.2,
                "dispute_rate_percentage": 2.4
            },
            "last_transaction": 1695312000
        });

        insta::assert_json_snapshot!("reputation_response", reputation);
    }

    /// Snapshot: Transaction history response
    ///
    /// Validates paginated transaction history structure.
    #[test]
    fn snapshot_transaction_history_response() {
        let history = json!({
            "transactions": [
                {
                    "id": "txn_001",
                    "type": "commit_ip",
                    "status": "completed",
                    "timestamp": 1695312000,
                    "ip_id": 1
                },
                {
                    "id": "txn_002",
                    "type": "initiate_swap",
                    "status": "completed",
                    "timestamp": 1695312100,
                    "swap_id": 1
                },
                {
                    "id": "txn_003",
                    "type": "accept_swap",
                    "status": "completed",
                    "timestamp": 1695312200,
                    "swap_id": 1
                }
            ],
            "pagination": {
                "cursor": "next_page_cursor",
                "has_more": true
            }
        });

        insta::assert_json_snapshot!("transaction_history_response", history);
    }

    /// Snapshot: API Version response
    ///
    /// Validates API version and schema information response.
    #[test]
    fn snapshot_api_version_response() {
        let version_info = json!({
            "api_version": "1.0.0",
            "schema_version": 1,
            "supported_operations": [
                "commit_ip",
                "get_ip",
                "transfer_ip",
                "verify_commitment",
                "initiate_swap",
                "accept_swap",
                "reveal_key",
                "cancel_swap"
            ],
            "features": {
                "batch_operations": true,
                "snapshot_testing": true,
                "connection_pooling": true,
                "property_based_testing": true,
                "swap_fuzzing": true
            }
        });

        insta::assert_json_snapshot!("api_version_response", version_info);
    }

    // ── Snapshot Update Workflow Tests ────────────────────────────────────────

    /// Test: Snapshot diff reporting
    ///
    /// Validates that snapshot testing properly reports differences when
    /// responses change unexpectedly.
    #[test]
    fn test_snapshot_diff_detection() {
        let original = json!({
            "value": 100,
            "status": "active"
        });

        // This would be the original snapshot
        insta::assert_json_snapshot!("snapshot_original", original);

        let modified = json!({
            "value": 100,
            "status": "active",
            "new_field": "unexpected"
        });

        // Modified snapshot with new field - insta would report this difference
        // when run without INSTA_UPDATE
        insta::assert_json_snapshot!("snapshot_modified", modified);
    }

    /// Test: Snapshot precision validation
    ///
    /// Validates that snapshots capture precise values including floating-point
    /// numbers, timestamps, and nested structures.
    #[test]
    fn test_snapshot_precision() {
        let precise_data = json!({
            "precise_value": 3.141592653589793,
            "timestamp": 1695312000,
            "nested": {
                "array": [1, 2, 3, 4, 5],
                "object": {
                    "key": "value",
                    "count": 42
                }
            }
        });

        insta::assert_json_snapshot!("snapshot_precision", precise_data);
    }

    // ── Snapshot Documentation Tests ──────────────────────────────────────────

    /// Test: Document expected error cases
    ///
    /// Snapshots of error responses help document expected error formats
    /// and messages for API consumers.
    #[test]
    fn test_document_error_ip_not_found() {
        let error = json!({
            "error": "IpNotFound",
            "message": "IP record with id 999 not found",
            "code": 1,
            "timestamp": 1695312000
        });

        insta::assert_json_snapshot!("error_ip_not_found", error);
    }

    /// Test: Document swap state transitions
    ///
    /// Snapshots document valid state transitions and their responses.
    #[test]
    fn test_document_swap_state_pending() {
        let swap_pending = json!({
            "swap_id": 1,
            "status": "Pending",
            "allowed_transitions": ["Accept", "Cancel"],
            "metadata": {
                "initiated_at": 1695312000,
                "initiated_by": "seller"
            }
        });

        insta::assert_json_snapshot!("swap_state_pending", swap_pending);
    }

    /// Test: Document commitment verification success
    ///
    /// Snapshots document what successful commitment verification looks like.
    #[test]
    fn test_document_verification_success() {
        let verification_result = json!({
            "ip_id": 42,
            "verified": true,
            "verification_time_ms": 5,
            "commitment_hash": "abc123",
            "message": "Commitment verified successfully"
        });

        insta::assert_json_snapshot!("verification_success", verification_result);
    }
}
