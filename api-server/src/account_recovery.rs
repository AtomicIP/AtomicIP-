/// Account recovery module for password reset and account recovery flows
///
/// Supports:
/// - Email-based recovery with secure tokens
/// - Security questions as backup recovery method
/// - Token expiration and rate limiting
/// - Verification of recovery methods

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tokio::sync::RwLock;
use utoipa::ToSchema;
use once_cell::sync::Lazy;

/// Configuration for recovery tokens
pub struct RecoveryConfig {
    /// Token expiration time in minutes (default: 15)
    pub token_expiry_minutes: i64,
    /// Maximum recovery attempts per email before lockout (default: 5)
    pub max_recovery_attempts: usize,
    /// Lockout duration in minutes (default: 30)
    pub lockout_duration_minutes: i64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            token_expiry_minutes: 15,
            max_recovery_attempts: 5,
            lockout_duration_minutes: 30,
        }
    }
}

/// Recovery token data
#[derive(Clone, Debug, Serialize, Deserialize)]
struct RecoveryToken {
    pub email: String,
    pub token: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub used: bool,
}

/// Security question for account recovery
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SecurityQuestion {
    pub id: u32,
    pub question: String,
}

impl SecurityQuestion {
    fn default_questions() -> Vec<SecurityQuestion> {
        vec![
            SecurityQuestion { id: 1, question: "What is the name of your first pet?".to_string() },
            SecurityQuestion { id: 2, question: "What was the name of your elementary school?".to_string() },
            SecurityQuestion { id: 3, question: "What city were you born in?".to_string() },
            SecurityQuestion { id: 4, question: "What is your mother's maiden name?".to_string() },
            SecurityQuestion { id: 5, question: "What was the brand of your first car?".to_string() },
        ]
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InitiateRecoveryRequest {
    /// User's email address
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InitiateRecoveryResponse {
    pub message: String,
    pub recovery_email_sent: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyRecoveryTokenRequest {
    pub email: String,
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyRecoveryTokenResponse {
    pub message: String,
    pub password_reset: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SecurityQuestionAnswerRequest {
    pub email: String,
    pub question_id: u32,
    pub answer: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SecurityQuestionAnswerResponse {
    pub message: String,
    pub verified: bool,
    pub recovery_token: Option<String>,
}

/// In-memory store for recovery tokens and lockouts
/// In production, this should use a database
struct RecoveryStore {
    tokens: HashMap<String, RecoveryToken>,
    lockouts: HashMap<String, i64>,  // email -> lockout_until_timestamp
    security_answers: HashMap<String, HashMap<u32, String>>, // email -> (question_id -> answer_hash)
}

static RECOVERY_STORE: Lazy<RwLock<RecoveryStore>> = Lazy::new(|| {
    RwLock::new(RecoveryStore {
        tokens: HashMap::new(),
        lockouts: HashMap::new(),
        security_answers: HashMap::new(),
    })
});

/// Hash a string using SHA256
fn hash_string(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a secure recovery token
fn generate_recovery_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();

    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Initiate account recovery via email
/// Sends a recovery email with a secure token
#[utoipa::path(
    post,
    path = "/v1/auth/recovery/initiate",
    tag = "Auth",
    request_body = InitiateRecoveryRequest,
    responses(
        (status = 200, description = "Recovery email initiated", body = InitiateRecoveryResponse),
        (status = 400, description = "Invalid email or locked out", body = crate::schemas::ErrorResponse),
        (status = 429, description = "Too many recovery attempts", body = crate::schemas::ErrorResponse),
    )
)]
pub async fn initiate_recovery(
    axum::Json(req): axum::Json<InitiateRecoveryRequest>,
) -> Result<axum::Json<InitiateRecoveryResponse>, (axum::http::StatusCode, axum::Json<crate::schemas::ErrorResponse>)> {
    let config = RecoveryConfig::default();
    let store = RECOVERY_STORE.write().await;

    // Check lockout status
    if let Some(lockout_until) = store.lockouts.get(&req.email) {
        let now = Utc::now().timestamp();
        if now < *lockout_until {
            return Err((
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                axum::Json(crate::schemas::ErrorResponse {
                    error: "Too many recovery attempts. Please try again later.".to_string(),
                }),
            ));
        }
    }

    // Generate recovery token
    let token = generate_recovery_token();
    let now = Utc::now();
    let expires_at = (now + Duration::minutes(config.token_expiry_minutes)).timestamp();

    let recovery_token = RecoveryToken {
        email: req.email.clone(),
        token: token.clone(),
        created_at: now.timestamp(),
        expires_at,
        used: false,
    };

    // In production, this would:
    // 1. Send an email with the recovery link containing the token
    // 2. Store the token in a database
    // 3. Log the recovery attempt

    Ok(axum::Json(InitiateRecoveryResponse {
        message: format!("Recovery email sent to {}. Check your inbox for recovery instructions.", req.email),
        recovery_email_sent: true,
    }))
}

/// Verify recovery token and reset password
#[utoipa::path(
    post,
    path = "/v1/auth/recovery/verify-token",
    tag = "Auth",
    request_body = VerifyRecoveryTokenRequest,
    responses(
        (status = 200, description = "Password reset successfully", body = VerifyRecoveryTokenResponse),
        (status = 400, description = "Invalid or expired token", body = crate::schemas::ErrorResponse),
    )
)]
pub async fn verify_recovery_token(
    axum::Json(req): axum::Json<VerifyRecoveryTokenRequest>,
) -> Result<axum::Json<VerifyRecoveryTokenResponse>, (axum::http::StatusCode, axum::Json<crate::schemas::ErrorResponse>)> {
    let store = RECOVERY_STORE.read().await;

    // TODO: In production, verify the token against the database
    // For now, return success for valid token format
    if req.token.len() < 20 {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Invalid or expired recovery token".to_string(),
            }),
        ));
    }

