use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Standard API response wrapper for all endpoints
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiResponse<T> {
    /// HTTP status code
    pub status: u16,
    /// Human-readable message
    pub message: String,
    /// Response data (null for errors or empty responses)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error details (only present on error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetails>,
    /// Request metadata
    pub meta: ResponseMeta,
}

/// Error details in standardized format
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetails {
    /// Error code for programmatic handling
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Field-level validation errors
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, Vec<String>>>,
}

/// Response metadata
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResponseMeta {
    /// Request ID for tracing
    pub request_id: String,
    /// Unix timestamp of response
    pub timestamp: u64,
    /// API version
    pub version: String,
}

/// Pagination metadata for list responses
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginationMeta {
    /// Total number of items
    pub total: u64,
    /// Current page number (1-indexed)
    pub page: u64,
    /// Items per page
    pub per_page: u64,
    /// Total number of pages
    pub total_pages: u64,
    /// Whether there are more pages
    pub has_next: bool,
    /// Whether there are previous pages
    pub has_prev: bool,
}

/// Paginated API response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginatedApiResponse<T> {
    /// HTTP status code
    pub status: u16,
    /// Human-readable message
    pub message: String,
    /// Response data
    pub data: Vec<T>,
    /// Pagination metadata
    pub pagination: PaginationMeta,
    /// Request metadata
    pub meta: ResponseMeta,
}

/// Response formatter for consistent API responses
pub struct ResponseFormatter;

impl ResponseFormatter {
    /// Create a successful response
    pub fn success<T: Serialize>(
        data: T,
        message: impl Into<String>,
    ) -> ApiResponse<T> {
        ApiResponse {
            status: 200,
            message: message.into(),
            data: Some(data),
            error: None,
            meta: Self::create_meta(),
        }
    }

    /// Create a created response (201)
    pub fn created<T: Serialize>(
        data: T,
        message: impl Into<String>,
    ) -> ApiResponse<T> {
        ApiResponse {
            status: 201,
            message: message.into(),
            data: Some(data),
            error: None,
            meta: Self::create_meta(),
        }
    }

    /// Create an accepted response (202)
    pub fn accepted<T: Serialize>(
        data: T,
        message: impl Into<String>,
    ) -> ApiResponse<T> {
        ApiResponse {
            status: 202,
            message: message.into(),
            data: Some(data),
            error: None,
            meta: Self::create_meta(),
        }
    }

    /// Create a no-content response (204)
    pub fn no_content() -> (u16, String) {
        (204, "No content".to_string())
    }

