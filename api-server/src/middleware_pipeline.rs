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
}

impl Default for MiddlewareConfig {
    fn default() -> Self {
        Self {
            enable_request_logging: true,
            enable_response_timing: true,
            enable_request_validation: true,
            enable_rate_limiting: false,
            enable_cors: true,
        }
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
        method,
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

/// CORS middleware
pub async fn cors_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(req).await;

    response.headers_mut().insert(
        "Access-Control-Allow-Origin",
        "*".parse().unwrap(),
    );
    response.headers_mut().insert(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, PATCH, OPTIONS".parse().unwrap(),
    );
    response.headers_mut().insert(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, X-Signature, X-Timestamp, X-Public-Key"
            .parse()
            .unwrap(),
    );

    response
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
        };
        let pipeline = MiddlewarePipeline::new(config);
        let cfg = pipeline.get_config();
        assert!(!cfg.enable_request_logging);
        assert!(cfg.enable_rate_limiting);
    }
}
