use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub success_threshold: usize,
    pub timeout_secs: u64,
    pub half_open_max_calls: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout_secs: 60,
            half_open_max_calls: 3,
        }
    }
}

pub struct CircuitBreaker {
    state: Arc<std::sync::Mutex<CircuitState>>,
    failure_count: Arc<AtomicUsize>,
    success_count: Arc<AtomicUsize>,
    last_failure_time: Arc<AtomicU64>,
    half_open_calls: Arc<AtomicUsize>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(AtomicUsize::new(0)),
            success_count: Arc::new(AtomicUsize::new(0)),
            last_failure_time: Arc::new(AtomicU64::new(0)),
            half_open_calls: Arc::new(AtomicUsize::new(0)),
            config,
        }
    }

    pub fn get_state(&self) -> CircuitState {
        let mut state = self.state.lock().unwrap();
        
        if *state == CircuitState::Open {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let last_failure = self.last_failure_time.load(Ordering::SeqCst);
            
            if now - last_failure >= self.config.timeout_secs {
                *state = CircuitState::HalfOpen;
                self.half_open_calls.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
            }
        }
        
        *state
    }

    pub fn record_success(&self) {
        let state = self.get_state();
        
        match state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::SeqCst);
            }
            CircuitState::HalfOpen => {
                let success = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if success >= self.config.success_threshold {
                    *self.state.lock().unwrap() = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.success_count.store(0, Ordering::SeqCst);
                    self.half_open_calls.store(0, Ordering::SeqCst);
                }
            }
            CircuitState::Open => {}
        }
    }

    pub fn record_failure(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_failure_time.store(now, Ordering::SeqCst);
        
        let state = self.get_state();
        
        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold {
                    *self.state.lock().unwrap() = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                *self.state.lock().unwrap() = CircuitState::Open;
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
            }
            CircuitState::Open => {}
        }
    }

    pub fn can_execute(&self) -> bool {
        let state = self.get_state();
        
        match state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => {
                let calls = self.half_open_calls.load(Ordering::SeqCst);
                if calls < self.config.half_open_max_calls {
                    self.half_open_calls.fetch_add(1, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            }
            CircuitState::Open => false,
        }
    }

    pub fn reset(&self) {
        *self.state.lock().unwrap() = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.half_open_calls.store(0, Ordering::SeqCst);
        self.last_failure_time.store(0, Ordering::SeqCst);
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            failure_count: Arc::clone(&self.failure_count),
            success_count: Arc::clone(&self.success_count),
            last_failure_time: Arc::clone(&self.last_failure_time),
            half_open_calls: Arc::clone(&self.half_open_calls),
            config: self.config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);
        
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Closed);
        
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout_secs: 0,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);
        
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Open);
        
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout_secs: 0,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);
        
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Open);
        
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);
        
        cb.record_success();
        cb.record_success();
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    #[test]
    fn test_can_execute_respects_state() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert!(cb.can_execute());
        
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.can_execute());
    }

    #[test]
    fn test_reset_clears_state() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        
        assert_eq!(cb.get_state(), CircuitState::Open);
        cb.reset();
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }
}
