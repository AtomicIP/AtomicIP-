use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use metrics::{counter, gauge, describe_counter, describe_gauge};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half_open"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub success_threshold: usize,
    /// Seconds the circuit stays Open before transitioning to HalfOpen.
    pub timeout_secs: u64,
    pub half_open_max_calls: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout_secs: 30,
            half_open_max_calls: 3,
        }
    }
}

pub struct CircuitBreaker {
    name: String,
    state: Arc<std::sync::Mutex<CircuitState>>,
    failure_count: Arc<AtomicUsize>,
    success_count: Arc<AtomicUsize>,
    last_failure_time: Arc<AtomicU64>,
    half_open_calls: Arc<AtomicUsize>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        let name = name.into();

        describe_counter!(
            "circuit_breaker_state_transitions_total",
            "Total number of circuit breaker state transitions"
        );
        describe_gauge!(
            "circuit_breaker_state",
            "Current circuit breaker state: 0=closed, 1=open, 2=half_open"
        );
        describe_counter!(
            "circuit_breaker_calls_total",
            "Total calls attempted through the circuit breaker"
        );
        describe_counter!(
            "circuit_breaker_calls_rejected_total",
            "Calls rejected because the circuit is open"
        );

        gauge!(
            "circuit_breaker_state",
            "service" => name.clone(),
        )
        .set(0.0);

