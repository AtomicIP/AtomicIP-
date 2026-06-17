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
}
