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
}
