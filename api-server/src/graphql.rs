use async_graphql::{Context, EmptyMutation, Object, Schema, SimpleObject, Subscription};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use futures::{Stream, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use redis::AsyncCommands;
use redis::streams::{StreamMaxlen, StreamReadOptions};

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
#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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

// ── Subscription Event Types ──────────────────────────────────────────────────

/// Subscription event emitted when a swap changes status.
/// Includes `seller` and `buyer` so clients can apply server-side filters.
#[derive(SimpleObject, Clone, Debug, Serialize, Deserialize)]
pub struct SwapStatusChanged {
    pub swap_id: u64,
    pub seller: String,
    pub buyer: String,
    pub old_status: SwapStatus,
    pub new_status: SwapStatus,
    pub timestamp: u64,
}

/// Subscription event emitted when an IP commitment is recorded on-chain.
#[derive(SimpleObject, Clone, Debug, Serialize, Deserialize)]
pub struct IpCommitted {
    pub ip_id: u64,
    pub owner: String,
    pub timestamp: u64,
}

/// Subscription event emitted when an atomic swap is initiated.
#[derive(SimpleObject, Clone, Debug, Serialize, Deserialize)]
pub struct SwapInitiated {
    pub swap_id: u64,
    pub ip_id: u64,
    pub seller: String,
    pub buyer: String,
    pub price: String,
    pub timestamp: u64,
}

/// Subscription event emitted when an atomic swap completes successfully.
#[derive(SimpleObject, Clone, Debug, Serialize, Deserialize)]
pub struct SwapCompleted {
    pub swap_id: u64,
    pub ip_id: u64,
    pub seller: String,
    pub buyer: String,
    pub price: String,
    pub timestamp: u64,
}

/// Subscription event emitted when an IP is assigned to a category.
#[derive(SimpleObject, Clone, Debug, Serialize, Deserialize)]
pub struct CategoryAssigned {
    pub ip_id: u64,
    pub owner: String,
    /// Hex-encoded Pedersen hash of the category path.
    pub category_hash: String,
    pub timestamp: u64,
}

/// Gives the Redis-backed backfill path a uniform way to filter buffered
/// events by `timestamp` without matching on each concrete event type.
trait HasTimestamp {
    fn timestamp(&self) -> u64;
}

impl HasTimestamp for SwapStatusChanged {
    fn timestamp(&self) -> u64 { self.timestamp }
}
impl HasTimestamp for IpCommitted {
    fn timestamp(&self) -> u64 { self.timestamp }
}
impl HasTimestamp for SwapInitiated {
    fn timestamp(&self) -> u64 { self.timestamp }
}
impl HasTimestamp for SwapCompleted {
    fn timestamp(&self) -> u64 { self.timestamp }
}
impl HasTimestamp for CategoryAssigned {
    fn timestamp(&self) -> u64 { self.timestamp }
}

// ── Soroban RPC Client Interface ─────────────────────────────────────────────

/// Interface for Soroban RPC client operations.
/// This allows for easy testing and mocking.
#[async_trait::async_trait]
pub trait SorobanRpcClient: Send + Sync {
    async fn get_ip_record(&self, ip_id: u64) -> Result<Option<IpRecord>, String>;
    async fn get_ips_by_owner(&self, owner: &str) -> Result<Vec<u64>, String>;
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

    async fn get_ips_by_owner(&self, _owner: &str) -> Result<Vec<u64>, String> {
        Ok(vec![])
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

/// Shared contract-query client used by REST handlers as well as GraphQL.
#[derive(Clone)]
pub struct SorobanQueryClient {
    rpc_client: Arc<dyn SorobanRpcClient>,
}

impl SorobanQueryClient {
    pub fn new(rpc_client: Arc<dyn SorobanRpcClient>) -> Self {
        Self { rpc_client }
    }

    pub async fn list_ip_by_owner(&self, owner: &str) -> Result<Vec<u64>, String> {
        self.rpc_client.get_ips_by_owner(owner).await
    }
}

impl GraphQLContext {
    pub fn new(rpc_client: Arc<dyn SorobanRpcClient>) -> Self {
        Self { rpc_client }
    }
}

fn get_rpc_client(ctx: &Context<'_>) -> Arc<dyn SorobanRpcClient> {
    ctx.data::<GraphQLContext>()
        .map(|c| c.rpc_client.clone())
        .unwrap_or_else(|_| Arc::new(MockSorobanRpcClient::default()))
}

fn get_broadcaster(ctx: &Context<'_>) -> Arc<SubscriptionBroadcaster> {
    ctx.data::<Arc<SubscriptionBroadcaster>>()
        .cloned()
        .unwrap_or_else(|_| Arc::new(SubscriptionBroadcaster::new()))
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
//
// All subscriptions are delivered over the `graphql-transport-ws` WebSocket
// subprotocol at `GET /graphql/ws`. Clients connect with a standard
// GraphQL WebSocket client (e.g., `graphql-ws` npm package or Apollo Client).
//
// ## Example — subscribe to all IP commitment events for a specific owner
//
// ```graphql
// subscription {
//   ipCommitted(owner: "GABC123XYZ") {
//     ipId
//     owner
//     timestamp
//   }
// }
// ```
//
// ## Example — subscribe to swap status changes for a specific swap
//
// ```graphql
// subscription {
//   swapStatusChanged(swapId: 42) {
//     swapId
//     oldStatus
//     newStatus
//     timestamp
//   }
// }
// ```
//
// ## Example — subscribe to completed swaps filtered by seller
//
// ```graphql
// subscription {
//   swapCompleted(seller: "GSELLER123") {
//     swapId
//     ipId
//     seller
//     buyer
//     price
//     timestamp
//   }
// }
// ```
//
// ## Example — subscribe to category assignment events for an owner
//
// ```graphql
// subscription {
//   categoryAssigned(owner: "GOWNER456") {
//     ipId
//     owner
//     categoryHash
//     timestamp
//   }
// }
// ```

/// Chains buffered backfill events (oldest first) in front of the live
/// broadcast stream, wrapping both sides in the same `Result` shape so a
/// single `filter_map` closure can apply per-connection filters uniformly
/// to backfilled and live events alike.
fn combined_stream<T: Clone + Send + Sync + 'static>(
    backfill: Vec<T>,
    rx: broadcast::Receiver<T>,
) -> impl Stream<Item = Result<T, tokio_stream::wrappers::errors::BroadcastStreamRecvError>> {
    futures::stream::iter(backfill.into_iter().map(Ok)).chain(BroadcastStream::new(rx))
}

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Stream every swap-status-change event.
    ///
    /// Filters (all optional, ANDed):
    /// - `swap_id` — only changes for this specific swap
    /// - `seller`  — only changes where the seller matches
    /// - `buyer`   — only changes where the buyer matches
    ///
    /// `since` (optional): the `timestamp` of the last event this client saw.
    /// When the server is running with `REDIS_URL` set, buffered events with
    /// a later timestamp are replayed before live delivery resumes — see
    /// docs/integration-guide.md for the exact guarantee. Ignored (no
    /// replay) in single-instance mode.
    ///
    /// ```graphql
    /// subscription {
    ///   swapStatusChanged(swapId: 7, seller: "GSELLER") {
    ///     swapId seller buyer oldStatus newStatus timestamp
    ///   }
    /// }
    /// ```
    async fn swap_status_changed(
        &self,
        ctx: &Context<'_>,
        swap_id: Option<u64>,
        seller: Option<String>,
        buyer: Option<String>,
        since: Option<u64>,
    ) -> impl Stream<Item = SwapStatusChanged> {
        let broadcaster = get_broadcaster(ctx);
        let backfilled = broadcaster.backfill_swap_status_changed(since).await;
        let rx = broadcaster.subscribe_swap_status();
        combined_stream(backfilled, rx).filter_map(move |r| {
            let seller = seller.clone();
            let buyer = buyer.clone();
            async move {
                let ev = r.ok()?;
                if swap_id.map_or(false, |id| ev.swap_id != id) { return None; }
                if seller.as_deref().map_or(false, |s| ev.seller != s) { return None; }
                if buyer.as_deref().map_or(false, |b| ev.buyer != b) { return None; }
                Some(ev)
            }
        })
    }

    /// Stream IP commitment events.
    ///
    /// Filter (optional):
    /// - `owner` — only events where the committing address matches
    ///
    /// ```graphql
    /// subscription {
    ///   ipCommitted(owner: "GABC123") {
    ///     ipId owner timestamp
    ///   }
    /// }
    /// ```
    async fn ip_committed(
        &self,
        ctx: &Context<'_>,
        owner: Option<String>,
        since: Option<u64>,
    ) -> impl Stream<Item = IpCommitted> {
        let broadcaster = get_broadcaster(ctx);
        let backfilled = broadcaster.backfill_ip_committed(since).await;
        let rx = broadcaster.subscribe_ip_committed();
        combined_stream(backfilled, rx).filter_map(move |r| {
            let owner = owner.clone();
            async move {
                let ev = r.ok()?;
                if owner.as_deref().map_or(false, |o| ev.owner != o) { return None; }
                Some(ev)
            }
        })
    }

    /// Stream swap initiation events.
    ///
    /// Filters (all optional, ANDed):
    /// - `seller` — only events where this address is the seller
    /// - `buyer`  — only events where this address is the buyer
    /// - `ip_id`  — only events for this specific IP
    ///
    /// ```graphql
    /// subscription {
    ///   swapInitiated(seller: "GSELLER") {
    ///     swapId ipId seller buyer price timestamp
    ///   }
    /// }
    /// ```
    async fn swap_initiated(
        &self,
        ctx: &Context<'_>,
        seller: Option<String>,
        buyer: Option<String>,
        ip_id: Option<u64>,
        since: Option<u64>,
    ) -> impl Stream<Item = SwapInitiated> {
        let broadcaster = get_broadcaster(ctx);
        let backfilled = broadcaster.backfill_swap_initiated(since).await;
        let rx = broadcaster.subscribe_swap_initiated();
        combined_stream(backfilled, rx).filter_map(move |r| {
            let seller = seller.clone();
            let buyer = buyer.clone();
            async move {
                let ev = r.ok()?;
                if seller.as_deref().map_or(false, |s| ev.seller != s) { return None; }
                if buyer.as_deref().map_or(false, |b| ev.buyer != b) { return None; }
                if ip_id.map_or(false, |id| ev.ip_id != id) { return None; }
                Some(ev)
            }
        })
    }

    /// Stream swap completion events.
    ///
    /// Filters (all optional, ANDed):
    /// - `swap_id` — only completions for this specific swap
    /// - `seller`  — only completions where this address is the seller
    /// - `buyer`   — only completions where this address is the buyer
    ///
    /// ```graphql
    /// subscription {
    ///   swapCompleted(buyer: "GBUYER99") {
    ///     swapId ipId seller buyer price timestamp
    ///   }
    /// }
    /// ```
    async fn swap_completed(
        &self,
        ctx: &Context<'_>,
        swap_id: Option<u64>,
        seller: Option<String>,
        buyer: Option<String>,
        since: Option<u64>,
    ) -> impl Stream<Item = SwapCompleted> {
        let broadcaster = get_broadcaster(ctx);
        let backfilled = broadcaster.backfill_swap_completed(since).await;
        let rx = broadcaster.subscribe_swap_completed();
        combined_stream(backfilled, rx).filter_map(move |r| {
            let seller = seller.clone();
            let buyer = buyer.clone();
            async move {
                let ev = r.ok()?;
                if swap_id.map_or(false, |id| ev.swap_id != id) { return None; }
                if seller.as_deref().map_or(false, |s| ev.seller != s) { return None; }
                if buyer.as_deref().map_or(false, |b| ev.buyer != b) { return None; }
                Some(ev)
            }
        })
    }

    /// Stream category assignment events.
    ///
    /// Filters (all optional, ANDed):
    /// - `owner`         — only events for this owner address
    /// - `category_hash` — only events for this specific category
    ///
    /// ```graphql
    /// subscription {
    ///   categoryAssigned(owner: "GOWNER456") {
    ///     ipId owner categoryHash timestamp
    ///   }
    /// }
    /// ```
    async fn category_assigned(
        &self,
        ctx: &Context<'_>,
        owner: Option<String>,
        category_hash: Option<String>,
        since: Option<u64>,
    ) -> impl Stream<Item = CategoryAssigned> {
        let broadcaster = get_broadcaster(ctx);
        let backfilled = broadcaster.backfill_category_assigned(since).await;
        let rx = broadcaster.subscribe_category_assigned();
        combined_stream(backfilled, rx).filter_map(move |r| {
            let owner = owner.clone();
            let category_hash = category_hash.clone();
            async move {
                let ev = r.ok()?;
                if owner.as_deref().map_or(false, |o| ev.owner != o) { return None; }
                if category_hash.as_deref().map_or(false, |c| ev.category_hash != c) { return None; }
                Some(ev)
            }
        })
    }

    /// Convenience: stream all swap-status-change events where `seller` is a party.
    ///
    /// ```graphql
    /// subscription {
    ///   sellerSwapEvents(seller: "GSELLER123") {
    ///     swapId oldStatus newStatus timestamp
    ///   }
    /// }
    /// ```
    async fn seller_swap_events(
        &self,
        ctx: &Context<'_>,
        seller: String,
        since: Option<u64>,
    ) -> impl Stream<Item = SwapStatusChanged> {
        let broadcaster = get_broadcaster(ctx);
        let backfilled = broadcaster.backfill_swap_status_changed(since).await;
        let rx = broadcaster.subscribe_swap_status();
        combined_stream(backfilled, rx).filter_map(move |r| {
            let seller = seller.clone();
            async move {
                let ev = r.ok()?;
                if ev.seller == seller { Some(ev) } else { None }
            }
        })
    }
}

// ── Subscription Broadcaster ──────────────────────────────────────────────────

/// Redis key names backing each event type's stream. Namespaced under
/// `atomicip:` and capped at `STREAM_MAXLEN` entries so the stream also
/// serves as the reconnect/backfill buffer (see `backfill_stream`).
const STREAM_SWAP_STATUS_CHANGED: &str  = "atomicip:swap_status_changed";
const STREAM_IP_COMMITTED: &str         = "atomicip:ip_committed";
const STREAM_SWAP_INITIATED: &str       = "atomicip:swap_initiated";
const STREAM_SWAP_COMPLETED: &str       = "atomicip:swap_completed";
const STREAM_CATEGORY_ASSIGNED: &str    = "atomicip:category_assigned";
const ALL_STREAMS: [&str; 5] = [
    STREAM_SWAP_STATUS_CHANGED,
    STREAM_IP_COMMITTED,
    STREAM_SWAP_INITIATED,
    STREAM_SWAP_COMPLETED,
    STREAM_CATEGORY_ASSIGNED,
];

/// Bounds each stream to (approximately) this many entries, trimmed by
/// Redis on every `XADD`. This is also the effective backfill window.
const STREAM_MAXLEN: usize = 500;

/// Holds the Redis connection used to fan events out across instances.
/// Kept as its own struct so `SubscriptionBroadcaster::redis` can stay a
/// plain `Option`, making the single-instance path (`redis: None`) a
/// straight passthrough with zero behavior change from before this field
/// existed.
struct RedisLink {
    /// Cheaply `Clone`-able handle used for `XADD` publishes.
    conn: redis::aio::MultiplexedConnection,
}

/// Holds the broadcast channels that connect contract-event ingestion to
/// GraphQL subscription streams.  Wrap in `Arc` and inject via schema data.
///
/// In single-instance mode (`redis: None`, via `new()`) `broadcast_*`
/// methods write directly to the local `tokio::sync::broadcast` channel, as
/// they always have. When constructed via `new_with_redis`, `broadcast_*`
/// instead publishes to a shared Redis Stream; a background task owned by
/// every instance tails that stream and is the *only* thing that feeds the
/// local channels in that mode — so an event published by any instance
/// reaches WebSocket clients connected to every instance exactly once,
/// including the instance that published it.
pub struct SubscriptionBroadcaster {
    swap_status_tx:      broadcast::Sender<SwapStatusChanged>,
    ip_committed_tx:     broadcast::Sender<IpCommitted>,
    swap_initiated_tx:   broadcast::Sender<SwapInitiated>,
    swap_completed_tx:   broadcast::Sender<SwapCompleted>,
    category_assigned_tx: broadcast::Sender<CategoryAssigned>,
    redis: Option<RedisLink>,
}

impl SubscriptionBroadcaster {
    pub fn new() -> Self {
        let (swap_status_tx, _)       = broadcast::channel(100);
        let (ip_committed_tx, _)      = broadcast::channel(100);
        let (swap_initiated_tx, _)    = broadcast::channel(100);
        let (swap_completed_tx, _)    = broadcast::channel(100);
        let (category_assigned_tx, _) = broadcast::channel(100);

        Self {
            swap_status_tx,
            ip_committed_tx,
            swap_initiated_tx,
            swap_completed_tx,
            category_assigned_tx,
            redis: None,
        }
    }

    /// Builds a broadcaster backed by a shared Redis Stream at `redis_url`,
    /// so events published on this instance reach WebSocket clients
    /// connected to every other instance pointed at the same Redis, and
    /// vice versa. Spawns a background task that tails all five streams for
    /// the lifetime of the returned `Arc`.
    pub async fn new_with_redis(redis_url: &str) -> Result<Arc<Self>, String> {
        let client = redis::Client::open(redis_url).map_err(|e| e.to_string())?;
        let publish_conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| e.to_string())?;
        let read_conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| e.to_string())?;

        let (swap_status_tx, _)       = broadcast::channel(100);
        let (ip_committed_tx, _)      = broadcast::channel(100);
        let (swap_initiated_tx, _)    = broadcast::channel(100);
        let (swap_completed_tx, _)    = broadcast::channel(100);
        let (category_assigned_tx, _) = broadcast::channel(100);

        let broadcaster = Arc::new(Self {
            swap_status_tx,
            ip_committed_tx,
            swap_initiated_tx,
            swap_completed_tx,
            category_assigned_tx,
            redis: Some(RedisLink { conn: publish_conn }),
        });

        tokio::spawn(tail_redis_streams(broadcaster.clone(), read_conn));

        Ok(broadcaster)
    }

    // ── Publishers ────────────────────────────────────────────────────────────
    //
    // In Redis mode these publish-and-return immediately (the actual XADD
    // happens on a spawned task) so the method keeps its existing sync
    // signature; delivery to *this* instance's own local subscribers still
    // happens, via the background tail task reading the same stream back.

    pub fn broadcast_swap_status_changed(&self, event: SwapStatusChanged) {
        match &self.redis {
            None => { let _ = self.swap_status_tx.send(event); }
            Some(link) => publish_to_stream(link, STREAM_SWAP_STATUS_CHANGED, &event),
        }
    }

    pub fn broadcast_ip_committed(&self, event: IpCommitted) {
        match &self.redis {
            None => { let _ = self.ip_committed_tx.send(event); }
            Some(link) => publish_to_stream(link, STREAM_IP_COMMITTED, &event),
        }
    }

    pub fn broadcast_swap_initiated(&self, event: SwapInitiated) {
        match &self.redis {
            None => { let _ = self.swap_initiated_tx.send(event); }
            Some(link) => publish_to_stream(link, STREAM_SWAP_INITIATED, &event),
        }
    }

    pub fn broadcast_swap_completed(&self, event: SwapCompleted) {
        match &self.redis {
            None => { let _ = self.swap_completed_tx.send(event); }
            Some(link) => publish_to_stream(link, STREAM_SWAP_COMPLETED, &event),
        }
    }

    pub fn broadcast_category_assigned(&self, event: CategoryAssigned) {
        match &self.redis {
            None => { let _ = self.category_assigned_tx.send(event); }
            Some(link) => publish_to_stream(link, STREAM_CATEGORY_ASSIGNED, &event),
        }
    }

    // ── Subscriber handles ────────────────────────────────────────────────────

    pub fn subscribe_swap_status(&self) -> broadcast::Receiver<SwapStatusChanged> {
        self.swap_status_tx.subscribe()
    }

    pub fn subscribe_ip_committed(&self) -> broadcast::Receiver<IpCommitted> {
        self.ip_committed_tx.subscribe()
    }

    pub fn subscribe_swap_initiated(&self) -> broadcast::Receiver<SwapInitiated> {
        self.swap_initiated_tx.subscribe()
    }

    pub fn subscribe_swap_completed(&self) -> broadcast::Receiver<SwapCompleted> {
        self.swap_completed_tx.subscribe()
    }

    pub fn subscribe_category_assigned(&self) -> broadcast::Receiver<CategoryAssigned> {
        self.category_assigned_tx.subscribe()
    }

    // ── Reconnect / backfill ──────────────────────────────────────────────────
    //
    // No-ops (empty result) outside Redis mode, or when the caller has no
    // `since` cursor — see docs/integration-guide.md for the documented
    // guarantee.

    pub async fn backfill_swap_status_changed(&self, since: Option<u64>) -> Vec<SwapStatusChanged> {
        self.backfill_stream(STREAM_SWAP_STATUS_CHANGED, since).await
    }

    pub async fn backfill_ip_committed(&self, since: Option<u64>) -> Vec<IpCommitted> {
        self.backfill_stream(STREAM_IP_COMMITTED, since).await
    }

    pub async fn backfill_swap_initiated(&self, since: Option<u64>) -> Vec<SwapInitiated> {
        self.backfill_stream(STREAM_SWAP_INITIATED, since).await
    }

    pub async fn backfill_swap_completed(&self, since: Option<u64>) -> Vec<SwapCompleted> {
        self.backfill_stream(STREAM_SWAP_COMPLETED, since).await
    }

    pub async fn backfill_category_assigned(&self, since: Option<u64>) -> Vec<CategoryAssigned> {
        self.backfill_stream(STREAM_CATEGORY_ASSIGNED, since).await
    }

    async fn backfill_stream<T: DeserializeOwned + HasTimestamp>(
        &self,
        stream: &str,
        since: Option<u64>,
    ) -> Vec<T> {
        let (Some(link), Some(since)) = (&self.redis, since) else {
            return Vec::new();
        };
        let mut conn = link.conn.clone();
        let reply: redis::streams::StreamRangeReply = match conn.xrange_all(stream).await {
            Ok(reply) => reply,
            Err(_) => return Vec::new(),
        };
        reply
            .ids
            .into_iter()
            .filter_map(|entry| entry.get::<String>("payload"))
            .filter_map(|payload| serde_json::from_str::<T>(&payload).ok())
            .filter(|event| event.timestamp() > since)
            .collect()
    }
}

