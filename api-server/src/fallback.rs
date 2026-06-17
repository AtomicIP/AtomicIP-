use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Configuration for fallback RPC endpoints
#[derive(Clone, Debug)]
pub struct FallbackConfig {
    pub primary_endpoint: String,
    pub fallback_endpoints: Vec<String>,
    pub health_check_interval: Duration,
    pub timeout: Duration,
}

/// Health status of an endpoint
#[derive(Clone, Debug)]
struct EndpointHealth {
    is_healthy: bool,
    last_check: Instant,
    consecutive_failures: u32,
}

/// Fallback endpoint manager
pub struct FallbackManager {
    config: FallbackConfig,
    endpoint_health: Arc<DashMap<String, EndpointHealth>>,
}

impl FallbackManager {
    pub fn new(config: FallbackConfig) -> Self {
        let endpoint_health = Arc::new(DashMap::new());
        
        // Initialize all endpoints as healthy
        endpoint_health.insert(
            config.primary_endpoint.clone(),
            EndpointHealth {
                is_healthy: true,
                last_check: Instant::now(),
                consecutive_failures: 0,
            },
        );
        
        for endpoint in &config.fallback_endpoints {
            endpoint_health.insert(
                endpoint.clone(),
                EndpointHealth {
                    is_healthy: true,
                    last_check: Instant::now(),
                    consecutive_failures: 0,
                },
            );
        }

        FallbackManager {
            config,
            endpoint_health,
        }
    }

    /// Get the next available endpoint (primary or fallback)
    pub fn get_active_endpoint(&self) -> String {
        // Try primary first
        if let Some(health) = self.endpoint_health.get(&self.config.primary_endpoint) {
            if health.is_healthy {
                return self.config.primary_endpoint.clone();
            }
        }

        // Try fallback endpoints in order
        for endpoint in &self.config.fallback_endpoints {
            if let Some(health) = self.endpoint_health.get(endpoint) {
                if health.is_healthy {
                    return endpoint.clone();
                }
            }
        }

        // If all are unhealthy, return primary (will retry)
        self.config.primary_endpoint.clone()
    }

    /// Mark an endpoint as failed
    pub fn mark_failed(&self, endpoint: &str) {
        if let Some(mut health) = self.endpoint_health.get_mut(endpoint) {
            health.consecutive_failures += 1;
            if health.consecutive_failures >= 3 {
                health.is_healthy = false;
                tracing::warn!(
                    endpoint = endpoint,
                    failures = health.consecutive_failures,
                    "Endpoint marked as unhealthy"
                );
            }
        }
    }

    /// Mark an endpoint as healthy
    pub fn mark_healthy(&self, endpoint: &str) {
        if let Some(mut health) = self.endpoint_health.get_mut(endpoint) {
            health.is_healthy = true;
            health.consecutive_failures = 0;
            health.last_check = Instant::now();
            tracing::info!(endpoint = endpoint, "Endpoint marked as healthy");
        }
    }

    /// Get health status of all endpoints
    pub fn get_health_status(&self) -> Vec<(String, bool)> {
        self.endpoint_health
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().is_healthy))
            .collect()
    }
}

/// Middleware to handle fallback endpoints
pub async fn fallback_middleware(
    req: Request,
    next: Next,
) -> Response {
    // This middleware would be used in conjunction with a request retry mechanism
    // The actual fallback logic would be implemented at the handler level
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_manager_creation() {
        let config = FallbackConfig {
            primary_endpoint: "http://primary.example.com".to_string(),
            fallback_endpoints: vec![
                "http://fallback1.example.com".to_string(),
                "http://fallback2.example.com".to_string(),
            ],
            health_check_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
        };

        let manager = FallbackManager::new(config);
        assert_eq!(
            manager.get_active_endpoint(),
            "http://primary.example.com"
        );
    }

    #[test]
    fn test_fallback_to_secondary_endpoint() {
        let config = FallbackConfig {
            primary_endpoint: "http://primary.example.com".to_string(),
            fallback_endpoints: vec!["http://fallback1.example.com".to_string()],
            health_check_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
        };

        let manager = FallbackManager::new(config);
        
        // Mark primary as failed
        manager.mark_failed("http://primary.example.com");
        manager.mark_failed("http://primary.example.com");
        manager.mark_failed("http://primary.example.com");

        // Should fallback to secondary
        assert_eq!(
            manager.get_active_endpoint(),
            "http://fallback1.example.com"
        );
    }

    #[test]
    fn test_endpoint_recovery() {
        let config = FallbackConfig {
            primary_endpoint: "http://primary.example.com".to_string(),
            fallback_endpoints: vec![],
            health_check_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
        };

        let manager = FallbackManager::new(config);
        
        // Mark as failed
        manager.mark_failed("http://primary.example.com");
        manager.mark_failed("http://primary.example.com");
        manager.mark_failed("http://primary.example.com");
        
        // Mark as healthy again
        manager.mark_healthy("http://primary.example.com");
        
        assert_eq!(
            manager.get_active_endpoint(),
            "http://primary.example.com"
        );
    }

    #[test]
    fn test_health_status() {
        let config = FallbackConfig {
            primary_endpoint: "http://primary.example.com".to_string(),
            fallback_endpoints: vec!["http://fallback1.example.com".to_string()],
            health_check_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
        };

        let manager = FallbackManager::new(config);
        let status = manager.get_health_status();
        
        assert_eq!(status.len(), 2);
        assert!(status.iter().all(|(_, healthy)| *healthy));
    }
}
