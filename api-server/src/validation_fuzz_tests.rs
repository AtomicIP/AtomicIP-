#[cfg(test)]
mod fuzz_tests {
    use crate::validation::{RequestValidator, ValidationError, ErrorSeverity};

    /// Fuzz test: malformed addresses
    #[test]
    fn fuzz_stellar_address_malformed_inputs() {
        let malformed_addresses = vec![
            "",
            "G",
            "G1",
            "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZX", // Wrong checksum
            "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD ", // Trailing space
            " GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD", // Leading space
            "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD\n", // Newline
            "GBRPYHIL2CI3\0WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD", // Null byte
            "0BRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD", // Invalid first char
            "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZ", // Too short
            "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZDDD", // Too long (57 chars)
            "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD/", // Special char
        ];

        for addr in malformed_addresses {
            let result = RequestValidator::validate_stellar_address(addr, "address");
            assert!(result.is_err(), "Should reject: {}", addr);
        }
    }

    /// Fuzz test: boundary conditions on string length
    #[test]
    fn fuzz_string_length_boundary_conditions() {
        for len in [0, 1, 512, 511, 10000, 10001, 1000000] {
            let test_string = "a".repeat(len);
            let result = RequestValidator::validate_string_length(&test_string, 1, 512, "test_field");

            if len < 1 || len > 512 {
                assert!(result.is_err(), "Should reject string of length {}", len);
            } else {
                assert!(result.is_ok(), "Should accept string of length {}", len);
            }
        }
    }

    /// Fuzz test: null byte injection in various positions
    #[test]
    fn fuzz_null_byte_injection() {
        let injection_tests = vec![
            ("hello\0world", "middle"),
            ("\0hello", "start"),
            ("hello\0", "end"),
            ("he\0llo\0world", "multiple"),
        ];

        for (test_string, position) in injection_tests {
            let result = RequestValidator::check_null_bytes(test_string, "field");
            assert!(result.is_err(), "Should detect null byte injection at {}", position);

            if let Err(errors) = result {
                assert_eq!(errors[0].severity, ErrorSeverity::High, "Null byte should be high severity");
            }
        }
    }

    /// Fuzz test: hex string validation with edge cases
    #[test]
    fn fuzz_hex_string_edge_cases() {
        let test_cases = vec![
            ("", 0, false),
            ("0", 0, false),
            ("00", 1, true),
            ("FF", 1, true),
            ("ff", 1, true),
            ("Ff", 1, true),
            ("0123456789ABCDEF", 8, true),
            ("0123456789abcdef", 8, true),
            ("0123456789ABCDEFG", 8, false), // Invalid hex char
            ("0123456789ABCDE", 8, false), // Wrong length
            ("0123456789ABCDEF00", 8, false), // Too long
        ];

        for (hex_str, expected_bytes, should_pass) in test_cases {
            let result = RequestValidator::validate_hex_string(hex_str, expected_bytes, "hash");
            if should_pass {
                assert!(result.is_ok(), "Should accept valid hex: {}", hex_str);
            } else {
                assert!(result.is_err(), "Should reject invalid hex: {}", hex_str);
            }
        }
    }

    /// Fuzz test: amount validation with boundary values
    #[test]
    fn fuzz_amount_validation_boundaries() {
        let test_amounts = vec![
            (i128::MIN, false),
            (-1000, false),
            (-1, false),
            (0, false),
            (1, true),
            (1000, true),
            (i128::MAX, true),
        ];

        for (amount, should_pass) in test_amounts {
            let result = RequestValidator::validate_positive_integer(amount, "amount");
            if should_pass {
                assert!(result.is_ok(), "Should accept positive amount: {}", amount);
            } else {
                assert!(result.is_err(), "Should reject non-positive amount: {}", amount);
            }
        }
    }

