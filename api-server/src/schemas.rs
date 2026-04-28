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

// ── #360: Cursor-based Pagination ─────────────────────────────────────────────

/// Cursor-based pagination parameters.
/// Use this instead of offset-based pagination for better performance on large datasets.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CursorPaginationParams {
    /// Maximum number of items to return (default: 50, max: 200).
    #[serde(default = "default_limit")]
    pub limit: u64,
    /// Cursor from the previous page response. Omit for the first page.
    #[serde(default)]
    pub cursor: Option<String>,
}

fn default_limit() -> u64 {
    50
}

/// Response type for cursor-paginated list endpoints.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponse<T> {
    /// Items on this page.
    pub items: Vec<T>,
    /// Cursor for the next page. Null if this is the last page.
    pub next_cursor: Option<String>,
    /// Whether there are more items available.
    pub has_more: bool,
    /// Total count of items (if available).
    pub total_count: Option<u64>,
}

/// Cursor encoding/decoding utilities.
pub mod cursor {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::{Deserialize, Serialize};

    /// Cursor data structure.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CursorData {
        pub last_id: u64,
        pub offset: u64,
    }

    /// Encode a cursor from cursor data.
    pub fn encode(data: &CursorData) -> String {
        let json = serde_json::to_string(data).unwrap_or_default();
        STANDARD.encode(json)
    }

    /// Decode a cursor to cursor data. Returns None if invalid.
    pub fn decode(cursor: &str) -> Option<CursorData> {
        let decoded = STANDARD.decode(cursor).ok()?;
        let json = String::from_utf8(decoded).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Create a cursor from the last item ID and offset.
    pub fn new(last_id: u64, offset: u64) -> String {
        encode(&CursorData { last_id, offset })
    }

    /// Get the next cursor for the following page.
    pub fn next_cursor(last_id: u64, current_offset: u64, items_per_page: u64) -> String {
        new(last_id, current_offset + items_per_page)
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn test_cursor_encode_decode_roundtrip() {
        let data = cursor::CursorData { last_id: 100, offset: 50 };
        let encoded = cursor::encode(&data);
        let decoded = cursor::decode(&encoded).unwrap();
        assert_eq!(decoded.last_id, 100);
        assert_eq!(decoded.offset, 50);
    }

    #[test]
    fn test_cursor_new_and_next() {
        let cursor = cursor::new(50, 0);
        let next = cursor::next_cursor(50, 0, 20);
        assert_ne!(cursor, next);
    }

    #[test]
    fn test_cursor_decode_invalid_returns_none() {
        assert!(cursor::decode("invalid_base64").is_none());
        assert!(cursor::decode("").is_none());
    }

    #[test]
    fn test_cursor_decode_malformed_json_returns_none() {
        let encoded = STANDARD.encode("not_json");
        assert!(cursor::decode(&encoded).is_none());
    }
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

/// #321: Bulk commit IP request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkCommitIpRequest {
    pub owner: String,
    pub commitment_hashes: Vec<String>,
}

/// #321: Bulk commit IP response with individual results
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkCommitIpResponse {
    pub results: Vec<BulkOperationResult<u64>>,
}

/// #321: Bulk initiate swap request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkInitiateSwapRequest {
    pub ip_registry_id: String,
    pub ip_ids: Vec<u64>,
    pub seller: String,
    pub prices: Vec<i128>,
    pub buyer: String,
    pub token: String,
    pub referrer: Option<String>,
}

/// #321: Bulk initiate swap response with individual results
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkInitiateSwapResponse {
    pub results: Vec<BulkOperationResult<u64>>,
}

/// #321: Individual operation result in bulk response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkOperationResult<T> {
    pub index: usize,
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
