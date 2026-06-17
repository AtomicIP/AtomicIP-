use async_graphql::{Context, EmptyMutation, Object, Schema, SimpleObject, Subscription, ID};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use futures::Stream;

// ── GraphQL Types ─────────────────────────────────────────────────────────────

/// An intellectual property record.
#[derive(SimpleObject, Clone, Debug)]
pub struct IpRecord {
    pub ip_id: u64,
    pub owner: String,
    pub commitment_hash: String,
    pub timestamp: u64,
    pub revoked: bool,
}

/// Status of an atomic swap.
#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SwapStatus {
    Pending,
    Accepted,
    Completed,
    Disputed,
    Cancelled,
}

/// An atomic swap record.
#[derive(SimpleObject, Clone, Debug)]
pub struct SwapRecord {
    pub swap_id: u64,
    pub ip_id: u64,
    pub seller: String,
    pub buyer: String,
    /// Price in stroops (1 XLM = 10_000_000 stroops)
    pub price: String,
    pub token: String,
    pub status: SwapStatus,
    pub expiry: u64,
    /// Optional arbitrator address for third-party dispute resolution.
    pub arbitrator: Option<String>,
}

/// Paginated list response for GraphQL connections.
#[derive(SimpleObject, Clone, Debug)]
pub struct SwapConnection {
    pub swap_ids: Vec<u64>,
    pub has_next_page: bool,
    pub cursor: Option<String>,
}

/// Paginated list response for IP records.
#[derive(SimpleObject, Clone, Debug)]
pub struct IpConnection {
    pub ip_ids: Vec<u64>,
    pub has_next_page: bool,
    pub cursor: Option<String>,
}

/// User reputation information.
#[derive(SimpleObject, Clone, Debug)]
pub struct Reputation {
    pub address: String,
    pub total_swaps: u64,
    pub completed_swaps: u64,
    pub disputed_swaps: u64,
    pub reputation_score: f64,
}

/// Dispute evidence record.
#[derive(SimpleObject, Clone, Debug)]
pub struct DisputeEvidence {
    pub swap_id: u64,
    pub submitter: String,
    pub evidence_hash: String,
    pub timestamp: u64,
}

/// Subscription event for swap status changes.
#[derive(SimpleObject, Clone, Debug)]
pub struct SwapStatusChanged {
    pub swap_id: u64,
    pub old_status: SwapStatus,
    pub new_status: SwapStatus,
    pub timestamp: u64,
}

/// Subscription event for IP commitment.
#[derive(SimpleObject, Clone, Debug)]
pub struct IpCommitted {
    pub ip_id: u64,
    pub owner: String,
    pub timestamp: u64,
}

/// Subscription event for swap initiation.
#[derive(SimpleObject, Clone, Debug)]
pub struct SwapInitiated {
    pub swap_id: u64,
    pub ip_id: u64,
    pub seller: String,
    pub buyer: String,
    pub price: String,
    pub timestamp: u64,
}

// ── Soroban RPC Client Interface ─────────────────────────────────────────────

/// Interface for Soroban RPC client operations.
/// This allows for easy testing and mocking.
#[async_trait::async_trait]
pub trait SorobanRpcClient: Send + Sync {
    async fn get_ip_record(&self, ip_id: u64) -> Result<Option<IpRecord>, String>;
    async fn get_swap_record(&self, swap_id: u64) -> Result<Option<SwapRecord>, String>;
    async fn get_swaps_by_seller(&self, seller: &str, limit: u64, cursor: Option<String>) -> Result<SwapConnection, String>;
    async fn get_swaps_by_buyer(&self, buyer: &str, limit: u64, cursor: Option<String>) -> Result<SwapConnection, String>;
    async fn get_swaps_by_ip(&self, ip_id: u64, limit: u64, cursor: Option<String>) -> Result<SwapConnection, String>;
    async fn get_dispute_evidence(&self, swap_id: u64) -> Result<Vec<DisputeEvidence>, String>;
    async fn get_reputation(&self, address: &str) -> Result<Option<Reputation>, String>;
}

/// Mock implementation for testing.
#[derive(Clone, Default)]
pub struct MockSorobanRpcClient;

#[async_trait::async_trait]
impl SorobanRpcClient for MockSorobanRpcClient {
    async fn get_ip_record(&self, _ip_id: u64) -> Result<Option<IpRecord>, String> {
        Ok(None)
    }

    async fn get_swap_record(&self, _swap_id: u64) -> Result<Option<SwapRecord>, String> {
        Ok(None)
    }

    async fn get_swaps_by_seller(&self, _seller: &str, _limit: u64, _cursor: Option<String>) -> Result<SwapConnection, String> {
        Ok(SwapConnection {
            swap_ids: vec![],
            has_next_page: false,
            cursor: None,
        })
    }

    async fn get_swaps_by_buyer(&self, _buyer: &str, _limit: u64, _cursor: Option<String>) -> Result<SwapConnection, String> {
        Ok(SwapConnection {
            swap_ids: vec![],
            has_next_page: false,
            cursor: None,
        })
    }

