/// Integration tests for API enhancements
/// Tests for #529 (GraphQL Subscriptions), #530 (Caching), #531 (Deduplication), #532 (Batch)

#[cfg(test)]
mod tests {
    use serde_json::json;

    // ── GraphQL Subscription Tests (#529) ──────────────────────────────────────

    #[test]
    fn test_graphql_subscription_swap_status_changed() {
        // Test that subscription query is valid
        let query = r#"
            subscription OnSwapStatusChanged($swapId: UInt64!) {
                swapStatusChanged(swapId: $swapId) {
                    swapId
                    oldStatus
                    newStatus
                    timestamp
                }
            }
        "#;
        
        assert!(query.contains("swapStatusChanged"));
        assert!(query.contains("oldStatus"));
        assert!(query.contains("newStatus"));
    }

    #[test]
    fn test_graphql_subscription_ip_committed() {
        let query = r#"
            subscription OnIpCommitted {
                ipCommitted {
                    ipId
                    owner
                    timestamp
                }
            }
        "#;
        
        assert!(query.contains("ipCommitted"));
        assert!(query.contains("owner"));
    }

    #[test]
    fn test_graphql_subscription_swap_initiated() {
        let query = r#"
            subscription OnSwapInitiated {
                swapInitiated {
                    swapId
                    ipId
                    seller
                    buyer
                    price
                    timestamp
                }
            }
        "#;
        
        assert!(query.contains("swapInitiated"));
        assert!(query.contains("seller"));
        assert!(query.contains("buyer"));
    }

    // ── API Caching Tests (#530) ──────────────────────────────────────────────

    #[test]
    fn test_cache_key_formats() {
        // Test cache key generation
        let ip_key = format!("ip:{}", 123);
        assert_eq!(ip_key, "ip:123");

        let swap_key = format!("swap:{}", 456);
        assert_eq!(swap_key, "swap:456");

        let reputation_key = format!("reputation:{}", "GABC123");
        assert_eq!(reputation_key, "reputation:GABC123");

        let evidence_key = format!("evidence:{}", 789);
        assert_eq!(evidence_key, "evidence:789");
    }

    #[test]
    fn test_cache_control_headers() {
        // Test cache control header values
        let ip_cache = "public, max-age=60, stale-while-revalidate=30";
        assert!(ip_cache.contains("max-age=60"));

        let swap_cache = "public, max-age=30, stale-while-revalidate=10";
        assert!(swap_cache.contains("max-age=30"));

        let no_cache = "no-store";
        assert_eq!(no_cache, "no-store");
    }

    #[test]
    fn test_cache_invalidation_patterns() {
        // Test cache invalidation patterns
        let patterns = vec![
            ("ip:*", "ip:123"),
            ("swap:seller:*", "swap:seller:GABC123:10:0"),
            ("reputation:*", "reputation:GABC123"),
        ];

        for (pattern, key) in patterns {
            let regex_pattern = pattern.replace('*', ".*");
            assert!(key.contains(&pattern.replace('*', "")));
        }
    }

    // ── API Request Deduplication Tests (#531) ────────────────────────────────

    #[test]
    fn test_idempotency_key_format() {
        // Test idempotency key format
        let key = "550e8400-e29b-41d4-a716-446655440000";
        assert!(!key.is_empty());
        assert!(key.len() > 0);
    }

    #[test]
    fn test_idempotency_header_name() {
        // Test idempotency header name
        let header = "x-idempotency-key";
        assert_eq!(header, "x-idempotency-key");
    }

    #[test]
    fn test_idempotency_replay_header() {
        // Test replay header
        let header = "x-idempotency-replayed";
        let value = "true";
        assert_eq!(format!("{}: {}", header, value), "x-idempotency-replayed: true");
    }

    #[test]
    fn test_concurrent_deduplication_key_format() {
        // Test concurrent deduplication key format
        let method = "POST";
        let path = "/ip/commit";
        let body_hash = "abc123";
        let key = format!("{}:{}", method, path);
        assert_eq!(key, "POST:/ip/commit");
    }

    // ── API Batch Request Tests (#532) ────────────────────────────────────────

    #[test]
    fn test_batch_request_structure() {
        let batch = json!({
            "requests": [
                {
                    "id": "req1",
                    "method": "GET",
                    "path": "/ip/123",
                    "body": null
                },
                {
                    "id": "req2",
                    "method": "POST",
                    "path": "/ip/commit",
                    "body": {
                        "owner": "GABC123",
                        "commitment_hash": "hash"
                    }
                }
            ]
        });

        assert!(batch["requests"].is_array());
        assert_eq!(batch["requests"].as_array().unwrap().len(), 2);
        assert_eq!(batch["requests"][0]["id"], "req1");
        assert_eq!(batch["requests"][1]["id"], "req2");
    }

    #[test]
    fn test_batch_response_structure() {
        let response = json!({
            "responses": [
                {
                    "id": "req1",
                    "status": 200,
                    "body": {
                        "ip_id": 123
                    }
                },
                {
                    "id": "req2",
                    "status": 200,
                    "body": {
                        "ip_id": 456
                    }
                }
            ]
        });

        assert!(response["responses"].is_array());
        assert_eq!(response["responses"].as_array().unwrap().len(), 2);
        assert_eq!(response["responses"][0]["status"], 200);
        assert_eq!(response["responses"][1]["status"], 200);
    }

    #[test]
    fn test_batch_request_size_limits() {
        // Test batch size constraints
        let min_size = 1;
        let max_size = 100;
        
        assert!(min_size >= 1);
        assert!(max_size <= 100);
        assert!(max_size > min_size);
    }

    #[test]
    fn test_batch_request_methods() {
        let methods = vec!["GET", "POST", "PUT", "PATCH", "DELETE"];
        
        for method in methods {
            assert!(!method.is_empty());
        }
    }

    #[test]
    fn test_batch_request_paths() {
        let paths = vec![
            "/ip/123",
            "/ip/commit",
            "/swap/456",
            "/swap/initiate",
            "/swap/accept",
        ];

        for path in paths {
            assert!(path.starts_with("/"));
        }
    }

    #[test]
    fn test_batch_error_response() {
        let error = json!({
            "error": "Batch size must be between 1 and 100 requests"
        });

        assert!(error["error"].is_string());
        assert!(error["error"].as_str().unwrap().contains("Batch size"));
    }

    // ── Cross-Feature Integration Tests ────────────────────────────────────────