        Self {
            name,
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
                .as_millis() as u64;
            let last_failure = self.last_failure_time.load(Ordering::SeqCst);
            let timeout_ms = if self.config.timeout_secs == 0 {
                5
            } else {
                self.config.timeout_secs * 1000
            };

            if now.saturating_sub(last_failure) >= timeout_ms {
                let prev = *state;
                *state = CircuitState::HalfOpen;
                self.half_open_calls.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
                self.emit_transition(prev, CircuitState::HalfOpen);
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
                    let mut s = self.state.lock().unwrap();
                    *s = CircuitState::Closed;
                    drop(s);
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.success_count.store(0, Ordering::SeqCst);
                    self.half_open_calls.store(0, Ordering::SeqCst);
                    self.emit_transition(CircuitState::HalfOpen, CircuitState::Closed);
                }
            }
            CircuitState::Open => {}
        }
    }

    pub fn record_failure(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.last_failure_time.store(now, Ordering::SeqCst);

        let state = self.get_state();

        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold {
                    let mut s = self.state.lock().unwrap();
                    *s = CircuitState::Open;
                    drop(s);
                    self.emit_transition(CircuitState::Closed, CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                let mut s = self.state.lock().unwrap();
                *s = CircuitState::Open;
                drop(s);
                self.failure_count.store(0, Ordering::SeqCst);
                self.success_count.store(0, Ordering::SeqCst);
                self.emit_transition(CircuitState::HalfOpen, CircuitState::Open);
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
                    counter!(
                        "circuit_breaker_calls_rejected_total",
                        "service" => self.name.clone(),
                        "state" => "half_open",
                    )
                    .increment(1);
                    false
                }
            }
            CircuitState::Open => {
                counter!(
                    "circuit_breaker_calls_rejected_total",
                    "service" => self.name.clone(),
                    "state" => "open",
                )
                .increment(1);
                false
            }
        }
    }

    /// Execute `f`, recording the outcome automatically.
    /// Returns `Err(CircuitOpenError)` when the circuit is open/half-open-saturated.
    pub fn call<F, T, E>(&self, f: F) -> Result<T, CallError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        counter!(
            "circuit_breaker_calls_total",
            "service" => self.name.clone(),
        )
        .increment(1);

        if !self.can_execute() {
            return Err(CallError::CircuitOpen);
        }

        match f() {
            Ok(val) => {
                self.record_success();
                Ok(val)
            }
            Err(e) => {
                self.record_failure();
                Err(CallError::ServiceError(e))
            }
        }
    }

    pub fn reset(&self) {
        let prev = self.get_state();
        *self.state.lock().unwrap() = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.half_open_calls.store(0, Ordering::SeqCst);
        self.last_failure_time.store(0, Ordering::SeqCst);
        if prev != CircuitState::Closed {
            self.emit_transition(prev, CircuitState::Closed);
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn emit_transition(&self, from: CircuitState, to: CircuitState) {
        counter!(
            "circuit_breaker_state_transitions_total",
            "service" => self.name.clone(),
            "from" => from.to_string(),
            "to" => to.to_string(),
        )
        .increment(1);

        let state_value = match to {
            CircuitState::Closed => 0.0,
            CircuitState::Open => 1.0,
            CircuitState::HalfOpen => 2.0,
        };
        gauge!(
            "circuit_breaker_state",
            "service" => self.name.clone(),
        )
        .set(state_value);

        tracing::info!(
            service = %self.name,
            from = %from,
            to = %to,
            "circuit breaker state transition"
        );
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            state: Arc::clone(&self.state),
            failure_count: Arc::clone(&self.failure_count),
            success_count: Arc::clone(&self.success_count),
            last_failure_time: Arc::clone(&self.last_failure_time),
            half_open_calls: Arc::clone(&self.half_open_calls),
            config: self.config.clone(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CallError<E> {
    CircuitOpen,
    ServiceError(E),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cb(failure_threshold: usize, success_threshold: usize, timeout_secs: u64) -> CircuitBreaker {
        CircuitBreaker::new(
            "test-service",
            CircuitBreakerConfig {
                failure_threshold,
                success_threshold,
                timeout_secs,
                half_open_max_calls: 3,
            },
        )
    }

    // ── Basic state checks ────────────────────────────────────────────────────

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new("oracle", CircuitBreakerConfig::default());
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    #[test]
    fn test_default_config_has_30s_timeout() {
        let cfg = CircuitBreakerConfig::default();
        assert_eq!(cfg.timeout_secs, 30);
        assert_eq!(cfg.failure_threshold, 5);
        assert_eq!(cfg.success_threshold, 2);
    }

    // ── Service outage simulation ─────────────────────────────────────────────

    #[test]
    fn test_circuit_opens_after_failure_threshold() {
        let cb = make_cb(5, 2, 30);

        for _ in 0..4 {
            cb.record_failure();
            assert_eq!(cb.get_state(), CircuitState::Closed, "should still be closed");
        }

        cb.record_failure(); // 5th failure trips the breaker
        assert_eq!(cb.get_state(), CircuitState::Open);
    }

    #[test]
    fn test_open_circuit_rejects_calls() {
        let cb = make_cb(3, 2, 30);
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();

        assert_eq!(cb.get_state(), CircuitState::Open);
        assert!(!cb.can_execute(), "open circuit must reject calls");
    }

    #[test]
    fn test_call_returns_circuit_open_error_when_open() {
        let cb = make_cb(1, 2, 30);
        cb.record_failure();

        let result: Result<(), CallError<&str>> = cb.call(|| Ok(()));
        assert_eq!(result, Err(CallError::CircuitOpen));
    }

    #[test]
    fn test_call_propagates_service_error_and_records_failure() {
        let cb = make_cb(5, 2, 30);
        let result: Result<(), CallError<&str>> = cb.call(|| Err("service down"));
        assert_eq!(result, Err(CallError::ServiceError("service down")));
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_call_records_success_and_clears_failures() {
        let cb = make_cb(5, 2, 30);
        cb.record_failure(); // count = 1

        let result: Result<i32, CallError<()>> = cb.call(|| Ok(42));
        assert_eq!(result, Ok(42));
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0);
    }

    // ── State transition: Closed -> Open -> HalfOpen -> Closed ───────────────

    #[test]
    fn test_half_open_after_timeout() {
        let cb = make_cb(1, 2, 0);
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_recovery_half_open_to_closed_after_success_threshold() {
        let cb = make_cb(1, 2, 0);
        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);

        cb.record_success(); // 1 success
        assert_eq!(cb.get_state(), CircuitState::HalfOpen, "needs 2 successes");

        cb.record_success(); // 2 successes => Closed
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    #[test]
    fn test_failure_in_half_open_reopens_circuit() {
        let cb = make_cb(1, 2, 0);
        cb.record_failure();

        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);

        cb.record_failure();
        assert_eq!(cb.get_state(), CircuitState::Open);
    }

    // ── Half-open test-request limiting ──────────────────────────────────────

    #[test]
    fn test_half_open_allows_limited_test_requests() {
        let cb = CircuitBreaker::new(
            "db",
            CircuitBreakerConfig {
                failure_threshold: 1,
                success_threshold: 2,
                timeout_secs: 0,
                half_open_max_calls: 2,
            },
        );
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);

        assert!(cb.can_execute(), "first test request allowed");
        assert!(cb.can_execute(), "second test request allowed");
        assert!(!cb.can_execute(), "third request rejected — limit reached");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_reset_clears_open_state() {
        let cb = make_cb(5, 2, 30);
        for _ in 0..5 {
            cb.record_failure();
        }
        assert_eq!(cb.get_state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.get_state(), CircuitState::Closed);
        assert!(cb.can_execute());
    }

    #[test]
    fn test_reset_clears_failure_count() {
        let cb = make_cb(5, 2, 30);
        cb.record_failure();
        cb.record_failure();

        cb.reset();
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0);
    }

    // ── Clone shares state ────────────────────────────────────────────────────

    #[test]
    fn test_clone_shares_state() {
        let cb1 = make_cb(3, 2, 30);
        let cb2 = cb1.clone();

        cb1.record_failure();
        cb1.record_failure();
        cb1.record_failure();

        assert_eq!(cb2.get_state(), CircuitState::Open);
    }

    // ── Name accessor ─────────────────────────────────────────────────────────

    #[test]
    fn test_name_is_stored() {
        let cb = CircuitBreaker::new("price-oracle", CircuitBreakerConfig::default());
        assert_eq!(cb.name(), "price-oracle");
    }

    // ── End-to-end: oracle outage + automatic recovery ────────────────────────

    #[test]
    fn test_oracle_outage_and_recovery_scenario() {
        let cb = CircuitBreaker::new(
            "price-oracle",
            CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 2,
                timeout_secs: 0,
                half_open_max_calls: 3,
            },
        );

        // Phase 1 — normal operation
        for _ in 0..3 {
            let r: Result<(), CallError<()>> = cb.call(|| Ok(()));
            assert!(r.is_ok());
        }
        assert_eq!(cb.get_state(), CircuitState::Closed);

        // Phase 2 — oracle goes down (5 failures trip the breaker)
        for _ in 0..5 {
            let _: Result<(), CallError<&str>> = cb.call(|| Err("timeout"));
        }
        assert_eq!(cb.get_state(), CircuitState::Open);

        // Phase 3 — all calls rejected while open
        let rejected: Result<(), CallError<()>> = cb.call(|| Ok(()));
        assert_eq!(rejected, Err(CallError::CircuitOpen));

        // Phase 4 — timeout elapses, circuit moves to half-open
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);

        // Phase 5 — two successful test requests close the circuit
        let _: Result<(), CallError<()>> = cb.call(|| Ok(()));
        let _: Result<(), CallError<()>> = cb.call(|| Ok(()));
        assert_eq!(cb.get_state(), CircuitState::Closed);

        // Phase 6 — normal operation resumes
        let r: Result<i32, CallError<()>> = cb.call(|| Ok(99));
        assert_eq!(r, Ok(99));
    }

    // ── End-to-end: database outage + failed recovery ─────────────────────────

    #[test]
    fn test_database_failed_recovery_stays_open() {
        let cb = CircuitBreaker::new(
            "postgres",
            CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout_secs: 0,
                half_open_max_calls: 3,
            },
        );

        for _ in 0..3 {
            let _: Result<(), CallError<&str>> = cb.call(|| Err("connection refused"));
        }
        assert_eq!(cb.get_state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cb.get_state(), CircuitState::HalfOpen);

        // DB still not ready
        let _: Result<(), CallError<&str>> = cb.call(|| Err("still down"));
        assert_eq!(cb.get_state(), CircuitState::Open);
    }
}
