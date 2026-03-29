use axum::{extract::Path, http::StatusCode, Json};
use crate::schemas::*;

/// Timestamp a new IP commitment. Returns the assigned IP ID.
#[utoipa::path(
    post,
    path = "/ip/commit",
    tag = "IP Registry",
    request_body = CommitIpRequest,
    responses(
        (status = 200, description = "IP committed successfully, returns assigned ip_id", body = u64),
        (status = 400, description = "Invalid request (zero hash, duplicate hash)", body = ErrorResponse),
    )
)]
pub async fn commit_ip(Json(body): Json<CommitIpRequest>) -> Result<Json<u64>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke ip_registry.commit_ip
    // For now, return a stub response
    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "commit_ip not yet implemented".to_string(),
        }),
    ))
}

/// Retrieve an IP record by ID.
#[utoipa::path(
    get,
    path = "/ip/{ip_id}",
    tag = "IP Registry",
    params(("ip_id" = u64, Path, description = "IP record identifier")),
    responses(
        (status = 200, description = "IP record found", body = IpRecord),
        (status = 404, description = "IP record not found", body = ErrorResponse),
    )
)]
pub async fn get_ip(Path(ip_id): Path<u64>) -> Result<Json<IpRecord>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke ip_registry.get_ip
    // For now, return a stub response
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("IP record {} not found", ip_id),
        }),
    ))
}

/// Transfer IP ownership to a new address.
#[utoipa::path(
    post,
    path = "/ip/transfer",
    tag = "IP Registry",
    request_body = TransferIpRequest,
    responses(
        (status = 200, description = "Ownership transferred successfully"),
        (status = 404, description = "IP record not found", body = ErrorResponse),
    )
)]
pub async fn transfer_ip(Json(body): Json<TransferIpRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke ip_registry.transfer_ip
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("IP record {} not found", body.ip_id),
        }),
    ))
}

/// Verify a Pedersen commitment: sha256(secret || blinding_factor) == commitment_hash.
#[utoipa::path(
    post,
    path = "/ip/verify",
    tag = "IP Registry",
    request_body = VerifyCommitmentRequest,
    responses(
        (status = 200, description = "Verification result", body = VerifyCommitmentResponse),
        (status = 404, description = "IP record not found", body = ErrorResponse),
    )
)]
pub async fn verify_commitment(Json(body): Json<VerifyCommitmentRequest>) -> Result<Json<VerifyCommitmentResponse>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke ip_registry.verify_commitment
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("IP record {} not found", body.ip_id),
        }),
    ))
}

/// List all IP IDs owned by a Stellar address.
#[utoipa::path(
    get,
    path = "/ip/owner/{owner}",
    tag = "IP Registry",
    params(("owner" = String, Path, description = "Stellar address of the owner")),
    responses(
        (status = 200, description = "List of IP IDs (null if none)", body = ListIpByOwnerResponse),
    )
)]
pub async fn list_ip_by_owner(Path(owner): Path<String>) -> Json<ListIpByOwnerResponse> {
    // TODO: Call Soroban RPC to invoke ip_registry.list_ip_by_owner
    Json(ListIpByOwnerResponse { ip_ids: None })
}

/// Seller initiates a patent sale. Returns the swap ID.
#[utoipa::path(
    post,
    path = "/swap/initiate",
    tag = "Atomic Swap",
    request_body = InitiateSwapRequest,
    responses(
        (status = 200, description = "Swap initiated, returns swap_id", body = u64),
        (status = 400, description = "Seller is not IP owner or active swap exists", body = ErrorResponse),
    )
)]
pub async fn initiate_swap(Json(body): Json<InitiateSwapRequest>) -> Result<Json<u64>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.initiate_swap
    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "initiate_swap not yet implemented".to_string(),
        }),
    ))
}

/// Buyer accepts a pending swap.
#[utoipa::path(
    post,
    path = "/swap/{swap_id}/accept",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = AcceptSwapRequest,
    responses(
        (status = 200, description = "Swap accepted"),
        (status = 400, description = "Swap not in Pending state", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
pub async fn accept_swap(Path(swap_id): Path<u64>, Json(body): Json<AcceptSwapRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.accept_swap
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Swap {} not found", swap_id),
        }),
    ))
}

/// Seller reveals the decryption key; payment releases and swap completes.
#[utoipa::path(
    post,
    path = "/swap/{swap_id}/reveal",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = RevealKeyRequest,
    responses(
        (status = 200, description = "Key revealed, swap completed"),
        (status = 400, description = "Swap not in Accepted state or caller is not seller", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
pub async fn reveal_key(Path(swap_id): Path<u64>, Json(body): Json<RevealKeyRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.reveal_key
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Swap {} not found", swap_id),
        }),
    ))
}

/// Cancel a pending swap. Only the seller or buyer may cancel.
#[utoipa::path(
    post,
    path = "/swap/{swap_id}/cancel",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = CancelSwapRequest,
    responses(
        (status = 200, description = "Swap cancelled"),
        (status = 400, description = "Swap not in Pending state or canceller is not seller/buyer", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
pub async fn cancel_swap(Path(swap_id): Path<u64>, Json(body): Json<CancelSwapRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.cancel_swap
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Swap {} not found", swap_id),
        }),
    ))
}

/// Buyer cancels an Accepted swap after the expiry timestamp.
#[utoipa::path(
    post,
    path = "/swap/{swap_id}/cancel-expired",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = CancelExpiredSwapRequest,
    responses(
        (status = 200, description = "Expired swap cancelled"),
        (status = 400, description = "Swap not expired, not Accepted, or caller is not buyer", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
pub async fn cancel_expired_swap(Path(swap_id): Path<u64>, Json(body): Json<CancelExpiredSwapRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.cancel_expired_swap
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Swap {} not found", swap_id),
        }),
    ))
}

/// Read a swap record by ID.
#[utoipa::path(
    get,
    path = "/swap/{swap_id}",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    responses(
        (status = 200, description = "Swap record found", body = SwapRecord),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
pub async fn get_swap(Path(swap_id): Path<u64>) -> Result<Json<SwapRecord>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.get_swap
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Swap {} not found", swap_id),
        }),
    ))
}
