use axum::{
    extract::{Path, Query},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use tracing::instrument;
use crate::cache;
use crate::schemas::*;
use crate::webhook;

// ── IP Registry ───────────────────────────────────────────────────────────────

/// Timestamp a new IP commitment. Returns the assigned IP ID.
#[utoipa::path(
    post,
    path = "/v1/ip/commit",
    tag = "IP Registry",
    request_body = CommitIpRequest,
    responses(
        (status = 200, description = "IP committed successfully, returns assigned ip_id", body = u64),
        (status = 400, description = "Invalid request (zero hash, duplicate hash)", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn commit_ip(Json(body): Json<CommitIpRequest>) -> Result<Json<u64>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke ip_registry.commit_ip
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
    path = "/v1/ip/{ip_id}",
    tag = "IP Registry",
    params(("ip_id" = u64, Path, description = "IP record identifier")),
    responses(
        (status = 200, description = "IP record found", body = IpRecord),
        (status = 404, description = "IP record not found", body = ErrorResponse),
    )
)]
#[instrument]
pub async fn get_ip(Path(ip_id): Path<u64>) -> impl IntoResponse {
    // #316: Check cache first
    let cache_key = cache::ip_key(ip_id);
    if let Some(cached) = cache::get::<IpRecord>(&cache_key) {
        return (
            StatusCode::OK,
            [(header::CACHE_CONTROL, cache::cache_control_header())],
            Json(serde_json::to_value(cached).unwrap()),
        ).into_response();
    }

    // TODO: Call Soroban RPC to invoke ip_registry.get_ip
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, cache::no_cache_header())],
        Json(serde_json::json!({ "error": format!("IP record {} not found", ip_id) })),
    ).into_response()
}

/// Transfer IP ownership to a new address.
#[utoipa::path(
    post,
    path = "/v1/ip/transfer",
    tag = "IP Registry",
    request_body = TransferIpRequest,
    responses(
        (status = 200, description = "Ownership transferred successfully"),
        (status = 404, description = "IP record not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn transfer_ip(Json(body): Json<TransferIpRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // #316: Invalidate cache on mutation
    cache::invalidate(&cache::ip_key(body.ip_id));
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
    path = "/v1/ip/verify",
    tag = "IP Registry",
    request_body = VerifyCommitmentRequest,
    responses(
        (status = 200, description = "Verification result", body = VerifyCommitmentResponse),
        (status = 404, description = "IP record not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
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
/// Supports `limit` and `offset` query parameters for pagination (#317).
#[utoipa::path(
    get,
    path = "/v1/ip/owner/{owner}",
    tag = "IP Registry",
    params(
        ("owner" = String, Path, description = "Stellar address of the owner"),
        PaginationParams,
    ),
    responses(
        (status = 200, description = "Paginated list of IP IDs", body = ListIpByOwnerResponse),
    )
)]
#[instrument]
pub async fn list_ip_by_owner(
    Path(owner): Path<String>,
    Query(pagination): Query<PaginationParams>,
) -> impl IntoResponse {
    let limit = pagination.limit.min(200);
    let offset = pagination.offset;

    // #316: Check cache
    let cache_key = cache::ip_list_key(&owner, limit, offset);
    if let Some(cached) = cache::get::<ListIpByOwnerResponse>(&cache_key) {
        return (
            StatusCode::OK,
            [(header::CACHE_CONTROL, cache::cache_control_header())],
            Json(serde_json::to_value(cached).unwrap()),
        ).into_response();
    }

    // TODO: Call Soroban RPC to invoke ip_registry.list_ip_by_owner
    // Stub: empty paginated response
    let all_ids: Vec<u64> = vec![];
    let total_count = all_ids.len() as u64;
    let page: Vec<u64> = all_ids
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    let has_more = offset + limit < total_count;

    let resp = ListIpByOwnerResponse { ip_ids: page, total_count, has_more };
    cache::set(&cache_key, &resp);

    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, cache::cache_control_header())],
        Json(serde_json::to_value(resp).unwrap()),
    ).into_response()
}