    /// Fuzz test: timestamp validation with edge cases
    #[test]
    fn fuzz_timestamp_validation_boundaries() {
        let test_timestamps = vec![
            (0, false), // Too old
            (100, false), // Still too old
            (946684799, false), // Just before 2000
            (946684800, true), // 2000-01-01
            (1672531200, true), // 2023-01-01
            (4102444800, true), // 2100-01-01
            (4102444801, false), // Just after 2100
            (u64::MAX, false), // Way too far in future
        ];

        for (timestamp, should_pass) in test_timestamps {
            let result = RequestValidator::validate_timestamp(timestamp, "timestamp");
            if should_pass {
                assert!(result.is_ok(), "Should accept timestamp: {}", timestamp);
            } else {
                assert!(result.is_err(), "Should reject timestamp: {}", timestamp);
            }
        }
    }

    /// Fuzz test: array length validation
    #[test]
    fn fuzz_array_length_validation() {
        // Test with different array sizes
        for size in [0, 1, 500, 1000, 1001, 2000] {
            let array: Vec<u64> = (0..size as u64).collect();
            let result = RequestValidator::validate_non_empty_vec(&array, "ids");

            if size == 0 || size > 1000 {
                assert!(result.is_err(), "Should reject array of size {}", size);
            } else {
                assert!(result.is_ok(), "Should accept array of size {}", size);
            }
        }
    }

    /// Fuzz test: URL validation with various protocols and patterns
    #[test]
    fn fuzz_url_validation_edge_cases() {
        let long_url = format!("http://{}", "a".repeat(1000));
        let test_urls = vec![
            ("", false),
            ("http://", true),
            ("https://", true),
            ("http://example.com", true),
            ("https://example.com", true),
            ("http://example.com/path", true),
            ("https://example.com:8080/path?query=value", true),
            ("ftp://example.com", false),
            ("example.com", false),
            ("//example.com", false),
            ("http://example.com\0malicious", false),
            (long_url.as_str(), false), // Exceeds OWASP length limit
        ];

        for (url, should_pass) in test_urls {
            let result = RequestValidator::validate_url(url);
            if should_pass {
                assert!(result.is_ok(), "Should accept URL: {}", url);
            } else {
                assert!(result.is_err(), "Should reject URL: {}", url);
            }
        }
    }

    /// Fuzz test: combined validation errors
    #[test]
    fn fuzz_multiple_validation_errors() {
        let invalid_address = "INVALID";
        let invalid_hash = "XYZ";
        let invalid_amount = -100i128;

        let result1 = RequestValidator::validate_stellar_address(invalid_address, "address");
        let result2 = RequestValidator::validate_hex_string(invalid_hash, 16, "hash");
        let result3 = RequestValidator::validate_positive_integer(invalid_amount, "amount");

        let combined = RequestValidator::combine_results(vec![result1, result2, result3]);
        assert!(combined.is_err(), "Should have combined errors");

        if let Err(errors) = combined {
            assert_eq!(errors.len(), 3, "Should have 3 errors");
        }
    }

    /// Fuzz test: special characters and encoding attacks
    #[test]
    fn fuzz_special_characters_and_encoding() {
        let malicious_inputs = vec![
            "<script>alert('xss')</script>",
            "'; DROP TABLE users; --",
            "../../etc/passwd",
            "\x00\x01\x02\x03",
            "\\x00\\x01",
            "%00%01",
            "unicode:\u{202E}", // Right-to-left override
            "emoji:😀😁😂",
            "\t\n\r",
        ];

        for input in malicious_inputs {
            // Most of these should be caught by string length or null byte checks
            let _ = RequestValidator::validate_non_empty_string(input, "field");
        }
    }

    /// Fuzz test: amount range validation
    #[test]
    fn fuzz_amount_range_validation() {
        let test_cases = vec![
            (50, 0, 100, true),
            (0, 0, 100, true),
            (100, 0, 100, true),
            (-1, 0, 100, false),
            (101, 0, 100, false),
            (1000, 0, 100, false),
            (i128::MAX, i128::MIN, i128::MAX, true),
        ];

        for (value, min, max, should_pass) in test_cases {
            let result = RequestValidator::validate_amount_range(value, min, max, "amount");
            if should_pass {
                assert!(result.is_ok(), "Should accept {} in range [{}, {}]", value, min, max);
            } else {
                assert!(result.is_err(), "Should reject {} outside range [{}, {}]", value, min, max);
            }
        }
    }