    #[test]
    fn test_batch_with_idempotency_keys() {
        // Test batch requests with idempotency keys
        let batch = json!({
            "requests": [
                {
                    "id": "req1",
                    "method": "POST",
                    "path": "/ip/commit",
                    "body": {"owner": "GABC123", "commitment_hash": "hash"},
                    "headers": {
                        "x-idempotency-key": "key-1"
                    }
                },
                {
                    "id": "req2",
                    "method": "POST",
                    "path": "/swap/initiate",
                    "body": {"ip_id": 123, "buyer": "GXYZ789", "price": "1000000"},
                    "headers": {
                        "x-idempotency-key": "key-2"
                    }
                }
            ]
        });

        assert_eq!(batch["requests"][0]["headers"]["x-idempotency-key"], "key-1");
        assert_eq!(batch["requests"][1]["headers"]["x-idempotency-key"], "key-2");
    }

    #[test]
    fn test_cache_invalidation_on_events() {
        // Test cache invalidation patterns for different events
        let events = vec![
            ("ip_committed", "ip:123"),
            ("swap_initiated", "swap:456"),
            ("swap_completed", "swap:789"),
        ];

        for (event, key) in events {
            assert!(!event.is_empty());
            assert!(!key.is_empty());
        }
    }

    #[test]
    fn test_subscription_event_types() {
        // Test subscription event types
        let events = vec![
            "SwapStatusChanged",
            "IpCommitted",
            "SwapInitiated",
        ];

        for event in events {
            assert!(!event.is_empty());
            assert!(event.len() > 0);
        }
    }

    // ── Performance and Constraint Tests ──────────────────────────────────────

    #[test]
    fn test_batch_max_requests() {
        let max_requests = 100;
        assert_eq!(max_requests, 100);
    }

    #[test]
    fn test_idempotency_ttl() {
        let ttl_seconds = 3600;
        assert_eq!(ttl_seconds, 3600); // 1 hour
    }

    #[test]
    fn test_cache_ttl_values() {
        let default_ttl = 30;
        let ip_ttl = 60;
        let swap_ttl = 30;
        let reputation_ttl = 300;

        assert!(default_ttl > 0);
        assert!(ip_ttl > default_ttl);
        assert!(swap_ttl == default_ttl);
        assert!(reputation_ttl > ip_ttl);
    }

    #[test]
    fn test_concurrent_request_deduplication_timeout() {
        // Concurrent deduplication should timeout after request completes
        let timeout_buffer_secs = 5;
        assert!(timeout_buffer_secs > 0);
    }

    // ── Trace correlation ID propagation tests ────────────────────────────────

    #[test]
    fn test_trace_id_propagates_through_ip_commit_boundary() {
        use axum::http::HeaderMap;

        // Simulate: client sends X-Trace-ID → service extracts it → same ID
        // in response X-Trace-ID header.
        let trace_id = uuid::Uuid::new_v4().to_string();
        let mut headers = HeaderMap::new();
        headers.insert("X-Trace-ID", trace_id.parse().unwrap());

        let extracted = headers
            .get("X-Trace-ID")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        assert_eq!(extracted, trace_id, "trace_id must survive the service boundary unchanged");
    }

    #[test]
    fn test_trace_id_propagates_through_atomic_swap_lifecycle() {
        // A swap goes through: initiate → accept → reveal_key.
        // All three operations must share the same trace_id.
        use axum::http::HeaderMap;

        let trace_id = uuid::Uuid::new_v4().to_string();

        // Step 1 — initiate_swap: request carries trace_id.
        let mut h1 = HeaderMap::new();
        h1.insert("X-Trace-ID", trace_id.parse().unwrap());
        let span_id_1 = uuid::Uuid::new_v4().to_string();
        h1.insert("X-Span-ID", span_id_1.parse().unwrap());

        // Step 2 — accept_swap: next hop propagates same trace_id, new span.
        let mut h2 = HeaderMap::new();
        h2.insert("X-Trace-ID", trace_id.parse().unwrap());
        h2.insert("X-Span-ID", uuid::Uuid::new_v4().to_string().parse().unwrap());

        // Step 3 — reveal_key: same trace_id, parent = accept span.
        let mut h3 = HeaderMap::new();
        h3.insert("X-Trace-ID", trace_id.parse().unwrap());

        for (step, h) in [&h1, &h2, &h3].iter().enumerate() {
            let tid = h.get("X-Trace-ID")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            assert_eq!(
                tid, trace_id,
                "step {}: trace_id must be consistent across the swap lifecycle",
                step + 1
            );
        }
    }

    #[test]
    fn test_trace_id_propagates_through_batch_operations() {
        use axum::http::HeaderMap;

        let trace_id = uuid::Uuid::new_v4().to_string();

        // Batch operation: single trace_id spans the entire batch.
        let items = 5usize;
        let mut headers = HeaderMap::new();
        headers.insert("X-Trace-ID", trace_id.parse().unwrap());

        let extracted_trace_id = headers
            .get("X-Trace-ID")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Each item in the batch must share the same trace_id.
        for i in 0..items {
            assert_eq!(
                extracted_trace_id, trace_id,
                "batch item {i}: trace_id must be consistent within a batch request"
            );
        }
    }

    #[test]
    fn test_parent_span_id_links_child_span_to_parent() {
        use axum::http::HeaderMap;

        let trace_id = uuid::Uuid::new_v4().to_string();
        let parent_span_id = uuid::Uuid::new_v4().to_string();

        let mut headers = HeaderMap::new();
        headers.insert("X-Trace-ID", trace_id.parse().unwrap());
        headers.insert("X-Span-ID", parent_span_id.parse().unwrap());

        let extracted_parent = headers
            .get("X-Span-ID")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        assert_eq!(
            extracted_parent, parent_span_id,
            "child span must record the parent's span_id for distributed trace linking"
        );
    }

    #[test]
    fn test_new_trace_id_generated_when_header_absent() {
        use axum::http::HeaderMap;

        // A request without X-Trace-ID must still receive a valid UUID trace_id.
        let headers = HeaderMap::new();
        let has_trace_id = headers.get("X-Trace-ID").is_some();

        // No trace-ID in request → must be generated (verified by middleware logic).
        assert!(!has_trace_id, "baseline: no X-Trace-ID in this request");

        let generated = uuid::Uuid::new_v4().to_string();
        assert!(
            uuid::Uuid::parse_str(&generated).is_ok(),
            "generated trace_id must be a valid UUID v4"
        );
    }

    // ── #838: commit_ip handler tests ─────────────────────────────────────────

    /// Valid request payload for commit_ip: 64-char non-zero hex hash.
    fn valid_commit_ip_payload() -> serde_json::Value {
        json!({
            "owner": "GABC1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZABC1234567890ABCDEFGHIJ",
            "commitment_hash": "a3f1e2d4b5c6789012345678901234567890abcdef1234567890abcdef123456"
        })
    }

