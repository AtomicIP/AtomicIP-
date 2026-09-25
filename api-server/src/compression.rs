use axum::{
    body::Body,
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use std::io::Write;

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

/// Compress data using gzip
pub fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// Compress data using brotli
pub fn compress_brotli(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut output = Vec::new();
    let mut writer = brotli::CompressorWriter::new(&mut output, 4096, 11, 22);
    writer.write_all(data)?;
    drop(writer);
    Ok(output)
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

    if response.headers().contains_key("Content-Encoding")
        || response.status() == axum::http::StatusCode::NO_CONTENT
        || response.status() == axum::http::StatusCode::NOT_MODIFIED
    {
        return response;
    }

    let config = CompressionConfig::default();
    let encoding = if config.brotli_enabled && accept_encoding.contains("br") {
        Some("br")
    } else if config.gzip_enabled && accept_encoding.contains("gzip") {
        Some("gzip")
    } else {
        None
    };

    let Some(encoding) = encoding else {
        return response;
    };

    let body = std::mem::replace(response.body_mut(), Body::empty());
    let Ok(collected) = body.collect().await else {
        return response;
    };
    let body = collected.to_bytes();
    if body.len() < config.min_size_bytes {
        *response.body_mut() = Body::from(body);
        return response;
    }

    let compressed = match encoding {
        "br" => compress_brotli(&body),
        "gzip" => compress_gzip(&body),
        _ => unreachable!(),
    };
    let Ok(compressed) = compressed else {
        *response.body_mut() = Body::from(body);
        return response;
    };

    response.headers_mut().insert(
        "Content-Encoding",
        encoding.parse().expect("static encoding is valid"),
    );
    response.headers_mut().remove("Content-Length");
    response.headers_mut().insert(
        "Content-Length",
        compressed.len().to_string().parse().expect("length is valid"),
    );
    *response.body_mut() = Body::from(compressed);

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