    /// Fuzz test: non-negative integer validation
    #[test]
    fn fuzz_non_negative_validation() {
        let test_values = vec![
            (i128::MIN, false),
            (-1, false),
            (0, true),
            (1, true),
            (i128::MAX, true),
        ];

        for (value, should_pass) in test_values {
            let result = RequestValidator::validate_non_negative_integer(value, "value");
            if should_pass {
                assert!(result.is_ok(), "Should accept non-negative: {}", value);
            } else {
                assert!(result.is_err(), "Should reject negative: {}", value);
            }
        }
    }

    // ── Per-schema integration fuzz tests (#886) ──────────────────────────────
    //
    // These tests verify that `RequestValidator` correctly accepts / rejects
    // fully-assembled request bodies for every public endpoint schema defined in
    // schemas.rs.  Each test exercises the same code path that the HTTP handler
    // uses, so a crash here maps directly to a production crash.

    /// CommitIpRequest — fuzz owner + commitment_hash together
    #[test]
    fn fuzz_commit_ip_request_schema() {
        use crate::schemas::CommitIpRequest;

        let valid_address = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";
        let valid_hash    = "a".repeat(64); // 32 bytes hex

        // Valid request — must pass
        let req = CommitIpRequest {
            owner: valid_address.to_string(),
            commitment_hash: valid_hash.clone(),
        };
        assert!(RequestValidator::validate_stellar_address(&req.owner, "owner").is_ok());
        assert!(RequestValidator::validate_hex_string(&req.commitment_hash, 32, "commitment_hash").is_ok());

        // Bad owner variants
        for bad_owner in &["", "short", "not_a_stellar_addr", &"G".repeat(57)] {
            assert!(
                RequestValidator::validate_stellar_address(bad_owner, "owner").is_err(),
                "Should reject bad owner: {bad_owner}"
            );
        }

        // Bad commitment_hash variants
        for bad_hash in &[
            "",
            "ZZ",                   // non-hex chars
            &"a".repeat(63),        // 31 bytes — too short
            &"a".repeat(65),        // 32.5 bytes — wrong parity
            &"a".repeat(128),       // 64 bytes — too long
        ] {
            assert!(
                RequestValidator::validate_hex_string(bad_hash, 32, "commitment_hash").is_err(),
                "Should reject bad hash: {bad_hash}"
            );
        }
    }

    /// TransferIpRequest — fuzz ip_id + new_owner
    #[test]
    fn fuzz_transfer_ip_request_schema() {
        use crate::schemas::TransferIpRequest;

        let valid_address = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";

        // Valid new_owner
        let req = TransferIpRequest {
            ip_id: 1,
            new_owner: valid_address.to_string(),
        };
        assert!(req.ip_id > 0);
        assert!(RequestValidator::validate_stellar_address(&req.new_owner, "new_owner").is_ok());

        // ip_id = 0 is a sentinel "not found" value — treat as non-positive
        assert!(
            RequestValidator::validate_positive_integer(0_i128, "ip_id").is_err(),
            "ip_id 0 must be rejected"
        );

        // Bad new_owner
        assert!(RequestValidator::validate_stellar_address("", "new_owner").is_err());
        assert!(RequestValidator::validate_stellar_address("\0bad", "new_owner").is_err());
    }

    /// VerifyCommitmentRequest — fuzz secret + blinding_factor together
    #[test]
    fn fuzz_verify_commitment_request_schema() {
        use crate::schemas::VerifyCommitmentRequest;

        let valid_hex = "b".repeat(64); // 32 bytes

        let req = VerifyCommitmentRequest {
            ip_id: 42,
            secret: valid_hex.clone(),
            blinding_factor: valid_hex.clone(),
        };
        assert!(RequestValidator::validate_hex_string(&req.secret, 32, "secret").is_ok());
        assert!(RequestValidator::validate_hex_string(&req.blinding_factor, 32, "blinding_factor").is_ok());

        // Mismatched length for secret
        for bad in &["", &"b".repeat(63), &"g".repeat(64)] {
            assert!(
                RequestValidator::validate_hex_string(bad, 32, "secret").is_err(),
                "Should reject bad secret: {bad}"
            );
        }
        // Null byte injection in blinding_factor
        assert!(RequestValidator::check_null_bytes("abc\0def", "blinding_factor").is_err());
    }

