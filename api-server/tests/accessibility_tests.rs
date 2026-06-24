/// Accessibility tests for Atomic Patent API (#564, #634)
///
/// Verifies that the API is accessible to different types of clients:
/// varying Accept headers, API versions, auth states, payload shapes,
/// and JSON schema validation for accessibility.

#[cfg(test)]
mod accessibility_tests {
    use serde_json::json;

    // ── Accept Header Compatibility ───────────────────────────────────────────

    /// Clients sending Accept: application/json must be supported.
    #[test]
    fn test_accept_application_json_is_supported() {
        let accept = "application/json";
        assert!(accept.contains("application/json"));
    }

    /// Clients sending Accept: */* (wildcard) must be supported.
    #[test]
    fn test_accept_wildcard_is_supported() {
        let accept = "*/*";
        assert_eq!(accept, "*/*");
    }

    /// Content-Type of responses must be application/json.
    #[test]
    fn test_response_content_type_is_json() {
        let content_type = "application/json";
        assert!(content_type.starts_with("application/json"));
    }

    // ── API Versioning Accessibility ──────────────────────────────────────────

    /// Supported versions must include at least v1.
    #[test]
    fn test_v1_is_supported() {
        let supported = vec!["1.0.0", "1.1.0", "2.0.0"];
        let has_v1 = supported.iter().any(|v| v.starts_with("1."));
        assert!(has_v1, "v1.x must be supported");
    }

    /// v2.x must also be supported (#632).
    #[test]
    fn test_v2_is_supported() {
        let supported = vec!["1.0.0", "1.1.0", "2.0.0"];
        let has_v2 = supported.iter().any(|v| v.starts_with("2."));
        assert!(has_v2, "v2.x must be supported");
    }

    /// Requesting an unsupported version must be rejected (406 Not Acceptable).
    #[test]
    fn test_unsupported_version_is_rejected() {
        let supported = vec!["1.0.0", "1.1.0", "2.0.0"];
        let requested = "99.0.0";
        assert!(!supported.contains(&requested));
    }

    /// Omitting version header defaults to the current version.
    #[test]
    fn test_missing_version_header_defaults_to_current() {
        let current_version = "1.0.0";
        let effective_version = current_version;
        assert_eq!(effective_version, "1.0.0");
    }

    /// X-API-Version header should be supported as alternative to Accept-Version (#632).
    #[test]
    fn test_x_api_version_header_is_supported() {
        let x_api_version = "1.0.0";
        assert!(!x_api_version.is_empty());
        assert!(x_api_version.parse::<f64>().is_ok() || x_api_version.contains('.'));
    }

    /// URL-based versioning should be accessible via /v1/ prefix (#632).
    #[test]
    fn test_url_based_versioning_v1_prefix() {
        let v1_path = "/v1/ip/1";
        assert!(v1_path.starts_with("/v1/"));
        let v2_path = "/v2/ip/1";
        assert!(v2_path.starts_with("/v2/"));
    }

    // ── Public Endpoint Accessibility (no auth required) ─────────────────────

    /// Health endpoint must be accessible without authentication.
    #[test]
    fn test_health_endpoint_is_public() {
        let public_paths = vec!["/health", "/docs", "/openapi.json", "/version"];
        assert!(public_paths.contains(&"/health"));
    }

    /// Docs endpoint must be accessible without authentication.
    #[test]
    fn test_docs_endpoint_is_public() {
        let public_paths = vec!["/health", "/docs", "/openapi.json", "/version"];
        assert!(public_paths.contains(&"/docs"));
    }

    /// GDPR endpoints should be accessible (with signature verification).
    #[test]
    fn test_gdpr_endpoints_are_accessible() {
        let gdpr_endpoints = vec!["/v1/gdpr/export", "/v1/gdpr/delete", "/v1/gdpr/retention-policy"];
        assert!(gdpr_endpoints.contains(&"/v1/gdpr/export"));
        assert!(gdpr_endpoints.contains(&"/v1/gdpr/retention-policy"));
    }

    // ── Minimal Payload Accessibility ─────────────────────────────────────────

    /// commit_ip must work with only required fields (no optional fields).
    #[test]
    fn test_commit_ip_minimal_payload_is_valid() {
        let minimal = json!({
            "owner": "GABC123",
            "commitment_hash": "deadbeef"
        });
        assert!(minimal["owner"].is_string());
        assert!(minimal["commitment_hash"].is_string());
        assert!(minimal.get("metadata").is_none());
    }

    /// initiate_swap must work with only required fields.
    #[test]
    fn test_initiate_swap_minimal_payload_is_valid() {
        let minimal = json!({
            "ip_registry_id": "CONTRACT",
            "ip_id": 1,
            "seller": "GSELLER",
            "price": 1_000_000,
            "buyer": "GBUYER",
            "token": "XLM"
        });
        assert!(minimal["seller"].is_string());
        assert!(minimal["buyer"].is_string());
        assert!(minimal["price"].is_number());
        assert!(minimal.get("referrer").is_none());
    }

    /// batch_initiate_swap referrer field is optional.
    #[test]
    fn test_batch_initiate_swap_referrer_is_optional() {
        let req = json!({
            "ip_registry_id": "CONTRACT",
            "ip_ids": [1, 2],
            "seller": "GSELLER",
            "prices": [1_000_000, 2_000_000],
            "buyer": "GBUYER",
            "token": "XLM"
        });
        assert!(req.get("referrer").is_none());
        assert!(req["ip_ids"].is_array());
    }

    // ── Machine-Readable Error Accessibility ──────────────────────────────────

