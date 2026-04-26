use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};

// ── GraphQL Types ─────────────────────────────────────────────────────────────

/// An intellectual property record.
#[derive(SimpleObject, Clone)]
pub struct IpRecord {
    pub ip_id: u64,
    pub owner: String,
    pub commitment_hash: String,
    pub timestamp: u64,
    pub revoked: bool,
}

/// Status of an atomic swap.
#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq)]
pub enum SwapStatus {
    Pending,
    Accepted,
    Completed,
    Disputed,
    Cancelled,
}

/// An atomic swap record.
#[derive(SimpleObject, Clone)]
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

// ── Query Root ────────────────────────────────────────────────────────────────

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Fetch an IP record by its ID.
    async fn ip(&self, _ctx: &Context<'_>, ip_id: u64) -> Option<IpRecord> {
        // TODO: wire up to Soroban RPC
        let _ = ip_id;
        None
    }

    /// Fetch a swap record by its ID.
    async fn swap(&self, _ctx: &Context<'_>, swap_id: u64) -> Option<SwapRecord> {
        // TODO: wire up to Soroban RPC
        let _ = swap_id;
        None
    }

    /// List all swap IDs for a given seller address.
    async fn swaps_by_seller(&self, _ctx: &Context<'_>, seller: String) -> Vec<u64> {
        // TODO: wire up to Soroban RPC
        let _ = seller;
        vec![]
    }

    /// List all swap IDs for a given buyer address.
    async fn swaps_by_buyer(&self, _ctx: &Context<'_>, buyer: String) -> Vec<u64> {
        // TODO: wire up to Soroban RPC
        let _ = buyer;
        vec![]
    }

    /// List all swap IDs ever created for a given IP.
    async fn swaps_by_ip(&self, _ctx: &Context<'_>, ip_id: u64) -> Vec<u64> {
        // TODO: wire up to Soroban RPC
        let _ = ip_id;
        vec![]
    }

    /// Retrieve all dispute evidence hashes for a swap.
    async fn dispute_evidence(&self, _ctx: &Context<'_>, swap_id: u64) -> Vec<String> {
        // TODO: wire up to Soroban RPC
        let _ = swap_id;
        vec![]
    }
}

// ── Schema ────────────────────────────────────────────────────────────────────

pub type AtomicIpSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn build_schema() -> AtomicIpSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish()
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
        assert_eq!(res.data.to_string(), r#"{"swapsBySeller":[]}"#);
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
}
