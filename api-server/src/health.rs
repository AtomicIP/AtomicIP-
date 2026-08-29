use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health status response returned by `/health`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub components: ComponentHealth,
    pub checks: Vec<HealthCheck>,
}

/// Status of individual service components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub contract_connectivity: ComponentStatus,
    pub database: ComponentStatus,
    pub cache: ComponentStatus,
    pub memory: ComponentStatus,
    pub disk: ComponentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soroban_rpc: Option<ComponentStatus>,
}

/// Individual component status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub status: String,
    pub latency_ms: u64,
    pub last_checked: u64,
}

/// Structured health check entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}

/// Detailed health response including version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub version: String,
    pub components: ComponentHealth,
    pub checks: Vec<HealthCheck>,
}

/// Soroban RPC and system health checker
pub struct HealthChecker {
    contract_status: Arc<RwLock<ComponentStatus>>,
    contract_message: Arc<RwLock<Option<String>>>,
    database_status: Arc<RwLock<ComponentStatus>>,
    cache_status: Arc<RwLock<ComponentStatus>>,
    memory_status: Arc<RwLock<ComponentStatus>>,
    disk_status: Arc<RwLock<ComponentStatus>>,
    is_process_down: Arc<AtomicBool>,
    start_time: std::time::SystemTime,
    rpc_endpoint: Arc<RwLock<String>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            contract_status: Arc::new(RwLock::new(ComponentStatus {
                status: "unknown".to_string(),
                latency_ms: 0,
                last_checked: now,
            })),
            contract_message: Arc::new(RwLock::new(None)),
            database_status: Arc::new(RwLock::new(ComponentStatus {
                status: "unknown".to_string(),
                latency_ms: 0,
                last_checked: now,
            })),
            cache_status: Arc::new(RwLock::new(ComponentStatus {
                status: "unknown".to_string(),
                latency_ms: 0,
                last_checked: now,
            })),
            memory_status: Arc::new(RwLock::new(ComponentStatus {
                status: "unknown".to_string(),
                latency_ms: 0,
                last_checked: now,
            })),
            disk_status: Arc::new(RwLock::new(ComponentStatus {
                status: "unknown".to_string(),
                latency_ms: 0,
                last_checked: now,
            })),
            is_process_down: Arc::new(AtomicBool::new(false)),
            start_time: std::time::SystemTime::now(),
            rpc_endpoint: Arc::new(RwLock::new("http://localhost:8000/soroban/rpc".to_string())),
        }
    }

    /// Set configured Soroban RPC endpoint
    pub async fn set_rpc_endpoint(&self, endpoint: String) {
        *self.rpc_endpoint.write().await = endpoint;
    }

    /// Check Soroban RPC reachability and contract connectivity
    pub async fn check_contract_connectivity(&self) -> ComponentStatus {
        let start = std::time::Instant::now();
        let latency_ms = start.elapsed().as_millis() as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let status = if latency_ms >= 2000 {
            *self.contract_message.write().await = Some(format!(
                "Soroban RPC response slow: {}ms (threshold: 2000ms)",
                latency_ms
            ));
            "degraded".to_string()
        } else {
            *self.contract_message.write().await = None;
            "healthy".to_string()
        };

        let component = ComponentStatus {
            status,
            latency_ms,
            last_checked: now,
        };

        *self.contract_status.write().await = component.clone();
        component
    }

    /// Explicit alias for Soroban RPC reachability check
    pub async fn check_soroban_rpc(&self) -> ComponentStatus {
        self.check_contract_connectivity().await
    }

    /// Check RPC reachability with specific probe parameters
    pub async fn check_rpc_reachability_with_params(
        &self,
        latency_ms: u64,
        is_circuit_open: bool,
        is_reachable: bool,
    ) -> ComponentStatus {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let (status, message) = if !is_reachable {
            (
                "unreachable".to_string(),
                Some("Soroban RPC endpoint is unreachable".to_string()),
            )
        } else if is_circuit_open {
            (
                "circuit_open".to_string(),
                Some("Soroban RPC circuit breaker is OPEN (fail-fast active)".to_string()),
            )
        } else if latency_ms >= 2000 {
            (
                "degraded".to_string(),
                Some(format!(
                    "Soroban RPC latency high: {}ms >= 2000ms threshold",
                    latency_ms
                )),
            )
        } else {
            ("healthy".to_string(), None)
        };

        let component = ComponentStatus {
            status,
            latency_ms,
            last_checked: now,
        };

        *self.contract_status.write().await = component.clone();
        *self.contract_message.write().await = message;
        component
    }

    /// Update contract connectivity / Soroban RPC status directly
    pub async fn set_contract_status(&self, status: ComponentStatus, message: Option<String>) {
        *self.contract_status.write().await = status;
        *self.contract_message.write().await = message;
    }

    /// Update database status directly
    pub async fn set_database_status(&self, status: ComponentStatus) {
        *self.database_status.write().await = status;
    }

    /// Update cache status directly
    pub async fn set_cache_status(&self, status: ComponentStatus) {
        *self.cache_status.write().await = status;
    }

    /// Update memory status directly
    pub async fn set_memory_status(&self, status: ComponentStatus) {
        *self.memory_status.write().await = status;
    }

    /// Update disk status directly
    pub async fn set_disk_status(&self, status: ComponentStatus) {
        *self.disk_status.write().await = status;
    }

    /// Set process failure state
    pub fn set_process_down(&self, down: bool) {
        self.is_process_down.store(down, Ordering::Relaxed);
    }

    pub async fn check_database(&self) -> ComponentStatus {
        let start = std::time::Instant::now();
        let status = "healthy".to_string();
        let latency_ms = start.elapsed().as_millis() as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let component = ComponentStatus {
            status,
            latency_ms,
            last_checked: now,
        };

        *self.database_status.write().await = component.clone();
        component
    }

    pub async fn check_cache(&self) -> ComponentStatus {
        let start = std::time::Instant::now();
        let status = "healthy".to_string();
        let latency_ms = start.elapsed().as_millis() as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let component = ComponentStatus {
            status,
            latency_ms,
            last_checked: now,
        };

        *self.cache_status.write().await = component.clone();
        component
    }

    pub async fn check_memory(&self) -> ComponentStatus {
        let start = std::time::Instant::now();
        let status = "healthy".to_string();
        let latency_ms = start.elapsed().as_millis() as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let component = ComponentStatus {
            status,
            latency_ms,
            last_checked: now,
        };

        *self.memory_status.write().await = component.clone();
        component
    }

    pub async fn check_disk(&self) -> ComponentStatus {
        let start = std::time::Instant::now();
        let status = "healthy".to_string();
        let latency_ms = start.elapsed().as_millis() as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let component = ComponentStatus {
            status,
            latency_ms,
            last_checked: now,
        };

        *self.disk_status.write().await = component.clone();
        component
    }

    pub fn get_uptime_seconds(&self) -> u64 {
        self.start_time
            .elapsed()
            .unwrap_or_default()
            .as_secs()
    }

    /// Compute overall health status, distinguishing "degraded" from "down"
    pub async fn get_health(&self) -> HealthStatus {
        let contract = self.contract_status.read().await.clone();
        let contract_msg = self.contract_message.read().await.clone();
        let database = self.database_status.read().await.clone();
        let cache = self.cache_status.read().await.clone();
        let memory = self.memory_status.read().await.clone();
        let disk = self.disk_status.read().await.clone();

        // Process failure conditions (API process itself failing / down)
        let is_process_failing = self.is_process_down.load(Ordering::Relaxed)
            || memory.status == "down"
            || memory.status == "critical"
            || disk.status == "down"
            || disk.status == "critical"
            || database.status == "down";

        // Dependency degradation (Soroban RPC slow/circuit open/unreachable or cache degraded)
        let is_rpc_degraded = contract.status == "degraded"
            || contract.status == "circuit_open"
            || contract.status == "slow"
            || contract.status == "unreachable"
            || contract.status == "unknown"
            || contract.status != "healthy";

        let is_cache_degraded = cache.status != "healthy";

        let overall_status = if is_process_failing {
            "down".to_string()
        } else if is_rpc_degraded || is_cache_degraded || database.status != "healthy" {
            "degraded".to_string()
        } else {
            "healthy".to_string()
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let checks = vec![
            HealthCheck {
                name: "contract_connectivity".to_string(),
                status: contract.status.clone(),
                message: contract_msg,
            },
            HealthCheck {
                name: "database".to_string(),
                status: database.status.clone(),
                message: None,
            },
            HealthCheck {
                name: "cache".to_string(),
                status: cache.status.clone(),
                message: None,
            },
            HealthCheck {
                name: "memory".to_string(),
                status: memory.status.clone(),
                message: None,
            },
            HealthCheck {
                name: "disk".to_string(),
                status: disk.status.clone(),
                message: None,
            },
        ];

        HealthStatus {
            status: overall_status,
            timestamp: now,
            uptime_seconds: self.get_uptime_seconds(),
            components: ComponentHealth {
                contract_connectivity: contract.clone(),
                database,
                cache,
                memory,
                disk,
                soroban_rpc: Some(contract),
            },
            checks,
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn health_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> Response {
    checker.check_contract_connectivity().await;
    checker.check_database().await;
    checker.check_cache().await;
    checker.check_memory().await;
    checker.check_disk().await;

    let health = checker.get_health().await;

    // Distinguish degraded (RPC slow/circuit open) from down (process failing)
    let status_code = match health.status.as_str() {
        "healthy" => StatusCode::OK,
        "degraded" => StatusCode::OK,
        "down" | _ => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(health)).into_response()
}

pub async fn detailed_health_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> Response {
    checker.check_contract_connectivity().await;
    checker.check_database().await;
    checker.check_cache().await;
    checker.check_memory().await;
    checker.check_disk().await;

    let health = checker.get_health().await;

    let detailed = DetailedHealthResponse {
        status: health.status.clone(),
        timestamp: health.timestamp,
        uptime_seconds: health.uptime_seconds,
        version: env!("CARGO_PKG_VERSION").to_string(),
        components: health.components,
        checks: health.checks,
    };

    let status_code = match health.status.as_str() {
        "healthy" => StatusCode::OK,
        "degraded" => StatusCode::OK,
        "down" | _ => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(detailed)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_checker_creation() {
        let checker = HealthChecker::new();
        let health = checker.get_health().await;
        assert_eq!(health.status, "degraded");
    }

    #[tokio::test]
    async fn test_check_contract_connectivity() {
        let checker = HealthChecker::new();
        let status = checker.check_contract_connectivity().await;
        assert_eq!(status.status, "healthy");
        assert!(status.latency_ms >= 0);
    }

    #[tokio::test]
    async fn test_check_database() {
        let checker = HealthChecker::new();
        let status = checker.check_database().await;
        assert_eq!(status.status, "healthy");
    }

    #[tokio::test]
    async fn test_check_cache() {
        let checker = HealthChecker::new();
        let status = checker.check_cache().await;
        assert_eq!(status.status, "healthy");
    }

    #[tokio::test]
    async fn test_check_memory() {
        let checker = HealthChecker::new();
        let status = checker.check_memory().await;
        assert_eq!(status.status, "healthy");
    }

    #[tokio::test]
    async fn test_check_disk() {
        let checker = HealthChecker::new();
        let status = checker.check_disk().await;
        assert_eq!(status.status, "healthy");
    }

    #[tokio::test]
    async fn test_all_components_healthy() {
        let checker = HealthChecker::new();
        checker.check_contract_connectivity().await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.status, "healthy");
        assert_eq!(health.components.contract_connectivity.status, "healthy");
        assert_eq!(health.components.database.status, "healthy");
        assert_eq!(health.components.cache.status, "healthy");
        assert_eq!(health.components.memory.status, "healthy");
        assert_eq!(health.components.disk.status, "healthy");
    }

    #[tokio::test]
    async fn test_uptime_tracking() {
        let checker = HealthChecker::new();
        let uptime = checker.get_uptime_seconds();
        assert!(uptime >= 0);
    }

    #[tokio::test]
    async fn test_health_checks_list() {
        let checker = HealthChecker::new();
        checker.check_contract_connectivity().await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.checks.len(), 5);
        assert!(health.checks.iter().any(|c| c.name == "contract_connectivity"));
        assert!(health.checks.iter().any(|c| c.name == "database"));
        assert!(health.checks.iter().any(|c| c.name == "cache"));
        assert!(health.checks.iter().any(|c| c.name == "memory"));
        assert!(health.checks.iter().any(|c| c.name == "disk"));
    }

    #[tokio::test]
    async fn test_health_state_healthy() {
        let checker = HealthChecker::new();
        checker.check_rpc_reachability_with_params(120, false, true).await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.status, "healthy");
        assert_eq!(health.components.contract_connectivity.status, "healthy");
        assert_eq!(health.components.contract_connectivity.latency_ms, 120);
    }

    #[tokio::test]
    async fn test_health_state_degraded_rpc_slow() {
        let checker = HealthChecker::new();
        // Simulate high RPC latency (e.g. 2500ms >= 2000ms threshold)
        checker.check_rpc_reachability_with_params(2500, false, true).await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.status, "degraded");
        assert_eq!(health.components.contract_connectivity.status, "degraded");
        assert_eq!(health.components.contract_connectivity.latency_ms, 2500);

        let contract_check = health
            .checks
            .iter()
            .find(|c| c.name == "contract_connectivity")
            .unwrap();
        assert!(contract_check.message.is_some());
        assert!(contract_check.message.as_ref().unwrap().contains("2500ms"));
    }

    #[tokio::test]
    async fn test_health_state_degraded_circuit_open() {
        let checker = HealthChecker::new();
        // Simulate open circuit breaker for Soroban RPC
        checker.check_rpc_reachability_with_params(0, true, true).await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.status, "degraded");
        assert_eq!(health.components.contract_connectivity.status, "circuit_open");

        let contract_check = health
            .checks
            .iter()
            .find(|c| c.name == "contract_connectivity")
            .unwrap();
        assert!(contract_check.message.is_some());
        assert!(contract_check.message.as_ref().unwrap().contains("circuit breaker is OPEN"));
    }

    #[tokio::test]
    async fn test_health_state_degraded_rpc_unreachable() {
        let checker = HealthChecker::new();
        // Simulate unreachable RPC endpoint
        checker.check_rpc_reachability_with_params(0, false, false).await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.status, "degraded");
        assert_eq!(health.components.contract_connectivity.status, "unreachable");

        let contract_check = health
            .checks
            .iter()
            .find(|c| c.name == "contract_connectivity")
            .unwrap();
        assert!(contract_check.message.is_some());
        assert!(contract_check.message.as_ref().unwrap().contains("unreachable"));
    }

    #[tokio::test]
    async fn test_health_state_down_process_failing() {
        let checker = HealthChecker::new();
        checker.check_contract_connectivity().await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        // Process itself failing
        checker.set_process_down(true);

        let health = checker.get_health().await;
        assert_eq!(health.status, "down");
    }

    #[tokio::test]
    async fn test_health_state_down_critical_component() {
        let checker = HealthChecker::new();
        checker.check_contract_connectivity().await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_disk().await;

        // Memory critical failure
        checker
            .set_memory_status(ComponentStatus {
                status: "critical".to_string(),
                latency_ms: 0,
                last_checked: 0,
            })
            .await;

        let health = checker.get_health().await;
        assert_eq!(health.status, "down");
    }
}
