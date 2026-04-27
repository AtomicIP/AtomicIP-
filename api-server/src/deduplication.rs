use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

pub type DeduplicationStore = Arc<DashMap<String, (Value, Instant)>>;

pub fn create_store() -> DeduplicationStore {
    Arc::new(DashMap::new())
}

pub async fn deduplication_middleware(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Only apply to write operations
    if !matches!(req.method(), &axum::http::Method::POST | &axum::http::Method::PUT | &axum::http::Method::PATCH) {
        return Ok(next.run(req).await);
    }

    let idempotency_key = headers
        .get("x-idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let store = req.extensions().get::<DeduplicationStore>().unwrap().clone();
    
    // Check for existing result
    if let Some((cached_result, timestamp)) = store.get(idempotency_key) {
        // Return cached result if within TTL (1 hour)
        if timestamp.elapsed() < Duration::from_secs(3600) {
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(cached_result.to_string().into())
                .unwrap();
            return Ok(response);
        } else {
            store.remove(idempotency_key);
        }
    }

    let response = next.run(req).await;
    
    // Cache successful responses
    if response.status().is_success() {
        if let Ok(body_bytes) = axum::body::to_bytes(response.into_body(), usize::MAX).await {
            if let Ok(json_value) = serde_json::from_slice::<Value>(&body_bytes) {
                store.insert(idempotency_key.to_string(), (json_value.clone(), Instant::now()));
                
                let new_response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .body(body_bytes.into())
                    .unwrap();
                return Ok(new_response);
            }
        }
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};

    #[tokio::test]
    async fn test_deduplication_requires_idempotency_key() {
        let store = create_store();
        let req = Request::builder()
            .method("POST")
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        
        let next = |_: Request| async { 
            Response::builder().status(200).body(Body::empty()).unwrap()
        };
        
        let result = deduplication_middleware(HeaderMap::new(), req, next).await;
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }
}