/// Serializes `event` and hands it off to a spawned task that `XADD`s it to
/// `stream`, trimmed to `STREAM_MAXLEN`. Fire-and-forget, matching the
/// best-effort delivery semantics `broadcast::Sender::send` already had.
fn publish_to_stream<T: Serialize>(link: &RedisLink, stream: &'static str, event: &T) {
    let Ok(payload) = serde_json::to_string(event) else { return };
    let mut conn = link.conn.clone();
    tokio::spawn(async move {
        let _: Result<String, _> = conn
            .xadd_maxlen(stream, StreamMaxlen::Approx(STREAM_MAXLEN), "*", &[("payload", payload)])
            .await;
    });
}

/// Background task (one per `SubscriptionBroadcaster` built via
/// `new_with_redis`) that blocks on `XREAD` across all five streams and
/// republishes each entry into the matching local `broadcast::Sender`. This
/// is the only path that feeds the local channels in Redis mode, whether
/// the event originated on this instance or another one.
async fn tail_redis_streams(broadcaster: Arc<SubscriptionBroadcaster>, mut conn: redis::aio::MultiplexedConnection) {
    // Indices line up 1:1 with `ALL_STREAMS`. Starting every cursor at `$`
    // means a freshly-started tail only sees entries added from here on —
    // history is served separately, on demand, via `backfill_stream`.
    let mut last_ids: [String; 5] = std::array::from_fn(|_| "$".to_string());

    loop {
        let ids: Vec<String> = last_ids.to_vec();
        let options = StreamReadOptions::default().block(5_000);
        let reply: redis::streams::StreamReadReply =
            match conn.xread_options(&ALL_STREAMS, &ids, &options).await {
                Ok(reply) => reply,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
            };

        for key in reply.keys {
            let stream_index = ALL_STREAMS.iter().position(|s| *s == key.key);
            for entry in key.ids {
                if let Some(idx) = stream_index {
                    last_ids[idx] = entry.id.clone();
                }
                let Some(payload) = entry.get::<String>("payload") else { continue };

                if key.key == STREAM_SWAP_STATUS_CHANGED {
                    if let Ok(ev) = serde_json::from_str(&payload) {
                        let _ = broadcaster.swap_status_tx.send(ev);
                    }
                } else if key.key == STREAM_IP_COMMITTED {
                    if let Ok(ev) = serde_json::from_str(&payload) {
                        let _ = broadcaster.ip_committed_tx.send(ev);
                    }
                } else if key.key == STREAM_SWAP_INITIATED {
                    if let Ok(ev) = serde_json::from_str(&payload) {
                        let _ = broadcaster.swap_initiated_tx.send(ev);
                    }
                } else if key.key == STREAM_SWAP_COMPLETED {
                    if let Ok(ev) = serde_json::from_str(&payload) {
                        let _ = broadcaster.swap_completed_tx.send(ev);
                    }
                } else if key.key == STREAM_CATEGORY_ASSIGNED {
                    if let Ok(ev) = serde_json::from_str(&payload) {
                        let _ = broadcaster.category_assigned_tx.send(ev);
                    }
                }
            }
        }
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
    use futures::StreamExt;
    use tokio_stream::wrappers::BroadcastStream;

    // ── Existing query tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_graphql_ip_query_returns_null_for_unknown() {
        let schema = build_schema();
        let res = schema.execute(Request::new("{ ip(ipId: 999) { ipId owner } }")).await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"ip":null}"#);
    }

    #[tokio::test]
    async fn test_graphql_swap_query_returns_null_for_unknown() {
        let schema = build_schema();
        let res = schema.execute(Request::new("{ swap(swapId: 1) { swapId status } }")).await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"swap":null}"#);
    }

    #[tokio::test]
    async fn test_graphql_swaps_by_seller_returns_empty_list() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new(r#"{ swapsBySeller(seller: "GABC") { swapIds hasNextPage cursor } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"swapsBySeller":{"swapIds":[],"hasNextPage":false,"cursor":null}}"#);
    }

    #[tokio::test]
    async fn test_graphql_dispute_evidence_returns_empty_list() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new("{ disputeEvidence(swapId: 1) { swapId submitter } }"))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"disputeEvidence":[]}"#);
    }

    #[tokio::test]
    async fn test_graphql_with_mock_client() {
        let mock_client: Arc<dyn SorobanRpcClient> = Arc::new(MockSorobanRpcClient);
        let schema = build_schema_with_context(mock_client);

        let res = schema
            .execute(Request::new(r#"{ swapsBySeller(seller: "GABC", limit: 10) { swapIds hasNextPage cursor } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"swapsBySeller":{"swapIds":[],"hasNextPage":false,"cursor":null}}"#);
    }

    #[tokio::test]
    async fn test_graphql_reputation_query() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new(r#"{ reputation(address: "GABC123") { address totalSwaps completedSwaps } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"reputation":null}"#);
    }

    #[tokio::test]
    async fn test_graphql_swaps_by_ip_with_pagination() {
        let schema = build_schema();
        let res = schema
            .execute(Request::new(r#"{ swapsByIp(ipId: 1, limit: 5, cursor: "abc") { swapIds hasNextPage cursor } }"#))
            .await;
        assert!(res.errors.is_empty(), "unexpected errors: {:?}", res.errors);
        assert_eq!(serde_json::to_string(&res.data).unwrap(), r#"{"swapsByIp":{"swapIds":[],"hasNextPage":false,"cursor":null}}"#);
    }

    // ── Subscription broadcaster unit tests ───────────────────────────────────

    #[tokio::test]
    async fn test_ip_committed_broadcast_received() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let mut rx = b.subscribe_ip_committed();

        b.broadcast_ip_committed(IpCommitted {
            ip_id: 1,
            owner: "GOWNER1".to_string(),
            timestamp: 1000,
        });

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.ip_id, 1);
        assert_eq!(ev.owner, "GOWNER1");
    }

    #[tokio::test]
    async fn test_swap_initiated_broadcast_received() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let mut rx = b.subscribe_swap_initiated();

        b.broadcast_swap_initiated(SwapInitiated {
            swap_id: 5,
            ip_id: 2,
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: "1000000".to_string(),
            timestamp: 2000,
        });

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.swap_id, 5);
    }

    #[tokio::test]
    async fn test_swap_completed_broadcast_received() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let mut rx = b.subscribe_swap_completed();

        b.broadcast_swap_completed(SwapCompleted {
            swap_id: 9,
            ip_id: 3,
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: "500000".to_string(),
            timestamp: 3000,
        });

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.swap_id, 9);
        assert_eq!(ev.ip_id, 3);
    }

    #[tokio::test]
    async fn test_category_assigned_broadcast_received() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let mut rx = b.subscribe_category_assigned();

        b.broadcast_category_assigned(CategoryAssigned {
            ip_id: 7,
            owner: "GOWNER".to_string(),
            category_hash: "deadbeef".to_string(),
            timestamp: 4000,
        });

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.ip_id, 7);
        assert_eq!(ev.category_hash, "deadbeef");
    }

    // ── Multiple concurrent subscriptions ────────────────────────────────────

    #[tokio::test]
    async fn test_multiple_concurrent_subscribers_all_receive_event() {
        let b = Arc::new(SubscriptionBroadcaster::new());

        let mut receivers: Vec<broadcast::Receiver<IpCommitted>> =
            (0..5).map(|_| b.subscribe_ip_committed()).collect();

        b.broadcast_ip_committed(IpCommitted {
            ip_id: 42,
            owner: "GMULTI".to_string(),
            timestamp: 9999,
        });

        for rx in &mut receivers {
            let ev = rx.recv().await.unwrap();
            assert_eq!(ev.ip_id, 42);
        }
    }

    #[tokio::test]
    async fn test_concurrent_mixed_subscriptions_independent() {
        let b = Arc::new(SubscriptionBroadcaster::new());

        let mut ip_rx1 = b.subscribe_ip_committed();
        let mut ip_rx2 = b.subscribe_ip_committed();
        let mut swap_rx = b.subscribe_swap_initiated();

        b.broadcast_ip_committed(IpCommitted { ip_id: 1, owner: "A".into(), timestamp: 1 });
        b.broadcast_swap_initiated(SwapInitiated {
            swap_id: 2, ip_id: 1,
            seller: "S".into(), buyer: "B".into(),
            price: "100".into(), timestamp: 2,
        });

        assert_eq!(ip_rx1.recv().await.unwrap().ip_id, 1);
        assert_eq!(ip_rx2.recv().await.unwrap().ip_id, 1);
        assert_eq!(swap_rx.recv().await.unwrap().swap_id, 2);
    }

    // ── Rapid event delivery ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_rapid_event_delivery_no_drops_within_capacity() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let mut rx = b.subscribe_ip_committed();

        // Channel capacity is 100; send 50 events synchronously before reading.
        for i in 0u64..50 {
            b.broadcast_ip_committed(IpCommitted {
                ip_id: i,
                owner: "GRAPID".to_string(),
                timestamp: i,
            });
        }

        for i in 0u64..50 {
            let ev = rx.recv().await.unwrap();
            assert_eq!(ev.ip_id, i);
        }
    }

    #[tokio::test]
    async fn test_broadcast_stream_skips_lagged_messages_gracefully() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let rx = b.subscribe_ip_committed();

        // Overflow the channel (capacity = 100) to trigger Lagged errors.
        for i in 0u64..120 {
            b.broadcast_ip_committed(IpCommitted {
                ip_id: i,
                owner: "GLAG".to_string(),
                timestamp: i,
            });
        }

        // BroadcastStream::filter_map with r.ok() silently skips Lagged entries.
        let received: Vec<IpCommitted> = BroadcastStream::new(rx)
            .filter_map(|r| async move { r.ok() })
            .take(10)
            .collect()
            .await;

        // We should receive *some* events — the ones that weren't overwritten.
        assert!(!received.is_empty());
    }

    // ── Subscription filter tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_ip_committed_owner_filter_excludes_other_owners() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let rx = b.subscribe_ip_committed();

        let target_owner = "GTARGET".to_string();
        let filtered: tokio::task::JoinHandle<Vec<IpCommitted>> = {
            let target_owner = target_owner.clone();
            tokio::spawn(async move {
                BroadcastStream::new(rx)
                    .filter_map(move |r| {
                        let owner = target_owner.clone();
                        async move {
                            let ev = r.ok()?;
                            if ev.owner == owner { Some(ev) } else { None }
                        }
                    })
                    .take(1)
                    .collect()
                    .await
            })
        };

        // Emit one non-matching event, then the matching one.
        b.broadcast_ip_committed(IpCommitted { ip_id: 1, owner: "GOTHER".into(), timestamp: 1 });
        b.broadcast_ip_committed(IpCommitted { ip_id: 2, owner: target_owner.clone(), timestamp: 2 });

        let results = filtered.await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ip_id, 2);
        assert_eq!(results[0].owner, target_owner);
    }

    #[tokio::test]
    async fn test_swap_status_changed_swap_id_filter() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let rx = b.subscribe_swap_status();

        let target_id = 99u64;
        let handle: tokio::task::JoinHandle<Vec<SwapStatusChanged>> = tokio::spawn(async move {
            BroadcastStream::new(rx)
                .filter_map(move |r| async move {
                    let ev = r.ok()?;
                    if ev.swap_id == target_id { Some(ev) } else { None }
                })
                .take(1)
                .collect()
                .await
        });

        b.broadcast_swap_status_changed(SwapStatusChanged {
            swap_id: 1, seller: "S".into(), buyer: "B".into(),
            old_status: SwapStatus::Pending, new_status: SwapStatus::Accepted, timestamp: 1,
        });
        b.broadcast_swap_status_changed(SwapStatusChanged {
            swap_id: 99, seller: "S".into(), buyer: "B".into(),
            old_status: SwapStatus::Accepted, new_status: SwapStatus::Completed, timestamp: 2,
        });

        let results = handle.await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].swap_id, 99);
        assert_eq!(results[0].new_status, SwapStatus::Completed);
    }

    #[tokio::test]
    async fn test_seller_swap_events_filters_by_seller() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let rx = b.subscribe_swap_status();

        let handle: tokio::task::JoinHandle<Vec<SwapStatusChanged>> = tokio::spawn(async move {
            BroadcastStream::new(rx)
                .filter_map(|r| async move {
                    let ev = r.ok()?;
                    if ev.seller == "GSELLER_A" { Some(ev) } else { None }
                })
                .take(1)
                .collect()
                .await
        });

        b.broadcast_swap_status_changed(SwapStatusChanged {
            swap_id: 1, seller: "GSELLER_B".into(), buyer: "B".into(),
            old_status: SwapStatus::Pending, new_status: SwapStatus::Accepted, timestamp: 1,
        });
        b.broadcast_swap_status_changed(SwapStatusChanged {
            swap_id: 2, seller: "GSELLER_A".into(), buyer: "B".into(),
            old_status: SwapStatus::Pending, new_status: SwapStatus::Accepted, timestamp: 2,
        });

        let results = handle.await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].swap_id, 2);
    }

    #[tokio::test]
    async fn test_category_assigned_filter_by_owner_and_hash() {
        let b = Arc::new(SubscriptionBroadcaster::new());
        let rx = b.subscribe_category_assigned();

        let handle: tokio::task::JoinHandle<Vec<CategoryAssigned>> = tokio::spawn(async move {
            BroadcastStream::new(rx)
                .filter_map(|r| async move {
                    let ev = r.ok()?;
                    if ev.owner == "GOWNER" && ev.category_hash == "cafebabe" {
                        Some(ev)
                    } else {
                        None
                    }
                })
                .take(1)
                .collect()
                .await
        });

        b.broadcast_category_assigned(CategoryAssigned {
            ip_id: 1, owner: "GOTHER".into(), category_hash: "cafebabe".into(), timestamp: 1,
        });
        b.broadcast_category_assigned(CategoryAssigned {
            ip_id: 2, owner: "GOWNER".into(), category_hash: "deadbeef".into(), timestamp: 2,
        });
        b.broadcast_category_assigned(CategoryAssigned {
            ip_id: 3, owner: "GOWNER".into(), category_hash: "cafebabe".into(), timestamp: 3,
        });

        let results = handle.await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ip_id, 3);
    }

    // ── Connection loss / disconnect cleanup ─────────────────────────────────

    #[tokio::test]
    async fn test_dropped_subscriber_does_not_affect_remaining_subscribers() {
        let b = Arc::new(SubscriptionBroadcaster::new());

        let mut rx_a = b.subscribe_ip_committed();
        let rx_b = b.subscribe_ip_committed();

        // Simulate connection loss for rx_b by dropping it.
        drop(rx_b);

        b.broadcast_ip_committed(IpCommitted { ip_id: 77, owner: "G".into(), timestamp: 77 });

        // rx_a should still receive the event.
        let ev = rx_a.recv().await.unwrap();
        assert_eq!(ev.ip_id, 77);
    }

    #[tokio::test]
    async fn test_broadcaster_survives_all_subscribers_disconnected() {
        let b = Arc::new(SubscriptionBroadcaster::new());

        {
            let _rx = b.subscribe_ip_committed();
            // rx dropped at end of scope — no subscribers remain.
        }

        // Sending with no active receivers must not panic.
        b.broadcast_ip_committed(IpCommitted { ip_id: 1, owner: "G".into(), timestamp: 1 });

        // New subscriber created after the gap picks up future events only.
        let mut rx_new = b.subscribe_ip_committed();
        b.broadcast_ip_committed(IpCommitted { ip_id: 2, owner: "G".into(), timestamp: 2 });

        let ev = rx_new.recv().await.unwrap();
        assert_eq!(ev.ip_id, 2);
    }

    #[tokio::test]
    async fn test_broadcast_stream_ends_cleanly_when_sender_dropped() {
        let (tx, rx) = broadcast::channel::<IpCommitted>(16);

        let handle: tokio::task::JoinHandle<Vec<IpCommitted>> = tokio::spawn(async move {
            BroadcastStream::new(rx)
                .filter_map(|r| async move { r.ok() })
                .collect()
                .await
        });

        tx.send(IpCommitted { ip_id: 1, owner: "G".into(), timestamp: 1 }).unwrap();
        drop(tx); // Sender dropped — stream should terminate.

        let results = handle.await.unwrap();
        assert_eq!(results.len(), 1);
    }

    // ── Multi-instance delivery via Redis (#783) ─────────────────────────────
    //
    // Gated on `REDIS_URL`: CI's `redis:7-alpine` service container sets it,
    // so these run for real there. Locally, run e.g.
    // `docker run --rm -d -p 6379:6379 redis:7-alpine` and export
    // `REDIS_URL=redis://localhost:6379` to exercise them; otherwise they
    // skip (never fail) so `cargo test --workspace` still passes without
    // Redis installed.

    fn redis_url_for_test() -> Option<String> {
        match std::env::var("REDIS_URL") {
            Ok(url) if !url.is_empty() => Some(url),
            _ => {
                eprintln!("skipping: REDIS_URL not set");
                None
            }
        }
    }

    #[tokio::test]
    async fn test_new_with_redis_errors_on_unreachable_url() {
        // Malformed/unroutable URL — must return Err, not hang or panic,
        // regardless of whether a real Redis is available in this environment.
        let result = SubscriptionBroadcaster::new_with_redis("redis://127.0.0.1:1/").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cross_instance_ip_committed_delivered_via_redis() {
        let Some(url) = redis_url_for_test() else { return };

        let instance_a = SubscriptionBroadcaster::new_with_redis(&url).await.unwrap();
        let instance_b = SubscriptionBroadcaster::new_with_redis(&url).await.unwrap();

        let mut rx_b = instance_b.subscribe_ip_committed();
        // Let both instances' background XREAD tails establish their
        // blocking read before we publish.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        instance_a.broadcast_ip_committed(IpCommitted {
            ip_id: 555,
            owner: "GCROSSINSTANCE".to_string(),
            timestamp: 1_000,
        });

        let ev = tokio::time::timeout(std::time::Duration::from_secs(10), rx_b.recv())
            .await
            .expect("timed out waiting for cross-instance delivery")
            .unwrap();
        assert_eq!(ev.ip_id, 555);
        assert_eq!(ev.owner, "GCROSSINSTANCE");
    }

    #[tokio::test]
    async fn test_cross_instance_publisher_also_receives_its_own_event() {
        // Regression guard for the self-echo/double-delivery hazard: in
        // Redis mode, publishing must still deliver back to subscribers on
        // the SAME instance that published — exactly once.
        let Some(url) = redis_url_for_test() else { return };

        let instance_a = SubscriptionBroadcaster::new_with_redis(&url).await.unwrap();
        let mut rx_a = instance_a.subscribe_swap_initiated();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        instance_a.broadcast_swap_initiated(SwapInitiated {
            swap_id: 777,
            ip_id: 1,
            seller: "GSELF".to_string(),
            buyer: "GBUYER".to_string(),
            price: "42".to_string(),
            timestamp: 2_000,
        });

        let ev = tokio::time::timeout(std::time::Duration::from_secs(10), rx_a.recv())
            .await
            .expect("timed out waiting for self-delivery")
            .unwrap();
        assert_eq!(ev.swap_id, 777);

        // No second copy should arrive.
        let second = tokio::time::timeout(std::time::Duration::from_millis(500), rx_a.recv()).await;
        assert!(second.is_err(), "event was delivered more than once");
    }

    #[tokio::test]
    async fn test_backfill_replays_events_after_since_timestamp() {
        let Some(url) = redis_url_for_test() else { return };

        let instance_a = SubscriptionBroadcaster::new_with_redis(&url).await.unwrap();

        instance_a.broadcast_category_assigned(CategoryAssigned {
            ip_id: 1, owner: "GBEFORE".into(), category_hash: "aaaa".into(), timestamp: 10_000,
        });
        instance_a.broadcast_category_assigned(CategoryAssigned {
            ip_id: 2, owner: "GAFTER".into(), category_hash: "bbbb".into(), timestamp: 10_050,
        });
        // Give the fire-and-forget XADD tasks time to land before XRANGE reads them back.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let replayed = instance_a.backfill_category_assigned(Some(10_000)).await;
        assert!(replayed.iter().any(|ev| ev.ip_id == 2), "event after `since` must be replayed");
        assert!(!replayed.iter().any(|ev| ev.ip_id == 1), "event at/before `since` must not be replayed");
    }

    #[tokio::test]
    async fn test_backfill_is_noop_in_single_instance_mode() {
        // No Redis configured — `since` must be silently ignored, not error.
        let b = SubscriptionBroadcaster::new();
        b.broadcast_ip_committed(IpCommitted { ip_id: 9, owner: "G".into(), timestamp: 1 });
        let replayed = b.backfill_ip_committed(Some(0)).await;
        assert!(replayed.is_empty());
    }
}
