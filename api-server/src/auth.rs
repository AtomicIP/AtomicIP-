use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{request::Parts, header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// JWT claims.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // public key
    pub exp: i64,
    pub iat: i64,
    pub token_type: String, // "access" or "refresh"
}

/// Extension key for authenticated claims.
pub struct AuthExtension(pub Claims);

/// JWT secret — in production this should come from env.
static JWT_SECRET: Lazy<Mutex<Vec<u8>>> = Lazy::new(|| {
    let secret = rand::random::<[u8; 32]>().to_vec();
    Mutex::new(secret)
});

fn encoding_key() -> EncodingKey {
    let secret = JWT_SECRET.lock().unwrap();
    EncodingKey::from_secret(&secret)
}

fn decoding_key() -> DecodingKey {
    let secret = JWT_SECRET.lock().unwrap();
    DecodingKey::from_secret(&secret)
}

/// Issue an access token (15 min) and refresh token (7 days).
pub fn issue_tokens(public_key: &str) -> Result<(String, String), jsonwebtoken::errors::Error> {
    let now = Utc::now();

    let access_claims = Claims {
        sub: public_key.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::minutes(15)).timestamp(),
        token_type: "access".to_string(),
    };

    let refresh_claims = Claims {
        sub: public_key.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::days(7)).timestamp(),
        token_type: "refresh".to_string(),
    };

    let access = encode(&Header::default(), &access_claims, &encoding_key())?;
    let refresh = encode(&Header::default(), &refresh_claims, &encoding_key())?;

    Ok((access, refresh))
}

/// Refresh access token using a valid refresh token.
pub fn refresh_access_token(refresh_token: &str) -> Result<String, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(refresh_token, &decoding_key(), &validation)
        .map_err(|_| AuthError::InvalidToken)?;

    if token_data.claims.token_type != "refresh" {
        return Err(AuthError::InvalidToken);
    }

    let now = Utc::now();
    let new_claims = Claims {
        sub: token_data.claims.sub,
        iat: now.timestamp(),
        exp: (now + Duration::minutes(15)).timestamp(),
        token_type: "access".to_string(),
    };

    encode(&Header::default(), &new_claims, &encoding_key())
        .map_err(|_| AuthError::TokenCreation)
}

/// Verify a Stellar Ed25519 signature.
/// `public_key` is the Stellar G-strkey, `message` is the signed payload, `signature` is hex-encoded.
pub fn verify_stellar_signature(
    public_key: &str,
    message: &str,
    signature_hex: &str,
) -> Result<bool, AuthError> {
    // Decode Stellar public key strkey -> raw bytes
    let pk_bytes = stellar_strkey::Strkey::from_string(public_key)
        .map_err(|_| AuthError::InvalidPublicKey)?;

    let pk_raw = match pk_bytes {
        stellar_strkey::Strkey::PublicKeyEd25519(pk) => pk.0.to_vec(),
        _ => return Err(AuthError::InvalidPublicKey),
    };

    let verifying_key = VerifyingKey::from_slice(&pk_raw)
        .map_err(|_| AuthError::InvalidPublicKey)?;

    let signature_bytes = hex::decode(signature_hex)
        .map_err(|_| AuthError::InvalidSignature)?;

    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| AuthError::InvalidSignature)?;

    // Sign the SHA-256 hash of the message (Stellar convention)
    let mut hasher = Sha256::new();
    hasher.update(message.as_bytes());
    let message_hash = hasher.finalize();

    Ok(verifying_key.verify(&message_hash, &signature).is_ok())
}

/// Auth middleware: extracts Bearer token, validates JWT, injects Claims into extensions.
pub async fn require_auth(mut req: Request, next: Next) -> Result<Response, AuthError> {
    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidToken)?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(token, &decoding_key(), &validation)
        .map_err(|_| AuthError::InvalidToken)?;

    if token_data.claims.token_type != "access" {
        return Err(AuthError::InvalidToken);
    }

    req.extensions_mut().insert(AuthExtension(token_data.claims));
    Ok(next.run(req).await)
}

/// Custom auth errors with HTTP responses.
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    InvalidPublicKey,
    InvalidSignature,
    TokenCreation,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authorization token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid or expired token"),
            AuthError::InvalidPublicKey => (StatusCode::BAD_REQUEST, "Invalid public key"),
            AuthError::InvalidSignature => (StatusCode::BAD_REQUEST, "Invalid signature format"),
            AuthError::TokenCreation => (StatusCode::INTERNAL_SERVER_ERROR, "Token creation failed"),
        };
        let body = serde_json::json!({ "error": msg });
        (status, Json(body)).into_response()
    }
}

/// Extractor for authenticated claims from request extensions.
#[async_trait]
impl<S> FromRequestParts<S> for AuthExtension
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthExtension>()
            .cloned()
            .ok_or(AuthError::InvalidToken)
    }
}

impl Clone for AuthExtension {
    fn clone(&self) -> Self {
        AuthExtension(self.0.clone())
    }
}