/// List all IP IDs owned by a Stellar address with cursor-based pagination (#360).
/// This endpoint is more efficient for large datasets than offset-based pagination.
#[utoipa::path(
    get,
    path = "/v1/ip/owner/{owner}/cursor",
    tag = "IP Registry",
    params(
        ("owner" = String, Path, description = "Stellar address of the owner"),
        CursorPaginationParams,
    ),
    responses(
        (status = 200, description = "Cursor-paginated list of IP IDs", body = PaginatedResponse<u64>),
    )
)]
#[instrument]
pub async fn list_ip_by_owner_cursor(
    Path(owner): Path<String>,
    Query(pagination): Query<CursorPaginationParams>,
) -> impl IntoResponse {
    let limit = pagination.limit.min(200);

    // Decode cursor if provided
    let (last_id, offset) = match pagination.cursor {
        Some(cursor) => {
            match crate::schemas::cursor::decode(&cursor) {
                Some(data) => (data.last_id, data.offset),
                None => (0, 0), // Invalid cursor, start from beginning
            }
        }
        None => (0, 0),
    };

    // #316: Check cache with cursor-based key
    let cache_key = cache::ip_list_key(&owner, limit, &format!("{}", offset));
    if let Some(cached) = cache::get::<PaginatedResponse<u64>>(&cache_key) {
        return (
            StatusCode::OK,
            [(header::CACHE_CONTROL, cache::cache_control_header())],
            Json(serde_json::to_value(cached).unwrap()),
        ).into_response();
    }

    // TODO: Call Soroban RPC to invoke ip_registry.list_ip_by_owner with cursor
    // Stub: empty paginated response
    let all_ids: Vec<u64> = vec![];
    let total_count = all_ids.len() as u64;

    // Apply cursor-based pagination
    let page: Vec<u64> = all_ids
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    // Calculate next cursor
    let next_cursor = if page.len() == limit as usize && (offset + limit as usize) < total_count as usize {
        let last_item_id = page.last().copied().unwrap_or(0);
        Some(crate::schemas::cursor::new(last_item_id, offset + limit))
    } else {
        None
    };

    let has_more = offset + limit < total_count;

    let resp = PaginatedResponse {
        items: page,
        next_cursor,
        has_more,
        total_count: Some(total_count),
    };
    cache::set(&cache_key, &resp);

    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, cache::cache_control_header())],
        Json(serde_json::to_value(resp).unwrap()),
    ).into_response()
}

// ── Atomic Swap ───────────────────────────────────────────────────────────────

/// Seller initiates a patent sale. Returns the swap ID.
#[utoipa::path(
    post,
    path = "/v1/swap/initiate",
    tag = "Atomic Swap",
    request_body = InitiateSwapRequest,
    responses(
        (status = 200, description = "Swap initiated, returns swap_id", body = u64),
        (status = 400, description = "Seller is not IP owner or active swap exists", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn initiate_swap(Json(body): Json<InitiateSwapRequest>) -> Result<Json<u64>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Call Soroban RPC to invoke atomic_swap.initiate_swap
    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "initiate_swap not yet implemented".to_string(),
        }),
    ))
}

/// Seller initiates multiple patent sales in one call. Returns a list of swap IDs (#309).
#[utoipa::path(
    post,
    path = "/v1/swap/batch-initiate",
    tag = "Atomic Swap",
    request_body = BatchInitiateSwapRequest,
    responses(
        (status = 200, description = "Swaps initiated, returns swap_ids", body = BatchInitiateSwapResponse),
        (status = 400, description = "Validation error (mismatched lengths, invalid IP, etc.)", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn batch_initiate_swap(Json(body): Json<BatchInitiateSwapRequest>) -> Result<Json<BatchInitiateSwapResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.ip_ids.len() != body.prices.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ip_ids and prices must have the same length".to_string(),
            }),
        ));
    }
    if body.ip_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ip_ids must not be empty".to_string(),
            }),
        ));
    }
    // TODO: Call Soroban RPC to invoke atomic_swap.batch_initiate_swap
    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "batch_initiate_swap not yet implemented".to_string(),
        }),
    ))
}

