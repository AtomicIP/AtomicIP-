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
use std::time::Duration as StdDuration;
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
    pub token_hash: String,
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
    attempts: HashMap<String, usize>,
    security_answers: HashMap<String, HashMap<u32, String>>, // email -> (question_id -> answer_hash)
}

static RECOVERY_STORE: Lazy<RwLock<RecoveryStore>> = Lazy::new(|| {
    RwLock::new(RecoveryStore {
        tokens: HashMap::new(),
        lockouts: HashMap::new(),
        attempts: HashMap::new(),
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

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn allow_recovery_attempt(
    store: &mut RecoveryStore,
    email: &str,
    config: &RecoveryConfig,
    now: i64,
) -> bool {
    let email = normalize_email(email);
    if store
        .lockouts
        .get(&email)
        .is_some_and(|lockout_until| now < *lockout_until)
    {
        return false;
    }

    if store.lockouts.remove(&email).is_some() {
        store.attempts.remove(&email);
    }

    let attempts = store.attempts.entry(email.clone()).or_default();
    if *attempts >= config.max_recovery_attempts {
        store.lockouts.insert(
            email,
            now + Duration::minutes(config.lockout_duration_minutes).num_seconds(),
        );
        return false;
    }
    *attempts += 1;
    true
}

fn security_answer_matches(
    store: &RecoveryStore,
    email: &str,
    question_id: u32,
    answer: &str,
) -> bool {
    store
        .security_answers
        .get(&normalize_email(email))
        .and_then(|answers| answers.get(&question_id))
        .is_some_and(|expected_hash| hash_string(answer.trim()).as_str() == expected_hash.as_str())
}

fn consume_recovery_token(
    store: &mut RecoveryStore,
    email: &str,
    token: &str,
    now: i64,
) -> bool {
    let Some(stored_token) = store.tokens.get_mut(&normalize_email(email)) else {
        return false;
    };
    if stored_token.used
        || now >= stored_token.expires_at
        || stored_token.token_hash != hash_string(token)
    {
        return false;
    }
    stored_token.used = true;
    true
}

#[derive(Debug)]
enum RecoveryEmailError {
    MissingConfiguration(&'static str),
    InvalidEndpoint,
    InsecureEndpoint,
    Client(reqwest::Error),
    Request(reqwest::Error),
    Rejected(reqwest::StatusCode),
}

#[derive(Serialize)]
struct RecoveryEmailPayload<'a> {
    from: &'a str,
    to: &'a str,
    subject: &'static str,
    text: String,
}

async fn send_recovery_email(
    email: &str,
    token: &str,
    expiry_minutes: i64,
) -> Result<(), RecoveryEmailError> {
    let endpoint = std::env::var("RECOVERY_EMAIL_API_URL")
        .map_err(|_| RecoveryEmailError::MissingConfiguration("RECOVERY_EMAIL_API_URL"))?;
    let api_key = std::env::var("RECOVERY_EMAIL_API_KEY")
        .map_err(|_| RecoveryEmailError::MissingConfiguration("RECOVERY_EMAIL_API_KEY"))?;
    let from = std::env::var("RECOVERY_FROM_EMAIL")
        .map_err(|_| RecoveryEmailError::MissingConfiguration("RECOVERY_FROM_EMAIL"))?;
    if api_key.is_empty() || from.is_empty() {
        return Err(RecoveryEmailError::MissingConfiguration(
            "RECOVERY_EMAIL_API_KEY or RECOVERY_FROM_EMAIL",
        ));
    }

    let endpoint = reqwest::Url::parse(&endpoint).map_err(|_| RecoveryEmailError::InvalidEndpoint)?;
    if endpoint.scheme() != "https" {
        return Err(RecoveryEmailError::InsecureEndpoint);
    }

    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(10))
        .build()
        .map_err(RecoveryEmailError::Client)?;
    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&RecoveryEmailPayload {
            from: &from,
            to: email,
            subject: "Account recovery instructions",
            text: format!(
                "Your account recovery token is: {token}\n\nThis token expires in {expiry_minutes} minutes. If you did not request account recovery, you can ignore this email."
            ),
        })
        .send()
        .await
        .map_err(RecoveryEmailError::Request)?;
    if !response.status().is_success() {
        return Err(RecoveryEmailError::Rejected(response.status()));
    }
    Ok(())
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
        (status = 400, description = "Invalid email", body = crate::schemas::ErrorResponse),
        (status = 429, description = "Too many recovery attempts", body = crate::schemas::ErrorResponse),
        (status = 503, description = "Recovery email delivery is not configured or unavailable", body = crate::schemas::ErrorResponse),
    )
)]
pub async fn initiate_recovery(
    axum::Json(req): axum::Json<InitiateRecoveryRequest>,
) -> Result<axum::Json<InitiateRecoveryResponse>, (axum::http::StatusCode, axum::Json<crate::schemas::ErrorResponse>)> {
    let config = RecoveryConfig::default();
    if req.email.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "email must not be empty".to_string(),
            }),
        ));
    }
    {
        let mut store = RECOVERY_STORE.write().await;
        if !allow_recovery_attempt(&mut store, &req.email, &config, Utc::now().timestamp()) {
            return Err((
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                axum::Json(crate::schemas::ErrorResponse {
                    error: "Too many recovery attempts. Please try again later.".to_string(),
                }),
            ));
        }
    }

    let token = generate_recovery_token();
    if let Err(error) = send_recovery_email(&req.email, &token, config.token_expiry_minutes).await {
        tracing::error!(?error, "recovery email delivery failed");
        return Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Recovery email delivery is unavailable. Please try again later.".to_string(),
            }),
        ));
    }

    let now = Utc::now();
    RECOVERY_STORE.write().await.tokens.insert(
        normalize_email(&req.email),
        RecoveryToken {
            token_hash: hash_string(&token),
            created_at: now.timestamp(),
            expires_at: (now + Duration::minutes(config.token_expiry_minutes)).timestamp(),
            used: false,
        },
    );

    Ok(axum::Json(InitiateRecoveryResponse {
        message: "If recovery is available for this address, instructions have been sent.".to_string(),
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
        (status = 501, description = "Password reset unavailable until credential storage is configured", body = crate::schemas::ErrorResponse),
        (status = 429, description = "Too many recovery attempts", body = crate::schemas::ErrorResponse),
        (status = 400, description = "Invalid or expired token", body = crate::schemas::ErrorResponse),
    )
)]
pub async fn verify_recovery_token(
    axum::Json(req): axum::Json<VerifyRecoveryTokenRequest>,
) -> Result<axum::Json<()>, (axum::http::StatusCode, axum::Json<crate::schemas::ErrorResponse>)> {
    // Validate password strength (example validation)
    if req.new_password.len() < 12 {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Password must be at least 12 characters long".to_string(),
            }),
        ));
    }

    if req.email.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "email must not be empty".to_string(),
            }),
        ));
    }
    let mut store = RECOVERY_STORE.write().await;
    if !allow_recovery_attempt(
        &mut store,
        &req.email,
        &RecoveryConfig::default(),
        Utc::now().timestamp(),
    ) {
        return Err((
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Too many recovery attempts. Please try again later.".to_string(),
            }),
        ));
    }
    if !consume_recovery_token(&mut store, &req.email, &req.token, Utc::now().timestamp()) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Invalid or expired recovery token".to_string(),
            }),
        ));
    }

    Err((
        axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(crate::schemas::ErrorResponse {
            error: "Password reset is unavailable because account credential storage is not configured."
                .to_string(),
        }),
    ))
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
        (status = 429, description = "Too many recovery attempts", body = crate::schemas::ErrorResponse),
        (status = 400, description = "Incorrect answer or invalid email", body = crate::schemas::ErrorResponse),
    )
)]
pub async fn verify_security_question(
    axum::Json(req): axum::Json<SecurityQuestionAnswerRequest>,
) -> Result<axum::Json<SecurityQuestionAnswerResponse>, (axum::http::StatusCode, axum::Json<crate::schemas::ErrorResponse>)> {
    if req.email.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "email must not be empty".to_string(),
            }),
        ));
    }
    let mut store = RECOVERY_STORE.write().await;
    let config = RecoveryConfig::default();
    if !allow_recovery_attempt(&mut store, &req.email, &config, Utc::now().timestamp()) {
        return Err((
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Too many recovery attempts. Please try again later.".to_string(),
            }),
        ));
    }

    // Check if user has security questions set up
    let email = normalize_email(&req.email);
    if !store.security_answers.contains_key(&email) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "No security questions found for this email. Use email recovery instead.".to_string(),
            }),
        ));
    }

    if !security_answer_matches(&store, &email, req.question_id, &req.answer) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(crate::schemas::ErrorResponse {
                error: "Incorrect answer to security question".to_string(),
            }),
        ));
    }

    // Generate a temporary recovery token
    let recovery_token = generate_recovery_token();
    let now = Utc::now();
    store.tokens.insert(
        email,
        RecoveryToken {
            token_hash: hash_string(&recovery_token),
            created_at: now.timestamp(),
            expires_at: (now + Duration::minutes(config.token_expiry_minutes)).timestamp(),
            used: false,
        },
    );

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
            token_hash: hash_string("test_token_123"),
            created_at: 100,
            expires_at: 200,
            used: false,
        };

        assert!(!token.used);
    }

    #[test]
    fn recovery_tokens_expire_and_are_single_use() {
        let mut store = RecoveryStore {
            tokens: HashMap::new(),
            lockouts: HashMap::new(),
            attempts: HashMap::new(),
            security_answers: HashMap::new(),
        };
        store.tokens.insert(
            "test@example.com".to_string(),
            RecoveryToken {
                token_hash: hash_string("valid-token"),
                created_at: 100,
                expires_at: 200,
                used: false,
            },
        );

        assert!(!consume_recovery_token(&mut store, "test@example.com", "wrong-token", 150));
        assert!(!consume_recovery_token(&mut store, "test@example.com", "valid-token", 200));
        assert!(consume_recovery_token(&mut store, "test@example.com", "valid-token", 199));
        assert!(!consume_recovery_token(&mut store, "test@example.com", "valid-token", 199));
    }

    #[test]
    fn recovery_attempts_lock_and_reset_after_the_lockout_expires() {
        let mut store = RecoveryStore {
            tokens: HashMap::new(),
            lockouts: HashMap::new(),
            attempts: HashMap::new(),
            security_answers: HashMap::new(),
        };
        let config = RecoveryConfig {
            max_recovery_attempts: 2,
            lockout_duration_minutes: 30,
            ..RecoveryConfig::default()
        };

        assert!(allow_recovery_attempt(&mut store, "Test@example.com", &config, 100));
        assert!(allow_recovery_attempt(&mut store, "test@example.com", &config, 101));
        assert!(!allow_recovery_attempt(&mut store, "TEST@example.com", &config, 102));
        assert!(!allow_recovery_attempt(&mut store, "test@example.com", &config, 103));
        assert!(allow_recovery_attempt(
            &mut store,
            "test@example.com",
            &config,
            102 + Duration::minutes(30).num_seconds()
        ));
    }

    #[test]
    fn security_answers_must_match_the_stored_hash() {
        let mut store = RecoveryStore {
            tokens: HashMap::new(),
            lockouts: HashMap::new(),
            attempts: HashMap::new(),
            security_answers: HashMap::new(),
        };
        store.security_answers.insert(
            "test@example.com".to_string(),
            HashMap::from([(1, hash_string("correct answer"))]),
        );

        assert!(security_answer_matches(&store, "TEST@example.com", 1, "correct answer"));
        assert!(!security_answer_matches(&store, "test@example.com", 1, "any answer"));
        assert!(!security_answer_matches(&store, "test@example.com", 2, "correct answer"));
    }

    #[test]
    fn test_default_security_questions() {
        let questions = SecurityQuestion::default_questions();
        assert_eq!(questions.len(), 5);
        assert!(questions[0].question.contains("pet"));
    }
}
