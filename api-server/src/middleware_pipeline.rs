use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::time::Instant;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddlewareConfig {
    pub enable_request_logging: bool,
    pub enable_response_timing: bool,
    pub enable_request_validation: bool,
    pub enable_rate_limiting: bool,
    pub enable_cors: bool,
    /// Allowlist of origins permitted to receive CORS headers.
    /// Populated from the `CORS_ALLOWED_ORIGINS` environment variable
    /// (comma-separated). Empty means no origin is allowed.
    pub allowed_origins: Vec<String>,
}

impl Default for MiddlewareConfig {
    fn default() -> Self {
        Self {
            enable_request_logging: true,
            enable_response_timing: true,
            enable_request_validation: true,
            enable_rate_limiting: false,
            enable_cors: true,
            allowed_origins: Self::allowed_origins_from_env(),
        }
    }
}

impl MiddlewareConfig {
    /// Read the CORS origin allowlist from `CORS_ALLOWED_ORIGINS`.
    fn allowed_origins_from_env() -> Vec<String> {
        std::env::var("CORS_ALLOWED_ORIGINS")
            .map(|raw| {
                raw.split(',')
                    .map(|origin| origin.trim().to_string())
                    .filter(|origin| !origin.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub struct MiddlewarePipeline {
    config: Arc<MiddlewareConfig>,
}

impl MiddlewarePipeline {
    pub fn new(config: MiddlewareConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    pub fn default() -> Self {
        Self::new(MiddlewareConfig::default())
    }

    pub fn get_config(&self) -> Arc<MiddlewareConfig> {
        self.config.clone()
    }
}

/// Request logging middleware
pub async fn request_logging_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let timestamp = chrono::Local::now().to_rfc3339();

    tracing::info!(
        target: "api_requests",
        method = %method,
        uri = %uri,
        timestamp = %timestamp,
        "Incoming request"
    );

    next.run(req).await
}

/// Response timing middleware
pub async fn response_timing_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    tracing::info!(
        target: "api_timing",
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = duration.as_millis(),
        "Request completed"
    );

    response
}

/// Request validation middleware
pub async fn request_validation_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = req.method();

    // Validate method
    if !matches!(
        *method,
        axum::http::Method::GET
            | axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::DELETE
            | axum::http::Method::PATCH
    ) {
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }

    Ok(next.run(req).await)
}

/// Returns true when the request targets a public read route that may
/// receive CORS headers. Admin and recovery routes are always excluded.
pub fn is_public_read_route(method: &axum::http::Method, path: &str) -> bool {
    if method != axum::http::Method::GET {
        return false;
    }

    // Never expose admin or recovery routes.
    if path.starts_with("/v1/admin") || path.starts_with("/v1/auth/recovery") {
        return false;
    }

    path.starts_with("/v1/ip")
        || path.starts_with("/v1/swap")
        || path == "/graphql"
        || path == "/health"
        || path == "/version"
}

/// CORS middleware.
///
/// Only echoes `Access-Control-Allow-Origin` for allowlisted origins on
/// public read routes. All other requests (including admin and recovery
/// routes, and unlisted origins) receive no CORS headers at all.
pub async fn cors_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let origin = req
        .headers()
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let allowed_origins = MiddlewareConfig::allowed_origins_from_env();
    let origin_allowed = origin
        .as_deref()
        .map(|o| allowed_origins.iter().any(|allowed| allowed == o))
        .unwrap_or(false);

    let is_public = is_public_read_route(&method, &path);

    // Handle preflight requests for allowed origins on public routes.
    if method == axum::http::Method::OPTIONS {
        if is_public && origin_allowed {
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::NO_CONTENT;
            apply_cors_headers(response.headers_mut(), origin.as_deref().unwrap());
            return response;
        }
        return next.run(req).await;
    }

    let mut response = next.run(req).await;

    if is_public && origin_allowed {
        apply_cors_headers(response.headers_mut(), origin.as_deref().unwrap());
    }

    response
}

/// Insert the CORS response headers for an allowed origin.
fn apply_cors_headers(headers: &mut axum::http::HeaderMap, origin: &str) {
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        origin.parse().unwrap(),
    );
    headers.insert(
        axum::http::header::VARY,
        "Origin".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_METHODS,
        "GET, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS,
        "Content-Type, Authorization, X-Signature, X-Timestamp, X-Public-Key, X-API-Key"
            .parse()
            .unwrap(),
    );
    headers.insert(
        axum::http::header::ACCESS_CONTROL_EXPOSE_HEADERS,
        "X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset, X-RateLimit-Scope, X-RateLimit-Tier, Retry-After"
            .parse()
            .unwrap(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_middleware_config_default() {
        let config = MiddlewareConfig::default();
        assert!(config.enable_request_logging);
        assert!(config.enable_response_timing);
        assert!(config.enable_request_validation);
        assert!(!config.enable_rate_limiting);
        assert!(config.enable_cors);
    }

    #[test]
    fn test_middleware_pipeline_creation() {
        let pipeline = MiddlewarePipeline::default();
        let config = pipeline.get_config();
        assert!(config.enable_request_logging);
    }

    #[test]
    fn test_middleware_pipeline_custom_config() {
        let config = MiddlewareConfig {
            enable_request_logging: false,
            enable_response_timing: true,
            enable_request_validation: true,
            enable_rate_limiting: true,
            enable_cors: false,
            allowed_origins: vec!["https://example.com".to_string()],
        };
        let pipeline = MiddlewarePipeline::new(config);
        let cfg = pipeline.get_config();
        assert!(!cfg.enable_request_logging);
        assert!(cfg.enable_rate_limiting);
        assert_eq!(cfg.allowed_origins, vec!["https://example.com".to_string()]);
    }

    #[test]
    fn test_is_public_read_route() {
        use axum::http::Method;

        assert!(is_public_read_route(&Method::GET, "/v1/ip/1.2.3.4"));
        assert!(is_public_read_route(&Method::GET, "/v1/swap/quote"));
        assert!(is_public_read_route(&Method::GET, "/graphql"));
        assert!(is_public_read_route(&Method::GET, "/health"));
        assert!(is_public_read_route(&Method::GET, "/version"));

        // Non-GET methods are not public read routes.
        assert!(!is_public_read_route(&Method::POST, "/v1/ip/1.2.3.4"));

        // Admin and recovery routes are never public.
        assert!(!is_public_read_route(&Method::GET, "/v1/admin/stats"));
        assert!(!is_public_read_route(&Method::GET, "/v1/auth/recovery/status"));
    }

    #[test]
    fn test_apply_cors_headers_echoes_origin() {
        let mut headers = axum::http::HeaderMap::new();
        apply_cors_headers(&mut headers, "https://allowed.example");

        assert_eq!(
            headers.get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://allowed.example"
        );
        assert_eq!(
            headers.get(axum::http::header::VARY).unwrap(),
            "Origin"
        );
        assert!(headers
            .get(axum::http::header::ACCESS_CONTROL_ALLOW_METHODS)
            .is_some());
    }

    #[test]
    fn test_unlisted_origin_receives_no_allow_header() {
        use axum::http::header::ORIGIN;

        let allowed = vec!["https://allowed.example".to_string()];
        let origin = "https://evil.example";

        // Simulate the decision logic used by cors_middleware.
        let origin_allowed = allowed.iter().any(|a| a == origin);
        let is_public = is_public_read_route(&axum::http::Method::GET, "/v1/ip/1.2.3.4");

        let mut headers = axum::http::HeaderMap::new();
        if is_public && origin_allowed {
            apply_cors_headers(&mut headers, origin);
        }

        assert!(headers.get(ORIGIN).is_none());
        assert!(headers
            .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }

    #[test]
    fn test_admin_and_recovery_paths_get_no_cors_headers() {
        let allowed = vec!["https://allowed.example".to_string()];
        let origin = "https://allowed.example";

        for path in ["/v1/admin/stats", "/v1/auth/recovery/status"] {
            let origin_allowed = allowed.iter().any(|a| a == origin);
            let is_public = is_public_read_route(&axum::http::Method::GET, path);

            let mut headers = axum::http::HeaderMap::new();
            if is_public && origin_allowed {
                apply_cors_headers(&mut headers, origin);
            }

            assert!(
                headers
                    .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                    .is_none(),
                "path {path} must not receive CORS headers"
            );
        }
    }
}
