use crate::auth;
use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct SignaturePayload {
    pub method: String,
    pub path: String,
    pub timestamp: u64,
    pub body_hash: String,
}

/// Default window (seconds) within which a request's `X-Timestamp` must fall
/// of the server's clock, overridable via `REQUEST_SIGNATURE_SKEW_SECS`.
const DEFAULT_TIMESTAMP_SKEW_SECS: u64 = 300;

fn timestamp_skew_secs() -> u64 {
    std::env::var("REQUEST_SIGNATURE_SKEW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TIMESTAMP_SKEW_SECS)
}

/// The canonical bytes signed/verified for a request: binds method, path,
/// timestamp, and body hash together so a signature cannot be replayed
/// against a different endpoint, time, or body.
fn signing_payload(method: &str, path: &str, timestamp: u64, body_hash: &str) -> String {
    format!("{}||{}||{}||{}", method, path, timestamp, body_hash)
}

/// Sign a request with an Ed25519 keypair. The signed message is the SHA-256
/// digest of the canonical payload (Stellar convention — see
/// `auth::verify_stellar_signature`, which this must round-trip with).
pub fn generate_signature(
    method: &str,
    path: &str,
    timestamp: u64,
    body_hash: &str,
    signing_key: &SigningKey,
) -> String {
    let payload = signing_payload(method, path, timestamp, body_hash);
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    let message_hash = hasher.finalize();
    let signature = signing_key.sign(&message_hash);
    hex::encode(signature.to_bytes())
}

/// Compute SHA256 hash of request body
pub fn hash_body(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Verify a request signature against the caller's Stellar Ed25519 public
/// key. This is real asymmetric verification (mirrors
/// `auth::verify_stellar_signature`): only the holder of the private key
/// matching `public_key` can produce a signature that passes. Never panics —
/// any malformed signature or key simply fails verification.
pub fn verify_signature(
    method: &str,
    path: &str,
    timestamp: u64,
    body_hash: &str,
    signature: &str,
    public_key: &str,
) -> bool {
    let payload = signing_payload(method, path, timestamp, body_hash);
    auth::verify_stellar_signature(public_key, &payload, signature).unwrap_or(false)
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

    // Check timestamp is within the configured skew window (replay protection).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let skew = now.abs_diff(timestamp);
    if skew > timestamp_skew_secs() {
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

    fn test_keypair(seed: u8) -> (SigningKey, String) {
        let signing_key = SigningKey::from_bytes(&[seed; 32]);
        let verifying_key = signing_key.verifying_key();
        let public_key =
            stellar_strkey::Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey(
                verifying_key.to_bytes(),
            ))
            .to_string();
        (signing_key, public_key)
    }

    #[test]
    fn test_signature_generation() {
        let (signing_key, _) = test_keypair(1);
        let signature = generate_signature("POST", "/ip/commit", 1234567890, "body_hash", &signing_key);
        assert!(!signature.is_empty());
    }

    #[test]
    fn test_signature_verification() {
        let (signing_key, public_key) = test_keypair(2);
        let signature = generate_signature("POST", "/ip/commit", 1234567890, "body_hash", &signing_key);

        assert!(verify_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "body_hash",
            &signature,
            &public_key,
        ));
    }

    /// Issue #793: a signature produced by any key other than the one behind
    /// `public_key` must be rejected — this is what makes it a real signature
    /// rather than an unkeyed checksum of public data.
    #[test]
    fn test_signature_from_wrong_key_is_rejected() {
        let (wrong_signing_key, _) = test_keypair(3);
        let (_, real_public_key) = test_keypair(4);

        let forged = generate_signature("POST", "/ip/commit", 1234567890, "body_hash", &wrong_signing_key);

        assert!(!verify_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "body_hash",
            &forged,
            &real_public_key,
        ));
    }

    /// Issue #793: a signature computed over one path/body cannot be replayed
    /// against a different path or body after the attacker recomputes the
    /// old-style unkeyed hash — the signature is bound to the exact payload.
    #[test]
    fn test_signature_rejected_when_path_tampered() {
        let (signing_key, public_key) = test_keypair(5);
        let signature = generate_signature("POST", "/ip/commit", 1234567890, "body_hash", &signing_key);

        assert!(!verify_signature(
            "POST",
            "/ip/transfer",
            1234567890,
            "body_hash",
            &signature,
            &public_key,
        ));
    }

    #[test]
    fn test_signature_rejected_when_body_tampered() {
        let (signing_key, public_key) = test_keypair(6);
        let signature = generate_signature("POST", "/ip/commit", 1234567890, "body_hash", &signing_key);

        assert!(!verify_signature(
            "POST",
            "/ip/commit",
            1234567890,
            "tampered_body_hash",
            &signature,
            &public_key,
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
        let valid_key = "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3XVQCRWGSGAX";
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

    // ── Issue #793: request-level integration tests ────────────────────────
    // These exercise `verify_request_signature` mounted as real Axum
    // middleware in front of a handler, not just the bare function.

    fn signed_app() -> axum::Router {
        axum::Router::new()
            .route("/protected", axum::routing::post(|| async { "ok" }))
            .layer(axum::middleware::from_fn(verify_request_signature))
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[tokio::test]
    async fn test_request_with_valid_signature_is_allowed() {
        use tower::ServiceExt;

        let (signing_key, public_key) = test_keypair(10);
        let body = b"{}".to_vec();
        let timestamp = now_secs();
        let body_hash = hash_body(&body);
        let signature =
            generate_signature("POST", "/protected", timestamp, &body_hash, &signing_key);

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/protected")
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp.to_string())
            .header("X-Public-Key", public_key)
            .body(axum::body::Body::from(body))
            .unwrap();

        let resp = signed_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    /// This must fail against the pre-fix code: with an unkeyed hash, any
    /// caller could compute a "valid" signature for any request without
    /// possessing any key. With real Ed25519 verification, a request signed
    /// under a different key than the one advertised in `X-Public-Key` is
    /// rejected.
    #[tokio::test]
    async fn test_request_with_wrong_key_signature_is_rejected() {
        use tower::ServiceExt;

        let (wrong_signing_key, _) = test_keypair(11);
        let (_, real_public_key) = test_keypair(12);
        let body = b"{}".to_vec();
        let timestamp = now_secs();
        let body_hash = hash_body(&body);
        let forged =
            generate_signature("POST", "/protected", timestamp, &body_hash, &wrong_signing_key);

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/protected")
            .header("X-Signature", forged)
            .header("X-Timestamp", timestamp.to_string())
            .header("X-Public-Key", real_public_key)
            .body(axum::body::Body::from(body))
            .unwrap();

        let resp = signed_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_request_with_tampered_body_is_rejected() {
        use tower::ServiceExt;

        let (signing_key, public_key) = test_keypair(13);
        let signed_body = b"{}".to_vec();
        let timestamp = now_secs();
        let body_hash = hash_body(&signed_body);
        let signature =
            generate_signature("POST", "/protected", timestamp, &body_hash, &signing_key);

        // Send a different body than the one that was signed.
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/protected")
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp.to_string())
            .header("X-Public-Key", public_key)
            .body(axum::body::Body::from(b"{\"tampered\":true}".to_vec()))
            .unwrap();

        let resp = signed_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_request_missing_signature_headers_is_rejected() {
        use tower::ServiceExt;

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/protected")
            .body(axum::body::Body::from(b"{}".to_vec()))
            .unwrap();

        let resp = signed_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
    }
}
