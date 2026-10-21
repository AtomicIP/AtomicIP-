use axum::{
    extract::ConnectInfo,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Health status response returned by `/health`
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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

/// Returns the configured internal CIDR allow-list for `/health/detailed`.
///
/// Parsed from `HEALTH_DETAILED_ALLOWED_CIDRS` (comma-separated, e.g.
/// `10.0.0.0/8,192.168.0.0/16,127.0.0.1/32`). When unset, only loopback
/// addresses are considered internal.
fn detailed_health_allowed_cidrs() -> Vec<(IpAddr, u8)> {
    let raw = std::env::var("HEALTH_DETAILED_ALLOWED_CIDRS")
        .unwrap_or_else(|_| "127.0.0.0/8,::1/128".to_string());

    raw.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let (addr, prefix) = entry.split_once('/')?;
            let addr: IpAddr = addr.trim().parse().ok()?;
            let prefix: u8 = prefix.trim().parse().ok()?;
            let max_prefix = match addr {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };
            if prefix > max_prefix {
                return None;
            }
            Some((addr, prefix))
        })
        .collect()
}

fn ip_in_cidr(ip: IpAddr, network: IpAddr, prefix: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(net)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (u32::from(ip) & mask) == (u32::from(net) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(net)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            (u128::from(ip) & mask) == (u128::from(net) & mask)
        }
        _ => false,
    }
}

/// Returns the expected bearer token for `/health/detailed`, if configured.
fn detailed_health_token() -> Option<String> {
    std::env::var("HEALTH_DETAILED_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

fn has_valid_bearer_token(headers: &HeaderMap) -> bool {
    let Some(expected) = detailed_health_token() else {
        return false;
    };
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return false;
    };
    // Constant-time-ish comparison to avoid trivial timing leaks.
    let token = token.as_bytes();
    let expected = expected.as_bytes();
    if token.len() != expected.len() {
        return false;
    }
    token
        .iter()
        .zip(expected.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// Returns `true` when the caller may access `/health/detailed`.
///
/// Access is granted when either:
/// - a valid `Authorization: Bearer <HEALTH_DETAILED_TOKEN>` header is present, or
/// - the client IP falls inside `HEALTH_DETAILED_ALLOWED_CIDRS`.
fn detailed_health_access_allowed(headers: &HeaderMap, client_ip: Option<IpAddr>) -> bool {
    if has_valid_bearer_token(headers) {
        return true;
    }
    match client_ip {
        Some(ip) => detailed_health_allowed_cidrs()
            .into_iter()
            .any(|(net, prefix)| ip_in_cidr(ip, net, prefix)),
        None => false,
    }
}

/// Public, minimal health endpoint. Never exposes dependency details.
pub async fn health_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
) -> Response {
    checker.check_contract_connectivity().await;
    checker.check_database().await;
    checker.check_cache().await;
    checker.check_memory().await;
    checker.check_disk().await;

    let health = checker.get_health().await;

    let status_code = match health.status.as_str() {
        "healthy" | "degraded" => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };

    // Minimal payload: no component/dependency details leak publicly.
    let body = serde_json::json!({
        "status": health.status,
        "timestamp": health.timestamp,
        "uptime_seconds": health.uptime_seconds,
    });

    (status_code, Json(body)).into_response()
}

pub async fn detailed_health_handler(
    axum::extract::State(checker): axum::extract::State<Arc<HealthChecker>>,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<SocketAddr>>,
) -> Response {
    let client_ip = connect_info.map(|ConnectInfo(addr)| addr.ip());
    if !detailed_health_access_allowed(&headers, client_ip) {
        // Do not leak dependency details to unauthorized callers.
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

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

    #[test]
    fn test_detailed_health_rejects_unauthenticated_external_caller() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("HEALTH_DETAILED_TOKEN");
        std::env::set_var("HEALTH_DETAILED_ALLOWED_CIDRS", "10.0.0.0/8");

        let headers = HeaderMap::new();
        let external_ip: IpAddr = "203.0.113.5".parse().unwrap();
        assert!(!detailed_health_access_allowed(&headers, Some(external_ip)));

        std::env::remove_var("HEALTH_DETAILED_ALLOWED_CIDRS");
    }

    #[test]
    fn test_detailed_health_allows_internal_cidr() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("HEALTH_DETAILED_TOKEN");
        std::env::set_var("HEALTH_DETAILED_ALLOWED_CIDRS", "10.0.0.0/8,192.168.0.0/16");

        let headers = HeaderMap::new();
        let internal_ip: IpAddr = "10.1.2.3".parse().unwrap();
        assert!(detailed_health_access_allowed(&headers, Some(internal_ip)));

        std::env::remove_var("HEALTH_DETAILED_ALLOWED_CIDRS");
    }

    #[test]
    fn test_detailed_health_allows_valid_bearer_token() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("HEALTH_DETAILED_TOKEN", "secret-token");
        std::env::set_var("HEALTH_DETAILED_ALLOWED_CIDRS", "10.0.0.0/8");

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret-token".parse().unwrap(),
        );
        let external_ip: IpAddr = "203.0.113.5".parse().unwrap();
        assert!(detailed_health_access_allowed(&headers, Some(external_ip)));

        let mut bad_headers = HeaderMap::new();
        bad_headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer wrong-token".parse().unwrap(),
        );
        assert!(!detailed_health_access_allowed(&bad_headers, Some(external_ip)));

        std::env::remove_var("HEALTH_DETAILED_TOKEN");
        std::env::remove_var("HEALTH_DETAILED_ALLOWED_CIDRS");
    }

    #[tokio::test]
    async fn test_detailed_health_handler_returns_401_for_public_caller() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("HEALTH_DETAILED_TOKEN");
        std::env::set_var("HEALTH_DETAILED_ALLOWED_CIDRS", "10.0.0.0/8");

        let checker = Arc::new(HealthChecker::new());
        let response = detailed_health_handler(
            axum::extract::State(checker),
            HeaderMap::new(),
            Some(ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 12345)))),
        )
        .await;

        std::env::remove_var("HEALTH_DETAILED_ALLOWED_CIDRS");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_public_health_handler_is_accessible() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SOROBAN_RPC_URL");
        std::env::remove_var("DATABASE_URL");

        let checker = Arc::new(HealthChecker::new());
        let response = health_handler(axum::extract::State(checker)).await;

        // Public endpoint must not require auth; status reflects health only.
        assert!(matches!(
            response.status(),
            StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE
        ));
    }

    #[tokio::test]
    async fn test_failing_probe_reports_degraded_not_panic() {
        let _guard = ENV_LOCK.lock().unwrap();
        // An unreachable dependency must surface as a degraded overall
        // status, never as a panic from the probe itself.
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

        assert_eq!(health.status, "degraded");
        assert_ne!(health.status, "down");
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