    /// InitiateSwapRequest — fuzz all address fields and price
    #[test]
    fn fuzz_initiate_swap_request_schema() {
        use crate::schemas::InitiateSwapRequest;

        let addr = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";

        let req = InitiateSwapRequest {
            ip_registry_id: "registry_id_123".to_string(),
            ip_id: 1,
            seller: addr.to_string(),
            price: 1_000_000,
            buyer: addr.to_string(),
            token: addr.to_string(),
            referrer: None,
        };

        assert!(RequestValidator::validate_stellar_address(&req.seller, "seller").is_ok());
        assert!(RequestValidator::validate_stellar_address(&req.buyer, "buyer").is_ok());
        assert!(RequestValidator::validate_stellar_address(&req.token, "token").is_ok());
        assert!(RequestValidator::validate_positive_integer(req.price, "price").is_ok());

        // Price boundary tests
        assert!(RequestValidator::validate_positive_integer(0, "price").is_err());
        assert!(RequestValidator::validate_positive_integer(-1, "price").is_err());
        assert!(RequestValidator::validate_positive_integer(i128::MIN, "price").is_err());
        assert!(RequestValidator::validate_positive_integer(1, "price").is_ok());

        // XSS / injection in ip_registry_id
        let evil_id = "<script>alert(1)</script>";
        let len = evil_id.len();
        // Must not exceed string length limits (1–512)
        assert!(RequestValidator::validate_string_length(evil_id, 1, 512, "ip_registry_id").is_ok(),
            "Length check passes but caller must sanitise: len={len}");
        // Null byte injection is always rejected
        assert!(RequestValidator::check_null_bytes("id\0evil", "ip_registry_id").is_err());

        // Referrer is optional but must be a valid address if present
        assert!(RequestValidator::validate_stellar_address(addr, "referrer").is_ok());
        assert!(RequestValidator::validate_stellar_address("not_valid", "referrer").is_err());
    }

    /// BatchInitiateSwapRequest — fuzz ip_ids and prices arrays
    #[test]
    fn fuzz_batch_initiate_swap_request_schema() {
        use crate::schemas::BatchInitiateSwapRequest;

        let addr = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";

        // Valid request with matching arrays
        let req = BatchInitiateSwapRequest {
            ip_registry_id: "reg".to_string(),
            ip_ids: vec![1, 2, 3],
            seller: addr.to_string(),
            prices: vec![100, 200, 300],
            buyer: addr.to_string(),
            token: addr.to_string(),
            referrer: None,
            idempotency_key: None,
        };
        let ip_ids_u64: Vec<u64> = req.ip_ids.iter().map(|&id| id as u64).collect();
        assert!(RequestValidator::validate_non_empty_vec(&ip_ids_u64, "ip_ids").is_ok());
        for &p in &req.prices {
            assert!(RequestValidator::validate_positive_integer(p, "price").is_ok());
        }

        // Empty ip_ids array
        let empty: Vec<u64> = vec![];
        assert!(RequestValidator::validate_non_empty_vec(&empty, "ip_ids").is_err());

        // Oversized array (>1000)
        let huge: Vec<u64> = (0..1001).collect();
        assert!(RequestValidator::validate_non_empty_vec(&huge, "ip_ids").is_err());

        // A price of 0 in the batch
        assert!(RequestValidator::validate_positive_integer(0_i128, "prices[0]").is_err());

        // Idempotency key must respect length limits when present
        let short_key = "key_123";
        assert!(RequestValidator::validate_string_length(short_key, 1, 512, "idempotency_key").is_ok());
        let long_key = "k".repeat(513);
        assert!(RequestValidator::validate_string_length(&long_key, 1, 512, "idempotency_key").is_err());
    }

