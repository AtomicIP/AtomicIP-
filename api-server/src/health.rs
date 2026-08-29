use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Memory usage past this percentage is reported as `unhealthy`.
const MEMORY_UNHEALTHY_PERCENT: f64 = 90.0;
/// Memory usage past this percentage (but below the unhealthy threshold) is
/// reported as `degraded`. Both are overridable via env vars so operators
/// can tune thresholds per-deployment without a code change.
const MEMORY_DEGRADED_PERCENT: f64 = 75.0;
/// Disk usage past this percentage is reported as `unhealthy`.
const DISK_UNHEALTHY_PERCENT: f64 = 90.0;
/// Disk usage past this percentage (but below the unhealthy threshold) is
/// reported as `degraded`.
const DISK_DEGRADED_PERCENT: f64 = 75.0;

fn threshold_from_env(var: &str, default: f64) -> f64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub components: ComponentHealth,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub contract_connectivity: ComponentStatus,
    pub database: ComponentStatus,
    pub cache: ComponentStatus,
    pub memory: ComponentStatus,
    pub disk: ComponentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub status: String,
    pub latency_ms: u64,
    pub last_checked: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub version: String,
    pub components: ComponentHealth,
    pub checks: Vec<HealthCheck>,
}

/// Extracts `(host, port)` from a `scheme://[user[:pass]@]host:port[/path]`
/// connection string, e.g. a `postgres://` URL.
fn parse_host_port(url: &str) -> Option<(String, u16)> {
    let after_scheme = url.split("://").nth(1)?;
    let after_auth = after_scheme.rsplit('@').next()?;
    let host_port = after_auth.split('/').next()?;
    let mut parts = host_port.rsplitn(2, ':');
    let port: u16 = parts.next()?.parse().ok()?;
    let host = parts.next()?.to_string();
    Some((host, port))
}

/// Percentage of host memory currently in use, read from `/proc/meminfo`.
/// Returns `None` when the file isn't available (e.g. non-Linux hosts).
fn read_memory_used_percent() -> Option<f64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb: Option<f64> = None;
    let mut available_kb: Option<f64> = None;

    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest.trim().split_whitespace().next()?.parse().ok();
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available_kb = rest.trim().split_whitespace().next()?.parse().ok();
        }
    }

    let total_kb = total_kb?;
    let available_kb = available_kb?;
    if total_kb <= 0.0 {
        return None;
    }
    Some(100.0 * (total_kb - available_kb) / total_kb)
}

