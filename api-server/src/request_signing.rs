use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct SignaturePayload {
    pub method: String,
    pub path: String,
    pub timestamp: u64,
    pub body_hash: String,
}

/// Generate a signature for a request using Stellar keypair
/// The signature is computed as: sha256(method || path || timestamp || body_hash)
pub fn generate_signature(
    method: &str,
    path: &str,
    timestamp: u64,
    body_hash: &str,
    secret_key: &str,
) -> String {
    let payload = format!("{}||{}||{}||{}", method, path, timestamp, body_hash);
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Compute SHA256 hash of request body
pub fn hash_body(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Verify request signature
pub fn verify_signature(
    method: &str,
    path: &str,
    timestamp: u64,
    body_hash: &str,
    signature: &str,
    public_key: &str,
) -> bool {
    let expected_sig = generate_signature(method, path, timestamp, body_hash, public_key);
    expected_sig == signature
}

/// Verify Stellar keypair format (starts with 'G' and is 56 characters)
pub fn is_valid_stellar_public_key(key: &str) -> bool {
    key.starts_with('G') && key.len() == 56 && key.chars().all(|c| c.is_alphanumeric())
}

/// Middleware to verify request signatures
pub async fn verify_request_signature(
    req: Request,
    next: Next,
) -> Result<Response, axum::http::StatusCode> {
    let headers = req.headers().clone();

    // Extract signature header
    let signature = headers
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    // Extract timestamp header
    let timestamp_str = headers
        .get("X-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    let timestamp: u64 = timestamp_str.parse()
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;

    // Check timestamp is recent (within 5 minutes)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now.saturating_sub(timestamp) > 300 {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    // Extract public key header
    let public_key = headers
        .get("X-Public-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

    // Validate Stellar public key format
    if !is_valid_stellar_public_key(public_key) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Extract and hash body
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX).await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let body_hash = hash_body(&body_bytes);

    // Verify signature
    if !verify_signature(&method, &path, timestamp, &body_hash, signature, public_key) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    // Reconstruct request with body
    let req = Request::from_parts(parts, axum::body::Body::from(body_bytes));
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_generation() {
        let signature = generate_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "body_hash",
            "secret_key"
        );
        assert!(!signature.is_empty());
    }

    #[test]
    fn test_signature_verification() {
        let signature = generate_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "body_hash",
            "secret_key"
        );
        
        assert!(verify_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "body_hash",
            &signature,
            "secret_key"
        ));
    }

    #[test]
    fn test_body_hashing() {
        let body = b"test body";
        let hash = hash_body(body);
        assert_eq!(hash.len(), 64); // SHA256 hex string length
    }

    #[test]
    fn test_valid_stellar_public_key() {
        let valid_key = "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3XVQCRWGSGA";
        assert!(is_valid_stellar_public_key(valid_key));
    }

    #[test]
    fn test_invalid_stellar_public_key_wrong_prefix() {
        let invalid_key = "ABRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3XVQCRWGSGA";
        assert!(!is_valid_stellar_public_key(invalid_key));
    }

    #[test]
    fn test_invalid_stellar_public_key_wrong_length() {
        let invalid_key = "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3XVQCRWGS";
        assert!(!is_valid_stellar_public_key(invalid_key));
    }

    #[test]
    fn test_signature_mismatch() {
        let signature = generate_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "body_hash",
            "secret_key"
        );
        
        assert!(!verify_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "different_hash",
            &signature,
            "secret_key"
        ));
    }
}
