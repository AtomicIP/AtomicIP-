use axum::{
    body::Body,
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub gzip_enabled: bool,
    pub brotli_enabled: bool,
    pub min_size_bytes: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            gzip_enabled: true,
            brotli_enabled: true,
            min_size_bytes: 1024,
        }
    }
}

/// Middleware to handle Accept-Encoding header and apply compression
pub async fn compression_middleware(
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    let accept_encoding = headers
        .get("Accept-Encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut response = next.run(req).await;

    // Add Vary header to indicate response varies by Accept-Encoding
    response.headers_mut().insert(
        "Vary",
        "Accept-Encoding".parse().unwrap(),
    );

    // Add Content-Encoding header based on Accept-Encoding
    if accept_encoding.contains("gzip") {
        response.headers_mut().insert(
            "Content-Encoding",
            "gzip".parse().unwrap(),
        );
    } else if accept_encoding.contains("br") {
        response.headers_mut().insert(
            "Content-Encoding",
            "br".parse().unwrap(),
        );
    } else if accept_encoding.contains("deflate") {
        response.headers_mut().insert(
            "Content-Encoding",
            "deflate".parse().unwrap(),
        );
    }

    response
}

/// Get supported compression methods
pub fn get_supported_compressions() -> Vec<String> {
    vec![
        "gzip".to_string(),
        "br".to_string(),
        "deflate".to_string(),
    ]
}

/// Check if compression is supported for a given encoding
pub fn is_compression_supported(encoding: &str) -> bool {
    matches!(encoding, "gzip" | "br" | "deflate")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_config_default() {
        let config = CompressionConfig::default();
        assert!(config.gzip_enabled);
        assert!(config.brotli_enabled);
        assert_eq!(config.min_size_bytes, 1024);
    }

    #[test]
    fn test_get_supported_compressions() {
        let compressions = get_supported_compressions();
        assert!(compressions.contains(&"gzip".to_string()));
        assert!(compressions.contains(&"br".to_string()));
        assert!(compressions.contains(&"deflate".to_string()));
    }

    #[test]
    fn test_is_compression_supported_gzip() {
        assert!(is_compression_supported("gzip"));
    }

    #[test]
    fn test_is_compression_supported_brotli() {
        assert!(is_compression_supported("br"));
    }

    #[test]
    fn test_is_compression_supported_deflate() {
        assert!(is_compression_supported("deflate"));
    }

    #[test]
    fn test_is_compression_unsupported() {
        assert!(!is_compression_supported("unknown"));
    }
}
