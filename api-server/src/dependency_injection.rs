//! Dependency Injection Container for Atomic Patent API Server.
//!
//! # Issue #865: RPC Client Wiring
//!
//! This module implements a service container for dependency injection of the Soroban RPC client
//! and related services. The container ensures:
//!
//! - **Centralized Registration**: All services (RPC client, query client, etc.) are registered
//!   in one place at application startup (`main()` in `main.rs`).
//! - **Testability**: Handlers and business logic receive services via injection, enabling mocks
//!   to be swapped without code changes. See `create_test_container()` and `create_test_container_with_rpc()`.
//! - **Singleton Lifetime**: The RPC client is registered as a singleton so a single, reusable
//!   connection is shared across all handlers and requests.
//! - **Type Safety**: The container uses Rust's `TypeId` to maintain type-safe lookups without
//!   reflection or string keys.
//!
//! ## Handler Integration Pattern
//!
//! Handlers should extract the RPC client from `AppState` (which holds the container) or
//! directly from the container:
//!
//! ```ignore
//! // In a handler:
//! let state = axum::extract::State(app_state);
//! let container = state.container;
//! let rpc_client = container.resolve_rpc_client().expect("RPC client not registered");
//! // Use rpc_client for Soroban RPC calls
//! ```
//!
//! ## Test Setup Pattern
//!
//! ```ignore
//! #[tokio::test]
//! async fn test_handler_with_mock_rpc() {
//!     let mock_rpc = Arc::new(
//!         MockSorobanRpcClient::new()
//!             .with_swap_record(SwapRecord { ... })
//!     );
//!     let container = ServiceContainer::create_test_container_with_rpc(mock_rpc);
//!     let rpc = container.resolve_rpc_client().unwrap();
//!     // Exercise code that depends on the RPC client
//! }
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// Service lifetime configuration for dependency injection (#865).
/// Determines how instances are managed and reused in the container.
///
/// - **Singleton**: Single instance shared across all requests (e.g., RPC client, Redis connection).
/// - **Scoped**: New instance per request scope, useful for request-specific state.
/// - **Transient**: Fresh instance every resolution, for stateless utilities.
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
    pub fn register_instance<T: 'static + Send + Sync + ?Sized>(
        &self,
        instance: Arc<T>,
        name: String,
    ) {
        let type_id = TypeId::of::<Arc<T>>();
        let mut services = self.services.write().unwrap();
        let mut descriptors = self.descriptors.write().unwrap();

        services.insert(type_id, Arc::new(instance));
        descriptors.insert(
            type_id,
            ServiceDescriptor {
                name,
                lifetime: ServiceLifetime::Singleton,
            },
        );
    }

    /// Register a Soroban RPC client implementation in the DI container.
    /// 
    /// **Issue #865**: Ensures the Soroban RPC client is injected through the DI mechanism
    /// for testability and mocking. Handlers should resolve the client via `resolve_rpc_client()`
    /// rather than constructing it ad hoc.
    ///
    /// # Usage
    /// ```ignore
    /// let mock_rpc = Arc::new(MockSorobanRpcClient::default());
    /// container.register_rpc_client(mock_rpc);
    /// ```
    pub fn register_rpc_client(&self, client: Arc<dyn crate::graphql::SorobanRpcClient>) {
        self.register_instance::<dyn crate::graphql::SorobanRpcClient>(client, "SorobanRpcClient".to_string());
    }

    /// Resolve the registered Soroban RPC client from the container.
    /// 
    /// **Issue #865**: Returns `None` if the RPC client was not registered. Handlers and
    /// business logic should use this to obtain an RPC client for Soroban contract invocations.
    /// Enables unit testing by swapping a live RPC with a mock.
    ///
    /// # Returns
    /// `Some(Arc<dyn SorobanRpcClient>)` if registered; `None` otherwise.
    pub fn resolve_rpc_client(&self) -> Option<Arc<dyn crate::graphql::SorobanRpcClient>> {
        let type_id = TypeId::of::<Arc<dyn crate::graphql::SorobanRpcClient>>();
        let services = self.services.read().unwrap();
        services.get(&type_id).and_then(|service| {
            service.clone().downcast::<Arc<dyn crate::graphql::SorobanRpcClient>>().ok().map(|arc| (*arc).clone())
        })
    }

    /// Register a Soroban query client in the DI container.
    /// 
    /// The query client wraps the RPC client and provides high-level query interfaces
    /// for common read operations. See `resolve_query_client()` for retrieval.
    pub fn register_query_client(&self, client: Arc<crate::graphql::SorobanQueryClient>) {
        self.register_instance::<crate::graphql::SorobanQueryClient>(client, "SorobanQueryClient".to_string());
    }

    /// Resolve the registered Soroban query client from the container.
    /// 
    /// Returns the registered query client (a wrapper around the RPC client).
    /// Handlers should resolve this from the container rather than constructing it inline.
    pub fn resolve_query_client(&self) -> Option<Arc<crate::graphql::SorobanQueryClient>> {
        let type_id = TypeId::of::<Arc<crate::graphql::SorobanQueryClient>>();
        let services = self.services.read().unwrap();
        services.get(&type_id).and_then(|service| {
            service.clone().downcast::<Arc<crate::graphql::SorobanQueryClient>>().ok().map(|arc| (*arc).clone())
        })
    }

    /// Build a pre-configured test DI container populated with `MockSorobanRpcClient` and `SorobanQueryClient`.
    /// 
    /// **Issue #865**: Simplifies test setup by providing a default mock RPC client that doesn't
    /// require a live Soroban network. Use this for unit tests that exercise handlers without
    /// network I/O.
    ///
    /// # Example
    /// ```ignore
    /// let container = ServiceContainer::create_test_container();
    /// let rpc = container.resolve_rpc_client().unwrap();
    /// // Now pass rpc to handlers or use it directly in tests
    /// ```
    pub fn create_test_container() -> Self {
        let container = Self::new();
        let mock_rpc: Arc<dyn crate::graphql::SorobanRpcClient> = Arc::new(crate::graphql::MockSorobanRpcClient::default());
        let query_client = Arc::new(crate::graphql::SorobanQueryClient::new(mock_rpc.clone()));
        container.register_rpc_client(mock_rpc);
        container.register_query_client(query_client);
        container
    }

    /// Build a pre-configured test DI container with a specific custom or mock RPC client.
    /// 
    /// **Issue #865**: Enables test authors to provide a custom mock RPC that returns
    /// specific test data. Use this when `create_test_container()` doesn't provide enough
    /// control over mock behavior.
    ///
    /// # Example
    /// ```ignore
    /// let mock = Arc::new(
    ///     MockSorobanRpcClient::new()
    ///         .with_swap_record(SwapRecord { ... })
    /// );
    /// let container = ServiceContainer::create_test_container_with_rpc(mock);
    /// ```
    pub fn create_test_container_with_rpc(mock_rpc: Arc<dyn crate::graphql::SorobanRpcClient>) -> Self {
        let container = Self::new();
        let query_client = Arc::new(crate::graphql::SorobanQueryClient::new(mock_rpc.clone()));
        container.register_rpc_client(mock_rpc);
        container.register_query_client(query_client);
        container
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
    use crate::graphql::{IpRecord, MockSorobanRpcClient, SorobanRpcClient, SwapRecord, SwapStatus};

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

    // ── RPC Client DI Tests (#865) ────────────────────────────────────────────

    #[tokio::test]
    async fn test_rpc_client_registration_and_resolution() {
        let container = ServiceContainer::new();
        let mock_rpc = Arc::new(
            MockSorobanRpcClient::new().with_swap_record(SwapRecord {
                swap_id: 100,
                ip_registry_id: "CREG123".to_string(),
                ip_id: 42,
                seller: "GSELLER".to_string(),
                buyer: "GBUYER".to_string(),
                price: "5000".to_string(),
                token: "CTOKEN".to_string(),
                status: SwapStatus::Pending,
                expiry: 999999,
                arbitrator: None,
            }),
        );

        container.register_rpc_client(mock_rpc);

        let resolved = container.resolve_rpc_client();
        assert!(resolved.is_some());

        let client = resolved.unwrap();
        let swap = client.get_swap_record(100).await.unwrap();
        assert!(swap.is_some());
        assert_eq!(swap.unwrap().price, "5000");
    }

    #[tokio::test]
    async fn test_query_client_registration_and_resolution() {
        let container = ServiceContainer::new();
        let mock_rpc = Arc::new(
            MockSorobanRpcClient::new().with_owner_ips("GOWNER123", vec![1, 2, 3]),
        );
        let query_client = Arc::new(crate::graphql::SorobanQueryClient::new(mock_rpc));

        container.register_query_client(query_client);

        let resolved = container.resolve_query_client();
        assert!(resolved.is_some());

        let client = resolved.unwrap();
        let ips = client.list_ip_by_owner("GOWNER123").await.unwrap();
        assert_eq!(ips, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_create_test_container() {
        let container = ServiceContainer::create_test_container();
        assert!(container.resolve_rpc_client().is_some());
        assert!(container.resolve_query_client().is_some());
    }

    #[tokio::test]
    async fn test_mock_rpc_client_unit_test_isolation() {
        let mock_rpc = Arc::new(
            MockSorobanRpcClient::new()
                .with_ip_record(IpRecord {
                    ip_id: 1,
                    owner: "GOWNER".to_string(),
                    commitment_hash: "abc".to_string(),
                    timestamp: 12345,
                    revoked: false,
                })
                .with_swap_record(SwapRecord {
                    swap_id: 7,
                    ip_registry_id: "REG".to_string(),
                    ip_id: 1,
                    seller: "GSELLER".to_string(),
                    buyer: "GBUYER".to_string(),
                    price: "1000".to_string(),
                    token: "XLM".to_string(),
                    status: SwapStatus::Completed,
                    expiry: 5000,
                    arbitrator: None,
                }),
        );

        let container = ServiceContainer::create_test_container_with_rpc(mock_rpc);
        let rpc = container.resolve_rpc_client().unwrap();

        // Exercise RPC methods directly without any network
        let ip = rpc.get_ip_record(1).await.unwrap();
        assert!(ip.is_some());
        assert_eq!(ip.unwrap().owner, "GOWNER");

        let swap = rpc.get_swap_record(7).await.unwrap();
        assert!(swap.is_some());
        assert_eq!(swap.unwrap().status, SwapStatus::Completed);
    }
}

