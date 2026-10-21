use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

const MAX_STRING_LENGTH: usize = 10000;
const MAX_ARRAY_LENGTH: usize = 1000;
const OWASP_MAX_FIELD_LENGTH: usize = 512;

/// Validation error details with severity level
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub severity: ErrorSeverity,
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// High severity: likely a security or correctness issue
    High,
    /// Medium severity: data quality issue
    Medium,
    /// Low severity: minor issue
    Low,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{:?}] {}: {}",
            self.severity, self.field, self.message
        )
    }
}

/// Result of validation
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Composable validation rules trait for flexible validator chaining
pub trait ValidationRules: Send + Sync {
    fn validate(&self) -> ValidationResult;
    fn chain(self, other: Box<dyn ValidationRules>) -> ChainedRules
    where
        Self: 'static + Sized,
    {
        ChainedRules {
            rules: vec![Box::new(self), other],
        }
    }
}

/// Chained validation rules
pub struct ChainedRules {
    rules: Vec<Box<dyn ValidationRules>>,
}

impl ValidationRules for ChainedRules {
    fn validate(&self) -> ValidationResult {
        let mut all_errors = Vec::new();
        for rule in &self.rules {
            if let Err(errors) = rule.validate() {
                all_errors.extend(errors);
            }
        }
        if all_errors.is_empty() {
            Ok(())
        } else {
            Err(all_errors)
        }
    }
}

/// Address validation rule
pub struct AddressValidationRule {
    pub address: String,
    pub field_name: String,
}

impl ValidationRules for AddressValidationRule {
    fn validate(&self) -> ValidationResult {
        RequestValidator::validate_stellar_address(&self.address, &self.field_name)
    }
}

/// Hash validation rule
pub struct HashValidationRule {
    pub hash: String,
    pub expected_bytes: usize,
    pub field_name: String,
}

impl ValidationRules for HashValidationRule {
    fn validate(&self) -> ValidationResult {
        RequestValidator::validate_hex_string(&self.hash, self.expected_bytes, &self.field_name)
    }
}

/// Amount validation rule
pub struct AmountValidationRule {
    pub amount: i128,
    pub field_name: String,
}

impl ValidationRules for AmountValidationRule {
    fn validate(&self) -> ValidationResult {
        RequestValidator::validate_positive_integer(self.amount, &self.field_name)
    }
}

/// Timestamp validation rule
pub struct TimestampValidationRule {
    pub timestamp: u64,
    pub field_name: String,
}

impl ValidationRules for TimestampValidationRule {
    fn validate(&self) -> ValidationResult {
        RequestValidator::validate_timestamp(self.timestamp, &self.field_name)
    }
}

/// String length validation rule
pub struct StringLengthValidationRule {
    pub value: String,
    pub min_length: usize,
    pub max_length: usize,
    pub field_name: String,
}

impl ValidationRules for StringLengthValidationRule {
    fn validate(&self) -> ValidationResult {
        RequestValidator::validate_string_length(
            &self.value,
            self.min_length,
            self.max_length,
            &self.field_name,
        )
    }
}

/// Centralized validation framework with OWASP compliance
pub struct RequestValidator;