    /// AcceptSwapRequest — fuzz buyer field
    #[test]
    fn fuzz_accept_swap_request_schema() {
        use crate::schemas::AcceptSwapRequest;

        let addr = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";

        let req = AcceptSwapRequest { buyer: addr.to_string() };
        assert!(RequestValidator::validate_stellar_address(&req.buyer, "buyer").is_ok());

        let bad_buyers = ["", " ", "\t", "0AAAA", &"G".repeat(60)];
        for bad in &bad_buyers {
            assert!(
                RequestValidator::validate_stellar_address(bad, "buyer").is_err(),
                "Should reject buyer: {bad}"
            );
        }
    }

    /// RevealKeyRequest — fuzz secret + blinding_factor
    #[test]
    fn fuzz_reveal_key_request_schema() {
        use crate::schemas::RevealKeyRequest;

        let addr     = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";
        let hex32    = "c".repeat(64);

        let req = RevealKeyRequest {
            caller: addr.to_string(),
            secret: hex32.clone(),
            blinding_factor: hex32.clone(),
        };
        assert!(RequestValidator::validate_stellar_address(&req.caller, "caller").is_ok());
        assert!(RequestValidator::validate_hex_string(&req.secret, 32, "secret").is_ok());
        assert!(RequestValidator::validate_hex_string(&req.blinding_factor, 32, "blinding_factor").is_ok());

        // Non-hex secret
        assert!(RequestValidator::validate_hex_string("ZZZZ", 2, "secret").is_err());

        // Null byte in blinding_factor
        assert!(RequestValidator::check_null_bytes("abc\0", "blinding_factor").is_err());
    }

    /// CancelSwapRequest + CancelExpiredSwapRequest — fuzz canceller/caller
    #[test]
    fn fuzz_cancel_swap_request_schema() {
        use crate::schemas::{CancelSwapRequest, CancelExpiredSwapRequest};

        let addr = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";

        let cancel = CancelSwapRequest { canceller: addr.to_string() };
        assert!(RequestValidator::validate_stellar_address(&cancel.canceller, "canceller").is_ok());

        let expired = CancelExpiredSwapRequest { caller: addr.to_string() };
        assert!(RequestValidator::validate_stellar_address(&expired.caller, "caller").is_ok());

        // Address with trailing newline
        assert!(RequestValidator::validate_stellar_address(&format!("{addr}\n"), "canceller").is_err());
    }

    /// RegisterWebhookRequest — fuzz url + events
    #[test]
    fn fuzz_register_webhook_request_schema() {
        use crate::schemas::RegisterWebhookRequest;

        let req = RegisterWebhookRequest {
            url: "https://example.com/webhook".to_string(),
            events: vec!["swap.completed".to_string(), "ip.committed".to_string()],
        };

        assert!(RequestValidator::validate_url(&req.url).is_ok());
        let urls_u64: Vec<u64> = (0..req.events.len() as u64).collect();
        assert!(RequestValidator::validate_non_empty_vec(&urls_u64, "events").is_ok());

        // Bad URL schemes
        for bad_url in &["ftp://x.com", "javascript:alert(1)", "", "not_a_url", "//x.com"] {
            assert!(RequestValidator::validate_url(bad_url).is_err(), "Bad URL: {bad_url}");
        }

        // Null byte in URL
        assert!(RequestValidator::check_null_bytes("https://x.com/\0", "url").is_err());

        // Empty events array → treated as non-empty vec with length 0 for validation purposes
        let empty: Vec<u64> = vec![];
        assert!(RequestValidator::validate_non_empty_vec(&empty, "events").is_err());

        // Oversized events (>1000)
        let huge: Vec<u64> = (0..1001).collect();
        assert!(RequestValidator::validate_non_empty_vec(&huge, "events").is_err());

        // Event name with null byte
        assert!(RequestValidator::check_null_bytes("swap\0.completed", "events[0]").is_err());
    }

