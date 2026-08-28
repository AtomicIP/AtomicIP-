use axum::{routing::get, routing::post, Router};
use axum::body::Body;
use axum::http::{StatusCode, HeaderMap};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::extract::{FromRef, Request};
use utoipa::OpenApi;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use std::sync::Arc;

/// Shared application state injected into every handler.
#[derive(Clone)]
struct AppState {
    schema:           graphql::AtomicIpSchema,
    query_client:     Arc<graphql::SorobanQueryClient>,
    ws_broadcaster:   Arc<websocket::EventBroadcaster>,
    sse_broadcaster:  Arc<events::EventBroadcaster>,
    health_checker:   Arc<health::HealthChecker>,
}

impl FromRef<AppState> for Arc<health::HealthChecker> {
    fn from_ref(state: &AppState) -> Self {
        state.health_checker.clone()
    }
}

impl FromRef<AppState> for Arc<dyn graphql::SorobanRpcClient> {
    fn from_ref(state: &AppState) -> Self {
        state.rpc_client.clone()
    }
}

mod auth;
mod batch;
mod cache;
mod circuit_breaker;
mod deduplication;
mod events;
mod graphql;
mod handlers;
mod metrics;
mod middleware_pipeline;
mod schemas;
mod tracing_middleware;
mod versioning;
mod webhook;
mod websocket;
mod request_signing;
mod invariants;
mod health;
mod compression;
mod fallback;
mod distributed_tracing;
mod error_recovery;
mod otel;
mod request_queue;
mod rate_limit;
mod validation;
mod validation_middleware;
#[cfg(test)]
mod validation_fuzz_tests;

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
        batch::batch_handler,
        events::events_handler,
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
        schemas::BulkOperationResult<schemas::IpRecord>,
    )),
    tags(
        (name = "IP Registry", description = "Commit and query intellectual property records"),
        (name = "Atomic Swap", description = "Trustless patent sale via atomic swap"),
        (name = "Webhooks", description = "Real-time event notifications"),
        (name = "Batch", description = "Batch API operations"),
        (name = "Events", description = "Server-Sent Events stream"),
    )
)]
pub struct ApiDoc;

/// Serve the OpenAPI 3.x spec as JSON.
async fn openapi_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::to_value(ApiDoc::openapi()).unwrap_or_default())
}

/// GraphQL query/mutation endpoint (HTTP POST).
async fn graphql_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    state.schema.execute(req.into_inner()).await.into()
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

/// Builds the GraphQL subscription broadcaster. When `REDIS_URL` is set,
/// events fan out across every instance pointed at that Redis (#783); on
/// any connection failure, or when `REDIS_URL` is unset, falls back to the
/// single-instance in-process broadcaster so the server always starts
/// without Redis (mirrors `cache::init_cache`'s fallback philosophy).
async fn init_subscription_broadcaster() -> Arc<graphql::SubscriptionBroadcaster> {
    match std::env::var("REDIS_URL") {
        Ok(url) if !url.is_empty() => {
            match graphql::SubscriptionBroadcaster::new_with_redis(&url).await {
                Ok(broadcaster) => {
                    tracing::info!("multi-instance GraphQL subscriptions enabled via REDIS_URL");
                    broadcaster
                }
                Err(e) => {
                    tracing::warn!("REDIS_URL set but unusable ({e}); falling back to single-instance GraphQL subscriptions");
                    Arc::new(graphql::SubscriptionBroadcaster::new())
                }
            }
        }
        _ => Arc::new(graphql::SubscriptionBroadcaster::new()),
    }
}