/// Buyer accepts a pending swap.
#[utoipa::path(
    post,
    path = "/v1/swap/{swap_id}/accept",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = AcceptSwapRequest,
    responses(
        (status = 200, description = "Swap accepted"),
        (status = 400, description = "Swap not in Pending state", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn accept_swap(Path(swap_id): Path<u64>, Json(body): Json<AcceptSwapRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // #316: Invalidate swap cache on state change
    cache::invalidate(&cache::swap_key(swap_id));
    // TODO: Call Soroban RPC to invoke atomic_swap.accept_swap
    webhook::trigger_swap_status_changed(swap_id, Some("Pending".to_string()), "Accepted".to_string());
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
    path = "/v1/swap/{swap_id}/reveal",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = RevealKeyRequest,
    responses(
        (status = 200, description = "Key revealed, swap completed"),
        (status = 400, description = "Swap not in Accepted state or caller is not seller", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn reveal_key(Path(swap_id): Path<u64>, Json(body): Json<RevealKeyRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // #316: Invalidate swap cache on state change
    cache::invalidate(&cache::swap_key(swap_id));
    // TODO: Call Soroban RPC to invoke atomic_swap.reveal_key
    webhook::trigger_swap_status_changed(swap_id, Some("Accepted".to_string()), "Completed".to_string());
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
    path = "/v1/swap/{swap_id}/cancel",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = CancelSwapRequest,
    responses(
        (status = 200, description = "Swap cancelled"),
        (status = 400, description = "Swap not in Pending state or canceller is not seller/buyer", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn cancel_swap(Path(swap_id): Path<u64>, Json(body): Json<CancelSwapRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // #316: Invalidate swap cache on state change
    cache::invalidate(&cache::swap_key(swap_id));
    // TODO: Call Soroban RPC to invoke atomic_swap.cancel_swap
    webhook::trigger_swap_status_changed(swap_id, Some("Pending".to_string()), "Cancelled".to_string());
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
    path = "/v1/swap/{swap_id}/cancel-expired",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    request_body = CancelExpiredSwapRequest,
    responses(
        (status = 200, description = "Expired swap cancelled"),
        (status = 400, description = "Swap not expired, not Accepted, or caller is not buyer", body = ErrorResponse),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn cancel_expired_swap(Path(swap_id): Path<u64>, Json(body): Json<CancelExpiredSwapRequest>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // #316: Invalidate swap cache on state change
    cache::invalidate(&cache::swap_key(swap_id));
    // TODO: Call Soroban RPC to invoke atomic_swap.cancel_expired_swap
    webhook::trigger_swap_status_changed(swap_id, Some("Accepted".to_string()), "Cancelled".to_string());
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
    path = "/v1/swap/{swap_id}",
    tag = "Atomic Swap",
    params(("swap_id" = u64, Path, description = "Swap identifier")),
    responses(
        (status = 200, description = "Swap record found", body = SwapRecord),
        (status = 404, description = "Swap not found", body = ErrorResponse),
    )
)]
#[instrument]
pub async fn get_swap(Path(swap_id): Path<u64>) -> impl IntoResponse {
    // #316: Check cache first
    let cache_key = cache::swap_key(swap_id);
    if let Some(cached) = cache::get::<SwapRecord>(&cache_key) {
        return (
            StatusCode::OK,
            [(header::CACHE_CONTROL, cache::cache_control_header())],
            Json(serde_json::to_value(cached).unwrap()),
        ).into_response();
    }

    // TODO: Call Soroban RPC to invoke atomic_swap.get_swap
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, cache::no_cache_header())],
        Json(serde_json::json!({ "error": format!("Swap {} not found", swap_id) })),
    ).into_response()
}

// ── Webhooks ──────────────────────────────────────────────────────────────────

/// Register a webhook URL to receive swap event notifications.
#[utoipa::path(
    post,
    path = "/v1/webhooks",
    tag = "Webhooks",
    request_body = RegisterWebhookRequest,
    responses(
        (status = 200, description = "Webhook registered", body = WebhookResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
    )
)]
pub async fn register_webhook(Json(body): Json<RegisterWebhookRequest>) -> Result<Json<WebhookResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.url.is_empty() || body.events.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "URL and events are required".to_string(),
            }),
        ));
    }

    let config = webhook::register(body.url, body.events);

    Ok(Json(WebhookResponse {
        id: config.id.to_string(),
        url: config.url,
        events: config.events,
        created_at: config.created_at,
    }))
}