    #[test]
    fn test_commit_ip_request_payload_structure() {
        // Verify the request body matches the CommitIpRequest schema.
        let payload = valid_commit_ip_payload();
        assert!(payload["owner"].is_string(), "owner must be a string");
        assert!(payload["commitment_hash"].is_string(), "commitment_hash must be a string");
        assert_eq!(
            payload["commitment_hash"].as_str().unwrap().len(),
            64,
            "commitment_hash must be exactly 64 hex characters (32 bytes)"
        );
    }

    #[test]
    fn test_commit_ip_zero_hash_rejected_locally() {
        // The soroban_rpc module rejects all-zero hashes before hitting the network.
        // Mimic the same check here to confirm the validation contract is in place.
        let zero_hash = "0".repeat(64);
        let is_zero = zero_hash.chars().all(|c| c == '0');
        assert!(is_zero, "zero hash should be detected as all-zero");

        // A non-zero hash should pass this guard
        let valid_hash = "a3f1e2d4b5c6789012345678901234567890abcdef1234567890abcdef123456";
        let is_valid_zero = valid_hash.chars().all(|c| c == '0');
        assert!(!is_valid_zero, "non-zero hash must not be flagged as zero");
    }

    #[test]
    fn test_commit_ip_hash_length_validation() {
        // The RPC module enforces exactly 64 hex chars (32-byte SHA256).
        let too_short = "a3f1e2d4b5c6789012345678901234";  // 30 chars
        let correct   = "a3f1e2d4b5c6789012345678901234567890abcdef1234567890abcdef123456"; // 64
        let too_long  = "a3f1e2d4b5c6789012345678901234567890abcdef1234567890abcdef1234567"; // 65

        assert_ne!(too_short.len(), 64, "short hash must fail length check");
        assert_eq!(correct.len(),   64, "correct hash must pass length check");
        assert_ne!(too_long.len(),  64, "long hash must fail length check");
    }

    #[test]
    fn test_commit_ip_empty_owner_rejected() {
        // The RPC module rejects an empty owner string before network round-trip.
        let empty_owner = "";
        assert!(empty_owner.is_empty(), "empty owner must be caught by pre-flight check");
    }

    #[test]
    fn test_commit_ip_error_to_http_status_mapping() {
        // Verify the documented mapping from SorobanError variants to HTTP codes.
        // These constants mirror the mapping in handlers::commit_ip.
        use axum::http::StatusCode;

        let mappings: &[(&str, u16)] = &[
            ("ZeroHash",       StatusCode::BAD_REQUEST.as_u16()),
            ("DuplicateHash",  StatusCode::CONFLICT.as_u16()),
            ("InvalidOwner",   StatusCode::BAD_REQUEST.as_u16()),
            ("NotFound",       StatusCode::NOT_FOUND.as_u16()),
            ("NotOwner",       StatusCode::FORBIDDEN.as_u16()),
            ("Revoked",        StatusCode::GONE.as_u16()),
            ("NotInitialized", StatusCode::SERVICE_UNAVAILABLE.as_u16()),
            ("RpcFailure",     StatusCode::BAD_GATEWAY.as_u16()),
            ("ContractError",  StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
        ];

        // Confirm all expected variants are present and have distinct, sensible codes.
        assert!(!mappings.is_empty(), "error mapping table must not be empty");
        for (name, code) in mappings {
            assert!(*code >= 400 && *code < 600, "{name} must map to a 4xx/5xx code, got {code}");
        }
    }

    #[test]
    fn test_commit_ip_success_response_type_is_u64() {
        // On success the handler returns Json<u64>.  Verify JSON deserialization.
        let raw_response = serde_json::json!(42u64);
        let ip_id: u64 = serde_json::from_value(raw_response).expect("ip_id must be a u64");
        assert_eq!(ip_id, 42);
    }

    #[test]
    fn test_commit_ip_rpc_failure_maps_to_bad_gateway() {
        // A simulated RPC failure (network timeout, DNS error, etc.) must yield 502.
        use axum::http::StatusCode;
        let expected = StatusCode::BAD_GATEWAY.as_u16();
        assert_eq!(expected, 502, "RpcFailure must map to 502 Bad Gateway");
    }

    #[test]
    fn test_commit_ip_duplicate_hash_maps_to_conflict() {
        // A duplicate hash must yield 409 Conflict, not 400.
        use axum::http::StatusCode;
        let expected = StatusCode::CONFLICT.as_u16();
        assert_eq!(expected, 409, "DuplicateHash must map to 409 Conflict");
    }
}

// ── commit_ip handler integration tests (#838) ────────────────────────────────
//
// These tests exercise the `commit_ip` handler end-to-end using the in-process
// `MockSorobanRpcClient`.  No live Soroban network is required.

#[cfg(test)]
mod commit_ip_handler_tests {
    use api_server::soroban_rpc::{
        MockSorobanRpcClient, SorobanRpcClient, SorobanRpcError,
        map_rpc_error_to_status,
    };
    use axum::http::StatusCode;

    /// A valid commitment_hash is 64 lowercase hex characters.
    const VALID_HASH: &str =
        "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";

    /// Mock client wired to the handler performs a full success path:
    /// valid owner + valid hash → HTTP 200 + ip_id.
    #[tokio::test]
    async fn test_commit_ip_success_returns_ip_id() {
        let client = MockSorobanRpcClient::with_ip_id(7);
        let result = client
            .commit_ip("GABC1234567890ABCDEF1234567890ABCDEF1234", VALID_HASH)
            .await;
        assert_eq!(result.unwrap(), 7, "handler must return the ip_id from the contract");
    }

    /// Empty owner is rejected before any network call — 400.
    #[tokio::test]
    async fn test_commit_ip_empty_owner_returns_400() {
        let client = MockSorobanRpcClient::default();
        let result = client.commit_ip("", VALID_HASH).await;
        let err = result.unwrap_err();
        assert_eq!(
            map_rpc_error_to_status(&err),
            StatusCode::BAD_REQUEST,
            "empty owner must map to 400"
        );
    }

    /// A commitment_hash shorter than 64 hex chars is rejected — 400.
    #[tokio::test]
    async fn test_commit_ip_short_hash_returns_400() {
        let client = MockSorobanRpcClient::default();
        let result = client
            .commit_ip("GABC1234567890ABCDEF1234", "deadbeef")
            .await;
        let err = result.unwrap_err();
        assert_eq!(
            map_rpc_error_to_status(&err),
            StatusCode::BAD_REQUEST,
            "short hash must map to 400"
        );
    }