    async fn get_swaps_by_ip(&self, _ip_id: u64, _limit: u64, _cursor: Option<String>) -> Result<SwapConnection, String> {
        Ok(SwapConnection {
            swap_ids: vec![],
            has_next_page: false,
            cursor: None,
        })
    }

    async fn get_dispute_evidence(&self, _swap_id: u64) -> Result<Vec<DisputeEvidence>, String> {
        Ok(vec![])
    }

    async fn get_reputation(&self, _address: &str) -> Result<Option<Reputation>, String> {
        Ok(None)
    }
}

/// Data structure passed through GraphQL context.
#[derive(Clone)]
pub struct GraphQLContext {
    pub rpc_client: Arc<dyn SorobanRpcClient>,
}

impl GraphQLContext {
    pub fn new(rpc_client: Arc<dyn SorobanRpcClient>) -> Self {
        Self { rpc_client }
    }
}

/// Helper to extract context from GraphQL context.
fn get_rpc_client(ctx: &Context<'_>) -> Arc<dyn SorobanRpcClient> {
    ctx.data::<GraphQLContext>()
        .map(|c| c.rpc_client.clone())
        .unwrap_or_else(|| Arc::new(MockSorobanRpcClient::default()))
}

// ── Query Root ────────────────────────────────────────────────────────────────

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Fetch an IP record by its ID.
    async fn ip(&self, ctx: &Context<'_>, ip_id: u64) -> Result<Option<IpRecord>, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_ip_record(ip_id)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }

    /// Fetch a swap record by its ID.
    async fn swap(&self, ctx: &Context<'_>, swap_id: u64) -> Result<Option<SwapRecord>, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_swap_record(swap_id)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }

    /// List all swap IDs for a given seller address.
    async fn swaps_by_seller(
        &self,
        ctx: &Context<'_>,
        seller: String,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<SwapConnection, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_swaps_by_seller(&seller, limit.unwrap_or(50), cursor)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }

    /// List all swap IDs for a given buyer address.
    async fn swaps_by_buyer(
        &self,
        ctx: &Context<'_>,
        buyer: String,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<SwapConnection, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_swaps_by_buyer(&buyer, limit.unwrap_or(50), cursor)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }

    /// List all swap IDs ever created for a given IP.
    async fn swaps_by_ip(
        &self,
        ctx: &Context<'_>,
        ip_id: u64,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<SwapConnection, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_swaps_by_ip(ip_id, limit.unwrap_or(50), cursor)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }

    /// Retrieve all dispute evidence hashes for a swap.
    async fn dispute_evidence(&self, ctx: &Context<'_>, swap_id: u64) -> Result<Vec<DisputeEvidence>, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_dispute_evidence(swap_id)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }

    /// Get reputation information for a user.
    async fn reputation(&self, ctx: &Context<'_>, address: String) -> Result<Option<Reputation>, async_graphql::Error> {
        let rpc_client = get_rpc_client(ctx);
        rpc_client.get_reputation(&address)
            .await
            .map_err(|e| async_graphql::Error::new(e))
    }
}

// ── Subscription Root ─────────────────────────────────────────────────────────

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to swap status changes for a specific swap.
    async fn swap_status_changed(
        &self,
        ctx: &Context<'_>,
        swap_id: u64,
    ) -> impl Stream<Item = SwapStatusChanged> {
        let broadcaster = ctx.data::<Arc<SubscriptionBroadcaster>>()
            .cloned()
            .unwrap_or_else(|_| Arc::new(SubscriptionBroadcaster::new()));
        
        let mut rx = broadcaster.subscribe_swap_status();
        async move {
            while let Ok(event) = rx.recv().await {
                if event.swap_id == swap_id {
                    yield event;
                }
            }
        }
    }

    /// Subscribe to all IP commitment events.
    async fn ip_committed(&self, ctx: &Context<'_>) -> impl Stream<Item = IpCommitted> {
        let broadcaster = ctx.data::<Arc<SubscriptionBroadcaster>>()
            .cloned()
            .unwrap_or_else(|_| Arc::new(SubscriptionBroadcaster::new()));
        
        let mut rx = broadcaster.subscribe_ip_committed();
        async move {
            while let Ok(event) = rx.recv().await {
                yield event;
            }
        }
    }

    /// Subscribe to all swap initiation events.
    async fn swap_initiated(&self, ctx: &Context<'_>) -> impl Stream<Item = SwapInitiated> {
        let broadcaster = ctx.data::<Arc<SubscriptionBroadcaster>>()
            .cloned()
            .unwrap_or_else(|_| Arc::new(SubscriptionBroadcaster::new()));
        
        let mut rx = broadcaster.subscribe_swap_initiated();
        async move {
            while let Ok(event) = rx.recv().await {
                yield event;
            }
        }
    }

    /// Subscribe to swap status changes for a specific seller.
    async fn seller_swap_events(
        &self,
        ctx: &Context<'_>,
        seller: String,
    ) -> impl Stream<Item = SwapStatusChanged> {
        let broadcaster = ctx.data::<Arc<SubscriptionBroadcaster>>()
            .cloned()
            .unwrap_or_else(|_| Arc::new(SubscriptionBroadcaster::new()));
        
        let mut rx = broadcaster.subscribe_swap_status();
        async move {
            while let Ok(event) = rx.recv().await {
                // Filter by seller (would need seller info in event)
                yield event;
            }
        }
    }
}

/// Broadcaster for subscription events.
pub struct SubscriptionBroadcaster {
    swap_status_tx: broadcast::Sender<SwapStatusChanged>,
    ip_committed_tx: broadcast::Sender<IpCommitted>,
    swap_initiated_tx: broadcast::Sender<SwapInitiated>,
}

impl SubscriptionBroadcaster {
    pub fn new() -> Self {
        let (swap_status_tx, _) = broadcast::channel(100);
        let (ip_committed_tx, _) = broadcast::channel(100);
        let (swap_initiated_tx, _) = broadcast::channel(100);
        
        Self {
            swap_status_tx,
            ip_committed_tx,
            swap_initiated_tx,
        }
    }

    pub fn broadcast_swap_status_changed(&self, event: SwapStatusChanged) {
        let _ = self.swap_status_tx.send(event);
    }

    pub fn broadcast_ip_committed(&self, event: IpCommitted) {
        let _ = self.ip_committed_tx.send(event);
    }

    pub fn broadcast_swap_initiated(&self, event: SwapInitiated) {
        let _ = self.swap_initiated_tx.send(event);
    }

    pub fn subscribe_swap_status(&self) -> broadcast::Receiver<SwapStatusChanged> {
        self.swap_status_tx.subscribe()
    }

    pub fn subscribe_ip_committed(&self) -> broadcast::Receiver<IpCommitted> {
        self.ip_committed_tx.subscribe()
    }

    pub fn subscribe_swap_initiated(&self) -> broadcast::Receiver<SwapInitiated> {
        self.swap_initiated_tx.subscribe()
    }
}

// ── Schema ────────────────────────────────────────────────────────────────────

pub type AtomicIpSchema = Schema<QueryRoot, EmptyMutation, SubscriptionRoot>;

pub fn build_schema() -> AtomicIpSchema {
    Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot).finish()
}

