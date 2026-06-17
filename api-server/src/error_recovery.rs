use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::time::Duration;

/// Retry configuration
#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
}

/// Error recovery strategy
#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryStrategy {
    /// Retry with exponential backoff
    Retry,
    /// Circuit breaker pattern
    CircuitBreaker,
    /// Fallback to cached response
    Fallback,
    /// Fail immediately
    Fail,
}

/// Determine if an error is retryable
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

/// Calculate exponential backoff duration
pub fn calculate_backoff(attempt: u32, config: &RetryConfig) -> Duration {
    let backoff_ms = config.initial_backoff.as_millis() as f64
        * config.backoff_multiplier.powi(attempt as i32);
    let backoff_ms = backoff_ms.min(config.max_backoff.as_millis() as f64);
    Duration::from_millis(backoff_ms as u64)
}

/// Error recovery context
#[derive(Clone, Debug)]
pub struct ErrorRecoveryContext {
    pub attempt: u32,
    pub last_error: Option<String>,
    pub recovery_strategy: RecoveryStrategy,
}

impl Default for ErrorRecoveryContext {
    fn default() -> Self {
        ErrorRecoveryContext {
            attempt: 0,
            last_error: None,
            recovery_strategy: RecoveryStrategy::Retry,
        }
    }
}

/// Middleware for automatic error recovery
pub async fn error_recovery_middleware(
    req: Request,
    next: Next,
) -> Response {
    let mut recovery_context = ErrorRecoveryContext::default();
    let config = RetryConfig::default();

    loop {
        let response = next.run(req.clone()).await;
        let status = response.status();

        if status.is_success() {
            return response;
        }

        if !is_retryable_error(status) {
            return response;
        }

        recovery_context.attempt += 1;
        if recovery_context.attempt >= config.max_retries {
            tracing::warn!(
                attempt = recovery_context.attempt,
                status = status.as_u16(),
                "Max retries exceeded"
            );
            return response;
        }

        let backoff = calculate_backoff(recovery_context.attempt - 1, &config);
        tracing::info!(
            attempt = recovery_context.attempt,
            backoff_ms = backoff.as_millis(),
            status = status.as_u16(),
            "Retrying request with exponential backoff"
        );

        tokio::time::sleep(backoff).await;
    }
}

/// Circuit breaker state
#[derive(Clone, Debug, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for error recovery
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    failure_threshold: u32,
    success_count: u32,
    success_threshold: u32,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, success_threshold: u32) -> Self {
        CircuitBreaker {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            failure_threshold,
            success_count: 0,
            success_threshold,
        }
    }

    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.success_threshold {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    tracing::info!("Circuit breaker closed");
                }
            }
            CircuitBreakerState::Open => {}
        }
    }

    pub fn record_failure(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                    tracing::warn!("Circuit breaker opened");
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Open;
                self.success_count = 0;
                tracing::warn!("Circuit breaker reopened");
            }
            CircuitBreakerState::Open => {}
        }
    }

    pub fn can_attempt(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                self.state = CircuitBreakerState::HalfOpen;
                self.success_count = 0;
                tracing::info!("Circuit breaker half-open, attempting request");
                true
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    pub fn get_state(&self) -> CircuitBreakerState {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error(StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_error(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable_error(StatusCode::GATEWAY_TIMEOUT));
        assert!(!is_retryable_error(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_error(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig::default();
        
        let backoff_0 = calculate_backoff(0, &config);
        let backoff_1 = calculate_backoff(1, &config);
        let backoff_2 = calculate_backoff(2, &config);
        
        assert!(backoff_1 > backoff_0);
        assert!(backoff_2 > backoff_1);
    }

    #[test]
    fn test_backoff_max_limit() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(1),
            backoff_multiplier: 10.0,
        };
        
        let backoff = calculate_backoff(10, &config);
        assert!(backoff <= config.max_backoff);
    }

    #[test]
    fn test_circuit_breaker_closed_to_open() {
        let mut cb = CircuitBreaker::new(3, 2);
        
        assert_eq!(cb.get_state(), CircuitBreakerState::Closed);
        
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        
        assert_eq!(cb.get_state(), CircuitBreakerState::Open);
    }

    #[test]
    fn test_circuit_breaker_half_open() {
        let mut cb = CircuitBreaker::new(1, 1);
        
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitBreakerState::Open);
        
        assert!(cb.can_attempt());
        assert_eq!(cb.get_state(), CircuitBreakerState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_recovery() {
        let mut cb = CircuitBreaker::new(1, 1);
        
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitBreakerState::Open);
        
        cb.can_attempt();
        assert_eq!(cb.get_state(), CircuitBreakerState::HalfOpen);
        
        cb.record_success();
        assert_eq!(cb.get_state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_error_recovery_context() {
        let ctx = ErrorRecoveryContext::default();
        assert_eq!(ctx.attempt, 0);
        assert_eq!(ctx.recovery_strategy, RecoveryStrategy::Retry);
    }
}
