//! # API Versioning & Deprecation Policy
//!
//! All REST handlers are explicitly attached to a version prefix (`/v1/...`).
//! Breaking changes to RPC mapping and schema layers are isolated behind new API versions
//! (e.g. `/v2/...`), ensuring that v1 clients are never silently affected.
//!
//! ## Deprecation Policy
//! - **Notice Period**: Minimum 6 months prior to retirement of any supported version.
//! - **Headers**: When a deprecated or non-current version is requested:
//!   - `Deprecation: true`
//!   - `Sunset: <RFC 2822 Timestamp>`
//! - **Negotiation**: Clients may specify `Accept-Version: 1.0.0`. Unsupported versions
//!   receive `406 Not Acceptable` with a structured JSON error body.

use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::extract::Request;
use serde::{Deserialize, Serialize};

/// Current API version
pub const CURRENT_VERSION: &str = "1.0.0";

/// Supported API versions
pub const SUPPORTED_VERSIONS: &[&str] = &["1.0.0", "1.1.0"];

/// Explicit list of the 13 core wired contract/API handlers covered under v1
pub const V1_HANDLERS: &[&str] = &[
    "/v1/ip/commit",
    "/v1/ip/{ip_id}",
    "/v1/ip/transfer",
    "/v1/ip/verify",
    "/v1/ip/owner/{owner}",
    "/v1/ip/owner/{owner}/cursor",
    "/v1/swap/initiate",
    "/v1/swap/batch-initiate",
    "/v1/swap/{swap_id}/accept",
    "/v1/swap/{swap_id}/reveal",
    "/v1/swap/{swap_id}/cancel",
    "/v1/swap/{swap_id}/cancel-expired",
    "/v1/swap/{swap_id}",
];

/// API version information attached to request extensions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiVersion {
    pub requested: String,
    pub current: String,
}

/// Deprecation policy details
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeprecationPolicyInfo {
    pub policy: String,
    pub min_notice_days: u32,
    pub sunset_date: Option<String>,
    pub breaking_changes_isolation: String,
}

/// Version routing configuration returned by `/version`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub status: String,
    pub supported_versions: Vec<String>,
    pub deprecation_date: Option<String>,
    pub features: Vec<String>,
    pub wired_handlers_count: usize,
    pub deprecation_policy: DeprecationPolicyInfo,
}

/// Middleware to handle API versioning via Accept-Version header
pub async fn version_negotiation(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, Response> {
    let requested_version = headers
        .get("Accept-Version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(CURRENT_VERSION);

    // Check if requested version is supported
    if !SUPPORTED_VERSIONS.contains(&requested_version) {
        let error_body = serde_json::json!({
            "error": format!("API version '{}' is not supported. Supported versions: {}", requested_version, SUPPORTED_VERSIONS.join(", ")),
            "requested_version": requested_version,
            "supported_versions": SUPPORTED_VERSIONS,
            "current_version": CURRENT_VERSION,
        });
        let res = Response::builder()
            .status(StatusCode::NOT_ACCEPTABLE)
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .header("API-Version", CURRENT_VERSION)
            .body(axum::body::Body::from(serde_json::to_string(&error_body).unwrap()))
            .unwrap();
        return Err(res);
    }

    // Store version in request extensions for handlers
    req.extensions_mut().insert(ApiVersion {
        requested: requested_version.to_string(),
        current: CURRENT_VERSION.to_string(),
    });

    let mut response = next.run(req).await;

    // Add API version to response headers
    if let Ok(version_header) = CURRENT_VERSION.parse() {
        response.headers_mut().insert("API-Version", version_header);
    }

    // Add deprecation warning if requesting old version
    if requested_version != CURRENT_VERSION {
        if let Ok(dep) = "true".parse() {
            response.headers_mut().insert("Deprecation", dep);
        }
        if let Ok(sunset) = "Sun, 31 Dec 2027 23:59:59 GMT".parse() {
            response.headers_mut().insert("Sunset", sunset);
        }
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
            "websocket-push".to_string(),
            "batch-operations".to_string(),
        ],
        wired_handlers_count: V1_HANDLERS.len(),
        deprecation_policy: DeprecationPolicyInfo {
            policy: "Semantic Versioning (MAJOR.MINOR.PATCH). Breaking changes are isolated to new MAJOR URL prefixes (/v2/...) with 6-month deprecation notice.".to_string(),
            min_notice_days: 180,
            sunset_date: None,
            breaking_changes_isolation: "Strict URL prefix and Accept-Version isolation".to_string(),
        },
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
        let unknown = "unknown-v9";
        assert!(!SUPPORTED_VERSIONS.contains(&unknown));
    }

    #[test]
    fn test_v1_handlers_audited_count() {
        assert_eq!(V1_HANDLERS.len(), 13);
        for handler in V1_HANDLERS {
            assert!(handler.starts_with("/v1/"), "Handler {} must have /v1/ prefix", handler);
        }
    }

    #[test]
    fn test_version_info_structure() {
        let info = VersionInfo {
            version: CURRENT_VERSION.to_string(),
            status: "stable".to_string(),
            supported_versions: SUPPORTED_VERSIONS.iter().map(|v| v.to_string()).collect(),
            deprecation_date: None,
            features: vec!["api-versioning".to_string()],
            wired_handlers_count: 13,
            deprecation_policy: DeprecationPolicyInfo {
                policy: "SemVer".to_string(),
                min_notice_days: 180,
                sunset_date: None,
                breaking_changes_isolation: "URL prefix".to_string(),
            },
        };
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.status, "stable");
        assert!(!info.supported_versions.is_empty());
        assert_eq!(info.wired_handlers_count, 13);
        assert_eq!(info.deprecation_policy.min_notice_days, 180);
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
        assert!(SUPPORTED_VERSIONS.len() >= 2);
        assert!(SUPPORTED_VERSIONS.contains(&"1.0.0"));
        assert!(SUPPORTED_VERSIONS.contains(&"1.1.0"));
    }
}

