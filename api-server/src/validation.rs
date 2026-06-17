use serde_json::Value;
use std::collections::HashMap;

/// Validation error details
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

/// Result of validation
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validator trait for custom validation logic
pub trait Validator {
    fn validate(&self) -> ValidationResult;
}

/// Centralized validation framework
pub struct RequestValidator;

impl RequestValidator {
    /// Validate a Stellar address format
    pub fn validate_stellar_address(address: &str) -> ValidationResult {
        if address.is_empty() {
            return Err(vec![ValidationError {
                field: "address".to_string(),
                message: "Address cannot be empty".to_string(),
            }]);
        }
        if !address.starts_with('G') || address.len() != 56 {
            return Err(vec![ValidationError {
                field: "address".to_string(),
                message: "Invalid Stellar address format".to_string(),
            }]);
        }
        Ok(())
    }

    /// Validate hex-encoded string of specific length
    pub fn validate_hex_string(value: &str, expected_bytes: usize) -> ValidationResult {
        if value.is_empty() {
            return Err(vec![ValidationError {
                field: "hex_string".to_string(),
                message: "Hex string cannot be empty".to_string(),
            }]);
        }
        if value.len() != expected_bytes * 2 {
            return Err(vec![ValidationError {
                field: "hex_string".to_string(),
                message: format!("Expected {} bytes (hex: {} chars), got {}", expected_bytes, expected_bytes * 2, value.len()),
            }]);
        }
        if !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(vec![ValidationError {
                field: "hex_string".to_string(),
                message: "Invalid hex characters".to_string(),
            }]);
        }
        Ok(())
    }

    /// Validate non-empty string
    pub fn validate_non_empty_string(value: &str, field_name: &str) -> ValidationResult {
        if value.is_empty() {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("{} cannot be empty", field_name),
            }]);
        }
        Ok(())
    }

    /// Validate positive integer
    pub fn validate_positive_integer(value: i128, field_name: &str) -> ValidationResult {
        if value <= 0 {
            return Err(vec![ValidationError {
                field: field_name.to_string(),
                message: format!("{} must be positive", field_name),
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
                message: format!("{} and {} must have the same length", field1, field2),
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
                }]);
            }
        }
        Ok(())
    }

    /// Validate URL format
    pub fn validate_url(url: &str) -> ValidationResult {
        if url.is_empty() {
            return Err(vec![ValidationError {
                field: "url".to_string(),
                message: "URL cannot be empty".to_string(),
            }]);
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(vec![ValidationError {
                field: "url".to_string(),
                message: "URL must start with http:// or https://".to_string(),
            }]);
        }
        Ok(())
    }

    /// Validate UUID format
    pub fn validate_uuid(uuid_str: &str) -> ValidationResult {
        if uuid::Uuid::parse_str(uuid_str).is_err() {
            return Err(vec![ValidationError {
                field: "uuid".to_string(),
                message: "Invalid UUID format".to_string(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_stellar_address_valid() {
        let addr = "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD";
        assert!(RequestValidator::validate_stellar_address(addr).is_ok());
    }

    #[test]
    fn test_validate_stellar_address_invalid() {
        assert!(RequestValidator::validate_stellar_address("").is_err());
        assert!(RequestValidator::validate_stellar_address("INVALID").is_err());
    }

    #[test]
    fn test_validate_hex_string_valid() {
        let hex = "0123456789abcdef0123456789abcdef";
        assert!(RequestValidator::validate_hex_string(hex, 16).is_ok());
    }

    #[test]
    fn test_validate_hex_string_invalid_length() {
        let hex = "0123456789abcdef";
        assert!(RequestValidator::validate_hex_string(hex, 32).is_err());
    }

    #[test]
    fn test_validate_hex_string_invalid_chars() {
        let hex = "0123456789abcdefGGGGGGGGGGGGGGGG";
        assert!(RequestValidator::validate_hex_string(hex, 16).is_err());
    }

    #[test]
    fn test_validate_positive_integer() {
        assert!(RequestValidator::validate_positive_integer(100, "price").is_ok());
        assert!(RequestValidator::validate_positive_integer(0, "price").is_err());
        assert!(RequestValidator::validate_positive_integer(-1, "price").is_err());
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
}