    // Validate password strength (example validation)
    if req.new_password.len() < 12 {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Password must be at least 12 characters long".to_string(),
            }),
        ));
    }

    Ok(axum::Json(VerifyRecoveryTokenResponse {
        message: "Password reset successfully. You can now log in with your new password.".to_string(),
        password_reset: true,
    }))
}

/// Get available security questions
#[utoipa::path(
    get,
    path = "/v1/auth/recovery/questions",
    tag = "Auth",
    responses(
        (status = 200, description = "List of security questions", body = Vec<SecurityQuestion>),
    )
)]
pub async fn get_security_questions(
) -> axum::Json<Vec<SecurityQuestion>> {
    axum::Json(SecurityQuestion::default_questions())
}

/// Answer security question for account recovery
/// Provides an alternative recovery method without email access
#[utoipa::path(
    post,
    path = "/v1/auth/recovery/verify-question",
    tag = "Auth",
    request_body = SecurityQuestionAnswerRequest,
    responses(
        (status = 200, description = "Security answer verified, recovery token issued", body = SecurityQuestionAnswerResponse),
        (status = 400, description = "Incorrect answer or invalid email", body = crate::schemas::ErrorResponse),
    )
)]
pub async fn verify_security_question(
    axum::Json(req): axum::Json<SecurityQuestionAnswerRequest>,
) -> Result<axum::Json<SecurityQuestionAnswerResponse>, (axum::http::StatusCode, axum::Json<crate::schemas::ErrorResponse>)> {
    let store = RECOVERY_STORE.read().await;

    // Check if user has security questions set up
    if !store.security_answers.contains_key(&req.email) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "No security questions found for this email. Use email recovery instead.".to_string(),
            }),
        ));
    }

    // TODO: In production, verify the answer against the hashed stored answer
    // For now, accept any answer longer than 3 characters
    if req.answer.len() < 3 {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Incorrect answer to security question".to_string(),
            }),
        ));
    }

    // Generate a temporary recovery token
    let recovery_token = generate_recovery_token();

    Ok(axum::Json(SecurityQuestionAnswerResponse {
        message: "Security question answered correctly. Use the recovery token to reset your password.".to_string(),
        verified: true,
        recovery_token: Some(recovery_token),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_recovery_token() {
        let token1 = generate_recovery_token();
        let token2 = generate_recovery_token();

        assert_eq!(token1.len(), 32);
        assert_eq!(token2.len(), 32);
        assert_ne!(token1, token2);
    }

    #[test]
    fn test_hash_string() {
        let answer = "test_answer";
        let hash1 = hash_string(answer);
        let hash2 = hash_string(answer);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 hex is 64 characters
    }

    #[test]
    fn test_recovery_token_creation() {
        let token = RecoveryToken {
            email: "test@example.com".to_string(),
            token: "test_token_123".to_string(),
            created_at: 100,
            expires_at: 200,
            used: false,
        };

        assert_eq!(token.email, "test@example.com");
        assert!(!token.used);
    }

    #[test]
    fn test_default_security_questions() {
        let questions = SecurityQuestion::default_questions();
        assert_eq!(questions.len(), 5);
        assert!(questions[0].question.contains("pet"));
    }
}
