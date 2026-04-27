use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::extract::Request;
use serde::{Deserialize, Serialize};

/// Current API version
pub const CURRENT_VERSION: &str = "1.0.0";

/// Supported API versions
pub const SUPPORTED_VERSIONS: &[&str] = &["1.0.0"];

/// API version information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiVersion {
    pub requested: String,
    pub current: String,
}

/// Version routing configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub status: String,
    pub supported_versions: Vec<String>,
    pub deprecation_date: Option<String>,
}

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

/// Get version information endpoint
pub async fn get_version_info() -> axum::Json<VersionInfo> {
    axum::Json(VersionInfo {
        version: CURRENT_VERSION.to_string(),
        status: "stable".to_string(),
        supported_versions: SUPPORTED_VERSIONS.iter().map(|v| v.to_string()).collect(),
        deprecation_date: None,
    })
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

    #[test]
    fn test_version_info_structure() {
        let info = VersionInfo {
            version: CURRENT_VERSION.to_string(),
            status: "stable".to_string(),
            supported_versions: SUPPORTED_VERSIONS.iter().map(|v| v.to_string()).collect(),
            deprecation_date: None,
        };
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.status, "stable");
        assert!(!info.supported_versions.is_empty());
    }

    #[test]
    fn test_api_version_struct() {
        let version = ApiVersion {
            requested: "1.0.0".to_string(),
            current: "1.0.0".to_string(),
        };
        assert_eq!(version.requested, version.current);
    }
}
