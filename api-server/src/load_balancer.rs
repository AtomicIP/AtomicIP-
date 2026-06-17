use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceHealth {
    pub id: String,
    pub url: String,
    pub healthy: bool,
    pub request_count: usize,
    pub error_count: usize,
}

pub struct LoadBalancer {
    instances: Vec<Arc<Instance>>,
    current_index: Arc<AtomicUsize>,
}

struct Instance {
    id: String,
    url: String,
    request_count: Arc<AtomicUsize>,
    error_count: Arc<AtomicUsize>,
}

impl LoadBalancer {
    pub fn new(instance_urls: Vec<String>) -> Self {
        let instances = instance_urls
            .into_iter()
            .enumerate()
            .map(|(idx, url)| {
                Arc::new(Instance {
                    id: format!("instance-{}", idx),
                    url,
                    request_count: Arc::new(AtomicUsize::new(0)),
                    error_count: Arc::new(AtomicUsize::new(0)),
                })
            })
            .collect();

        Self {
            instances,
            current_index: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Round-robin load balancing
    pub fn get_next_instance(&self) -> Option<String> {
        if self.instances.is_empty() {
            return None;
        }

        let idx = self.current_index.fetch_add(1, Ordering::SeqCst) % self.instances.len();
        Some(self.instances[idx].url.clone())
    }

    /// Least-connections load balancing
    pub fn get_least_loaded_instance(&self) -> Option<String> {
        if self.instances.is_empty() {
            return None;
        }

        let instance = self.instances
            .iter()
            .min_by_key(|inst| inst.request_count.load(Ordering::SeqCst))
            .cloned();

        instance.map(|inst| inst.url.clone())
    }

    pub fn record_request(&self, instance_url: &str) {
        for inst in &self.instances {
            if inst.url == instance_url {
                inst.request_count.fetch_add(1, Ordering::SeqCst);
                break;
            }
        }
    }

    pub fn record_error(&self, instance_url: &str) {
        for inst in &self.instances {
            if inst.url == instance_url {
                inst.error_count.fetch_add(1, Ordering::SeqCst);
                break;
            }
        }
    }

    pub fn get_instance_health(&self) -> Vec<InstanceHealth> {
        self.instances
            .iter()
            .map(|inst| {
                let request_count = inst.request_count.load(Ordering::SeqCst);
                let error_count = inst.error_count.load(Ordering::SeqCst);
                let healthy = error_count == 0 || (request_count > 0 && error_count as f64 / request_count as f64 < 0.1);

                InstanceHealth {
                    id: inst.id.clone(),
                    url: inst.url.clone(),
                    healthy,
                    request_count,
                    error_count,
                }
            })
            .collect()
    }

    pub fn get_healthy_instances(&self) -> Vec<InstanceHealth> {
        self.get_instance_health()
            .into_iter()
            .filter(|h| h.healthy)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_balancer_creation() {
        let urls = vec!["http://localhost:8001".to_string(), "http://localhost:8002".to_string()];
        let lb = LoadBalancer::new(urls);
        assert_eq!(lb.instances.len(), 2);
    }

    #[test]
    fn test_round_robin_distribution() {
        let urls = vec!["http://localhost:8001".to_string(), "http://localhost:8002".to_string()];
        let lb = LoadBalancer::new(urls);

        let first = lb.get_next_instance();
        let second = lb.get_next_instance();
        let third = lb.get_next_instance();

        assert_eq!(first, Some("http://localhost:8001".to_string()));
        assert_eq!(second, Some("http://localhost:8002".to_string()));
        assert_eq!(third, Some("http://localhost:8001".to_string()));
    }

    #[test]
    fn test_least_loaded_instance() {
        let urls = vec!["http://localhost:8001".to_string(), "http://localhost:8002".to_string()];
        let lb = LoadBalancer::new(urls);

        lb.record_request("http://localhost:8001");
        lb.record_request("http://localhost:8001");

        let least_loaded = lb.get_least_loaded_instance();
        assert_eq!(least_loaded, Some("http://localhost:8002".to_string()));
    }

    #[test]
    fn test_instance_health_tracking() {
        let urls = vec!["http://localhost:8001".to_string()];
        let lb = LoadBalancer::new(urls);

        lb.record_request("http://localhost:8001");
        lb.record_request("http://localhost:8001");

        let health = lb.get_instance_health();
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].request_count, 2);
        assert_eq!(health[0].error_count, 0);
        assert!(health[0].healthy);
    }

    #[test]
    fn test_error_tracking() {
        let urls = vec!["http://localhost:8001".to_string()];
        let lb = LoadBalancer::new(urls);

        lb.record_request("http://localhost:8001");
        lb.record_error("http://localhost:8001");

        let health = lb.get_instance_health();
        assert_eq!(health[0].error_count, 1);
    }

    #[test]
    fn test_healthy_instances_filter() {
        let urls = vec!["http://localhost:8001".to_string(), "http://localhost:8002".to_string()];
        let lb = LoadBalancer::new(urls);

        lb.record_request("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");
        lb.record_error("http://localhost:8001");

        let healthy = lb.get_healthy_instances();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].url, "http://localhost:8002");
    }
}
