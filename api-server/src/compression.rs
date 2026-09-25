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

/// Compress data using deflate.
pub fn compress_deflate(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
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

    let response = next.run(req).await;

    // Add Vary header to indicate response varies by Accept-Encoding
    let (mut parts, body) = response.into_parts();
    parts.headers.insert("Vary", "Accept-Encoding".parse().unwrap());

    let encoding = if accept_encoding.contains("br") {
        Some(("br", compress_brotli as fn(&[u8]) -> Result<Vec<u8>, std::io::Error>))
    } else if accept_encoding.contains("gzip") {
        Some(("gzip", compress_gzip as fn(&[u8]) -> Result<Vec<u8>, std::io::Error>))
    } else if accept_encoding.contains("deflate") {
        Some(("deflate", compress_deflate as fn(&[u8]) -> Result<Vec<u8>, std::io::Error>))
    } else {
        None
    };

    let should_compress = encoding.is_some()
        && !parts.headers.contains_key("Content-Encoding")
        && parts.status != axum::http::StatusCode::NO_CONTENT
        && parts.status != axum::http::StatusCode::NOT_MODIFIED;

    if !should_compress {
        return Response::from_parts(parts, body);
    }

    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            tracing::error!(%error, "failed to read response body for compression");
            parts.status = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
            return Response::from_parts(
                parts,
                Body::from("failed to prepare response compression"),
            );
        }
    };

    let Some((name, compress)) = encoding else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    if bytes.len() < CompressionConfig::default().min_size_bytes {
        return Response::from_parts(parts, Body::from(bytes));
    }

    match compress(&bytes) {
        Ok(compressed) => {
            parts.headers.insert("Content-Encoding", name.parse().unwrap());
            parts.headers.remove("Content-Length");
            Response::from_parts(parts, Body::from(compressed))
        }
        Err(error) => {
            tracing::error!(%error, encoding = name, "failed to compress response body");
            parts.status = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
            Response::from_parts(parts, Body::from("failed to compress response"))
        }
    }
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

    #[test]
    fn test_deflate_round_trip() {
        let input = b"repeated batch payload ".repeat(100);
        let compressed = compress_deflate(&input).unwrap();
        let mut decoder = flate2::read::ZlibDecoder::new(compressed.as_slice());
        let mut output = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut output).unwrap();
        assert_eq!(output, input);
    }
}
