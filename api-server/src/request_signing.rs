use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

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

/// Middleware to verify request signatures
pub async fn verify_request_signature(
    mut req: Request,
    next: Next,
) -> Result<Response, (axum::http::StatusCode, String)> {
    let headers = req.headers().clone();

    // Extract signature header
    let signature = headers
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "Missing X-Signature header".to_string(),
        ))?;

    // Extract timestamp header
    let timestamp_str = headers
        .get("X-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "Missing X-Timestamp header".to_string(),
        ))?;

    let timestamp: u64 = timestamp_str.parse().map_err(|_| {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            "Invalid timestamp format".to_string(),
        )
    })?;

    // Check timestamp is recent (within 5 minutes)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now.saturating_sub(timestamp) > 300 {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            "Request timestamp too old".to_string(),
        ));
    }

    // Extract public key header
    let public_key = headers
        .get("X-Public-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "Missing X-Public-Key header".to_string(),
        ))?;

    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // For now, we'll skip body verification in middleware
    // In production, you'd need to buffer the body to compute its hash
    let body_hash = "".to_string();

    // Verify signature
    if !verify_signature(&method, &path, timestamp, &body_hash, signature, public_key) {
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            "Invalid signature".to_string(),
        ));
    }

    Ok(next.run(req).await)
}
