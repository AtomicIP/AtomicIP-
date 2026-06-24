use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── Error Classification ───────────────────────────────────────────────────────

/// Classification of whether an error is transient (retryable) or permanent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorClass {
    /// Temporary failure that may succeed on retry (e.g., network timeout, rate limit).
    Transient,
    /// Permanent failure that will not succeed on retry (e.g., bad request, not found).
    Permanent,
}

/// Classifies HTTP status codes and error kinds as transient or permanent.
pub struct ErrorClassifier;

impl ErrorClassifier {
    /// Classify an HTTP status code.
    pub fn classify_status(status: StatusCode) -> ErrorClass {
        if is_retryable_error(status) {
            ErrorClass::Transient
        } else {
            ErrorClass::Permanent
        }
    }

    /// Classify an error string/reason.
    pub fn classify_error(error: &str) -> ErrorClass {
        let lower = error.to_lowercase();
        let transient_keywords = [
            "timeout",
            "timed out",
            "too many requests",
            "rate limit",
            "service unavailable",
            "bad gateway",
            "gateway timeout",
            "connection refused",
            "connection reset",
            "broken pipe",
            "temporarily",
            "retry",
            "throttl",
            "unavailable",
            "overload",
            "busy",
            "try again",
            "network error",
            "eof",
            "handshake",
        ];
        if transient_keywords.iter().any(|kw| lower.contains(kw)) {
            return ErrorClass::Transient;
        }
        ErrorClass::Permanent
    }

    /// Determine if an error is retryable based on status code and error message.
    pub fn is_retryable(status: StatusCode, error: Option<&str>) -> bool {
        if is_retryable_error(status) {
            return true;
        }
        if let Some(msg) = error {
            Self::classify_error(msg) == ErrorClass::Transient
        } else {
            false
        }
    }
}

// ── Retry Strategy ─────────────────────────────────────────────────────────────

/// Configuration for exponential backoff retry strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub backoff_multiplier: f64,
    /// Add random jitter up to this fraction of the backoff (0.0 = no jitter).
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 10_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

/// Retry strategy with exponential backoff and jitter.
#[derive(Debug, Clone)]
pub struct RetryStrategy {
    config: RetryConfig,
}

impl RetryStrategy {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RetryConfig {
        &self.config
    }

    /// Calculate backoff duration for a given attempt (0-indexed).
    pub fn backoff(&self, attempt: u32) -> Duration {
        let backoff_ms = self.config.initial_backoff_ms as f64
            * self.config.backoff_multiplier.powi(attempt as i32);
        let backoff_ms = backoff_ms.min(self.config.max_backoff_ms as f64);

        let jitter = if self.config.jitter_factor > 0.0 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let jitter_range = backoff_ms * self.config.jitter_factor;
            rng.gen_range(-jitter_range..jitter_range)
        } else {
            0.0
        };

        Duration::from_millis((backoff_ms + jitter).max(1.0) as u64)
    }

    /// Returns true if more retries are available.
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.config.max_retries
    }

    /// Execute a fallible async operation with retry.
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        let mut last_error = None;
        for attempt in 0..=self.config.max_retries {
            match operation().await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    if attempt < self.config.max_retries {
                        let delay = self.backoff(attempt);
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            delay_ms = delay.as_millis(),
                            "retrying operation after error"
                        );
                        tokio::time::sleep(delay).await;
                    }
                    last_error = Some(e);
                }
            }
        }
        Err(RetryError {
            error: last_error.expect("at least one error occurred"),
            attempts: self.config.max_retries + 1,
        })
    }
}

#[derive(Debug)]
pub struct RetryError<E> {
    pub error: E,
    pub attempts: u32,
}

// ── Fallback Provider ──────────────────────────────────────────────────────────

/// A fallback provider that returns degraded-mode responses when the primary
/// service is unavailable.
pub struct FallbackProvider<T: Clone> {
    primary: Arc<dyn Fallbackable<T>>,
    fallback: Arc<dyn Fallbackable<T>>,
    degraded_threshold: u32,
    failure_count: Arc<std::sync::atomic::AtomicU32>,
}

use std::sync::Arc;

/// Trait for services that can provide fallback responses.
#[async_trait::async_trait]
pub trait Fallbackable<T: Clone>: Send + Sync {
    async fn execute(&self) -> Result<T, String>;
    fn degraded_response(&self) -> T;
}

