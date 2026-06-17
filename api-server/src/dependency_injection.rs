use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    pub name: String,
    pub lifetime: ServiceLifetime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServiceLifetime {
    Transient,
    Scoped,
    Singleton,
}

pub struct ServiceContainer {
    services: Arc<std::sync::RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
    descriptors: Arc<std::sync::RwLock<HashMap<TypeId, ServiceDescriptor>>>,
}

impl ServiceContainer {
    pub fn new() -> Self {
        Self {
            services: Arc::new(std::sync::RwLock::new(HashMap::new())),
            descriptors: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn register_singleton<T: 'static + Send + Sync>(
        &self,
        service: T,
        name: String,
    ) {
        let type_id = TypeId::of::<T>();
        let mut services = self.services.write().unwrap();
        let mut descriptors = self.descriptors.write().unwrap();

        services.insert(type_id, Arc::new(service));
        descriptors.insert(
            type_id,
            ServiceDescriptor {
                name,
                lifetime: ServiceLifetime::Singleton,
            },
        );
    }

    pub fn register_transient<T: 'static + Send + Sync>(
        &self,
        name: String,
    ) {
        let type_id = TypeId::of::<T>();
        let mut descriptors = self.descriptors.write().unwrap();

        descriptors.insert(
            type_id,
            ServiceDescriptor {
                name,
                lifetime: ServiceLifetime::Transient,
            },
        );
    }

    pub fn register_scoped<T: 'static + Send + Sync>(
        &self,
        name: String,
    ) {
        let type_id = TypeId::of::<T>();
        let mut descriptors = self.descriptors.write().unwrap();

        descriptors.insert(
            type_id,
            ServiceDescriptor {
                name,
                lifetime: ServiceLifetime::Scoped,
            },
        );
    }

    pub fn resolve<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let type_id = TypeId::of::<T>();
        let services = self.services.read().unwrap();

        services.get(&type_id).and_then(|service| {
            service.clone().downcast::<T>().ok()
        })
    }

    pub fn get_descriptor(&self, type_id: TypeId) -> Option<ServiceDescriptor> {
        let descriptors = self.descriptors.read().unwrap();
        descriptors.get(&type_id).cloned()
    }

    pub fn get_all_descriptors(&self) -> Vec<(TypeId, ServiceDescriptor)> {
        let descriptors = self.descriptors.read().unwrap();
        descriptors
            .iter()
            .map(|(type_id, desc)| (*type_id, desc.clone()))
            .collect()
    }

    pub fn is_registered<T: 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        let descriptors = self.descriptors.read().unwrap();
        descriptors.contains_key(&type_id)
    }
}

impl Default for ServiceContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ServiceContainer {
    fn clone(&self) -> Self {
        Self {
            services: self.services.clone(),
            descriptors: self.descriptors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockService {
        value: String,
    }

    #[test]
    fn test_service_container_creation() {
        let container = ServiceContainer::new();
        assert!(!container.is_registered::<MockService>());
    }

    #[test]
    fn test_register_singleton() {
        let container = ServiceContainer::new();
        let service = MockService {
            value: "test".to_string(),
        };

        container.register_singleton(service, "MockService".to_string());
        assert!(container.is_registered::<MockService>());
    }

    #[test]
    fn test_resolve_singleton() {
        let container = ServiceContainer::new();
        let service = MockService {
            value: "test".to_string(),
        };

        container.register_singleton(service, "MockService".to_string());
        let resolved = container.resolve::<MockService>();

        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().value, "test");
    }

    #[test]
    fn test_register_transient() {
        let container = ServiceContainer::new();
        container.register_transient::<MockService>("MockService".to_string());
        assert!(container.is_registered::<MockService>());
    }

    #[test]
    fn test_register_scoped() {
        let container = ServiceContainer::new();
        container.register_scoped::<MockService>("MockService".to_string());
        assert!(container.is_registered::<MockService>());
    }

    #[test]
    fn test_get_descriptor() {
        let container = ServiceContainer::new();
        container.register_singleton(
            MockService {
                value: "test".to_string(),
            },
            "MockService".to_string(),
        );

        let type_id = TypeId::of::<MockService>();
        let descriptor = container.get_descriptor(type_id);

        assert!(descriptor.is_some());
        assert_eq!(descriptor.unwrap().lifetime, ServiceLifetime::Singleton);
    }

    #[test]
    fn test_get_all_descriptors() {
        let container = ServiceContainer::new();
        container.register_singleton(
            MockService {
                value: "test".to_string(),
            },
            "MockService".to_string(),
        );

        let descriptors = container.get_all_descriptors();
        assert_eq!(descriptors.len(), 1);
    }

    #[test]
    fn test_container_clone() {
        let container = ServiceContainer::new();
        container.register_singleton(
            MockService {
                value: "test".to_string(),
            },
            "MockService".to_string(),
        );

        let cloned = container.clone();
        assert!(cloned.is_registered::<MockService>());
    }
}
