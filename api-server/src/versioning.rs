use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::extract::Request;

/// Current API version
pub const CURRENT_VERSION: &str = "1.0.0";

/// Supported API versions
pub const SUPPORTED_VERSIONS: &[&str] = &["1.0.0"];

/// Middleware to handle API versioning via Accept-Version header
pub async fn version_negotiation(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let requested_version = headers
        .get("Accept-Version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(CURRENT_VERSION);

    // Check if requested version is supported
    if !SUPPORTED_VERSIONS.contains(&requested_version) {
        return Err(StatusCode::NOT_ACCEPTABLE);
    }

    // Store version in request extensions for handlers
    req.extensions_mut().insert(ApiVersion {
        requested: requested_version.to_string(),
        current: CURRENT_VERSION.to_string(),
    });

    let mut response = next.run(req).await;

    // Add API version to response headers
    response.headers_mut().insert(
        "API-Version",
        CURRENT_VERSION.parse().unwrap(),
    );

    // Add deprecation warning if requesting old version
    if requested_version != CURRENT_VERSION {
        response.headers_mut().insert(
            "Deprecation",
            "true".parse().unwrap(),
        );
        response.headers_mut().insert(
            "Sunset",
            "Sun, 31 Dec 2026 23:59:59 GMT".parse().unwrap(),
        );
    }

    Ok(response)
}

#[derive(Clone, Debug)]
pub struct ApiVersion {
    pub requested: String,
    pub current: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version_is_supported() {
        assert!(SUPPORTED_VERSIONS.contains(&CURRENT_VERSION));
    }

    #[test]
    fn test_unsupported_version_rejected() {
        let unsupported = "2.0.0";
        assert!(!SUPPORTED_VERSIONS.contains(&unsupported));
    }
}
