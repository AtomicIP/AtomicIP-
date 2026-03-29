use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListIpByOwnerResponse {
    pub ip_ids: Vec<u64>,
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