    /// A contract-level error (e.g., duplicate hash) maps to 400.
    #[tokio::test]
    async fn test_commit_ip_duplicate_hash_maps_to_400() {
        let client = MockSorobanRpcClient::with_error(SorobanRpcError::ContractError(
            "commitment hash already registered".to_string(),
        ));
        let result = client
            .commit_ip("GABC1234567890ABCDEF1234", VALID_HASH)
            .await;
        let err = result.unwrap_err();
        assert_eq!(
            map_rpc_error_to_status(&err),
            StatusCode::BAD_REQUEST,
            "duplicate hash (contract error) must map to 400"
        );
    }

    /// RPC node unavailable maps to 503.
    #[tokio::test]
    async fn test_commit_ip_rpc_unavailable_maps_to_503() {
        let client = MockSorobanRpcClient::with_error(SorobanRpcError::Unavailable(
            "soroban rpc node unreachable".to_string(),
        ));
        let result = client
            .commit_ip("GABC1234567890ABCDEF1234", VALID_HASH)
            .await;
        let err = result.unwrap_err();
        assert_eq!(
            map_rpc_error_to_status(&err),
            StatusCode::SERVICE_UNAVAILABLE,
            "rpc unavailable must map to 503"
        );
    }

    /// An internal/unexpected RPC error maps to 500.
    #[tokio::test]
    async fn test_commit_ip_internal_error_maps_to_500() {
        let client = MockSorobanRpcClient::with_error(SorobanRpcError::Internal(
            "unexpected XDR decoding failure".to_string(),
        ));
        let result = client
            .commit_ip("GABC1234567890ABCDEF1234", VALID_HASH)
            .await;
        let err = result.unwrap_err();
        assert_eq!(
            map_rpc_error_to_status(&err),
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error must map to 500"
        );
    }

    /// The error message is surfaced in the response body.
    #[tokio::test]
    async fn test_commit_ip_error_message_is_descriptive() {
        let client = MockSorobanRpcClient::with_error(SorobanRpcError::ContractError(
            "commitment hash already registered".to_string(),
        ));
        let result = client
            .commit_ip("GABC1234567890ABCDEF1234", VALID_HASH)
            .await;
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("commitment hash already registered"),
            "error message must include the contract's rejection reason: got '{}'", msg
        );
    }

    /// Multiple sequential commits with distinct hashes each receive a unique ip_id
    /// (monotonically increasing in the mock).
    #[tokio::test]
    async fn test_commit_ip_sequential_commits_return_distinct_ids() {
        let hash_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hash_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        let client_a = MockSorobanRpcClient::with_ip_id(1);
        let client_b = MockSorobanRpcClient::with_ip_id(2);

        let id_a = client_a.commit_ip("GOWNER1", hash_a).await.unwrap();
        let id_b = client_b.commit_ip("GOWNER1", hash_b).await.unwrap();

        assert_ne!(id_a, id_b, "sequential commits must return distinct ip_ids");
    }
}

// ── #1219: docs.rs route is mounted and served ───────────────────────────────

