use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::extract::Request;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current API version
pub const CURRENT_VERSION: &str = "1.0.0";

/// Supported API versions
pub const SUPPORTED_VERSIONS: &[&str] = &["1.0.0", "1.1.0", "2.0.0"];

/// Deprecated versions that should trigger warnings
pub const DEPRECATED_VERSIONS: &[&str] = &[];

/// Version status labels
pub const VERSION_STATUS_STABLE: &str = "stable";
pub const VERSION_STATUS_DEPRECATED: &str = "deprecated";
pub const VERSION_STATUS_BETA: &str = "beta";

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
    pub features: Vec<String>,
}

/// Version compatibility map: (from, to) -> compatible
static VERSION_COMPATIBILITY: once_cell::sync::Lazy<HashMap<(&'static str, &'static str), bool>> =
    once_cell::sync::Lazy::new(|| {
        let mut m = HashMap::new();
        // 1.0.0 is compatible with 1.0.0 and 1.1.0
        m.insert(("1.0.0", "1.0.0"), true);
        m.insert(("1.0.0", "1.1.0"), true);
        m.insert(("1.1.0", "1.0.0"), true);
        m.insert(("1.1.0", "1.1.0"), true);
        // 2.0.0 has breaking changes from 1.x
        m.insert(("1.0.0", "2.0.0"), false);
        m.insert(("1.1.0", "2.0.0"), false);
        m.insert(("2.0.0", "2.0.0"), true);
        m.insert(("2.0.0", "1.0.0"), false);
        m.insert(("2.0.0", "1.1.0"), false);
        m
    });

/// Check if two versions are compatible (same major version).
pub fn are_versions_compatible(from: &str, to: &str) -> bool {
    VERSION_COMPATIBILITY
        .get(&(from, to))
        .copied()
        .unwrap_or_else(|| {
            let from_major = from.split('.').next().and_then(|s| s.parse::<u32>().ok());
            let to_major = to.split('.').next().and_then(|s| s.parse::<u32>().ok());
            from_major == to_major
        })
}

/// Extract version from URL path (e.g., /v1/...).
pub fn extract_version_from_path(path: &str) -> Option<&'static str> {
    if path.starts_with("/v1/") || path == "/v1" {
        Some("1.0.0")
    } else if path.starts_with("/v2/") || path == "/v2" {
        Some("2.0.0")
    } else {
        None
    }
}

/// Middleware to handle API versioning via Accept-Version or X-API-Version header.
pub async fn version_negotiation(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check X-API-Version header first (more explicit), then fall back to Accept-Version
    let requested_version = headers
        .get("X-API-Version")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get("Accept-Version")
                .and_then(|v| v.to_str().ok())
        })
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

    // Add API version headers
    response.headers_mut().insert(
        "API-Version",
        CURRENT_VERSION.parse().unwrap(),
    );
    response.headers_mut().insert(
        "X-API-Version",
        CURRENT_VERSION.parse().unwrap(),
    );

    // Add deprecation warning if requesting old version or deprecated version
    if requested_version != CURRENT_VERSION || DEPRECATED_VERSIONS.contains(&requested_version) {
        response.headers_mut().insert(
            "Deprecation",
            "true".parse().unwrap(),
        );
        response.headers_mut().insert(
            "Sunset",
            "Sun, 31 Dec 2027 23:59:59 GMT".parse().unwrap(),
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
        features: vec![
            "api-versioning".to_string(),
            "compression".to_string(),
            "request-signing".to_string(),
            "circuit-breaker".to_string(),
        ],
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
        let unsupported = "99.0.0";
        assert!(!SUPPORTED_VERSIONS.contains(&unsupported));
    }

    #[test]
    fn test_version_info_structure() {
        let info = VersionInfo {
            version: CURRENT_VERSION.to_string(),
            status: "stable".to_string(),
            supported_versions: SUPPORTED_VERSIONS.iter().map(|v| v.to_string()).collect(),
            deprecation_date: None,
            features: vec![],
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

    #[test]
    fn test_multiple_versions_supported() {
        assert!(SUPPORTED_VERSIONS.len() >= 3);
        assert!(SUPPORTED_VERSIONS.contains(&"1.0.0"));
        assert!(SUPPORTED_VERSIONS.contains(&"1.1.0"));
        assert!(SUPPORTED_VERSIONS.contains(&"2.0.0"));
    }

    #[test]
    fn test_are_versions_compatible_same_major() {
        assert!(are_versions_compatible("1.0.0", "1.1.0"));
        assert!(are_versions_compatible("1.1.0", "1.0.0"));
    }

    #[test]
    fn test_are_versions_compatible_different_major() {
        assert!(!are_versions_compatible("1.0.0", "2.0.0"));
        assert!(!are_versions_compatible("2.0.0", "1.0.0"));
    }

    #[test]
    fn test_extract_version_from_path_v1() {
        assert_eq!(extract_version_from_path("/v1/ip/1"), Some("1.0.0"));
        assert_eq!(extract_version_from_path("/v1"), Some("1.0.0"));
    }

    #[test]
    fn test_extract_version_from_path_v2() {
        assert_eq!(extract_version_from_path("/v2/ip/1"), Some("2.0.0"));
        assert_eq!(extract_version_from_path("/v2"), Some("2.0.0"));
    }

    #[test]
    fn test_extract_version_from_path_no_match() {
        assert_eq!(extract_version_from_path("/health"), None);
        assert_eq!(extract_version_from_path("/v3/ip/1"), None);
    }
}