/// Percentage of root filesystem disk space currently in use, via `df`.
/// Returns `None` when the command isn't available or output can't be parsed.
fn read_disk_used_percent() -> Option<f64> {
    let output = std::process::Command::new("df").args(["-kP", "/"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let data_line = stdout.lines().nth(1)?;
    let fields: Vec<&str> = data_line.split_whitespace().collect();
    let used_kb: f64 = fields.get(2)?.parse().ok()?;
    let available_kb: f64 = fields.get(3)?.parse().ok()?;
    let total = used_kb + available_kb;
    if total <= 0.0 {
        return None;
    }
    Some(100.0 * used_kb / total)
}

pub struct HealthChecker {
    contract_status: Arc<RwLock<ComponentStatus>>,
    database_status: Arc<RwLock<ComponentStatus>>,
    cache_status: Arc<RwLock<ComponentStatus>>,
    memory_status: Arc<RwLock<ComponentStatus>>,
    disk_status: Arc<RwLock<ComponentStatus>>,
    start_time: std::time::SystemTime,
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
            start_time: std::time::SystemTime::now(),
        }
    }

    /// Probes the configured Soroban RPC endpoint with a lightweight
    /// `getHealth` JSON-RPC call. When `SOROBAN_RPC_URL` isn't set there is
    /// no contract dependency wired into this deployment, so it is reported
    /// healthy rather than penalized for a feature that isn't configured
    /// (mirrors the Redis fallback philosophy in `cache.rs`).
    pub async fn check_contract_connectivity(&self) -> ComponentStatus {
        let start = std::time::Instant::now();

        let status = match std::env::var("SOROBAN_RPC_URL") {
            Ok(url) if !url.is_empty() => {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build();
                match client {
                    Ok(client) => {
                        let body = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": 1,
                            "method": "getHealth",
                        });
                        match client.post(&url).json(&body).send().await {
                            Ok(resp) if resp.status().is_success() => "healthy".to_string(),
                            _ => "unhealthy".to_string(),
                        }
                    }
                    Err(_) => "unhealthy".to_string(),
                }
            }
            _ => "healthy".to_string(),
        };

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

        *self.contract_status.write().await = component.clone();
        component
    }

    /// Probes `DATABASE_URL` with a raw TCP connect. This codebase has no
    /// database driver wired in yet, so a full query round-trip isn't
    /// possible; a reachability check is the honest signal available today.
    /// Unset `DATABASE_URL` means no database dependency is configured for
    /// this deployment, so it is reported healthy.
    pub async fn check_database(&self) -> ComponentStatus {
        let start = std::time::Instant::now();

        let status = match std::env::var("DATABASE_URL") {
            Ok(url) if !url.is_empty() => match parse_host_port(&url) {
                Some((host, port)) => {
                    match tokio::time::timeout(
                        Duration::from_secs(3),
                        tokio::net::TcpStream::connect((host.as_str(), port)),
                    )
                    .await
                    {
                        Ok(Ok(_)) => "healthy".to_string(),
                        _ => "unhealthy".to_string(),
                    }
                }
                None => "unhealthy".to_string(),
            },
            _ => "healthy".to_string(),
        };

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

    /// Exercises the actual cache backend (Redis-backed or the in-process
    /// fallback, see `cache.rs`) with a real set/get round-trip rather than
    /// just inspecting whether it happens to be running in fallback mode —
    /// the fallback mode is itself a healthy, intentional degraded state.
    pub async fn check_cache(&self) -> ComponentStatus {
        let start = std::time::Instant::now();

        let probe_key = "__health_check__";
        let probe_value = start.elapsed().as_nanos().to_string();
        crate::cache::set_with_ttl(probe_key, &probe_value, 5);
        let status = match crate::cache::get::<String>(probe_key) {
            Some(v) if v == probe_value => "healthy".to_string(),
            _ => "unhealthy".to_string(),
        };

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

    /// Reads actual process/host memory usage from `/proc/meminfo` and
    /// reports degraded/unhealthy past configurable thresholds. When memory
    /// stats can't be read (e.g. non-Linux host), reports healthy rather
    /// than a false negative.
    pub async fn check_memory(&self) -> ComponentStatus {
        let start = std::time::Instant::now();

        let status = match read_memory_used_percent() {
            Some(pct) if pct >= threshold_from_env("MEMORY_UNHEALTHY_PERCENT", MEMORY_UNHEALTHY_PERCENT) => {
                "unhealthy".to_string()
            }
            Some(pct) if pct >= threshold_from_env("MEMORY_DEGRADED_PERCENT", MEMORY_DEGRADED_PERCENT) => {
                "degraded".to_string()
            }
            _ => "healthy".to_string(),
        };

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

    /// Reads actual host disk usage of the root filesystem via `df` and
    /// reports degraded/unhealthy past configurable thresholds. When disk
    /// stats can't be read, reports healthy rather than a false negative.
    pub async fn check_disk(&self) -> ComponentStatus {
        let start = std::time::Instant::now();

        let status = match read_disk_used_percent() {
            Some(pct) if pct >= threshold_from_env("DISK_UNHEALTHY_PERCENT", DISK_UNHEALTHY_PERCENT) => {
                "unhealthy".to_string()
            }
            Some(pct) if pct >= threshold_from_env("DISK_DEGRADED_PERCENT", DISK_DEGRADED_PERCENT) => {
                "degraded".to_string()
            }
            _ => "healthy".to_string(),
        };

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

    pub async fn get_health(&self) -> HealthStatus {
        let contract = self.contract_status.read().await.clone();
        let database = self.database_status.read().await.clone();
        let cache = self.cache_status.read().await.clone();
        let memory = self.memory_status.read().await.clone();
        let disk = self.disk_status.read().await.clone();

        let overall_status = if contract.status == "healthy"
            && database.status == "healthy"
            && cache.status == "healthy"
            && memory.status == "healthy"
            && disk.status == "healthy"
        {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let checks = vec![
            HealthCheck {
                name: "contract_connectivity".to_string(),
                status: contract.status.clone(),
                message: None,
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
                contract_connectivity: contract,
                database,
                cache,
                memory,
                disk,
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

    let status_code = if health.status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
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

    let status_code = if health.status == "healthy" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(detailed)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `SOROBAN_RPC_URL`/`DATABASE_URL` are process-global, and `cargo test`
    /// runs tests in this module concurrently on separate threads within
    /// the same process, so any test that sets/unsets them must hold this
    /// lock for the duration of the env mutation + assertions.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn valid_status(status: &str) -> bool {
        matches!(status, "healthy" | "degraded" | "unhealthy")
    }

    #[tokio::test]
    async fn test_health_checker_creation() {
        let checker = HealthChecker::new();
        let health = checker.get_health().await;
        assert_eq!(health.status, "degraded");
    }

    #[tokio::test]
    async fn test_check_contract_connectivity_unconfigured_is_healthy() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SOROBAN_RPC_URL");
        let checker = HealthChecker::new();
        let status = checker.check_contract_connectivity().await;
        assert_eq!(status.status, "healthy");
    }

    #[tokio::test]
    async fn test_check_contract_connectivity_unreachable_is_unhealthy() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Port 1 is a privileged port nothing is listening on in test
        // environments, so the connection attempt reliably fails/times out.
        std::env::set_var("SOROBAN_RPC_URL", "http://127.0.0.1:1");
        let checker = HealthChecker::new();
        let status = checker.check_contract_connectivity().await;
        std::env::remove_var("SOROBAN_RPC_URL");
        assert_eq!(status.status, "unhealthy");
    }

    #[tokio::test]
    async fn test_check_database_unconfigured_is_healthy() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("DATABASE_URL");
        let checker = HealthChecker::new();
        let status = checker.check_database().await;
        assert_eq!(status.status, "healthy");
    }

    #[tokio::test]
    async fn test_check_database_unreachable_is_unhealthy() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("DATABASE_URL", "postgres://user:pass@127.0.0.1:1/db");
        let checker = HealthChecker::new();
        let status = checker.check_database().await;
        std::env::remove_var("DATABASE_URL");
        assert_eq!(status.status, "unhealthy");
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
        assert!(valid_status(&status.status));
    }

    #[tokio::test]
    async fn test_check_disk() {
        let checker = HealthChecker::new();
        let status = checker.check_disk().await;
        assert!(valid_status(&status.status));
    }

    #[tokio::test]
    async fn test_all_components_healthy() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SOROBAN_RPC_URL");
        std::env::remove_var("DATABASE_URL");

        let checker = HealthChecker::new();
        checker.check_contract_connectivity().await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        assert_eq!(health.components.contract_connectivity.status, "healthy");
        assert_eq!(health.components.database.status, "healthy");
        assert_eq!(health.components.cache.status, "healthy");
        assert!(valid_status(&health.components.memory.status));
        assert!(valid_status(&health.components.disk.status));
    }

    #[tokio::test]
    async fn test_overall_status_reflects_worst_component() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("SOROBAN_RPC_URL", "http://127.0.0.1:1");
        std::env::remove_var("DATABASE_URL");

        let checker = HealthChecker::new();
        checker.check_contract_connectivity().await;
        checker.check_database().await;
        checker.check_cache().await;
        checker.check_memory().await;
        checker.check_disk().await;

        let health = checker.get_health().await;
        std::env::remove_var("SOROBAN_RPC_URL");

        assert_eq!(health.components.contract_connectivity.status, "unhealthy");
        assert_ne!(health.status, "healthy");
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
}
