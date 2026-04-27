use axum::{routing::get, routing::post, Router};
use axum::body::Body;
use axum::http::{StatusCode, HeaderMap};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::extract::Request;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use std::sync::Arc;

mod auth;
mod cache;
mod graphql;
mod handlers;
mod metrics;
mod schemas;
mod tracing_middleware;
mod versioning;
mod webhook;
mod websocket;
mod graphql;
mod request_signing;
mod invariants;
mod health;

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
        handlers::bulk_commit_ip,
        handlers::bulk_initiate_swap,
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
        schemas::BulkCommitIpRequest,
        schemas::BulkCommitIpResponse,
        schemas::BulkInitiateSwapRequest,
        schemas::BulkInitiateSwapResponse,
        schemas::BulkOperationResult,
    )),
    tags(
        (name = "IP Registry", description = "Commit and query intellectual property records"),
        (name = "Atomic Swap", description = "Trustless patent sale via atomic swap"),
        (name = "Webhooks", description = "Real-time event notifications"),
    )
)]
pub struct ApiDoc;

/// GraphQL endpoint — accepts POST requests with a GraphQL query body.
async fn graphql_handler(
    axum::extract::State(schema): axum::extract::State<graphql::AtomicIpSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

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

    let schema = graphql::build_schema();
    let broadcaster = Arc::new(websocket::EventBroadcaster::new());
    let health_checker = Arc::new(health::HealthChecker::new());

    let app = Router::new()
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health::health_handler))
        .route("/metrics", get(metrics::metrics_handler))
        .route("/graphql", post(graphql_handler))
        .route("/ws", get(ws_handler))
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
        .with_state((schema, broadcaster.clone(), health_checker.clone()))
        .layer(middleware::from_fn(metrics::track));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Swagger UI   -> http://localhost:8080/docs");
    println!("OpenAPI JSON -> http://localhost:8080/openapi.json");
    println!("Health Check -> http://localhost:8080/health");
    println!("Metrics      -> http://localhost:8080/metrics");
    println!("WebSocket    -> ws://localhost:8080/ws");
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    axum::extract::State((_, broadcaster, _)): axum::extract::State<(graphql::AtomicIpSchema, Arc<websocket::EventBroadcaster>, Arc<health::HealthChecker>)>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|socket| websocket::handle_socket(socket, broadcaster))
}

fn build_app() -> Router {
    let schema = graphql::build_schema();
    let health_checker = Arc::new(health::HealthChecker::new());
    Router::new()
        .route("/health", get(health::health_handler))
        .route("/v1/graphql", post(graphql_handler))
        .route("/v1/ip/commit", post(handlers::commit_ip))
        .route("/v1/ip/:ip_id", get(handlers::get_ip))
        .route("/v1/ip/transfer", post(handlers::transfer_ip))
        .route("/v1/ip/verify", post(handlers::verify_commitment))
        .route("/v1/ip/owner/:owner", get(handlers::list_ip_by_owner))
        .route("/v1/swap/initiate", post(handlers::initiate_swap))
        .route("/v1/swap/bulk/initiate", post(handlers::batch_initiate_swap))
        .route("/v1/swap/:swap_id/accept", post(handlers::accept_swap))
        .route("/v1/swap/:swap_id/reveal", post(handlers::reveal_key))
        .route("/v1/swap/:swap_id/cancel", post(handlers::cancel_swap))
        .route("/v1/swap/:swap_id/cancel-expired", post(handlers::cancel_expired_swap))
        .route("/v1/swap/:swap_id", get(handlers::get_swap))
        .route("/v1/bulk/commit-ip", post(handlers::bulk_commit_ip))
        .route("/v1/bulk/initiate-swap", post(handlers::bulk_initiate_swap))
        .with_state((schema, health_checker))
        .layer(middleware::from_fn(tracing_middleware::trace_requests))
        .layer(middleware::from_fn(versioning::version_negotiation))
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
                    .uri("/v1/ip/commit")
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
                    .uri("/v1/ip/commit")
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
                    .uri("/v1/ip/commit")
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
                    .uri("/v1/ip/1")
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
                    .uri("/v1/ip/owner/GADDR?limit=10&offset=0")
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
                    .uri("/v1/ip/owner/GADDR")
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
                    .uri("/v1/ip/1")
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
                    .uri("/v1/swap/1")
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
                    .uri("/v1/swap/bulk/initiate")
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
                    .uri("/v1/swap/bulk/initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[],"seller":"G1","prices":[],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── #319: API Versioning tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_api_version_header_present_in_response() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("API-Version"));
        assert_eq!(resp.headers().get("API-Version").unwrap(), "1.0.0");
    }

    #[tokio::test]
    async fn test_accept_version_header_negotiation() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Version", "1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unsupported_version_returns_406() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Version", "2.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_ACCEPTABLE);
    }

    // ── #320: API Request Tracing tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_trace_id_header_present_in_response() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("X-Trace-ID"));
        assert!(resp.headers().contains_key("X-Request-ID"));
    }

    #[tokio::test]
    async fn test_trace_id_propagation() {
        let app = build_app();
        let original_trace_id = "550e8400-e29b-41d4-a716-446655440000";
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("X-Trace-ID", original_trace_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.headers().get("X-Trace-ID").unwrap().to_str().unwrap(),
            original_trace_id
        );
    }

    // ── #321: Bulk operations tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_bulk_commit_ip_empty_hashes_returns_400() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bulk/commit-ip")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"owner":"GADDR","commitment_hashes":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_bulk_commit_ip_returns_results() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bulk/commit-ip")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"owner":"GADDR","commitment_hashes":["abc123","def456"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["results"].is_array());
        assert_eq!(json["results"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_bulk_initiate_swap_mismatched_lengths_returns_400() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bulk/initiate-swap")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[1,2],"seller":"G1","prices":[100],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_bulk_initiate_swap_returns_results() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/bulk/initiate-swap")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[1,2],"seller":"G1","prices":[100,200],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["results"].is_array());
        assert_eq!(json["results"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_health_check_endpoint_returns_ok() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["status"].is_string());
        assert!(json["components"].is_object());
        assert!(json["components"]["contract_connectivity"].is_object());
        assert!(json["components"]["database"].is_object());
        assert!(json["components"]["cache"].is_object());
    }

    #[tokio::test]
    async fn test_health_check_includes_component_status() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["components"]["contract_connectivity"]["status"].is_string());
        assert!(json["components"]["contract_connectivity"]["latency_ms"].is_number());
        assert!(json["components"]["database"]["status"].is_string());
        assert!(json["components"]["cache"]["status"].is_string());
    }