#[tokio::main]
async fn main() {
    // Initialise OpenTelemetry SDK (OTLP exporter) + tracing subscriber.
    // Keep the provider alive until shutdown so spans are flushed on exit.
    let tracer_provider = otel::init_tracer();

    metrics::init();

    let subscription_broadcaster = init_subscription_broadcaster().await;
    let rpc_client: Arc<dyn graphql::SorobanRpcClient> = Arc::new(graphql::MockSorobanRpcClient::default());
    let query_client = Arc::new(graphql::SorobanQueryClient::new(rpc_client.clone()));
    let schema = graphql::build_schema_with_broadcaster(
        rpc_client,
        subscription_broadcaster.clone(),
    );

    let state = AppState {
        schema,
        query_client,
        ws_broadcaster:  Arc::new(websocket::EventBroadcaster::new()),
        sse_broadcaster: Arc::new(events::create_event_broadcaster().0),
        health_checker:  Arc::new(health::HealthChecker::new()),
    };

    let rate_limiter = rate_limit::RateLimitMiddleware::new(rate_limit::RateLimitConfig::default());
    let app = Router::new()
        .route("/health",          get(health::health_handler))
        .route("/health/detailed", get(health::detailed_health_handler))
        .route("/metrics",         get(metrics::metrics_handler))
        .route("/graphql",         post(graphql_handler))
        .route("/graphql/ws",      get(graphql_ws_handler))
        .route("/ws",              get(ws_handler))
        .route("/events",          get(events_handler))
        .route("/batch",           post(batch::batch_handler))
        .route("/ip/commit",                      post(handlers::commit_ip))
        .route("/ip/{ip_id}",                     get(handlers::get_ip))
        .route("/ip/transfer",                    post(handlers::transfer_ip))
        .route("/ip/verify",                      post(handlers::verify_commitment))
        .route("/ip/owner/{owner}",               get(handlers::list_ip_by_owner))
        .route("/ip/owner/{owner}/cursor",        get(handlers::list_ip_by_owner_cursor))
        .route("/ip/owner/{owner}/cursor",        get(handlers::list_ip_by_owner_cursor))
        .route("/swap/initiate",                  post(handlers::initiate_swap))
        .route("/swap/batch-initiate",            post(handlers::batch_initiate_swap))
        .route("/swap/{swap_id}/accept",          post(handlers::accept_swap))
        .route("/swap/{swap_id}/reveal",          post(handlers::reveal_key))
        .route("/swap/{swap_id}/cancel",          post(handlers::cancel_swap))
        .route("/swap/{swap_id}/cancel-expired",  post(handlers::cancel_expired_swap))
        .route("/swap/{swap_id}",                 get(handlers::get_swap))
        .route("/openapi.json", get(openapi_handler))
        .with_state(state)
        .layer(middleware::from_fn_with_state(rate_limiter, rate_limit::rate_limit_middleware))
        .layer(middleware::from_fn(metrics::track))
        .layer(middleware::from_fn(distributed_tracing::distributed_tracing_middleware))
        .layer(middleware::from_fn(middleware_pipeline::cors_middleware));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("OpenAPI JSON    -> http://localhost:8080/openapi.json");
    println!("Health Check    -> http://localhost:8080/health");
    println!("Metrics         -> http://localhost:8080/metrics");
    println!("WebSocket (raw) -> ws://localhost:8080/ws");
    println!("GraphQL WS      -> ws://localhost:8080/graphql/ws");
    println!("Events SSE      -> http://localhost:8080/events");
    println!("Batch API       -> http://localhost:8080/batch");
    println!("GraphQL         -> http://localhost:8080/graphql");
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();

    // Flush and shut down the OTel tracer so all pending spans are exported.
    if let Some(provider) = tracer_provider {
        otel::shutdown_tracer(provider);
    }
}

/// GraphQL subscription endpoint over `graphql-transport-ws` WebSocket protocol.
async fn graphql_ws_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    protocol: async_graphql_axum::GraphQLProtocol,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    let schema = state.schema.clone();
    ws.protocols(["graphql-transport-ws", "graphql-ws"])
        .on_upgrade(move |stream| {
            async_graphql_axum::GraphQLWebSocket::new(stream, schema, protocol).serve()
        })
}

/// Raw WebSocket endpoint (legacy, non-GraphQL JSON protocol).
async fn ws_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl axum::response::IntoResponse {
    let broadcaster = state.ws_broadcaster.clone();
    ws.on_upgrade(|socket| websocket::handle_socket(socket, broadcaster))
}

/// SSE endpoint — adapts the AppState to the handler's expected extractor.
async fn events_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl axum::response::IntoResponse {
    let sender = (*state.sse_broadcaster).clone();
    events::events_handler(axum::extract::State(Arc::new(sender))).await
}

