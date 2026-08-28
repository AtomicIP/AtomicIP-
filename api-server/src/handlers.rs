use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{Duration, Instant};
use tracing::instrument;
use crate::cache;
use crate::deduplication::{create_store, DeduplicationStore};
use crate::graphql::SorobanQueryClient;
use crate::schemas::*;
use std::sync::Arc;
use crate::webhook;

// #523: Per-handler idempotency store for batch swap operations.
static BATCH_SWAP_IDEMPOTENCY: Lazy<DeduplicationStore> = Lazy::new(create_store);

// #520: Mirrors the contract's MAX_BATCH_SIZE cap on a single batch.
const MAX_BATCH_SIZE: usize = 50;

// #469: Mirrors the contract's ~7-day swap expiry (ledger timestamp + 604800).
const SWAP_EXPIRY_SECONDS: u64 = 604800;

/// Process-local swap ID counter standing in for the contract's `NextId`
/// until the handlers are wired to a live Soroban RPC client.
static NEXT_SWAP_ID: AtomicU64 = AtomicU64::new(0);

/// Current Unix timestamp in seconds (substitute for the ledger timestamp).
fn now_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

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
    // TODO: Call Soroban RPC to invoke ip_registry.transfer_ip
    // #316: Invalidate AFTER the write commits. Invalidating before the RPC
    // call leaves a window where a concurrent read can repopulate the cache
    // with pre-write state and serve it stale for the full TTL; a post-write
    // invalidation clears any such entry. The full key set must go — a
    // transfer changes owner list membership too, so `ip:list:*` (not just
    // the record) is invalidated.
    cache::invalidate_ip(body.ip_id);
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
    axum::extract::State(client): axum::extract::State<Arc<SorobanQueryClient>>,
) -> impl IntoResponse {
    let limit = pagination.limit.min(200);
    let offset = pagination.offset;

    // #316: Check cache
    let cache_key = cache::ip_list_key(&owner, limit, &offset.to_string());
    if let Some(cached) = cache::get::<ListIpByOwnerResponse>(&cache_key) {
        return (
            StatusCode::OK,
            [(header::CACHE_CONTROL, cache::cache_control_header())],
            Json(serde_json::to_value(cached).unwrap()),
        ).into_response();
    }

    let all_ids = match client.list_ip_by_owner(&owner).await {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, owner = %owner, "failed to list IPs by owner");
            return (
                StatusCode::BAD_GATEWAY,
                [(header::CACHE_CONTROL, cache::no_cache_header())],
                Json(serde_json::json!({ "error": "failed to query IP registry" })),
            ).into_response();
        }
    };
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
    axum::extract::State(client): axum::extract::State<Arc<SorobanQueryClient>>,
) -> impl IntoResponse {
    let limit = pagination.limit.min(200);

    // Decode cursor if provided
    let offset = match pagination.cursor {
        Some(cursor) => {
            match crate::schemas::cursor::decode(&cursor) {
                Some(data) => data.offset,
                None => 0, // Invalid cursor, start from beginning
            }
        }
        None => 0,
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

    let all_ids = match client.list_ip_by_owner(&owner).await {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, owner = %owner, "failed to list IPs by owner");
            return (
                StatusCode::BAD_GATEWAY,
                [(header::CACHE_CONTROL, cache::no_cache_header())],
                Json(serde_json::json!({ "error": "failed to query IP registry" })),
            ).into_response();
        }
    };
    let total_count = all_ids.len() as u64;

    // Apply cursor-based pagination
    let page: Vec<u64> = all_ids
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    // Calculate next cursor
    let next_cursor = if page.len() == limit as usize && (offset as u64 + limit) < total_count {
        let last_item_id = page.last().copied().unwrap_or(0);
        Some(crate::schemas::cursor::new(last_item_id, offset as u64 + limit))
    } else {
        None
    };

    let has_more = offset as u64 + limit < total_count;

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
        (status = 400, description = "Validation error (mismatched lengths, empty/oversized batch, non-positive price, duplicate ip_ids, etc.)", body = ErrorResponse),
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

    // #524: Reject requests that contain duplicate ip_ids.
    let mut seen: HashSet<u64> = HashSet::new();
    for &id in &body.ip_ids {
        if !seen.insert(id) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("duplicate ip_id {} in batch request", id),
                }),
            ));
        }
    }

    // #523: Return cached result if the caller supplied a matching idempotency key.
    if let Some(ref key) = body.idempotency_key {
        if let Some(entry) = BATCH_SWAP_IDEMPOTENCY.get(key.as_str()) {
            let cached: &serde_json::Value = &entry.0;
            let ts: &tokio::time::Instant = &entry.1;
            if ts.elapsed() < Duration::from_secs(3600) {
                if let Ok(response) = serde_json::from_value::<BatchInitiateSwapResponse>(cached.clone()) {
                    return Ok(Json(response));
                }
            } else {
                drop(entry);
                BATCH_SWAP_IDEMPOTENCY.remove(key.as_str());
            }
        }
    }

    // #520: Cap the batch size at the contract's MAX_BATCH_SIZE (50).
    if body.ip_ids.len() > MAX_BATCH_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "batch size {} exceeds maximum of {}",
                    body.ip_ids.len(),
                    MAX_BATCH_SIZE
                ),
            }),
        ));
    }

    // #520: Every price must be positive (contract: require_positive_price).
    if let Some((&ip_id, _)) = body
        .ip_ids
        .iter()
        .zip(body.prices.iter())
        .find(|(_, &price)| price <= 0)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("price must be positive for ip_id {}", ip_id),
            }),
        ));
    }

    // Reuse the batch semantics of the contract's `batch_initiate_swap`
    // (validated in the contract's batch tests): every IP in the batch gets a
    // fresh Pending swap with a ~7-day expiry, IDs allocated sequentially
    // (contract NextId). Until the handlers are wired to a live Soroban RPC
    // client, the records are served back through the #316 cache so
    // GET /swap/{swap_id} can read them.
    let expiry = now_timestamp() + SWAP_EXPIRY_SECONDS;
    let mut swap_ids = Vec::with_capacity(body.ip_ids.len());
    for (&ip_id, &price) in body.ip_ids.iter().zip(body.prices.iter()) {
        let swap_id = NEXT_SWAP_ID.fetch_add(1, Ordering::Relaxed);
        let record = SwapRecord {
            ip_id,
            ip_registry_id: body.ip_registry_id.clone(),
            seller: body.seller.clone(),
            buyer: body.buyer.clone(),
            price,
            token: body.token.clone(),
            status: SwapStatus::Pending,
            expiry,
        };
        cache::set_with_ttl(&cache::swap_key(swap_id), &record, SWAP_EXPIRY_SECONDS);
        swap_ids.push(swap_id);
    }

    let response = BatchInitiateSwapResponse { swap_ids };

    // #523: Cache the result under the idempotency key so replays return the
    // same swap IDs instead of allocating new ones.
    if let Some(ref key) = body.idempotency_key {
        BATCH_SWAP_IDEMPOTENCY.insert(
            key.clone(),
            (serde_json::to_value(&response).unwrap(), Instant::now()),
        );
    }

    Ok(Json(response))
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
    // TODO: Call Soroban RPC to invoke atomic_swap.accept_swap
    // #316: Invalidate AFTER the write commits (see `transfer_ip`) so a
    // concurrent read cannot repopulate the cache with pre-write state. A
    // state transition also changes seller/buyer list membership, so the
    // swap record and both list prefixes are invalidated.
    cache::invalidate_swap(swap_id);
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
    // TODO: Call Soroban RPC to invoke atomic_swap.reveal_key
    // #316: Invalidate AFTER the write commits (see `transfer_ip`). A
    // completed swap changes seller/buyer list membership and both parties'
    // reputation, so the record, both list prefixes, and the reputation
    // cache are all invalidated.
    cache::invalidate_swap(swap_id);
    cache::invalidate_prefix("reputation:");
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
    // TODO: Call Soroban RPC to invoke atomic_swap.cancel_swap
    // #316: Invalidate AFTER the write commits (see `transfer_ip`) — the
    // swap record and both seller/buyer list prefixes.
    cache::invalidate_swap(swap_id);
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
    // TODO: Call Soroban RPC to invoke atomic_swap.cancel_expired_swap
    // #316: Invalidate AFTER the write commits (see `transfer_ip`) — the
    // swap record and both seller/buyer list prefixes.
    cache::invalidate_swap(swap_id);
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
pub async fn get_swap(
    State(rpc_client): State<Arc<dyn crate::graphql::SorobanRpcClient>>,
    Path(swap_id): Path<u64>,
) -> impl IntoResponse {
    // #316: Check cache first
    let cache_key = cache::swap_key(swap_id);
    if let Some(cached) = cache::get::<SwapRecord>(&cache_key) {
        return (
            StatusCode::OK,
            [(header::CACHE_CONTROL, cache::cache_control_header())],
            Json(serde_json::to_value(cached).unwrap()),
        ).into_response();
    }

    // #316: On cache miss, read through to the Soroban RPC layer and backfill
    // the cache so subsequent lookups hit the fast path.
    match rpc_client.get_swap_record(swap_id).await {
        Ok(Some(record)) => {
            let record: SwapRecord = record.into();
            cache::set(&cache_key, &record);
            (
                StatusCode::OK,
                [(header::CACHE_CONTROL, cache::cache_control_header())],
                Json(serde_json::to_value(record).unwrap()),
            ).into_response()
        }
        // RPC miss or error: the swap does not exist (or is currently unreadable).
        _ => (
            StatusCode::NOT_FOUND,
            [(header::CACHE_CONTROL, cache::no_cache_header())],
            Json(serde_json::json!({ "error": format!("Swap {} not found", swap_id) })),
        ).into_response(),
    }
}

/// Bridge the RPC layer's record shape (shared with GraphQL) to the REST
/// schema returned by `GET /v1/swap/{swap_id}`.
impl From<crate::graphql::SwapRecord> for SwapRecord {
    fn from(record: crate::graphql::SwapRecord) -> Self {
        Self {
            ip_id: record.ip_id,
            ip_registry_id: record.ip_registry_id,
            seller: record.seller,
            buyer: record.buyer,
            // GraphQL stringifies i128 prices to stay JSON-safe.
            price: record.price.parse().unwrap_or(0),
            token: record.token,
            status: match record.status {
                crate::graphql::SwapStatus::Pending => SwapStatus::Pending,
                crate::graphql::SwapStatus::Accepted => SwapStatus::Accepted,
                crate::graphql::SwapStatus::Completed => SwapStatus::Completed,
                crate::graphql::SwapStatus::Disputed => SwapStatus::Disputed,
                crate::graphql::SwapStatus::Cancelled => SwapStatus::Cancelled,
            },
            expiry: record.expiry,
        }
    }
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