    /// Create a bad request error response (400)
    pub fn bad_request(
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> ApiResponse<()> {
        ApiResponse {
            status: 400,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: code.into(),
                message: message.into(),
                details: None,
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create a bad request error with field details
    pub fn bad_request_with_details(
        message: impl Into<String>,
        code: impl Into<String>,
        details: HashMap<String, Vec<String>>,
    ) -> ApiResponse<()> {
        ApiResponse {
            status: 400,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: code.into(),
                message: message.into(),
                details: Some(details),
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create an unauthorized error response (401)
    pub fn unauthorized(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            status: 401,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: "UNAUTHORIZED".to_string(),
                message: message.into(),
                details: None,
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create a forbidden error response (403)
    pub fn forbidden(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            status: 403,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: "FORBIDDEN".to_string(),
                message: message.into(),
                details: None,
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create a not found error response (404)
    pub fn not_found(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            status: 404,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: "NOT_FOUND".to_string(),
                message: message.into(),
                details: None,
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create a conflict error response (409)
    pub fn conflict(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            status: 409,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: "CONFLICT".to_string(),
                message: message.into(),
                details: None,
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create an internal server error response (500)
    pub fn internal_error(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            status: 500,
            message: message.into(),
            data: None,
            error: Some(ErrorDetails {
                code: "INTERNAL_ERROR".to_string(),
                message: message.into(),
                details: None,
            }),
            meta: Self::create_meta(),
        }
    }

    /// Create a paginated response
    pub fn paginated<T: Serialize>(
        data: Vec<T>,
        total: u64,
        page: u64,
        per_page: u64,
        message: impl Into<String>,
    ) -> PaginatedApiResponse<T> {
        let total_pages = (total + per_page - 1) / per_page;
        let has_next = page < total_pages;
        let has_prev = page > 1;

        PaginatedApiResponse {
            status: 200,
            message: message.into(),
            data,
            pagination: PaginationMeta {
                total,
                page,
                per_page,
                total_pages,
                has_next,
                has_prev,
            },
            meta: Self::create_meta(),
        }
    }

    /// Create response metadata
    fn create_meta() -> ResponseMeta {
        ResponseMeta {
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            version: "1.0.0".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_response() {
        let response = ResponseFormatter::success("test_data", "Success");
        assert_eq!(response.status, 200);
        assert_eq!(response.message, "Success");
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_created_response() {
        let response = ResponseFormatter::created(42, "Created");
        assert_eq!(response.status, 201);
        assert_eq!(response.message, "Created");
    }

    #[test]
    fn test_bad_request_response() {
        let response = ResponseFormatter::bad_request("Invalid input", "INVALID_INPUT");
        assert_eq!(response.status, 400);
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, "INVALID_INPUT");
    }

    #[test]
    fn test_not_found_response() {
        let response = ResponseFormatter::not_found("Resource not found");
        assert_eq!(response.status, 404);
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, "NOT_FOUND");
    }

    #[test]
    fn test_paginated_response() {
        let data = vec![1, 2, 3];
        let response = ResponseFormatter::paginated(data, 100, 1, 10, "Success");
        assert_eq!(response.status, 200);
        assert_eq!(response.pagination.total, 100);
        assert_eq!(response.pagination.page, 1);
        assert_eq!(response.pagination.per_page, 10);
        assert_eq!(response.pagination.total_pages, 10);
        assert!(response.pagination.has_next);
        assert!(!response.pagination.has_prev);
    }

    #[test]
    fn test_paginated_response_last_page() {
        let data = vec![1, 2, 3];
        let response = ResponseFormatter::paginated(data, 30, 3, 10, "Success");
        assert!(!response.pagination.has_next);
        assert!(response.pagination.has_prev);
    }

    // ── Consistency Tests for Error Response Shape ──────────────────────────

    #[test]
    fn test_error_response_canonical_shape_bad_request() {
        let response = ResponseFormatter::bad_request("Invalid input", "INVALID_INPUT");

        // Verify canonical shape
        assert_eq!(response.status, 400);
        assert!(response.error.is_some());
        assert!(response.data.is_none());

        let error = response.error.unwrap();
        assert!(!error.code.is_empty(), "Error code must not be empty");
        assert!(!error.message.is_empty(), "Error message must not be empty");
        assert_eq!(error.code, "INVALID_INPUT");
        assert_eq!(error.message, "Invalid input");
    }

    #[test]
    fn test_error_response_canonical_shape_unauthorized() {
        let response = ResponseFormatter::unauthorized("Not authenticated");

        assert_eq!(response.status, 401);
        assert!(response.error.is_some());
        assert!(response.data.is_none());

        let error = response.error.unwrap();
        assert_eq!(error.code, "UNAUTHORIZED");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn test_error_response_canonical_shape_forbidden() {
        let response = ResponseFormatter::forbidden("Access denied");

        assert_eq!(response.status, 403);
        assert!(response.error.is_some());
        assert!(response.data.is_none());

        let error = response.error.unwrap();
        assert_eq!(error.code, "FORBIDDEN");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn test_error_response_canonical_shape_not_found() {
        let response = ResponseFormatter::not_found("Resource not found");

        assert_eq!(response.status, 404);
        assert!(response.error.is_some());
        assert!(response.data.is_none());

        let error = response.error.unwrap();
        assert_eq!(error.code, "NOT_FOUND");
        assert_eq!(error.message, "Resource not found");
    }

    #[test]
    fn test_error_response_canonical_shape_conflict() {
        let response = ResponseFormatter::conflict("Resource already exists");

        assert_eq!(response.status, 409);
        assert!(response.error.is_some());
        assert!(response.data.is_none());

        let error = response.error.unwrap();
        assert_eq!(error.code, "CONFLICT");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn test_error_response_canonical_shape_internal_error() {
        let response = ResponseFormatter::internal_error("Internal server error");

        assert_eq!(response.status, 500);
        assert!(response.error.is_some());
        assert!(response.data.is_none());

        let error = response.error.unwrap();
        assert_eq!(error.code, "INTERNAL_ERROR");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn test_error_response_always_has_error_details() {
        // All error responses must have ErrorDetails
        let responses: Vec<ApiResponse<()>> = vec![
            ResponseFormatter::bad_request("Error", "ERROR"),
            ResponseFormatter::unauthorized("Unauth"),
            ResponseFormatter::forbidden("Forbidden"),
            ResponseFormatter::not_found("Not found"),
            ResponseFormatter::conflict("Conflict"),
            ResponseFormatter::internal_error("Internal"),
        ];

        for response in responses {
            assert!(
                response.error.is_some(),
                "Error response must always have error details"
            );
            assert!(
                response.data.is_none(),
                "Error response must not have data field populated"
            );
            assert!(
                response.status >= 400,
                "Error response must have 4xx or 5xx status"
            );

            let error = response.error.unwrap();
            assert!(
                !error.code.is_empty(),
                "Error code must not be empty in error response"
            );
            assert!(
                !error.message.is_empty(),
                "Error message must not be empty in error response"
            );
        }
    }

    #[test]
    fn test_error_response_with_validation_details() {
        let mut details = HashMap::new();
        details.insert(
            "email".to_string(),
            vec!["Email is required".to_string(), "Email must be valid".to_string()],
        );
        details.insert(
            "age".to_string(),
            vec!["Age must be >= 18".to_string()],
        );

        let response = ResponseFormatter::bad_request_with_details(
            "Validation failed",
            "VALIDATION_ERROR",
            details.clone(),
        );

        assert_eq!(response.status, 400);
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, "VALIDATION_ERROR");
        assert!(error.details.is_some());

        let returned_details = error.details.unwrap();
        assert_eq!(returned_details.len(), 2);
        assert!(returned_details.contains_key("email"));
        assert!(returned_details.contains_key("age"));
        assert_eq!(returned_details["email"].len(), 2);
        assert_eq!(returned_details["age"].len(), 1);
    }

    #[test]
    fn test_success_response_never_has_error_field() {
        let success = ResponseFormatter::success("test_data", "Success");
        assert!(success.error.is_none(), "Success response must not have error field");
        assert!(success.data.is_some(), "Success response must have data field");
        assert_eq!(success.status, 200);
    }

    #[test]
    fn test_created_response_never_has_error_field() {
        let created = ResponseFormatter::created("new_resource", "Created");
        assert!(created.error.is_none(), "Created response must not have error field");
        assert!(created.data.is_some(), "Created response must have data field");
        assert_eq!(created.status, 201);
    }

    #[test]
    fn test_all_responses_have_metadata() {
        let success = ResponseFormatter::success("data", "Success");
        assert!(!success.meta.request_id.is_empty());
        assert!(success.meta.timestamp > 0);
        assert!(!success.meta.version.is_empty());

        let error = ResponseFormatter::bad_request("Error", "ERROR");
        assert!(!error.meta.request_id.is_empty());
        assert!(error.meta.timestamp > 0);
        assert!(!error.meta.version.is_empty());
    }

    #[test]
    fn test_error_code_format_consistency() {
        // All error codes should be UPPERCASE_SNAKE_CASE
        let error_codes = vec![
            "UNAUTHORIZED",
            "FORBIDDEN",
            "NOT_FOUND",
            "CONFLICT",
            "INTERNAL_ERROR",
            "INVALID_INPUT",
        ];

        for code in error_codes {
            assert!(
                code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "Error code '{}' must be UPPERCASE_SNAKE_CASE",
                code
            );
        }
    }

    #[test]
    fn test_paginated_response_has_correct_shape() {
        let data = vec![1, 2, 3];
        let response = ResponseFormatter::paginated(data, 100, 1, 10, "Success");

        // Verify response structure
        assert_eq!(response.status, 200);
        assert!(!response.message.is_empty());
        assert_eq!(response.data.len(), 3);

        // Verify pagination metadata
        assert_eq!(response.pagination.total, 100);
        assert_eq!(response.pagination.page, 1);
        assert_eq!(response.pagination.per_page, 10);
        assert_eq!(response.pagination.total_pages, 10);
        assert!(response.pagination.has_next);
        assert!(!response.pagination.has_prev);
    }

    #[test]
    fn test_different_error_codes_are_distinct() {
        let bad_request = ResponseFormatter::bad_request("Bad", "BAD_REQUEST");
        let unauthorized = ResponseFormatter::unauthorized("Unauth");
        let forbidden = ResponseFormatter::forbidden("Forbidden");
        let not_found = ResponseFormatter::not_found("Not found");
        let conflict = ResponseFormatter::conflict("Conflict");
        let internal = ResponseFormatter::internal_error("Internal");

        let codes: Vec<String> = vec![
            bad_request.error.unwrap().code,
            unauthorized.error.unwrap().code,
            forbidden.error.unwrap().code,
            not_found.error.unwrap().code,
            conflict.error.unwrap().code,
            internal.error.unwrap().code,
        ];

        // All codes should be unique
        let mut sorted_codes = codes.clone();
        sorted_codes.sort();
        sorted_codes.dedup();
        assert_eq!(sorted_codes.len(), codes.len(), "All error codes must be unique");
    }
}