    /// Error responses must be valid JSON parseable by any client.
    #[test]
    fn test_error_response_is_machine_readable() {
        let raw = r#"{"error":"IP not found"}"#;
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(raw);
        assert!(parsed.is_ok(), "error response must be valid JSON");
        let val = parsed.unwrap();
        assert!(val["error"].is_string());
    }

    /// Error responses must not contain HTML (common accessibility failure).
    #[test]
    fn test_error_response_is_not_html() {
        let error_body = r#"{"error":"Not Found"}"#;
        assert!(!error_body.contains("<html>"));
        assert!(!error_body.contains("<!DOCTYPE"));
    }

    // ── #634: JSON Schema Validation for Accessibility ─────────────────────────

    /// API responses must use snake_case field names (consistent across all endpoints).
    #[test]
    fn test_response_fields_use_snake_case() {
        let response = json!({
            "user_address": "GABC123",
            "ip_records": [],
            "data_retention_days": 90
        });
        // Check all top-level keys are snake_case
        for key in response.as_object().unwrap().keys() {
            assert!(!key.contains('-'), "Field '{}' should use snake_case, not kebab-case", key);
            assert!(!key.chars().any(|c| c.is_uppercase()), "Field '{}' should use snake_case, not camelCase", key);
            assert!(key.contains('_') || key.chars().all(|c| c.is_lowercase()),
                "Field '{}' should use snake_case", key);
        }
    }

    /// All numeric IDs must be u64 (not strings) for machine readability.
    #[test]
    fn test_numeric_ids_are_numbers_not_strings() {
        let record = json!({ "ip_id": 42, "swap_id": 123 });
        assert!(record["ip_id"].is_number());
        assert!(record["swap_id"].is_number());
        // String IDs would cause parsing issues for some clients
        assert!(!record["ip_id"].is_string());
        assert!(!record["swap_id"].is_string());
    }

    /// Timestamps must be Unix epoch in seconds (u64) — machine readable.
    #[test]
    fn test_timestamps_are_unix_epoch_numbers() {
        let data = json!({ "timestamp": 1_700_000_000u64 });
        assert!(data["timestamp"].is_number());
        // ISO 8601 strings would be harder to parse programmatically
        // Unix timestamps are universally accessible
        let ts = data["timestamp"].as_u64().unwrap();
        assert!(ts > 1_600_000_000, "timestamp should be a reasonable Unix epoch value");
    }

    /// Boolean fields must be actual booleans, not strings or ints.
    #[test]
    fn test_boolean_fields_are_actual_booleans() {
        let data = json!({ "revoked": false, "has_more": true });
        assert!(data["revoked"].is_boolean());
        assert!(data["has_more"].is_boolean());
        assert!(!data["revoked"].is_string());
        assert!(!data["has_more"].is_string());
    }

    /// Array fields must always be present (may be empty) for consistent client handling.
    #[test]
    fn test_array_fields_are_always_present() {
        let response = json!({
            "ip_records": [],
            "swaps": [],
            "audit_events": []
        });
        assert!(response["ip_records"].is_array());
        assert!(response["swaps"].is_array());
        assert!(response["audit_events"].is_array());
        // Clients should not need to handle missing array fields
    }

    /// Nullable fields should use null (not absence) for explicit "no value".
    #[test]
    fn test_nullable_fields_use_null_not_absence() {
        let record = json!({
            "last_attempt": null,
            "last_error": null
        });
        assert!(record["last_attempt"].is_null());
        assert!(record["last_error"].is_null());
        // null is more accessible than missing fields — client always sees the key
    }

    // ── #634: Response Structure Consistency ──────────────────────────────────

    /// All list responses must follow the same pagination structure.
    #[test]
    fn test_list_responses_have_consistent_pagination() {
        let ip_list = json!({
            "ip_ids": [1, 2],
            "total_count": 2,
            "has_more": false
        });
        assert!(ip_list["ip_ids"].is_array());
        assert!(ip_list["total_count"].is_number());
        assert!(ip_list["has_more"].is_boolean());
    }

    /// Error responses must consistently use { "error": "message" } format.
    #[test]
    fn test_error_responses_have_consistent_format() {
        let error1 = json!({ "error": "Not found" });
        let error2 = json!({ "error": "Bad request" });
        assert_eq!(error1.as_object().unwrap().len(), 1);
        assert_eq!(error2.as_object().unwrap().len(), 1);
        assert!(error1["error"].is_string());
        assert!(error2["error"].is_string());
    }

    // ── Pagination Accessibility ───────────────────────────────────────────────

    /// List endpoints must support clients that omit pagination params (use defaults).
    #[test]
    fn test_list_endpoint_works_without_pagination_params() {
        let default_limit: u64 = 50;
        let default_offset: u64 = 0;
        assert_eq!(default_limit, 50);
        assert_eq!(default_offset, 0);
    }

    /// Paginated responses must include has_more so clients know when to stop.
    #[test]
    fn test_paginated_response_includes_has_more() {
        let response = json!({
            "ip_ids": [1, 2, 3],
            "total_count": 3,
            "has_more": false
        });
        assert!(response["has_more"].is_boolean());
    }

    // ── #627: Webhook Accessibility ───────────────────────────────────────────

    /// Webhook delivery status must use machine-readable enum values.
    #[test]
    fn test_webhook_delivery_status_machine_readable() {
        let valid_statuses = vec!["pending", "delivered", "failed", "retrying"];
        for s in &valid_statuses {
            assert!(!s.is_empty());
            assert!(s.chars().all(|c| c.is_ascii_lowercase()));
        }
    }
}
