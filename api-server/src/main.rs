use axum::{routing::get, routing::post, Router};
use axum::body::Body;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::extract::Request;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod auth;
mod cache;
mod handlers;
mod metrics;
mod schemas;
mod webhook;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Atomic Patent API",
        version = "1.0.0",
        description = "Machine-readable specification for the Atomic Patent Soroban smart contract interface."
    ),
    paths(
        handlers::commit_ip,
        handlers::get_ip,
        handlers::transfer_ip,
        handlers::verify_commitment,
        handlers::list_ip_by_owner,
        handlers::initiate_swap,
        handlers::batch_initiate_swap,
        handlers::accept_swap,
        handlers::reveal_key,
        handlers::cancel_swap,
        handlers::cancel_expired_swap,
        handlers::get_swap,
        handlers::register_webhook,
        handlers::unregister_webhook,
    ),
    components(schemas(
        schemas::CommitIpRequest,
        schemas::IpRecord,
        schemas::TransferIpRequest,
        schemas::VerifyCommitmentRequest,
        schemas::VerifyCommitmentResponse,
        schemas::ListIpByOwnerResponse,
        schemas::InitiateSwapRequest,
        schemas::BatchInitiateSwapRequest,
        schemas::BatchInitiateSwapResponse,
        schemas::AcceptSwapRequest,
        schemas::RevealKeyRequest,
        schemas::CancelSwapRequest,
        schemas::CancelExpiredSwapRequest,
        schemas::SwapRecord,
        schemas::SwapStatus,
        schemas::ErrorResponse,
        schemas::RegisterWebhookRequest,
        schemas::WebhookResponse,
    )),
    tags(
        (name = "IP Registry", description = "Commit and query intellectual property records"),
        (name = "Atomic Swap", description = "Trustless patent sale via atomic swap"),
        (name = "Webhooks", description = "Real-time event notifications"),
    )
)]
pub struct ApiDoc;

/// Middleware: reject POST/PUT/PATCH requests whose body is non-empty but lacks
/// `Content-Type: application/json`.
async fn require_json_content_type(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    if matches!(method, axum::http::Method::POST | axum::http::Method::PUT | axum::http::Method::PATCH) {
        let content_type = req
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type.starts_with("application/json") {
            return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }
    }
    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() {
    metrics::init();

    let app = Router::new()
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .route("/metrics", get(metrics::metrics_handler))
        .route("/ip/commit", post(handlers::commit_ip))
        .route("/ip/{ip_id}", get(handlers::get_ip))
        .route("/ip/transfer", post(handlers::transfer_ip))
        .route("/ip/verify", post(handlers::verify_commitment))
        .route("/ip/owner/{owner}", get(handlers::list_ip_by_owner))
        .route("/swap/initiate", post(handlers::initiate_swap))
        .route("/swap/batch-initiate", post(handlers::batch_initiate_swap))
        .route("/swap/{swap_id}/accept", post(handlers::accept_swap))
        .route("/swap/{swap_id}/reveal", post(handlers::reveal_key))
        .route("/swap/{swap_id}/cancel", post(handlers::cancel_swap))
        .route("/swap/{swap_id}/cancel-expired", post(handlers::cancel_expired_swap))
        .route("/swap/{swap_id}", get(handlers::get_swap))
        .layer(middleware::from_fn(metrics::track));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Swagger UI   -> http://localhost:8080/docs");
    println!("OpenAPI JSON -> http://localhost:8080/openapi.json");
    println!("Metrics      -> http://localhost:8080/metrics");
    axum::serve(listener, app).await.unwrap();
}

fn build_app() -> Router {
    Router::new()
        .route("/ip/commit", post(handlers::commit_ip))
        .route("/ip/{ip_id}", get(handlers::get_ip))
        .route("/ip/transfer", post(handlers::transfer_ip))
        .route("/ip/verify", post(handlers::verify_commitment))
        .route("/ip/owner/{owner}", get(handlers::list_ip_by_owner))
        .route("/swap/initiate", post(handlers::initiate_swap))
        .route("/swap/batch-initiate", post(handlers::batch_initiate_swap))
        .route("/swap/{swap_id}/accept", post(handlers::accept_swap))
        .route("/swap/{swap_id}/reveal", post(handlers::reveal_key))
        .route("/swap/{swap_id}/cancel", post(handlers::cancel_swap))
        .route("/swap/{swap_id}/cancel-expired", post(handlers::cancel_expired_swap))
        .route("/swap/{swap_id}", get(handlers::get_swap))
        .layer(middleware::from_fn(require_json_content_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_post_without_content_type_returns_415() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ip/commit")
                    .body(Body::from(r#"{"owner":"G123","commitment_hash":"abc"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_post_with_wrong_content_type_returns_415() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ip/commit")
                    .header("content-type", "text/plain")
                    .body(Body::from(r#"{"owner":"G123","commitment_hash":"abc"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_post_with_json_content_type_passes_middleware() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ip/commit")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"owner":"G123","commitment_hash":"abc"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Middleware passes; handler returns 400 (stub), not 415
        assert_ne!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_get_request_bypasses_content_type_check() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ip/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_openapi_json_endpoint_returns_valid_spec() {
        let app = Router::new()
            .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
            .layer(middleware::from_fn(require_json_content_type));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(spec["info"]["title"], "Atomic Patent API");
        assert!(spec["paths"].is_object());
        assert!(spec["components"]["schemas"].is_object());
    }

    // ── #317: Pagination tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_ip_by_owner_returns_paginated_response() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ip/owner/GADDR?limit=10&offset=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["ip_ids"].is_array());
        assert!(json["total_count"].is_number());
        assert!(json["has_more"].is_boolean());
    }

    #[tokio::test]
    async fn test_list_ip_by_owner_default_pagination() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ip/owner/GADDR")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── #316: Cache-Control header tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_get_ip_returns_cache_control_header() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ip/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Cache-Control header should be present regardless of hit/miss
        assert!(resp.headers().contains_key("cache-control"));
    }

    #[tokio::test]
    async fn test_get_swap_returns_cache_control_header() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/swap/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("cache-control"));
    }

    // ── #309: Batch initiate swap validation tests ────────────────────────────

    #[tokio::test]
    async fn test_batch_initiate_swap_mismatched_lengths_returns_400() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/swap/batch-initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[1,2],"seller":"G1","prices":[100],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("same length"));
    }

    #[tokio::test]
    async fn test_batch_initiate_swap_empty_ids_returns_400() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/swap/batch-initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[],"seller":"G1","prices":[],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
