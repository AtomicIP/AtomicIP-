use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub requests: Vec<SingleRequest>,
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
/// Supports up to 100 requests per batch to reduce round trips.
/// Each request is processed independently and results are returned in order.
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
    // Validate batch size
    if batch_request.requests.is_empty() || batch_request.requests.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Batch size must be between 1 and 100 requests".to_string(),
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
    
    // Process requests sequentially for now (could be parallel for read-only operations)
    for request in batch_request.requests {
        let response = process_single_request(request).await;
        responses.push(response);
    }

    Ok(Json(BatchResponse { responses }))
}

async fn process_single_request(request: SingleRequest) -> SingleResponse {
    // Route to appropriate handler based on path and method
    let (status, body) = match (request.method.as_str(), request.path.as_str()) {
        ("GET", path) if path.starts_with("/ip/") => {
            // Extract IP ID from path
            if let Some(ip_id_str) = path.strip_prefix("/ip/") {
                if let Ok(ip_id) = ip_id_str.parse::<u64>() {
                    // Simulate get_ip call
                    (200, serde_json::json!({"ip_id": ip_id, "status": "retrieved"}))
                } else {
                    (400, serde_json::json!({"error": "Invalid IP ID"}))
                }
            } else {
                (400, serde_json::json!({"error": "Invalid path"}))
            }
        }
        ("GET", path) if path.starts_with("/swap/") => {
            // Extract Swap ID from path
            if let Some(swap_id_str) = path.strip_prefix("/swap/") {
                if let Ok(swap_id) = swap_id_str.parse::<u64>() {
                    (200, serde_json::json!({"swap_id": swap_id, "status": "retrieved"}))
                } else {
                    (400, serde_json::json!({"error": "Invalid Swap ID"}))
                }
            } else {
                (400, serde_json::json!({"error": "Invalid path"}))
            }
        }
        ("POST", "/ip/commit") => {
            // Simulate commit_ip call
            if let Some(body) = &request.body {
                (200, serde_json::json!({"ip_id": 12345, "data": body}))
            } else {
                (400, serde_json::json!({"error": "Missing request body"}))
            }
        }
        ("POST", "/swap/initiate") => {
            // Simulate initiate_swap call
            if let Some(body) = &request.body {
                (200, serde_json::json!({"swap_id": 67890, "data": body}))
            } else {
                (400, serde_json::json!({"error": "Missing request body"}))
            }
        }
        ("POST", "/swap/accept") => {
            // Simulate accept_swap call
            if let Some(body) = &request.body {
                (200, serde_json::json!({"status": "accepted", "data": body}))
            } else {
                (400, serde_json::json!({"error": "Missing request body"}))
            }
        }
        _ => (404, serde_json::json!({"error": "Endpoint not found"})),
    };

    SingleResponse {
        id: request.id,
        status,
        body,
        headers: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_request_processing() {
        let batch_request = BatchRequest {
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

        let batch_request = BatchRequest { requests };
        let result = batch_handler(Json(batch_request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_request_duplicate_ids() {
        let batch_request = BatchRequest {
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
        assert_eq!(response.responses[0].status, 200);
    }
}