#[async_trait::async_trait]
impl<T: Clone + Send + Sync> Fallbackable<T> for Box<dyn Fn() -> T + Send + Sync> {
    async fn execute(&self) -> Result<T, String> {
        Err("not implemented".to_string())
    }

    fn degraded_response(&self) -> T {
        (self)()
    }
}

impl<T: Clone> FallbackProvider<T> {
    pub fn new(
        primary: Arc<dyn Fallbackable<T>>,
        fallback: Arc<dyn Fallbackable<T>>,
        degraded_threshold: u32,
    ) -> Self {
        Self {
            primary,
            fallback,
            degraded_threshold,
            failure_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    pub async fn execute(&self) -> Result<T, String> {
        let failures = self.failure_count.load(std::sync::atomic::Ordering::SeqCst);
        if failures >= self.degraded_threshold {
            tracing::warn!(
                failures = failures,
                threshold = self.degraded_threshold,
                "using fallback provider (degraded mode)"
            );
            return Ok(self.fallback.degraded_response());
        }

        match self.primary.execute().await {
            Ok(val) => {
                self.failure_count.store(0, std::sync::atomic::Ordering::SeqCst);
                Ok(val)
            }
            Err(e) => {
                let count = self.failure_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                tracing::warn!(
                    error = %e,
                    consecutive_failures = count,
                    threshold = self.degraded_threshold,
                    "primary service failed"
                );
                if count >= self.degraded_threshold {
                    tracing::error!("degraded mode activated");
                }
                Ok(self.fallback.degraded_response())
            }
        }
    }

    pub fn reset(&self) {
        self.failure_count.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

// ── Structured Error Response ──────────────────────────────────────────────────

/// Structured error response with retry hints for clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredError {
    pub error: String,
    pub error_code: String,
    pub classification: ErrorClass,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_hint: Option<String>,
}

impl StructuredError {
    pub fn transient(
        error: impl Into<String>,
        code: impl Into<String>,
        retry_after_ms: Option<u64>,
    ) -> Self {
        Self {
            error: error.into(),
            error_code: code.into(),
            classification: ErrorClass::Transient,
            retryable: true,
            retry_after_ms,
            retry_hint: Some("Retry with exponential backoff. See Retry-After header.".to_string()),
        }
    }

    pub fn permanent(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            error_code: code.into(),
            classification: ErrorClass::Permanent,
            retryable: false,
            retry_after_ms: None,
            retry_hint: Some("This error is not retryable. Check the request and try again.".to_string()),
        }
    }

    pub fn from_status(status: StatusCode, message: impl Into<String>) -> Self {
        let msg = message.into();
        match ErrorClassifier::classify_status(status) {
            ErrorClass::Transient => Self::transient(msg, status.as_u16().to_string(), None),
            ErrorClass::Permanent => Self::permanent(msg, status.as_u16().to_string()),
        }
    }
}

impl IntoResponse for StructuredError {
    fn into_response(self) -> Response {
        let status = match self.classification {
            ErrorClass::Transient => StatusCode::SERVICE_UNAVAILABLE,
            ErrorClass::Permanent => StatusCode::BAD_REQUEST,
        };
        (status, Json(self)).into_response()
    }
}

// ── Existing Utilities ─────────────────────────────────────────────────────────

/// Determine if an error is retryable based on HTTP status code.
pub fn is_retryable_error(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

/// Calculate exponential backoff duration.
pub fn calculate_backoff(attempt: u32, config: &RetryConfig) -> Duration {
    let backoff_ms =
        config.initial_backoff_ms as f64 * config.backoff_multiplier.powi(attempt as i32);
    let backoff_ms = backoff_ms.min(config.max_backoff_ms as f64);
    Duration::from_millis(backoff_ms as u64)
}

/// Error recovery context tracking attempt state.
#[derive(Clone, Debug)]
pub struct ErrorRecoveryContext {
    pub attempt: u32,
    pub last_error: Option<String>,
    pub recovery_strategy: RecoveryStrategy,
}

impl Default for ErrorRecoveryContext {
    fn default() -> Self {
        Self {
            attempt: 0,
            last_error: None,
            recovery_strategy: RecoveryStrategy::Retry,
        }
    }
}

/// Error recovery strategy selection.
#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryStrategy {
    Retry,
    CircuitBreaker,
    Fallback,
    Fail,
}

/// Middleware for automatic error recovery with structured responses.
pub async fn error_recovery_middleware(
    req: Request,
    next: Next,
) -> Response {
    let response = next.run(req).await;
    let status = response.status();

    if is_retryable_error(status) {
        tracing::warn!(status = status.as_u16(), "Retryable error encountered");
        let error = StructuredError::from_status(
            status,
            format!("retryable error: {}", status.as_u16()),
        );
        return error.into_response();
    }

    response
}

// ── Circuit Breaker (moved to circuit_breaker.rs, re-exported here for compat) ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error(StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_error(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable_error(StatusCode::GATEWAY_TIMEOUT));
        assert!(is_retryable_error(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_error(StatusCode::BAD_GATEWAY));
        assert!(!is_retryable_error(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_error(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_error(StatusCode::NOT_FOUND));
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig::default();
        let strategy = RetryStrategy::new(config);

        let backoff_0 = strategy.backoff(0);
        let backoff_1 = strategy.backoff(1);
        let backoff_2 = strategy.backoff(2);

        assert!(backoff_1 > backoff_0);
        assert!(backoff_2 > backoff_1);
    }

    #[test]
    fn test_backoff_max_limit() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 1000,
            backoff_multiplier: 10.0,
            jitter_factor: 0.0,
        };
        let strategy = RetryStrategy::new(config);
        let backoff = strategy.backoff(10);
        assert!(backoff <= Duration::from_millis(1000));
    }

    #[test]
    fn test_should_retry() {
        let config = RetryConfig {
            max_retries: 3,
            ..Default::default()
        };
        let strategy = RetryStrategy::new(config);
        assert!(strategy.should_retry(0));
        assert!(strategy.should_retry(1));
        assert!(strategy.should_retry(2));
        assert!(!strategy.should_retry(3));
        assert!(!strategy.should_retry(4));
    }

    #[test]
    fn test_error_classifier_status() {
        assert_eq!(
            ErrorClassifier::classify_status(StatusCode::SERVICE_UNAVAILABLE),
            ErrorClass::Transient
        );
        assert_eq!(
            ErrorClassifier::classify_status(StatusCode::BAD_REQUEST),
            ErrorClass::Permanent
        );
    }

    #[test]
    fn test_error_classifier_message() {
        assert_eq!(
            ErrorClassifier::classify_error("connection timed out"),
            ErrorClass::Transient
        );
        assert_eq!(
            ErrorClassifier::classify_error("rate limit exceeded"),
            ErrorClass::Transient
        );
        assert_eq!(
            ErrorClassifier::classify_error("invalid input parameter"),
            ErrorClass::Permanent
        );
    }

    #[test]
    fn test_structured_error_transient() {
        let err = StructuredError::transient("service busy", "RATE_LIMITED", Some(5000));
        assert!(err.retryable);
        assert_eq!(err.classification, ErrorClass::Transient);
        assert_eq!(err.retry_after_ms, Some(5000));
        assert!(err.retry_hint.is_some());
    }

    #[test]
    fn test_structured_error_permanent() {
        let err = StructuredError::permanent("invalid request", "BAD_REQUEST");
        assert!(!err.retryable);
        assert_eq!(err.classification, ErrorClass::Permanent);
    }

    #[test]
    fn test_error_recovery_context() {
        let ctx = ErrorRecoveryContext::default();
        assert_eq!(ctx.attempt, 0);
        assert_eq!(ctx.recovery_strategy, RecoveryStrategy::Retry);
    }

    #[test]
    fn test_fallback_provider_returns_degraded_on_threshold() {
        struct OkProvider;
        #[async_trait::async_trait]
        impl Fallbackable<String> for OkProvider {
            async fn execute(&self) -> Result<String, String> {
                Ok("primary".to_string())
            }
            fn degraded_response(&self) -> String {
                "degraded".to_string()
            }
        }

        struct FailProvider;
        #[async_trait::async_trait]
        impl Fallbackable<String> for FailProvider {
            async fn execute(&self) -> Result<String, String> {
                Err("failed".to_string())
            }
            fn degraded_response(&self) -> String {
                "fallback".to_string()
            }
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        let provider = FallbackProvider::new(
            Arc::new(OkProvider),
            Arc::new(FailProvider),
            3,
        );

        let result = rt.block_on(provider.execute());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "primary");
    }
}
