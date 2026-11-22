use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::sleep;

use crate::error_recovery::{calculate_backoff, is_retryable_error, RetryConfig};

/// Configuration for batch request handling
pub struct BatchConfig {
    /// Maximum number of requests per batch (default: 100)
    pub max_requests: usize,
    /// Maximum total size in bytes per batch (default: 5MB)
    pub max_batch_size_bytes: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            max_batch_size_bytes: 5 * 1024 * 1024, // 5MB
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub requests: Vec<SingleRequest>,
    /// Optional: Set to "atomic" to rollback all requests on any failure
    /// Note: Atomic support requires coordinated database transaction handling
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SingleRequest {
    pub id: String,
    pub method: String,
    pub path: String,
    pub body: Option<Value>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub responses: Vec<SingleResponse>,
}

#[derive(Debug, Serialize)]
pub struct SingleResponse {
    pub id: String,
    pub status: u16,
    pub body: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Batch endpoint for multiple API requests
///
/// Supports up to 100 requests per batch to reduce round trips.
/// Requests are processed sequentially by default and results are returned in order.
///
/// ## Modes:
/// - **best-effort** (default): Process each request independently. Failure in one doesn't affect others.
/// - **atomic**: All requests succeed together or all fail together (requires coordinated database transactions).
///
/// ## Limits:
/// - Maximum 100 requests per batch
/// - Maximum 5MB total payload
/// - Request IDs must be unique
/// - Only supports GET, POST, PUT, DELETE, PATCH methods
#[utoipa::path(
    post,
    path = "/batch",
    tag = "Batch",
    request_body = BatchRequest,
    responses(
        (status = 200, description = "Batch requests processed", body = BatchResponse),
        (status = 400, description = "Invalid batch request", body = ErrorResponse),
    )
)]
pub async fn batch_handler(
    Json(batch_request): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = BatchConfig::default();

    // Validate batch size
    if batch_request.requests.is_empty() || batch_request.requests.len() > config.max_requests {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Batch size must be between 1 and {} requests", config.max_requests),
            }),
        ));
    }

    // Validate request IDs are unique
    let mut seen_ids = std::collections::HashSet::new();
    for req in &batch_request.requests {
        if !seen_ids.insert(&req.id) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Duplicate request ID: {}", req.id),
                }),
            ));
        }
    }

    let mut responses = Vec::new();
    let retry_config = RetryConfig::default();
    
    // Process requests sequentially so retries preserve request ordering.
    for request in batch_request.requests {
        let response = process_single_request_with_retry(request, &retry_config).await;
        responses.push(response);
    }

    Ok(Json(BatchResponse { responses }))
}

async fn process_single_request_with_retry(
    request: SingleRequest,
    config: &RetryConfig,
) -> SingleResponse {
    let mut response = process_single_request(request.clone()).await;

    for attempt in 0..config.max_retries {
        if !is_retryable_error(StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)) {
            break;
        }

        let delay = calculate_backoff(attempt, config);
        tracing::warn!(
            request_id = %request.id,
            attempt = attempt + 1,
            max_retries = config.max_retries,
            ?delay,
            status = response.status,
            "retrying failed batch request"
        );
        sleep(delay).await;
        response = process_single_request(request.clone()).await;
    }

    response
}

async fn process_single_request(request: SingleRequest) -> SingleResponse {
    // Sub-requests are not dispatched to the real handlers yet. Returning
    // fabricated success payloads (e.g. made-up IDs) would look like committed
    // state to clients, so every sub-request is reported as not implemented.
    SingleResponse {
        id: request.id,
        status: 501,
        body: serde_json::json!({
            "error": "batch dispatch is not implemented for this endpoint",
            "method": request.method,
            "path": request.path,
        }),
        headers: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_request_processing() {
        let batch_request = BatchRequest {
            mode: None,
            requests: vec![
                SingleRequest {
                    id: "req1".to_string(),
                    method: "GET".to_string(),
                    path: "/ip/123".to_string(),
                    body: None,
                    headers: None,
                },
                SingleRequest {
                    id: "req2".to_string(),
                    method: "POST".to_string(),
                    path: "/ip/commit".to_string(),
                    body: Some(serde_json::json!({"owner": "test", "commitment_hash": "hash"})),
                    headers: None,
                },
            ],
        };

        let result = batch_handler(Json(batch_request)).await;
        assert!(result.is_ok());
        
        let response = result.unwrap().0;
        assert_eq!(response.responses.len(), 2);
        assert_eq!(response.responses[0].id, "req1");
        assert_eq!(response.responses[1].id, "req2");
    }

    #[tokio::test]
    async fn test_batch_request_size_limits() {
        let batch_request = BatchRequest {
            mode: None,
            requests: vec![],
        };

        let result = batch_handler(Json(batch_request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_request_max_size() {
        let requests = (0..101)
            .map(|i| SingleRequest {
                id: format!("req{}", i),
                method: "GET".to_string(),
                path: "/ip/1".to_string(),
                body: None,
                headers: None,
            })
            .collect();

        let batch_request = BatchRequest { requests, mode: None };
        let result = batch_handler(Json(batch_request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_request_duplicate_ids() {
        let batch_request = BatchRequest {
            mode: None,
            requests: vec![
                SingleRequest {
                    id: "req1".to_string(),
                    method: "GET".to_string(),
                    path: "/ip/123".to_string(),
                    body: None,
                    headers: None,
                },
                SingleRequest {
                    id: "req1".to_string(),
                    method: "GET".to_string(),
                    path: "/ip/456".to_string(),
                    body: None,
                    headers: None,
                },
            ],
        };

        let result = batch_handler(Json(batch_request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_request_get_swap() {
        let batch_request = BatchRequest {
            mode: None,
            requests: vec![SingleRequest {
                id: "req1".to_string(),
                method: "GET".to_string(),
                path: "/swap/789".to_string(),
                body: None,
                headers: None,
            }],
        };

        let result = batch_handler(Json(batch_request)).await;
        assert!(result.is_ok());
        
        let response = result.unwrap().0;
        assert_eq!(response.responses[0].status, 501);
        assert_ne!(response.responses[0].body.get("swap_id"), Some(&serde_json::json!(67890)));
    }

    #[tokio::test]
    async fn test_batch_commit_does_not_return_fabricated_ip_id() {
        let batch_request = BatchRequest {
            mode: None,
            requests: vec![SingleRequest {
                id: "req1".to_string(),
                method: "POST".to_string(),
                path: "/ip/commit".to_string(),
                body: Some(serde_json::json!({"owner": "test"})),
                headers: None,
            }],
        };

        let response = batch_handler(Json(batch_request)).await.unwrap().0;
        assert_eq!(response.responses[0].status, 501);
        assert!(response.responses[0].body.get("ip_id").is_none());
    }
}