impl RequestValidator {
    /// Check for null bytes (OWASP: Injection Prevention)
    pub fn check_null_bytes(value: &str, field_name: &str) -> ValidationResult {
        if value.contains('\0') {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: "Null byte injection detected".to_string(),
                severity: ErrorSeverity::High,
            }]);
        }
        Ok(())
    }

    /// Validate string length with bounds
    pub fn validate_string_length(
        value: &str,
        min_length: usize,
        max_length: usize,
        field_name: &str,
    ) -> ValidationResult {
        let len = value.len();
        if len < min_length {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "{} length {} is below minimum {}",
                    field_name, len, min_length
                ),
                severity: ErrorSeverity::Medium,
            }]);
        }
        if len > max_length {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "{} length {} exceeds maximum {}",
                    field_name, len, max_length
                ),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Self::check_null_bytes(value, field_name)?;
        Ok(())
    }

    /// Validate a Stellar address format
    pub fn validate_stellar_address(address: &str, field_name: &str) -> ValidationResult {
        Self::check_null_bytes(address, field_name)?;

        if address.is_empty() {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: "Address cannot be empty".to_string(),
                severity: ErrorSeverity::High,
            }]);
        }

        if address.len() > OWASP_MAX_FIELD_LENGTH {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "Address length {} exceeds maximum {}",
                    address.len(),
                    OWASP_MAX_FIELD_LENGTH
                ),
                severity: ErrorSeverity::High,
            }]);
        }

        if !address.starts_with('G') || address.len() != 56 {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: "Invalid Stellar address format".to_string(),
                severity: ErrorSeverity::High,
            }]);
        }

        // Validate base32 characters
        if !address.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: "Address contains invalid characters".to_string(),
                severity: ErrorSeverity::High,
            }]);
        }

        Ok(())
    }

    /// Validate hex-encoded string of specific length
    pub fn validate_hex_string(
        value: &str,
        expected_bytes: usize,
        field_name: &str,
    ) -> ValidationResult {
        Self::check_null_bytes(value, field_name)?;

        if value.is_empty() {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: "Hex string cannot be empty".to_string(),
                severity: ErrorSeverity::High,
            }]);
        }

        if value.len() > MAX_STRING_LENGTH {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "Hex string length {} exceeds maximum {}",
                    value.len(),
                    MAX_STRING_LENGTH
                ),
                severity: ErrorSeverity::High,
            }]);
        }

        let expected_len = expected_bytes * 2;
        if value.len() != expected_len {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "Expected {} bytes (hex: {} chars), got {}",
                    expected_bytes,
                    expected_len,
                    value.len()
                ),
                severity: ErrorSeverity::Medium,
            }]);
        }

        if !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: "Invalid hex characters".to_string(),
                severity: ErrorSeverity::High,
            }]);
        }

        Ok(())
    }

    /// Validate non-empty string
    pub fn validate_non_empty_string(value: &str, field_name: &str) -> ValidationResult {
        Self::check_null_bytes(value, field_name)?;

        if value.is_empty() {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("{} cannot be empty", field_name),
                severity: ErrorSeverity::Medium,
            }]);
        }

        if value.len() > MAX_STRING_LENGTH {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "{} length {} exceeds maximum {}",
                    field_name,
                    value.len(),
                    MAX_STRING_LENGTH
                ),
                severity: ErrorSeverity::High,
            }]);
        }

        Ok(())
    }

    /// Validate positive integer (amount)
    pub fn validate_positive_integer(value: i128, field_name: &str) -> ValidationResult {
        if value <= 0 {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("{} must be positive", field_name),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Ok(())
    }

    /// Validate non-negative integer
    pub fn validate_non_negative_integer(value: i128, field_name: &str) -> ValidationResult {
        if value < 0 {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("{} must be non-negative", field_name),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Ok(())
    }

    /// Validate amount is within range
    pub fn validate_amount_range(
        value: i128,
        min: i128,
        max: i128,
        field_name: &str,
    ) -> ValidationResult {
        if value < min || value > max {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "{} must be between {} and {}, got {}",
                    field_name, min, max, value
                ),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Ok(())
    }

    /// Validate timestamp (must be a reasonable Unix timestamp)
    pub fn validate_timestamp(timestamp: u64, field_name: &str) -> ValidationResult {
        // Ensure timestamp is in reasonable range (after 2000-01-01 and before 2100-01-01)
        const MIN_TIMESTAMP: u64 = 946684800; // 2000-01-01
        const MAX_TIMESTAMP: u64 = 4102444800; // 2100-01-01

        if timestamp < MIN_TIMESTAMP || timestamp > MAX_TIMESTAMP {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "{} timestamp {} is outside reasonable range",
                    field_name, timestamp
                ),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Ok(())
    }

    /// Validate non-empty vector
    pub fn validate_non_empty_vec<T>(vec: &[T], field_name: &str) -> ValidationResult {
        if vec.is_empty() {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("{} cannot be empty", field_name),
                severity: ErrorSeverity::Medium,
            }]);
        }

        if vec.len() > MAX_ARRAY_LENGTH {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!(
                    "{} length {} exceeds maximum {}",
                    field_name,
                    vec.len(),
                    MAX_ARRAY_LENGTH
                ),
                severity: ErrorSeverity::High,
            }]);
        }

        Ok(())
    }

    /// Validate matching lengths of two vectors
    pub fn validate_matching_lengths(
        vec1: &[impl std::fmt::Debug],
        vec2: &[impl std::fmt::Debug],
        field1: &str,
        field2: &str,
    ) -> ValidationResult {
        if vec1.len() != vec2.len() {
            return Err(vec![ValidationError {
                field: format!("{} and {}", field1, field2),
                message: format!(
                    "{} and {} must have the same length ({} vs {})",
                    field1,
                    field2,
                    vec1.len(),
                    vec2.len()
                ),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Ok(())
    }

    /// Validate no duplicates in vector
    pub fn validate_no_duplicates(vec: &[u64], field_name: &str) -> ValidationResult {
        let mut seen = std::collections::HashSet::new();
        for &item in vec {
            if !seen.insert(item) {
                return Err(vec![ValidationError {
                    field: field_name.to_string(),
                    message: format!("Duplicate value {} in {}", item, field_name),
                    severity: ErrorSeverity::Medium,
                }]);
            }
        }
        Ok(())
    }

    /// Validate URL format
    pub fn validate_url(url: &str) -> ValidationResult {
        Self::check_null_bytes(url, "url")?;

        if url.is_empty() {
            return Err(vec![ValidationError {
                field: "url".to_string(),
                message: "URL cannot be empty".to_string(),
                severity: ErrorSeverity::Medium,
            }]);
        }

        if url.len() > OWASP_MAX_FIELD_LENGTH {
            return Err(vec![ValidationError {
                field: "url".to_string(),
                message: format!(
                    "URL length {} exceeds maximum {}",
                    url.len(),
                    OWASP_MAX_FIELD_LENGTH
                ),
                severity: ErrorSeverity::High,
            }]);
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(vec![ValidationError {
                field: "url".to_string(),
                message: "URL must start with http:// or https://".to_string(),
                severity: ErrorSeverity::Medium,
            }]);
        }

        Ok(())
    }

    /// Validate a webhook URL against SSRF: requires http/https and rejects
    /// hosts that are literal IPs in loopback, private, link-local,
    /// unique-local, unspecified, or multicast ranges.
    pub fn validate_webhook_url(url: &str) -> ValidationResult {
        Self::validate_url(url)?;

        let host = match extract_url_host(url) {
            Some(host) => host,
            None => {
                return Err(vec![ValidationError {
                    field: "url".to_string(),
                    message: "URL is missing a valid host".to_string(),
                    severity: ErrorSeverity::High,
                }]);
            }
        };

        // Strip IPv6 brackets if present.
        let host = host.trim_start_matches('[').trim_end_matches(']');

        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            if is_blocked_ip(&ip) {
                return Err(vec![ValidationError {
                    field: "url".to_string(),
                    message: format!("URL host {} is not allowed (internal address)", ip),
                    severity: ErrorSeverity::High,
                }]);
            }
        }

        Ok(())
    }

    /// Validate UUID format
    pub fn validate_uuid(uuid_str: &str) -> ValidationResult {
        Self::check_null_bytes(uuid_str, "uuid")?;

        if uuid::Uuid::parse_str(uuid_str).is_err() {
            return Err(vec![ValidationError {
                field: "uuid".to_string(),
                message: "Invalid UUID format".to_string(),
                severity: ErrorSeverity::Medium,
            }]);
        }
        Ok(())
    }

    /// Combine multiple validation results
    pub fn combine_results(results: Vec<ValidationResult>) -> ValidationResult {
        let mut all_errors = Vec::new();
        for result in results {
            if let Err(errors) = result {
                all_errors.extend(errors);
            }
        }
        if all_errors.is_empty() {
            Ok(())
        } else {
            Err(all_errors)
        }
    }

    /// Get high-severity errors only
    pub fn filter_high_severity_errors(errors: &[ValidationError]) -> Vec<ValidationError> {
        errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::High)
            .cloned()
            .collect()
    }
}

/// Extract the host portion of an http/https URL without external crates.
fn extract_url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;

    // Authority ends at the first '/', '?' or '#'.
    let authority = rest
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()
        .unwrap_or("");

    if authority.is_empty() {
        return None;
    }

    // Strip userinfo.
    let host_port = match authority.rsplit_once('@') {
        Some((_, hp)) => hp,
        None => authority,
    };

    // IPv6 literal: [::1]:8080
    if let Some(stripped) = host_port.strip_prefix('[') {
        let end = stripped.find(']')?;
        return Some(format!("[{}]", &stripped[..end]));
    }

    // Strip port for hostnames / IPv4.
    let host = match host_port.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host_port,
    };

    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Returns true if the IP is in a range that must not be reachable via webhooks.