fn build_app() -> Router {
    let query_client = Arc::new(graphql::SorobanQueryClient::new(Arc::new(graphql::MockSorobanRpcClient::default())));
    let schema = graphql::build_schema();
    let health_checker = Arc::new(health::HealthChecker::new());
    let circuit_breaker = Arc::new(circuit_breaker::CircuitBreaker::new(
        "default",
        circuit_breaker::CircuitBreakerConfig::default(),
    ));
    let state = AppState {
        schema,
        rpc_client,
        ws_broadcaster:  Arc::new(websocket::EventBroadcaster::new()),
        sse_broadcaster: Arc::new(events::create_event_broadcaster().0),
        health_checker,
    };

    let rate_limiter = rate_limit::RateLimitMiddleware::new(rate_limit::RateLimitConfig::default());
    let state = AppState {
        schema,
        query_client,
        ws_broadcaster: Arc::new(websocket::EventBroadcaster::new()),
        sse_broadcaster: Arc::new(events::create_event_broadcaster().0),
        health_checker,
    };

    Router::new()
        .route("/health", get(health::health_handler))
        .route("/version", get(versioning::get_version_info))
        .route("/v1/graphql", post(graphql_handler))
        .route("/v1/ip/commit", post(handlers::commit_ip))
        .route("/v1/ip/{ip_id}", get(handlers::get_ip))
        .route("/v1/ip/transfer", post(handlers::transfer_ip))
        .route("/v1/ip/verify", post(handlers::verify_commitment))
        .route("/v1/ip/owner/{owner}", get(handlers::list_ip_by_owner))
        .route("/v1/ip/owner/{owner}/cursor", get(handlers::list_ip_by_owner_cursor))
        .route("/v1/ip/owner/{owner}/cursor", get(handlers::list_ip_by_owner_cursor))
        .route("/v1/swap/initiate", post(handlers::initiate_swap))
        .route("/v1/swap/bulk/initiate", post(handlers::batch_initiate_swap))
        .route("/v1/swap/{swap_id}/accept", post(handlers::accept_swap))
        .route("/v1/swap/{swap_id}/reveal", post(handlers::reveal_key))
        .route("/v1/swap/{swap_id}/cancel", post(handlers::cancel_swap))
        .route("/v1/swap/{swap_id}/cancel-expired", post(handlers::cancel_expired_swap))
        .route("/v1/swap/{swap_id}", get(handlers::get_swap))
        .route("/v1/bulk/commit-ip", post(handlers::bulk_commit_ip))
        .route("/v1/bulk/initiate-swap", post(handlers::bulk_initiate_swap))
        .with_state(state)
        .layer(middleware::from_fn_with_state(rate_limiter, rate_limit::rate_limit_middleware))
        .layer(middleware::from_fn(tracing_middleware::trace_requests))
        .layer(middleware::from_fn(versioning::version_negotiation))
        .layer(middleware::from_fn(compression::compression_middleware))
        .layer(middleware::from_fn(validation_middleware::validation_logging_middleware))
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
            .route("/openapi.json", get(openapi_handler))
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

    // ── Swap-lookup read path: RPC read-through + cache backfill ────────────

    /// Stub RPC client that returns a single known swap so the read path can
    /// be exercised without a live Soroban node.
    struct StubSwapRpcClient;

    #[async_trait::async_trait]
    impl graphql::SorobanRpcClient for StubSwapRpcClient {
        async fn get_ip_record(&self, _ip_id: u64) -> Result<Option<graphql::IpRecord>, String> {
            Ok(None)
        }
        async fn get_swap_record(&self, _swap_id: u64) -> Result<Option<graphql::SwapRecord>, String> {
            Ok(Some(graphql::SwapRecord {
                swap_id: 42,
                ip_registry_id: "REG1".to_string(),
                ip_id: 7,
                seller: "GSELLER".to_string(),
                buyer: "GBUYER".to_string(),
                price: "1000".to_string(),
                token: "CTOKEN".to_string(),
                status: graphql::SwapStatus::Pending,
                expiry: 999,
                arbitrator: None,
            }))
        }
        async fn get_swaps_by_seller(&self, _seller: &str, _limit: u64, _cursor: Option<String>) -> Result<graphql::SwapConnection, String> {
            Ok(graphql::SwapConnection { swap_ids: vec![], has_next_page: false, cursor: None })
        }
        async fn get_swaps_by_buyer(&self, _buyer: &str, _limit: u64, _cursor: Option<String>) -> Result<graphql::SwapConnection, String> {
            Ok(graphql::SwapConnection { swap_ids: vec![], has_next_page: false, cursor: None })
        }
        async fn get_swaps_by_ip(&self, _ip_id: u64, _limit: u64, _cursor: Option<String>) -> Result<graphql::SwapConnection, String> {
            Ok(graphql::SwapConnection { swap_ids: vec![], has_next_page: false, cursor: None })
        }
        async fn get_dispute_evidence(&self, _swap_id: u64) -> Result<Vec<graphql::DisputeEvidence>, String> {
            Ok(vec![])
        }
        async fn get_reputation(&self, _address: &str) -> Result<Option<graphql::Reputation>, String> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_get_swap_reads_through_rpc_on_cache_miss() {
        cache::clear();
        let app = app_with_rpc_client(Arc::new(StubSwapRpcClient));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/swap/42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().contains_key("cache-control"));
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ip_id"], 7);
        assert_eq!(json["status"], "Pending");
        assert_eq!(json["ip_registry_id"], "REG1");
        assert_eq!(json["price"], 1000);
    }

    #[tokio::test]
    async fn test_get_swap_serves_from_cache_after_rpc_read_through() {
        cache::clear();
        let app = app_with_rpc_client(Arc::new(StubSwapRpcClient));
        // First request populates the cache via the RPC read-through.
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/swap/42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert!(cache::get::<schemas::SwapRecord>(&cache::swap_key(42)).is_some());
    }

    #[tokio::test]
    async fn test_get_swap_returns_404_when_rpc_has_no_record() {
        cache::clear();
        let app = build_app(); // MockSorobanRpcClient always returns None
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/swap/999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
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

    #[tokio::test]
    async fn test_batch_initiate_swap_success_returns_pending_swaps() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/swap/bulk/initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[1,2,3],"seller":"G1","prices":[100,200,300],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let swap_ids: Vec<u64> = json["swap_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect();
        assert_eq!(swap_ids.len(), 3);
        // IDs are allocated sequentially, mirroring the contract's NextId.
        assert_eq!(swap_ids[0] + 1, swap_ids[1]);
        assert_eq!(swap_ids[1] + 1, swap_ids[2]);

        // The created swaps are readable via GET /swap/{id} as Pending with a
        // ~7-day expiry, matching the contract's batch_initiate_swap.
        for swap_id in swap_ids {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/v1/swap/{}", swap_id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            let record: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(record["status"], "Pending");
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            assert!(record["expiry"].as_u64().unwrap() > now);
        }
    }

    #[tokio::test]
    async fn test_batch_initiate_swap_too_large_returns_400() {
        let app = build_app();
        let ip_ids: Vec<String> = (1..=51).map(|i| i.to_string()).collect();
        let prices: Vec<String> = (1..=51).map(|i| (i * 100).to_string()).collect();
        let body = format!(
            r#"{{"ip_registry_id":"C1","ip_ids":[{}],"seller":"G1","prices":[{}],"buyer":"G2","token":"C2"}}"#,
            ip_ids.join(","),
            prices.join(",")
        );
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/swap/bulk/initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_batch_initiate_swap_non_positive_price_returns_400() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/swap/bulk/initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"ip_registry_id":"C1","ip_ids":[1,2],"seller":"G1","prices":[100,0],"buyer":"G2","token":"C2"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("positive"));
    }

    #[tokio::test]
    async fn test_batch_initiate_swap_idempotent_replay_returns_same_ids() {
        let app = build_app();
        let body = r#"{"ip_registry_id":"C1","ip_ids":[10,11],"seller":"G1","prices":[100,200],"buyer":"G2","token":"C2","idempotency_key":"batch-init-test-key"}"#;
        let send = || {
            app.clone().oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/swap/bulk/initiate")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
        };

        let first = send().await.unwrap();
        let second = send().await.unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::OK);

        let parse_ids = |resp: axum::response::Response| {
            async move {
                let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
                let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
                json["swap_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_u64().unwrap())
                    .collect::<Vec<u64>>()
            }
        };

        let first_ids = parse_ids(first).await;
        let second_ids = parse_ids(second).await;
        assert_eq!(first_ids.len(), 2);
        // #523: replay with the same key must return the cached swap IDs.
        assert_eq!(first_ids, second_ids);
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

    #[tokio::test]
    async fn test_version_endpoint_returns_version_info() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["version"], "1.0.0");
        assert_eq!(json["status"], "stable");
        assert!(json["supported_versions"].is_array());
    }

    #[tokio::test]
    async fn test_version_negotiation_with_accept_version_header() {
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
        assert!(resp.headers().contains_key("API-Version"));
    }

    #[tokio::test]
    async fn test_version_negotiation_unsupported_version() {
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

    #[tokio::test]
    async fn test_version_header_in_response() {
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
    async fn test_compression_vary_header_present() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Encoding", "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("Vary"));
        assert_eq!(resp.headers().get("Vary").unwrap(), "Accept-Encoding");
    }

    #[tokio::test]
    async fn test_compression_gzip_encoding_header() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Encoding", "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("Content-Encoding"));
        assert_eq!(resp.headers().get("Content-Encoding").unwrap(), "gzip");
    }

    #[tokio::test]
    async fn test_compression_brotli_encoding_header() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Encoding", "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("Content-Encoding"));
        assert_eq!(resp.headers().get("Content-Encoding").unwrap(), "br");
    }

    #[tokio::test]
    async fn test_compression_deflate_encoding_header() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Encoding", "deflate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp.headers().contains_key("Content-Encoding"));
        assert_eq!(resp.headers().get("Content-Encoding").unwrap(), "deflate");
    }

    #[tokio::test]
    async fn test_compression_no_accept_encoding() {
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
        assert!(resp.headers().contains_key("Vary"));
    }

    #[tokio::test]
    async fn test_compression_multiple_encodings_prefers_gzip() {
        let app = build_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/ip/1")
                    .header("Accept-Encoding", "gzip, br, deflate")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.headers().get("Content-Encoding").unwrap(), "gzip");
    }
}