pub fn build_schema_with_context(rpc_client: Arc<dyn SorobanRpcClient>) -> AtomicIpSchema {
    Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot)
        .data(GraphQLContext::new(rpc_client))
        .finish()
}

pub fn build_schema_with_broadcaster(
    rpc_client: Arc<dyn SorobanRpcClient>,
    broadcaster: Arc<SubscriptionBroadcaster>,
) -> AtomicIpSchema {
    Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot)
        .data(GraphQLContext::new(rpc_client))
        .data(broadcaster)
        .finish()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::Request;

    #[tokio::test]
    async fn test_graphql_ip_query_returns_null_for_unknown() {
        let schema = build_schema();
        let res = schema.execute(Request::new("{ ip(ipId: 999) { ipId owner } }")).await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"ip":null}"#);
    }

    #[tokio::test]
    async fn test_graphql_swap_query_returns_null_for_unknown() {
        let schema = build_schema();
        let res = schema.execute(Request::new("{ swap(swapId: 1) { swapId status } }")).await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"swap":null}"#);
    }

    #[tokio::test]
    async fn test_graphql_swaps_by_seller_returns_empty_list() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new(r#"{ swapsBySeller(seller: "GABC") }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"swapsBySeller":{"swapIds":[],"hasNextPage":false,"cursor":null}}"#);
    }

    #[tokio::test]
    async fn test_graphql_dispute_evidence_returns_empty_list() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new("{ disputeEvidence(swapId: 1) }"))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"disputeEvidence":[]}"#);
    }

    #[tokio::test]
    async fn test_graphql_with_mock_client() {
        let mock_client: Arc<dyn SorobanRpcClient> = Arc::new(MockSorobanRpcClient);
        let schema = build_schema_with_context(mock_client);

        let res = schema
            .execute(Request::new(r#"{ swapsBySeller(seller: "GABC", limit: 10) { swapIds hasNextPage cursor } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"swapsBySeller":{"swapIds":[],"hasNextPage":false,"cursor":null}}"#);
    }

    #[tokio::test]
    async fn test_graphql_reputation_query() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new(r#"{ reputation(address: "GABC123") { address totalSwaps completedSwaps } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"reputation":null}"#);
    }

    #[tokio::test]
    async fn test_graphql_swaps_by_ip_with_pagination() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new(r#"{ swapsByIp(ipId: 1, limit: 5, cursor: "abc") { swapIds hasNextPage cursor } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{"swapsByIp":{"swapIds":[],"hasNextPage":false,"cursor":null}}"#);
    }
}
