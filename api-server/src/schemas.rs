use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitIpRequest {
    /// Stellar address of the IP owner (must sign the transaction)
    pub owner: String,
    /// 32-byte Pedersen commitment hash, hex-encoded
    pub commitment_hash: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IpRecord {
    pub ip_id: u64,
    pub owner: String,
    pub commitment_hash: String,
    pub timestamp: u64,
    /// Whether the IP record has been revoked
    pub revoked: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransferIpRequest {
    pub ip_id: u64,
    pub new_owner: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyCommitmentRequest {
    pub ip_id: u64,
    /// 32-byte secret, hex-encoded
    pub secret: String,
    /// 32-byte blinding factor, hex-encoded
    pub blinding_factor: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyCommitmentResponse {
    /// true if sha256(secret || blinding_factor) matches the stored commitment hash
    pub valid: bool,
}

/// #317: Pagination query parameters shared across list endpoints.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PaginationParams {
    /// Maximum number of items to return (default: 50, max: 200).
    #[serde(default = "default_limit")]
    pub limit: u64,
    /// Number of items to skip (default: 0).
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    50
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListIpByOwnerResponse {
    pub ip_ids: Vec<u64>,
    /// #317: Total number of IPs owned (before pagination).
    pub total_count: u64,
    /// #317: Whether more items exist beyond this page.
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum SwapStatus {
    Pending,
    Accepted,
    Completed,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SwapRecord {
    pub ip_id: u64,
    pub ip_registry_id: String,
    pub seller: String,
    pub buyer: String,
    /// Price in stroops (1 XLM = 10_000_000 stroops)
    pub price: i128,
    pub token: String,
    pub status: SwapStatus,
    /// Ledger timestamp after which buyer may cancel an Accepted swap
    pub expiry: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InitiateSwapRequest {
    pub ip_registry_id: String,
    pub ip_id: u64,
    pub seller: String,
    pub price: i128,
    pub buyer: String,
    /// Stellar asset contract address for the payment token
    pub token: String,
    /// #311: Optional referrer address for referral reward
    pub referrer: Option<String>,
}

/// #309: Batch swap initiation request.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInitiateSwapRequest {
    pub ip_registry_id: String,
    pub ip_ids: Vec<u64>,
    pub seller: String,
    pub prices: Vec<i128>,
    pub buyer: String,
    pub token: String,
    /// #311: Optional referrer address for referral reward
    pub referrer: Option<String>,
}

/// #309: Batch swap initiation response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInitiateSwapResponse {
    pub swap_ids: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AcceptSwapRequest {
    pub buyer: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RevealKeyRequest {
    pub caller: String,
    /// 32-byte secret, hex-encoded
    pub secret: String,
    /// 32-byte blinding factor, hex-encoded
    pub blinding_factor: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelSwapRequest {
    pub canceller: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CancelExpiredSwapRequest {
    pub caller: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterWebhookRequest {
    pub url: String,
    pub events: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WebhookResponse {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub created_at: u64,
}