fn is_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // 0.0.0.0/8
                || v4.octets()[0] == 0
                // 100.64.0.0/10 (carrier-grade NAT)
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
                // 192.0.0.0/24
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0)
                // 198.18.0.0/15 (benchmarking)
                || (v4.octets()[0] == 198 && (v4.octets()[1] == 18 || v4.octets()[1] == 19))
                // 240.0.0.0/4 (reserved)
                || v4.octets()[0] >= 240
        }
        std::net::IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return true;
            }
            let segments = v6.segments();
            // Unique local fc00::/7
            if (segments[0] & 0xfe00) == 0xfc00 {
                return true;
            }
            // Link-local fe80::/10
            if (segments[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            // IPv4-mapped ::ffff:0:0/96
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(&std::net::IpAddr::V4(v4));
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_stellar_address_valid() {
        let addr = "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3XVQCRWGSGAX";
        assert!(RequestValidator::validate_stellar_address(addr, "address").is_ok());
    }

    #[test]
    fn test_validate_stellar_address_invalid() {
        assert!(RequestValidator::validate_stellar_address("", "address").is_err());
        assert!(RequestValidator::validate_stellar_address("INVALID", "address").is_err());
    }

    #[test]
    fn test_validate_stellar_address_null_byte() {
        let addr = "GBRPYHIL2CI3\0WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD";
        let result = RequestValidator::validate_stellar_address(addr, "address");
        assert!(result.is_err());
        if let Err(errors) = result {
            assert_eq!(errors[0].severity, ErrorSeverity::High);
        }
    }

    #[test]
    fn test_validate_hex_string_valid() {
        let hex = "0123456789abcdef0123456789abcdef";
        assert!(RequestValidator::validate_hex_string(hex, 16, "hash").is_ok());
    }

    #[test]
    fn test_validate_hex_string_invalid_length() {
        let hex = "0123456789abcdef";
        assert!(RequestValidator::validate_hex_string(hex, 32, "hash").is_err());
    }

    #[test]
    fn test_validate_hex_string_invalid_chars() {
        let hex = "0123456789abcdefGGGGGGGGGGGGGGGG";
        assert!(RequestValidator::validate_hex_string(hex, 16, "hash").is_err());
    }

    #[test]
    fn test_validate_positive_integer() {
        assert!(RequestValidator::validate_positive_integer(100, "price").is_ok());
        assert!(RequestValidator::validate_positive_integer(0, "price").is_err());
        assert!(RequestValidator::validate_positive_integer(-1, "price").is_err());
    }

    #[test]
    fn test_validate_amount_range() {
        assert!(RequestValidator::validate_amount_range(50, 0, 100, "amount").is_ok());
        assert!(RequestValidator::validate_amount_range(-1, 0, 100, "amount").is_err());
        assert!(RequestValidator::validate_amount_range(101, 0, 100, "amount").is_err());
    }

    #[test]
    fn test_validate_timestamp() {
        let valid_timestamp: u64 = 1672531200; // 2023-01-01
        assert!(RequestValidator::validate_timestamp(valid_timestamp, "timestamp").is_ok());

        let invalid_timestamp: u64 = 100;
        assert!(RequestValidator::validate_timestamp(invalid_timestamp, "timestamp").is_err());
    }

    #[test]
    fn test_validate_string_length() {
        assert!(RequestValidator::validate_string_length("hello", 1, 10, "name").is_ok());
        assert!(RequestValidator::validate_string_length("", 1, 10, "name").is_err());
        assert!(RequestValidator::validate_string_length("hello world", 1, 5, "name").is_err());
    }

    #[test]
    fn test_check_null_bytes() {
        assert!(RequestValidator::check_null_bytes("hello", "field").is_ok());
        assert!(RequestValidator::check_null_bytes("hello\0world", "field").is_err());
    }

    #[test]
    fn test_validate_no_duplicates() {
        assert!(RequestValidator::validate_no_duplicates(&[1, 2, 3], "ids").is_ok());
        assert!(RequestValidator::validate_no_duplicates(&[1, 2, 1], "ids").is_err());
    }

    #[test]
    fn test_validate_url() {
        assert!(RequestValidator::validate_url("https://example.com").is_ok());
        assert!(RequestValidator::validate_url("http://example.com").is_ok());
        assert!(RequestValidator::validate_url("ftp://example.com").is_err());
        assert!(RequestValidator::validate_url("").is_err());
    }

    #[test]
    fn test_validate_webhook_url_allows_public() {
        assert!(RequestValidator::validate_webhook_url("https://example.com/hook").is_ok());
        assert!(RequestValidator::validate_webhook_url("http://8.8.8.8/hook").is_ok());
        assert!(RequestValidator::validate_webhook_url("https://[2001:4860:4860::8888]/hook").is_ok());
    }

    #[test]
    fn test_validate_webhook_url_rejects_loopback() {
        assert!(RequestValidator::validate_webhook_url("http://127.0.0.1/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://127.1.2.3:8080/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://[::1]/hook").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_private() {
        assert!(RequestValidator::validate_webhook_url("http://10.0.0.1/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://172.16.0.1/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://192.168.1.1/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://[fc00::1]/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://[fd12:3456::1]/hook").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_link_local() {
        assert!(RequestValidator::validate_webhook_url("http://169.254.169.254/latest").is_err());
        assert!(RequestValidator::validate_webhook_url("http://[fe80::1]/hook").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_unspecified_and_multicast() {
        assert!(RequestValidator::validate_webhook_url("http://0.0.0.0/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://224.0.0.1/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://[::]/hook").is_err());
        assert!(RequestValidator::validate_webhook_url("http://[ff02::1]/hook").is_err());
    }

    #[test]
    fn test_validate_webhook_url_rejects_bad_scheme() {
        assert!(RequestValidator::validate_webhook_url("ftp://example.com").is_err());
        assert!(RequestValidator::validate_webhook_url("").is_err());
    }

    #[test]
    fn test_validation_rule_address() {
        let rule = AddressValidationRule {
            address: "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3XVQCRWGSGAX".to_string(),
            field_name: "address".to_string(),
        };
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_validation_rule_hash() {
        let rule = HashValidationRule {
            hash: "0123456789abcdef0123456789abcdef".to_string(),
            expected_bytes: 16,
            field_name: "hash".to_string(),
        };
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_validation_rule_amount() {
        let rule = AmountValidationRule {
            amount: 100,
            field_name: "amount".to_string(),
        };
        assert!(rule.validate().is_ok());

        let rule_invalid = AmountValidationRule {
            amount: 0,
            field_name: "amount".to_string(),
        };
        assert!(rule_invalid.validate().is_err());
    }

    #[test]
    fn test_error_severity_levels() {
        let high_error = ValidationError {
            field: "field".to_string(),
            message: "error".to_string(),
            severity: ErrorSeverity::High,
        };
        let medium_error = ValidationError {
            field: "field".to_string(),
            message: "error".to_string(),
            severity: ErrorSeverity::Medium,
        };
        assert_eq!(high_error.severity, ErrorSeverity::High);
        assert_eq!(medium_error.severity, ErrorSeverity::Medium);
    }
}