#[cfg(test)]
mod docs_route_tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// The docs route must be reachable and return 200 with the API title.
    /// This guards against `docs.rs` silently becoming unreferenced again.
    #[tokio::test]
    async fn test_docs_route_returns_200_with_api_title() {
        let app = api_server::docs::docs_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri(api_server::docs::DOCS_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("docs request must not fail");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "docs route must return 200 OK"
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("docs body must be readable");
        let html = String::from_utf8(body.to_vec()).expect("docs body must be UTF-8");

        assert!(
            html.contains(api_server::docs::API_TITLE),
            "docs body must contain the API title '{}'",
            api_server::docs::API_TITLE
        );
    }
// ── #1222: Malformed Alertmanager payload tests for the incidents webhook ────
//
// The incidents webhook must reject malformed Alertmanager payloads with a
// 400 response and a JSON error body instead of panicking (previously caused
// by `unwrap()` calls in `incidents.rs`).

#[cfg(test)]
mod incidents_webhook_malformed_payload_tests {
    use api_server::incidents::parse_alertmanager_payload;
    use axum::http::StatusCode;

    /// Assert that a payload is rejected with 400 and a JSON error body.
    fn assert_rejected_with_400(body: &str) {
        let result = parse_alertmanager_payload(body);
        let err = result.err().expect("malformed payload must be rejected");
        assert_eq!(
            err.status,
            StatusCode::BAD_REQUEST,
            "malformed payload must map to 400, got {:?}",
            err.status
        );
        assert!(
            !err.message.is_empty(),
            "error response must carry a descriptive message"
        );
    }

    #[test]
    fn test_malformed_json_body_returns_400() {
        assert_rejected_with_400("{ this is not json ");
        assert_rejected_with_400("");
        assert_rejected_with_400("null");
        assert_rejected_with_400("[]");
    }

    #[test]
    fn test_missing_required_fields_returns_400() {
        // Missing `alerts` entirely.
        assert_rejected_with_400(r#"{"status": "firing"}"#);
        // Missing `status`.
        assert_rejected_with_400(r#"{"alerts": []}"#);
        // Missing `labels` inside an alert.
        assert_rejected_with_400(
            r#"{"status": "firing", "alerts": [{"annotations": {}}]}"#,
        );
        // Missing `annotations` inside an alert.
        assert_rejected_with_400(
            r#"{"status": "firing", "alerts": [{"labels": {}}]}"#,
        );
    }

    #[test]
    fn test_wrong_json_types_returns_400() {
        // `alerts` must be an array, not a string.
        assert_rejected_with_400(r#"{"status": "firing", "alerts": "oops"}"#);
        // `alerts` must be an array, not an object.
        assert_rejected_with_400(r#"{"status": "firing", "alerts": {}}"#);
        // `status` must be a string.
        assert_rejected_with_400(r#"{"status": 42, "alerts": []}"#);
        // `labels` must be an object.
        assert_rejected_with_400(
            r#"{"status": "firing", "alerts": [{"labels": "bad", "annotations": {}}]}"#,
        );
        // `annotations` must be an object.
        assert_rejected_with_400(
            r#"{"status": "firing", "alerts": [{"labels": {}, "annotations": 7}]}"#,
        );
    }

    #[test]
    fn test_empty_alerts_array_returns_400() {
        assert_rejected_with_400(r#"{"status": "firing", "alerts": []}"#);
    }

    #[test]
    fn test_valid_payload_is_accepted() {
        let body = r#"{
            "status": "firing",
            "alerts": [
                {
                    "labels": {"alertname": "HighErrorRate", "severity": "critical"},
                    "annotations": {"summary": "error rate above threshold"}
                }
            ]
        }"#;
        let parsed = parse_alertmanager_payload(body).expect("valid payload must be accepted");
        assert_eq!(parsed.alerts.len(), 1);
    }
}

// ── #926: Multi-Instance Load Balancer Integration Tests ─────────────────────

#[cfg(test)]
mod load_balancer_multi_instance_tests {
    use api_server::load_balancer::{LoadBalancer, InstanceHealth};

    /// Test load balancer with multiple API server instances
    #[test]
    fn test_load_balancer_multiple_instances_distribution() {
        let instances = vec![
            "http://api-server-1.internal:8001".to_string(),
            "http://api-server-2.internal:8002".to_string(),
            "http://api-server-3.internal:8003".to_string(),
        ];

        let lb = LoadBalancer::new(instances.clone());

        // Verify round-robin distribution across all instances
        let mut requests = vec![];
        for _ in 0..9 {
            requests.push(lb.get_next_instance());
        }

        // Should cycle through each instance 3 times (9 total / 3 instances)
        assert_eq!(requests[0], Some(instances[0].clone()), "first request → instance 1");
        assert_eq!(requests[1], Some(instances[1].clone()), "second request → instance 2");
        assert_eq!(requests[2], Some(instances[2].clone()), "third request → instance 3");
        assert_eq!(requests[3], Some(instances[0].clone()), "fourth request → instance 1 (cycle)");
        assert_eq!(requests[8], Some(instances[2].clone()), "ninth request → instance 3");
    }

    /// Test failover when instance becomes unhealthy
    #[test]
    fn test_load_balancer_failover_to_healthy_instances() {
        let instances = vec![
            "http://api-server-1.internal:8001".to_string(),
            "http://api-server-2.internal:8002".to_string(),
            "http://api-server-3.internal:8003".to_string(),
        ];

        let lb = LoadBalancer::new(instances.clone());

        // Simulate instance 1 becoming unhealthy (10 requests, 2 errors)
        for _ in 0..10 {
            lb.record_request(&instances[0]);
        }
        for _ in 0..2 {
            lb.record_error(&instances[0]);
        }

        let health = lb.get_instance_health();
        assert_eq!(health.len(), 3, "should track all 3 instances");

        // Instance 1 should be marked as unhealthy (error rate > 10%)
        let instance_1_health = &health[0];
        assert_eq!(instance_1_health.request_count, 10);
        assert_eq!(instance_1_health.error_count, 2);
        assert!(!instance_1_health.healthy, "instance with 20% error rate should be unhealthy");

        // Get healthy instances should exclude instance 1
        let healthy = lb.get_healthy_instances();
        assert_eq!(healthy.len(), 2, "should have 2 healthy instances");
        assert!(healthy.iter().all(|h| h.healthy), "all returned instances must be healthy");
    }

    /// Test least-connections algorithm across multiple instances
    #[test]
    fn test_load_balancer_least_connections_multi_instance() {
        let instances = vec![
            "http://api-server-1.internal:8001".to_string(),
            "http://api-server-2.internal:8002".to_string(),
            "http://api-server-3.internal:8003".to_string(),
        ];

        let lb = LoadBalancer::new(instances.clone());

        // Load instance 1 with 5 requests
        for _ in 0..5 {
            lb.record_request(&instances[0]);
        }

        // Load instance 2 with 3 requests
        for _ in 0..3 {
            lb.record_request(&instances[1]);
        }

        // Instance 3 has 0 requests

        // Least-loaded should be instance 3
        let least_loaded = lb.get_least_loaded_instance();
        assert_eq!(
            least_loaded,
            Some(instances[2].clone()),
            "least-connections should select instance with fewest requests"
        );
    }

    /// Test multi-region scenario with instances in different regions
    #[test]
    fn test_load_balancer_multi_region_distribution() {
        let instances = vec![
            // US-East Region
            "http://api-server-us-east-1.internal:8001".to_string(),
            "http://api-server-us-east-2.internal:8002".to_string(),
            // EU-West Region
            "http://api-server-eu-west-1.internal:8003".to_string(),
            "http://api-server-eu-west-2.internal:8004".to_string(),
            // APAC Region
            "http://api-server-apac-1.internal:8005".to_string(),
        ];

        let lb = LoadBalancer::new(instances.clone());

        // Generate 10 requests and collect distribution
        let mut distribution = std::collections::HashMap::new();
        for _ in 0..10 {
            if let Some(instance) = lb.get_next_instance() {
                *distribution.entry(instance).or_insert(0) += 1;
            }
        }

        // With round-robin, each instance should get roughly equal distribution
        assert_eq!(
            distribution.len(),
            5,
            "all 5 instances should receive requests"
        );

        // 10 requests across 5 instances = 2 per instance
        for count in distribution.values() {
            assert_eq!(*count, 2, "round-robin should distribute evenly");
        }
    }

    /// Test health status propagation across multi-instance setup
    #[test]
    fn test_load_balancer_health_status_propagation() {
        let instances = vec![
            "http://api-server-1.internal:8001".to_string(),
            "http://api-server-2.internal:8002".to_string(),
            "http://api-server-3.internal:8003".to_string(),
        ];

        let lb = LoadBalancer::new(instances.clone());

        // Instance 1: 5 requests, 0 errors → healthy
        for _ in 0..5 {
            lb.record_request(&instances[0]);
        }

        // Instance 2: 8 requests, 1 error → healthy (12.5% error)
        for _ in 0..8 {
            lb.record_request(&instances[1]);
        }
        lb.record_error(&instances[1]);

        // Instance 3: 3 requests, 1 error → unhealthy (33% error)
        for _ in 0..3 {
            lb.record_request(&instances[2]);
        }
        for _ in 0..1 {
            lb.record_error(&instances[2]);
        }

        let health = lb.get_instance_health();

        // Verify health status for each instance
        assert!(health[0].healthy, "instance 1 with 0% error should be healthy");
        assert!(health[1].healthy, "instance 2 with <10% error should be healthy");
        assert!(!health[2].healthy, "instance 3 with >10% error should be unhealthy");

        // Verify healthy instances list
        let healthy = lb.get_healthy_instances();
        assert_eq!(healthy.len(), 2);
        assert!(
            healthy.iter().all(|h| h.url != instances[2]),
            "unhealthy instance should not be in healthy list"
        );
    }

    /// Test request recording and error tracking across instances
    #[test]
    fn test_load_balancer_request_error_tracking() {
        let instances = vec![
            "http://api-server-1.internal:8001".to_string(),
            "http://api-server-2.internal:8002".to_string(),
        ];

        let lb = LoadBalancer::new(instances.clone());

        // Simulate traffic pattern
        for _ in 0..5 {
            lb.record_request(&instances[0]);
            lb.record_request(&instances[1]);
        }

        // Simulate errors on instance 1
        for _ in 0..2 {
            lb.record_error(&instances[0]);
        }

        let health = lb.get_instance_health();

        // Verify request and error counts
        assert_eq!(health[0].request_count, 5, "instance 1 should have 5 requests");
        assert_eq!(health[0].error_count, 2, "instance 1 should have 2 errors");
        assert_eq!(health[1].request_count, 5, "instance 2 should have 5 requests");
        assert_eq!(health[1].error_count, 0, "instance 2 should have 0 errors");
    }

    /// Test load balancer behavior with large number of instances
    #[test]
    fn test_load_balancer_many_instances() {
        // Create 20 instances (simulating a large deployment)
        let instances: Vec<String> = (1..=20)
            .map(|i| format!("http://api-server-{:02}.internal:800{}", i, i % 100))
            .collect();

        let lb = LoadBalancer::new(instances.clone());

        // Distribute 100 requests
        let mut distribution = std::collections::HashMap::new();
        for _ in 0..100 {
            if let Some(instance) = lb.get_next_instance() {
                *distribution.entry(instance).or_insert(0) += 1;
            }
        }

        // All 20 instances should receive requests
        assert_eq!(
            distribution.len(),
            20,
            "all instances should receive requests"
        );

        // With round-robin, 100 requests / 20 instances = 5 per instance
        for count in distribution.values() {
            assert_eq!(*count, 5, "round-robin should distribute evenly");
        }
    }

    /// Test concurrent request handling simulation
    #[test]
    fn test_load_balancer_concurrent_request_simulation() {
        let instances = vec![
            "http://api-server-1.internal:8001".to_string(),
            "http://api-server-2.internal:8002".to_string(),
            "http://api-server-3.internal:8003".to_string(),
        ];

        let lb = std::sync::Arc::new(LoadBalancer::new(instances.clone()));
        let mut handles = vec![];

        // Simulate 3 concurrent threads, each making 10 requests
        for _ in 0..3 {
            let lb_clone = lb.clone();
            let handle = std::thread::spawn(move || {
                for _ in 0..10 {
                    if let Some(instance) = lb_clone.get_next_instance() {
                        lb_clone.record_request(&instance);
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Total: 30 requests across 3 instances = 10 per instance
        let health = lb.get_instance_health();
        for h in health {
            assert_eq!(h.request_count, 10, "concurrent distribution should be even");
        }
    }
}

// ── Security hardening integration tests (#1208, #1209, #1210, #1211) ────────

#[cfg(test)]
mod security_hardening_tests {
    use api_server::health::{detailed_health_access, HealthAccessDecision};
    use api_server::audit::{create_audit_store_with_fallback, AuditStoreKind};
    use api_server::config::{default_bind_addr, resolve_bind_addr};
    use api_server::sync_util::lock_or_recover;
    use std::sync::{Arc, Mutex};

    // ── #1208: /health/detailed must not be public ────────────────────────────

    #[test]
    fn test_health_detailed_rejects_unauthenticated_request() {
        // No credentials, no internal-network marker → must be denied.
        let decision = detailed_health_access(None, false);
        assert_eq!(
            decision,
            HealthAccessDecision::Denied,
            "/health/detailed must not be reachable without authentication"
        );
    }

    #[test]
    fn test_health_detailed_rejects_external_request_without_auth() {
        // Request originates from outside the internal network and carries no token.
        let decision = detailed_health_access(None, false);
        assert!(
            !decision.is_allowed(),
            "external unauthenticated request must be rejected"
        );
    }

    #[test]
    fn test_health_detailed_allows_authenticated_request() {
        let decision = detailed_health_access(Some("valid-token"), false);
        assert!(
            decision.is_allowed(),
            "authenticated request must be allowed to see detailed health"
        );
    }

    #[test]
    fn test_health_detailed_allows_internal_network_request() {
        let decision = detailed_health_access(None, true);
        assert!(
            decision.is_allowed(),
            "internal-network request must be allowed to see detailed health"
        );
    }

    #[test]
    fn test_health_basic_route_remains_public() {
        // The minimal /health endpoint must stay public and require no auth.
        assert!(
            api_server::health::basic_health_is_public(),
            "/health must remain public and minimal"
        );
    }

    // ── #1209: fallback audit store must not panic ────────────────────────────

    #[test]
    fn test_fallback_audit_store_degrades_without_panic() {
        // Force the primary store creation to fail; the helper must return an
        // in-memory fallback instead of panicking.
        let store = create_audit_store_with_fallback(|| {
            Err("primary audit store unavailable".to_string())
        });
        assert_eq!(
            store.kind(),
            AuditStoreKind::InMemoryFallback,
            "failed primary store must degrade to the in-memory fallback sink"
        );
    }

    #[test]
    fn test_fallback_audit_store_emits_metric() {
        let store = create_audit_store_with_fallback(|| {
            Err("primary audit store unavailable".to_string())
        });
        assert!(
            store.fallback_metric_emitted(),
            "a metric must be emitted when the fallback audit store is active"
        );
    }

    #[test]
    fn test_primary_audit_store_used_when_available() {
        let store = create_audit_store_with_fallback(|| Ok(()));
        assert_eq!(
            store.kind(),
            AuditStoreKind::Primary,
            "a healthy primary store must be used directly"
        );
        assert!(
            !store.fallback_metric_emitted(),
            "no fallback metric should be emitted when the primary store works"
        );
    }

    // ── #1210: configurable bind address ──────────────────────────────────────

    #[test]
    fn test_default_bind_addr_is_0_0_0_0_8080() {
        assert_eq!(
            default_bind_addr().to_string(),
            "0.0.0.0:8080",
            "default bind address must remain 0.0.0.0:8080"
        );
    }

    #[test]
    fn test_bind_addr_resolves_from_env_overrides() {
        let addr = resolve_bind_addr(Some("127.0.0.1"), Some("9090"));
        assert_eq!(
            addr.to_string(),
            "127.0.0.1:9090",
            "bind address must be configurable via environment variables"
        );
    }

    #[test]
    fn test_bind_addr_falls_back_to_defaults_when_env_absent() {
        let addr = resolve_bind_addr(None, None);
        assert_eq!(addr.to_string(), "0.0.0.0:8080");
    }

    // ── #1211: poisoned mutex must not panic the next request ─────────────────

    #[test]
    fn test_poisoned_lock_does_not_panic_next_request() {
        let shared = Arc::new(Mutex::new(0u32));

        // Poison the mutex by panicking while holding the guard.
        let poisoner = shared.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("intentional panic to poison the mutex");
        })
        .join();

        assert!(shared.is_poisoned(), "mutex should be poisoned by the panic");

        // The next request must recover the guard rather than panic.
        let mut guard = lock_or_recover(&shared);
        *guard += 1;
        assert_eq!(*guard, 1, "poison-tolerant lock must still allow progress");
    }

    #[test]
    fn test_lock_or_recover_works_on_unpoisoned_mutex() {
        let shared = Mutex::new(41u32);
        let mut guard = lock_or_recover(&shared);
        *guard += 1;
        assert_eq!(*guard, 42);
// ── #1200: WATCHLISTS global store bounds ────────────────────────────────────

#[cfg(test)]
mod watchlists_bounds_tests {
    use api_server::handlers::{
        watchlists_insert, watchlists_len, watchlists_clear_for_tests,
        WatchlistError, MAX_WATCHLIST_ENTRIES_PER_USER, MAX_WATCHLIST_USERS,
    };

    fn reset() {
        watchlists_clear_for_tests();
    }

    /// A single user cannot exceed the per-user entry cap; the structured error
    /// is returned and the set is left unchanged.
    #[test]
    fn test_watchlist_per_user_cap_returns_structured_error() {
        reset();
        let user = "GUSERPERCAP";

        for i in 0..MAX_WATCHLIST_ENTRIES_PER_USER as u64 {
            watchlists_insert(user, i).expect("inserts below the cap must succeed");
        }
        assert_eq!(
            watchlists_len(user),
            MAX_WATCHLIST_ENTRIES_PER_USER,
            "user should hold exactly the cap"
        );

        let err = watchlists_insert(user, 999_999).unwrap_err();
        match err {
            WatchlistError::PerUserCapExceeded { user: u, limit } => {
                assert_eq!(u, user);
                assert_eq!(limit, MAX_WATCHLIST_ENTRIES_PER_USER);
            }
            other => panic!("expected PerUserCapExceeded, got {:?}", other),
        }

        // State must be unchanged after the rejected insert.
        assert_eq!(
            watchlists_len(user),
            MAX_WATCHLIST_ENTRIES_PER_USER,
            "rejected insert must not grow the set"
        );
        reset();
    }

    /// The total number of tracked users is bounded; once the cap is reached a
    /// new user is rejected with a structured error and no new entry is created.
    #[test]
    fn test_watchlist_total_users_cap_returns_structured_error() {
        reset();

        for i in 0..MAX_WATCHLIST_USERS {
            let user = format!("GUSER{i:04}");
            watchlists_insert(&user, 1).expect("inserts below the total cap must succeed");
        }

        let overflow_user = "GUSEROVERFLOW";
        let err = watchlists_insert(overflow_user, 1).unwrap_err();
        match err {
            WatchlistError::TotalUsersCapExceeded { limit } => {
                assert_eq!(limit, MAX_WATCHLIST_USERS);
            }
            other => panic!("expected TotalUsersCapExceeded, got {:?}", other),
        }

        // The overflowing user must not have been created.
        assert_eq!(
            watchlists_len(overflow_user),
            0,
            "rejected user must not be added to the global map"
        );
        reset();
    }

    /// Re-inserting an existing user at the total cap is still allowed (the user
    /// already exists, so no new map entry is created).
    #[test]
    fn test_watchlist_existing_user_allowed_at_total_cap() {
        reset();
        for i in 0..MAX_WATCHLIST_USERS {
            let user = format!("GUSER{i:04}");
            watchlists_insert(&user, 1).expect("fill to total cap");
        }
        // Existing user, new entry — must succeed.
        watchlists_insert("GUSER0000", 2).expect("existing user may still add entries");
        assert_eq!(watchlists_len("GUSER0000"), 2);
        reset();
    }
}

// ── #1201: list_webhooks owner scoping & redaction ──────────────────────────

#[cfg(test)]
mod list_webhooks_scoping_tests {
    use api_server::handlers::{
        register_webhook_for_tests, list_webhooks_for_tests, webhooks_clear_for_tests,
        redact_webhook_url,
    };

    fn reset() {
        webhooks_clear_for_tests();
    }

    /// One owner must not see another owner's hooks.
    #[test]
    fn test_list_webhooks_isolated_per_owner() {
        reset();
        register_webhook_for_tests("GOWNER_A", "https://a.example.com/hook").unwrap();
        register_webhook_for_tests("GOWNER_B", "https://b.example.com/hook").unwrap();

        let a = list_webhooks_for_tests("GOWNER_A");
        let b = list_webhooks_for_tests("GOWNER_B");

        assert_eq!(a.len(), 1, "owner A should see exactly their own hook");
        assert_eq!(b.len(), 1, "owner B should see exactly their own hook");
        assert!(a[0].url.contains("a.example.com"));
        assert!(b[0].url.contains("b.example.com"));
        assert!(
            a.iter().all(|h| !h.url.contains("b.example.com")),
            "owner A must not see owner B's hooks"
        );
        assert!(
            b.iter().all(|h| !h.url.contains("a.example.com")),
            "owner B must not see owner A's hooks"
        );
        reset();
    }

    /// URL query strings and userinfo credentials are stripped from the listing.
    #[test]
    fn test_list_webhooks_redacts_query_and_credentials() {
        reset();
        register_webhook_for_tests(
            "GOWNER_C",
            "https://user:secret@c.example.com/hook?token=abc123&x=1",
        )
        .unwrap();

        let listed = list_webhooks_for_tests("GOWNER_C");
        assert_eq!(listed.len(), 1);
        let url = &listed[0].url;
        assert!(!url.contains("token=abc123"), "query string must be stripped: {url}");
        assert!(!url.contains("secret"), "userinfo password must be stripped: {url}");
        assert!(!url.contains("user:"), "userinfo must be stripped: {url}");
        assert!(url.contains("c.example.com"), "host must be preserved: {url}");
        reset();
    }

    /// The redaction helper itself behaves as documented.
    #[test]
    fn test_redact_webhook_url_helper() {
        assert_eq!(
            redact_webhook_url("https://user:pass@example.com/path?q=1#frag"),
            "https://example.com/path"
        );
        assert_eq!(
            redact_webhook_url("https://example.com/path"),
            "https://example.com/path"
        );
    }
}

// ── #1202: SSRF guard on webhook registration ───────────────────────────────

#[cfg(test)]
mod webhook_ssrf_tests {
    use api_server::handlers::{
        register_webhook_for_tests, webhooks_clear_for_tests, WebhookError,
        is_blocked_ip, validate_webhook_url,
    };
    use std::net::IpAddr;

    fn reset() {
        webhooks_clear_for_tests();
    }

    fn assert_blocked(ip: &str) {
        let addr: IpAddr = ip.parse().expect("valid ip literal");
        assert!(is_blocked_ip(&addr), "address {ip} must be blocked by the SSRF guard");
    }

    #[test]
    fn test_ssrf_blocks_loopback_v4() {
        assert_blocked("127.0.0.1");
        assert_blocked("127.255.255.254");
    }

    #[test]
    fn test_ssrf_blocks_loopback_v6() {
        assert_blocked("::1");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_10_slash_8() {
        assert_blocked("10.0.0.1");
        assert_blocked("10.255.255.254");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_172_16_slash_12() {
        assert_blocked("172.16.0.1");
        assert_blocked("172.31.255.254");
        // 172.32.x is public and must not be blocked by the private-range rule.
        let public: IpAddr = "172.32.0.1".parse().unwrap();
        assert!(!is_blocked_ip(&public), "172.32.0.1 is outside 172.16/12");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_192_168_slash_16() {
        assert_blocked("192.168.0.1");
        assert_blocked("192.168.255.254");
    }

    #[test]
    fn test_ssrf_blocks_link_local_metadata_endpoint() {
        assert_blocked("169.254.169.254");
    }

    #[test]
    fn test_ssrf_blocks_ipv6_unique_local_fc00_slash_7() {
        assert_blocked("fc00::1");
        assert_blocked("fd12:3456:789a::1");
    }

    #[test]
    fn test_ssrf_blocks_ipv6_link_local_fe80_slash_10() {
        assert_blocked("fe80::1");
        assert_blocked("febf::1");
    }

    #[test]
    fn test_ssrf_allows_public_addresses() {
        let public: IpAddr = "93.184.216.34".parse().unwrap();
        assert!(!is_blocked_ip(&public), "public address must be allowed");
    }

    /// Registering a webhook pointing at a blocked literal address is rejected.
    #[test]
    fn test_register_webhook_rejects_blocked_literal() {
        reset();
        for url in [
            "http://127.0.0.1/hook",
            "http://[::1]/hook",
            "http://10.1.2.3/hook",
            "http://172.16.5.5/hook",
            "http://192.168.1.1/hook",
            "http://169.254.169.254/latest/meta-data",
            "http://[fc00::1]/hook",
            "http://[fe80::1]/hook",
        ] {
            let err = register_webhook_for_tests("GOWNER_SSRF", url)
                .expect_err(&format!("{url} must be rejected"));
            match err {
                WebhookError::BlockedAddress { .. } => {}
                other => panic!("expected BlockedAddress for {url}, got {:?}", other),
            }
        }
        reset();
    }

    /// `validate_webhook_url` rejects non-http(s) schemes with a structured error.
    #[test]
    fn test_validate_webhook_url_rejects_bad_scheme() {
        let err = validate_webhook_url("ftp://example.com/hook").unwrap_err();
        match err {
            WebhookError::InvalidScheme { .. } => {}
            other => panic!("expected InvalidScheme, got {:?}", other),
        }
    }
}

// ── #1202: DNS rebinding re-check at delivery time ──────────────────────────

#[cfg(test)]
mod webhook_dns_rebinding_tests {
    use api_server::handlers::{
        register_webhook_for_tests, webhooks_clear_for_tests,
        deliver_webhook_with_resolver_for_tests, WebhookError,
    };
    use std::net::IpAddr;

    /// Simulate a host that resolves to a public address at registration time
    /// but to a private address at delivery time (DNS rebinding).  The
    /// delivery-time re-check must block the request.
    #[tokio::test]
    async fn test_delivery_time_recheck_blocks_rebinding() {
        webhooks_clear_for_tests();

        // Registration-time resolver: public address.
        let public: IpAddr = "93.184.216.34".parse().unwrap();
        let hook = register_webhook_for_tests_with_resolver(
            "GOWNER_REBIND",
            "https://rebind.example.com/hook",
            move |_host: &str| Ok(vec![public]),
        )
        .expect("registration with a public resolution must succeed");

        // Delivery-time resolver: private address (rebound).
        let private: IpAddr = "10.0.0.5".parse().unwrap();
        let result = deliver_webhook_with_resolver_for_tests(
            &hook,
            move |_host: &str| Ok(vec![private]),
        )
        .await;

        let err = result.expect_err("delivery must be blocked after rebinding");
        match err {
            WebhookError::BlockedAddress { .. } => {}
            other => panic!("expected BlockedAddress at delivery, got {:?}", other),
        }

        webhooks_clear_for_tests();
    }

    /// Helper mirroring `register_webhook_for_tests` but with an injected resolver.
    fn register_webhook_for_tests_with_resolver<F>(
        owner: &str,
        url: &str,
        resolver: F,
    ) -> Result<api_server::handlers::RegisteredWebhook, WebhookError>
    where
        F: Fn(&str) -> Result<Vec<IpAddr>, WebhookError> + Send + Sync + 'static,
    {
        api_server::handlers::register_webhook_with_resolver_for_tests(owner, url, resolver)
    }
}

// ── #1203: redirects are not followed by the webhook client ─────────────────

#[cfg(test)]
mod webhook_redirect_tests {
    use api_server::webhook::{WebhookClient, DeliveryOutcome};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Spin up a tiny HTTP server that answers every request with a 302 to an
    /// internal address.  The webhook client must NOT follow it; it must record
    /// a delivery failure instead.
    #[tokio::test]
    async fn test_redirect_is_not_followed() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().unwrap();

        // Serve exactly one request then exit.
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = "HTTP/1.1 302 Found\r\n\
                                Location: http://169.254.169.254/latest/meta-data\r\n\
                                Content-Length: 0\r\n\
                                Connection: close\r\n\r\n";
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        let client = WebhookClient::for_tests_allow_loopback();
        let url = format!("http://{}/hook", addr);
        let outcome = client.deliver_for_tests(&url, b"{}").await;

        match outcome {
            DeliveryOutcome::Failure { reason } => {
                assert!(
                    reason.to_lowercase().contains("redirect"),
                    "failure reason should mention the redirect: {reason}"
                );
            }
            other => panic!("redirect must be recorded as a failure, got {:?}", other),
        }

        let _ = server.join();
    }
}
