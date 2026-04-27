use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub requests: Vec<SingleRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SingleRequest {
    pub id: String,
    pub method: String,
    pub path: String,
    pub body: Option<Value>,
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
}

/// Batch endpoint for multiple API requests
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
) -> Result<Json<BatchResponse>, (StatusCode, Json<crate::schemas::ErrorResponse>)> {
    if batch_request.requests.is_empty() || batch_request.requests.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(crate::schemas::ErrorResponse {
                error: "Batch size must be between 1 and 100 requests".to_string(),
            }),
        ));
    }

    let mut responses = Vec::new();
    
    // Process requests sequentially for now (could be parallel for read-only operations)
    for request in batch_request.requests {
        let response = process_single_request(request.clone()).await;
        responses.push(response);
    }

    Ok(Json(BatchResponse { responses }))
}

async fn process_single_request(request: SingleRequest) -> SingleResponse {
    // Route to appropriate handler based on path
    let (status, body) = match (request.method.as_str(), request.path.as_str()) {
        ("GET", path) if path.starts_with("/ip/") => {
            // Extract IP ID from path
            if let Some(ip_id_str) = path.strip_prefix("/ip/") {
                if let Ok(ip_id) = ip_id_str.parse::<u64>() {
                    // Simulate get_ip call
                    (200, serde_json::json!({"ip_id": ip_id, "status": "not_implemented"}))
                } else {
                    (400, serde_json::json!({"error": "Invalid IP ID"}))
                }
            } else {
                (400, serde_json::json!({"error": "Invalid path"}))
            }
        }
        ("POST", "/ip/commit") => {
            // Simulate commit_ip call
            (200, serde_json::json!({"ip_id": 12345}))
        }
        ("POST", "/swap/initiate") => {
            // Simulate initiate_swap call
            (200, serde_json::json!({"swap_id": 67890}))
        }
        _ => (404, serde_json::json!({"error": "Endpoint not found"})),
    };

    SingleResponse {
        id: request.id,
        status,
        body,
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
                },
                SingleRequest {
                    id: "req2".to_string(),
                    method: "POST".to_string(),
                    path: "/ip/commit".to_string(),
                    body: Some(serde_json::json!({"owner": "test", "commitment_hash": "hash"})),
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
}
