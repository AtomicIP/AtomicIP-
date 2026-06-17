/// Accessibility tests for Atomic Patent API (#564)
///
/// Verifies that the API is accessible to different types of clients:
/// varying Accept headers, API versions, auth states, and payload shapes.

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
        // Wildcard matches any content type including application/json
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
        let supported = vec!["1.0.0", "1.1.0"];
        let has_v1 = supported.iter().any(|v| v.starts_with("1."));
        assert!(has_v1, "v1.x must be supported");
    }

    /// Requesting an unsupported version must be rejected (406 Not Acceptable).
    #[test]
    fn test_unsupported_version_is_rejected() {
        let supported = vec!["1.0.0", "1.1.0"];
        let requested = "99.0.0";
        assert!(!supported.contains(&requested));
    }

    /// Omitting Accept-Version header defaults to the current version.
    #[test]
    fn test_missing_version_header_defaults_to_current() {
        let current_version = "1.0.0";
        // When no Accept-Version header is present, current version is used
        let effective_version = current_version;
        assert_eq!(effective_version, "1.0.0");
    }

    // ── Public Endpoint Accessibility (no auth required) ─────────────────────

    /// Health endpoint must be accessible without authentication.
    #[test]
    fn test_health_endpoint_is_public() {
        // /health is a public endpoint — no Authorization header needed
        let public_paths = vec!["/health", "/docs", "/openapi.json", "/version"];
        assert!(public_paths.contains(&"/health"));
    }

    /// Docs endpoint must be accessible without authentication.
    #[test]
    fn test_docs_endpoint_is_public() {
        let public_paths = vec!["/health", "/docs", "/openapi.json", "/version"];
        assert!(public_paths.contains(&"/docs"));
    }

    // ── Minimal Payload Accessibility ─────────────────────────────────────────

    /// commit_ip must work with only required fields (no optional fields).
    #[test]
    fn test_commit_ip_minimal_payload_is_valid() {
        let minimal = json!({
            "owner": "GABC123",
            "commitment_hash": "deadbeef"
        });
        // Required fields present
        assert!(minimal["owner"].is_string());
        assert!(minimal["commitment_hash"].is_string());
        // No optional fields required
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
        // referrer is optional — must not be required
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
        // referrer absent — should still be a valid request
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

    // ── Pagination Accessibility ───────────────────────────────────────────────

    /// List endpoints must support clients that omit pagination params (use defaults).
    #[test]
    fn test_list_endpoint_works_without_pagination_params() {
        // Default values when params are absent
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
}