    /// BulkCommitIpRequest — fuzz owner + commitment_hashes array
    #[test]
    fn fuzz_bulk_commit_ip_request_schema() {
        use crate::schemas::BulkCommitIpRequest;

        let addr      = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";
        let valid_hash = "d".repeat(64);

        let req = BulkCommitIpRequest {
            owner: addr.to_string(),
            commitment_hashes: vec![valid_hash.clone()],
        };
        assert!(RequestValidator::validate_stellar_address(&req.owner, "owner").is_ok());
        let hashes_as_ids: Vec<u64> = (0..req.commitment_hashes.len() as u64).collect();
        assert!(RequestValidator::validate_non_empty_vec(&hashes_as_ids, "commitment_hashes").is_ok());

        // Each hash must be exactly 32 bytes hex
        for bad in &["", &"d".repeat(63), "notHex!", &"d".repeat(65)] {
            assert!(
                RequestValidator::validate_hex_string(bad, 32, "commitment_hashes[i]").is_err(),
                "Bad hash: {bad}"
            );
        }

        // Empty hashes array
        let empty: Vec<u64> = vec![];
        assert!(RequestValidator::validate_non_empty_vec(&empty, "commitment_hashes").is_err());
    }

    /// BulkInitiateSwapRequest — fuzz mismatched ip_ids / prices lengths
    #[test]
    fn fuzz_bulk_initiate_swap_request_schema() {
        use crate::schemas::BulkInitiateSwapRequest;

        let addr = "GAHJJJKMOKYE4RVPZEWZTKH5FVI4PA3VL7GK2LFNUBSGBZE5HAHD8X";

        let req = BulkInitiateSwapRequest {
            ip_registry_id: "reg".to_string(),
            ip_ids: vec![1, 2],
            seller: addr.to_string(),
            prices: vec![100, 200],
            buyer: addr.to_string(),
            token: addr.to_string(),
            referrer: None,
        };
        let ids: Vec<u64> = req.ip_ids.iter().map(|&id| id as u64).collect();
        assert!(RequestValidator::validate_non_empty_vec(&ids, "ip_ids").is_ok());
        for &p in &req.prices {
            assert!(RequestValidator::validate_positive_integer(p, "prices[i]").is_ok());
        }

        // Negative price in batch
        assert!(RequestValidator::validate_positive_integer(-1_i128, "prices[i]").is_err());

        // Oversized ip_ids
        let huge: Vec<u64> = (0..1001).collect();
        assert!(RequestValidator::validate_non_empty_vec(&huge, "ip_ids").is_err());
    }

    /// PaginationParams — fuzz limit / offset boundary values
    #[test]
    fn fuzz_pagination_params_schema() {
        // limit: default 50, max 200
        let valid_limits = [1u64, 50, 200];
        let invalid_limits: &[i128] = &[0, -1, 201, i128::MAX];

        for &l in &valid_limits {
            assert!(
                RequestValidator::validate_amount_range(l as i128, 1, 200, "limit").is_ok(),
                "Should accept limit: {l}"
            );
        }
        for &l in invalid_limits {
            assert!(
                RequestValidator::validate_amount_range(l, 1, 200, "limit").is_err(),
                "Should reject limit: {l}"
            );
        }

        // offset: default 0, no upper bound (non-negative)
        assert!(RequestValidator::validate_non_negative_integer(0_i128, "offset").is_ok());
        assert!(RequestValidator::validate_non_negative_integer(-1_i128, "offset").is_err());
    }

    /// CursorPaginationParams — fuzz cursor field injection
    #[test]
    fn fuzz_cursor_pagination_params_schema() {
        // A None cursor is always valid
        let no_cursor: Option<&str> = None;
        assert!(no_cursor.is_none());

        // Cursor strings must not contain null bytes
        let cursors_with_nulls = ["abc\0def", "\0", "cursor\0"];
        for c in &cursors_with_nulls {
            assert!(
                RequestValidator::check_null_bytes(c, "cursor").is_err(),
                "Null byte in cursor must be rejected: {c}"
            );
        }

        // Cursor must respect max string length
        let long_cursor = "c".repeat(513);
        assert!(
            RequestValidator::validate_string_length(&long_cursor, 1, 512, "cursor").is_err()
        );
        let ok_cursor = "c".repeat(512);
        assert!(
            RequestValidator::validate_string_length(&ok_cursor, 1, 512, "cursor").is_ok()
        );
    }
}