/// Unregister a webhook by ID.
#[utoipa::path(
    delete,
    path = "/v1/webhooks/{id}",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Webhook UUID")),
    responses(
        (status = 200, description = "Webhook unregistered"),
        (status = 404, description = "Webhook not found", body = ErrorResponse),
    )
)]
pub async fn unregister_webhook(Path(id): Path<String>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "Invalid webhook ID format".to_string(),
        }),
    ))?;

    if webhook::unregister(uuid) {
        Ok(StatusCode::OK)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Webhook {} not found", id),
            }),
        ))
    }
}

// ── Bulk Operations ───────────────────────────────────────────────────────────

/// Commit multiple IP records in a single request (#321).
#[utoipa::path(
    post,
    path = "/v1/bulk/commit-ip",
    tag = "IP Registry",
    request_body = BulkCommitIpRequest,
    responses(
        (status = 200, description = "Bulk commit completed with individual results", body = BulkCommitIpResponse),
        (status = 400, description = "Invalid request (empty hashes, etc.)", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn bulk_commit_ip(Json(body): Json<BulkCommitIpRequest>) -> Result<Json<BulkCommitIpResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.commitment_hashes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "commitment_hashes must not be empty".to_string(),
            }),
        ));
    }

    let mut results = Vec::new();
    for (index, hash) in body.commitment_hashes.iter().enumerate() {
        // TODO: Call Soroban RPC to invoke ip_registry.commit_ip
        results.push(BulkOperationResult {
            index,
            success: false,
            data: None,
            error: Some("bulk_commit_ip not yet implemented".to_string()),
        });
    }

    Ok(Json(BulkCommitIpResponse { results }))
}

/// Initiate multiple swaps in a single request (#321).
#[utoipa::path(
    post,
    path = "/v1/bulk/initiate-swap",
    tag = "Atomic Swap",
    request_body = BulkInitiateSwapRequest,
    responses(
        (status = 200, description = "Bulk swap initiation completed with individual results", body = BulkInitiateSwapResponse),
        (status = 400, description = "Validation error (mismatched lengths, empty arrays, etc.)", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn bulk_initiate_swap(Json(body): Json<BulkInitiateSwapRequest>) -> Result<Json<BulkInitiateSwapResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.ip_ids.len() != body.prices.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ip_ids and prices must have the same length".to_string(),
            }),
        ));
    }
    if body.ip_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "ip_ids must not be empty".to_string(),
            }),
        ));
    }

    let mut results = Vec::new();
    for (index, ip_id) in body.ip_ids.iter().enumerate() {
        // TODO: Call Soroban RPC to invoke atomic_swap.initiate_swap
        results.push(BulkOperationResult {
            index,
            success: false,
            data: None,
            error: Some("bulk_initiate_swap not yet implemented".to_string()),
        });
    }

    Ok(Json(BulkInitiateSwapResponse { results }))
}
