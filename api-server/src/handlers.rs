use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    body::Body,
    Json,
};
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::{Duration, Instant};
use tracing::instrument;
use sha2::{Digest, Sha256};
use crate::cache;
use crate::deduplication::{create_store, DeduplicationStore};
use crate::graphql::SorobanQueryClient;
use crate::schemas::*;
use std::sync::Arc;
use crate::webhook;
use crate::websocket;
use crate::audit::{AuditLogStore, AuditLogQuery, SuspiciousPattern};
use dashmap::DashMap;

// #523/#800: Per-handler idempotency store for batch swap operations. Uses a
// shared Redis backend when REDIS_URL is configured, so a client's retry is
// deduplicated correctly no matter which api-server instance behind the load
// balancer it lands on; falls back to an in-process (single-instance-only)
// store otherwise, mirroring `cache::REDIS_POOL`'s init pattern.
static BATCH_SWAP_IDEMPOTENCY: Lazy<DeduplicationStore> = Lazy::new(|| match std::env::var("REDIS_URL") {
    Ok(url) if !url.is_empty() => {
        tracing::info!("batch-swap idempotency store: using shared Redis backend via REDIS_URL");
        create_store_with_backend(DeduplicationBackend::Redis(url))
    }
    _ => {
        tracing::warn!(
            "batch-swap idempotency store: REDIS_URL not set, falling back to in-process store \
             (not safe for a multi-instance deployment)"
        );
        create_store()
    }
});

// #520: Mirrors the contract's MAX_BATCH_SIZE cap on a single batch.
const MAX_BATCH_SIZE: usize = 50;

// #469: Mirrors the contract's ~7-day swap expiry (ledger timestamp + 604800).
const SWAP_EXPIRY_SECONDS: u64 = 604800;

/// Process-local swap ID counter standing in for the contract's `NextId`
/// until the handlers are wired to a live Soroban RPC client.
static NEXT_SWAP_ID: AtomicU64 = AtomicU64::new(0);
static WATCHLISTS: Lazy<Mutex<HashMap<String, BTreeSet<u64>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static COMMITMENT_INDEX: Lazy<DashMap<u64, IpRecord>> = Lazy::new(DashMap::new);

/// Audit log store for tracking all API access and sensitive operations
static AUDIT_LOG_STORE: Lazy<Arc<AuditLogStore>> = Lazy::new(|| {
    let audit_key = std::env::var("AUDIT_HMAC_KEY")
        .unwrap_or_else(|_| "default_audit_key_for_testing".to_string());
    let audit_path = std::env::var("AUDIT_LOG_PATH")
        .unwrap_or_else(|_| "/tmp/api_audit.log".to_string());

    match AuditLogStore::open(audit_key.into_bytes(), audit_path) {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!("Failed to initialize audit log store: {}", e);
            Arc::new(AuditLogStore::open("temp_key".into(), "/tmp/api_audit_fallback.log")
                .unwrap_or_else(|_| panic!("Failed to create fallback audit store")))
        }
    }
});

/// Current Unix timestamp in seconds (substitute for the ledger timestamp).
fn now_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// ── IP Registry ───────────────────────────────────────────────────────────────

/// Timestamp a new IP commitment. Returns the assigned IP ID.
///
/// #983: Per-user commitment rate limiting (10 commits/minute) is enforced on successful commits.
#[utoipa::path(
    post,
    path = "/v1/ip/commit",
    tag = "IP Registry",
    request_body = CommitIpRequest,
    responses(
        (status = 200, description = "IP committed successfully, returns assigned ip_id", body = u64),
        (status = 400, description = "Invalid request (zero hash, duplicate hash)", body = ErrorResponse),
        (status = 429, description = "User commitment rate limit exceeded (10 commits/minute)", body = ErrorResponse),
        (status = 503, description = "Soroban RPC node unavailable", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn commit_ip(
    State(rate_limiter): State<Arc<crate::rate_limit::RateLimitMiddleware>>,
    Json(body): Json<CommitIpRequest>,
) -> Result<Json<u64>, (StatusCode, Json<ErrorResponse>)> {
    // Delegate to the Soroban RPC client.  The client validates inputs before
    // making the network call, so validation errors are surfaced as 400 without
    // a round-trip to the RPC node.
    let ip_id = SOROBAN_CLIENT
        .commit_ip(&body.owner, &body.commitment_hash)
        .await
        .map_err(|err| {
            let status = soroban_rpc::map_rpc_error_to_status(&err);
            (
                status,
                Json(ErrorResponse {
                    error: err.to_string(),
                }),
            )
        })?;

    // #983: Check per-user commitment rate limit after successful commit
    let (allowed, _remaining, reset_after) = rate_limiter.check_commitment_rate_limit(&body.owner).await;
    if !allowed {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: format!(
                    "User commitment rate limit exceeded (10 commits/minute). Retry after {} seconds",
                    reset_after.as_secs().max(1)
                ),
            }),
        ));
    }

    Ok(Json(ip_id))
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

/// Reveal and verify multiple IP commitments in one request.
#[utoipa::path(
    post,
    path = "/v1/ip/reveal-batch",
    tag = "IP Registry",
    request_body = BatchRevealCommitmentsRequest,
    responses(
        (status = 200, description = "Commitments verified with per-item results", body = BatchRevealCommitmentsResponse),
        (status = 400, description = "Invalid or oversized batch", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn batch_reveal_commitments(
    Json(body): Json<BatchRevealCommitmentsRequest>,
) -> Result<Json<BatchRevealCommitmentsResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.commitments.is_empty() || body.commitments.len() > MAX_BATCH_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("commitments must contain between 1 and {} items", MAX_BATCH_SIZE),
            }),
        ));
    }

    /// Export selected commitment records as JSON, CSV, or XML.
    #[utoipa::path(
        get,
        path = "/v1/ip/export",
        tag = "IP Registry",
        params(ExportCommitmentsParams),
        responses(
            (status = 200, description = "Commitments exported"),
            (status = 400, description = "Invalid format or IP IDs", body = ErrorResponse),
            (status = 404, description = "An IP record was not found", body = ErrorResponse),
        )
    )]
    #[instrument]
    pub async fn export_commitments(
        Query(params): Query<ExportCommitmentsParams>,
    ) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
        let ids: Result<Vec<u64>, _> = params
            .ip_ids
            .split(',')
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().parse::<u64>())
            .collect();
        let ids = ids.map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: "ip_ids must be comma-separated unsigned integers".to_string() }),
            )
        })?;
        if ids.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: "ip_ids must not be empty".to_string() }),
            ));
        }

        /// Add a commitment to a user's watchlist.
        #[utoipa::path(
            post,
            path = "/v1/watchlist",
            tag = "IP Registry",
            request_body = WatchlistRequest,
            responses(
                (status = 200, description = "Commitment added to watchlist", body = WatchlistResponse),
                (status = 400, description = "Invalid user ID", body = ErrorResponse),
            )
        )]
        #[instrument(skip(body))]
        pub async fn add_to_watchlist(
            Json(body): Json<WatchlistRequest>,
        ) -> Result<Json<WatchlistResponse>, (StatusCode, Json<ErrorResponse>)> {
            if body.user_id.trim().is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse { error: "user_id must not be empty".to_string() }),
                ));
            }
            let mut watchlists = WATCHLISTS.lock().map_err(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "watchlist unavailable".to_string() }))
            })?;
            let ids = watchlists.entry(body.user_id.clone()).or_default();
            ids.insert(body.ip_id);
            Ok(Json(WatchlistResponse {
                user_id: body.user_id,
                ip_ids: ids.iter().copied().collect(),
            }))
        }

        /// List all commitments on a user's watchlist.
        #[utoipa::path(
            get,
            path = "/v1/watchlist",
            tag = "IP Registry",
            params(WatchlistQuery),
            responses((status = 200, description = "User watchlist", body = WatchlistResponse))
        )]
        #[instrument]
        pub async fn get_watchlist(
            Query(query): Query<WatchlistQuery>,
        ) -> Result<Json<WatchlistResponse>, (StatusCode, Json<ErrorResponse>)> {
            if query.user_id.trim().is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse { error: "user_id must not be empty".to_string() }),
                ));
            }
            let watchlists = WATCHLISTS.lock().map_err(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "watchlist unavailable".to_string() }))
            })?;
            Ok(Json(WatchlistResponse {
                user_id: query.user_id.clone(),
                ip_ids: watchlists
                    .get(&query.user_id)
                    .map(|ids| ids.iter().copied().collect())
                    .unwrap_or_default(),
            }))
        }

        /// Remove a commitment from a user's watchlist.
        #[utoipa::path(
            delete,
            path = "/v1/watchlist/{user_id}/{ip_id}",
            tag = "IP Registry",
            params(
                ("user_id" = String, Path, description = "User identifier"),
                ("ip_id" = u64, Path, description = "IP commitment identifier")
            ),
            responses((status = 200, description = "Commitment removed", body = WatchlistResponse))
        )]
        #[instrument]
        pub async fn remove_from_watchlist(
            Path((user_id, ip_id)): Path<(String, u64)>,
        ) -> Result<Json<WatchlistResponse>, (StatusCode, Json<ErrorResponse>)> {
            if user_id.trim().is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse { error: "user_id must not be empty".to_string() }),
                ));
            }
            let mut watchlists = WATCHLISTS.lock().map_err(|_| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "watchlist unavailable".to_string() }))
            })?;
            let ids = watchlists.entry(user_id.clone()).or_default();
            ids.remove(&ip_id);
            Ok(Json(WatchlistResponse {
                user_id,
                ip_ids: ids.iter().copied().collect(),
            }))
        }

        let mut records = Vec::with_capacity(ids.len());
        for id in ids {
            match cache::get::<IpRecord>(&cache::ip_key(id)) {
                Some(record) => records.push(record),
                None => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse { error: format!("IP record {} not found", id) }),
                    ))
                }
            }
        }

        let format = params.format.to_ascii_lowercase();
        let (content_type, extension, body) = match format.as_str() {
            "json" => (
                "application/json",
                "json",
                serde_json::to_string_pretty(&records).map_err(|error| {
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: error.to_string() }))
                })?,
            ),
            "csv" => ("text/csv; charset=utf-8", "csv", commitment_csv(&records)),
            "xml" => ("application/xml; charset=utf-8", "xml", commitment_xml(&records)),
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse { error: "format must be one of json, csv, or xml".to_string() }),
                ))
            }
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"commitments.{}\"", extension))
            .body(Body::from(body))
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: error.to_string() })))
    }

    fn csv_field(value: &str) -> String {
        if value.chars().any(|character| matches!(character, ',' | '"' | '\n' | '\r')) {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }

    fn commitment_csv(records: &[IpRecord]) -> String {
        let mut output = "ip_id,owner,commitment_hash,timestamp,revoked\n".to_string();
        for record in records {
            output.push_str(&format!(
                "{},{},{},{},{}\n",
                record.ip_id,
                csv_field(&record.owner),
                csv_field(&record.commitment_hash),
                record.timestamp,
                record.revoked
            ));
        }
        output
    }

    fn xml_field(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn commitment_xml(records: &[IpRecord]) -> String {
        let mut output = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<commitments>\n".to_string();
        for record in records {
            output.push_str(&format!(
                "  <commitment><ip_id>{}</ip_id><owner>{}</owner><commitment_hash>{}</commitment_hash><timestamp>{}</timestamp><revoked>{}</revoked></commitment>\n",
                record.ip_id, xml_field(&record.owner), xml_field(&record.commitment_hash), record.timestamp, record.revoked
            ));
        }
        output.push_str("</commitments>\n");
        output
    }

    let results = body
        .commitments
        .into_iter()
        .map(|item| {
            let ip_id = item.ip_id;
            let (valid, error) = match (
                hex::decode(&item.secret),
                hex::decode(&item.blinding_factor),
            ) {
                (Ok(secret), Ok(blinding_factor)) if secret.len() == 32 && blinding_factor.len() == 32 => {
                    match cache::get::<IpRecord>(&cache::ip_key(ip_id)) {
                        Some(record) => {
                            COMMITMENT_INDEX.insert(ip_id, record.clone());
                            let mut hasher = Sha256::new();
                            hasher.update(&secret);
                            hasher.update(&blinding_factor);
                            let computed = hex::encode(hasher.finalize());
                            (
                                computed.eq_ignore_ascii_case(&record.commitment_hash),
                                None,
                            )
                        }
                        None => (false, Some(format!("IP record {} not found", ip_id))),
                    }
                }
                _ => (
                    false,
                    Some("secret and blinding_factor must be 32-byte hex values".to_string()),
                ),
            };
            BatchRevealResult { ip_id, valid, error }
        })
        .collect();

    Ok(Json(BatchRevealCommitmentsResponse { results }))
}

/// Find indexed commitments by Hamming distance from a supplied hash.
#[utoipa::path(
    get,
    path = "/v1/ip/similar",
    tag = "IP Registry",
    params(SimilarCommitmentsParams),
    responses(
        (status = 200, description = "Similar commitments ordered by distance", body = SimilarCommitmentsResponse),
        (status = 400, description = "Invalid commitment hash", body = ErrorResponse),
    )
)]
#[instrument]
pub async fn find_similar_commitments(
    Query(params): Query<SimilarCommitmentsParams>,
) -> Result<Json<SimilarCommitmentsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let query_hash = hex::decode(&params.commitment_hash).map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "commitment_hash must be hex".to_string() }))
    })?;
    if query_hash.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "commitment_hash must be exactly 32 bytes".to_string() }),
        ));
    }
    let max_distance = params.max_distance.min(256);
    let limit = params.limit.clamp(1, 100) as usize;
    let mut results: Vec<SimilarCommitment> = COMMITMENT_INDEX
        .iter()
        .filter_map(|entry| {
            let record = entry.value();
            let candidate = hex::decode(&record.commitment_hash).ok()?;
            if candidate.len() != query_hash.len() {
                return None;
            }
            let distance = query_hash
                .iter()
                .zip(candidate.iter())
                .map(|(left, right)| (left ^ right).count_ones() as u16)
                .sum::<u16>();
            (distance <= max_distance).then(|| SimilarCommitment {
                ip_id: record.ip_id,
                commitment_hash: record.commitment_hash.clone(),
                distance,
            })
        })
        .collect();
    results.sort_by_key(|result| (result.distance, result.ip_id));
    results.truncate(limit);
    Ok(Json(SimilarCommitmentsResponse { results }))
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
    // The store itself only returns unexpired entries (TTL is enforced by the backend).
    if let Some(ref key) = body.idempotency_key {
        if let Some(cached) = BATCH_SWAP_IDEMPOTENCY.get(key.as_str()).await {
            if let Ok(response) = serde_json::from_value::<BatchInitiateSwapResponse>(cached) {
                return Ok(Json(response));
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
    websocket::trigger_swap_status_changed(swap_id, Some("Pending".to_string()), "Accepted".to_string());
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
    websocket::trigger_swap_status_changed(swap_id, Some("Accepted".to_string()), "Completed".to_string());
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
    websocket::trigger_swap_status_changed(swap_id, Some("Pending".to_string()), "Cancelled".to_string());
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
    websocket::trigger_swap_status_changed(swap_id, Some("Accepted".to_string()), "Cancelled".to_string());
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

/// #982: Execute multiple swaps in batch with configurable modes.
/// Supports atomic (all-or-nothing) and partial (fault-tolerant) execution.
#[utoipa::path(
    post,
    path = "/v1/swaps/execute-batch",
    tag = "Atomic Swap",
    request_body = ExecuteBatchSwapsRequest,
    responses(
        (status = 200, description = "Batch swap execution completed", body = ExecuteBatchSwapsResponse),
        (status = 400, description = "Validation error (empty batch, too large, etc.)", body = ErrorResponse),
        (status = 503, description = "Soroban RPC node unavailable", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn execute_batch_swaps(
    Json(body): Json<ExecuteBatchSwapsRequest>,
) -> Result<Json<ExecuteBatchSwapsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate batch parameters
    if body.swap_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "swap_ids must not be empty".to_string(),
            }),
        ));
    }

    if body.swap_ids.len() > 50 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "swap_ids exceeds maximum batch size of 50".to_string(),
            }),
        ));
    }

    // Call the contract's execute_batch_swaps function via Soroban RPC
    // For now, return a placeholder response structure
    // TODO: Implement Soroban RPC client call to execute the batch

    let total_count = body.swap_ids.len() as u32;
    let results: Vec<bool> = (0..total_count).map(|_| false).collect();
    let successful_count = results.iter().filter(|&&r| r).count() as u32;

    Ok(Json(ExecuteBatchSwapsResponse {
        results,
        successful_count,
        total_count,
        atomic: body.atomic,
    }))
}

// ── #984: Two-Factor Authentication ────────────────────────────────────────

static TWO_FACTOR_STORE: once_cell::sync::Lazy<crate::auth_2fa::TwoFactorStore> =
    once_cell::sync::Lazy::new(crate::auth_2fa::TwoFactorStore::new);

/// #984: Enable 2FA for a user
/// Returns TOTP secret for QR code and backup codes for account recovery
#[utoipa::path(
    post,
    path = "/auth/2fa/enable",
    tag = "Authentication",
    request_body = Enable2faRequest,
    responses(
        (status = 200, description = "2FA enabled successfully, returns secret and backup codes", body = Enable2faResponse),
        (status = 400, description = "Invalid user ID", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn enable_2fa(
    Json(body): Json<Enable2faRequest>,
) -> Result<Json<Enable2faResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.user_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "user_id must not be empty".to_string(),
            }),
        ));
    }

    // Generate TOTP secret
    let secret = crate::auth_2fa::generate_totp_secret();
    let backup_codes = crate::auth_2fa::generate_backup_codes(8);
    let qr_code_uri = crate::auth_2fa::generate_qr_code_uri(&body.user_id, &secret, "AtomicPatent");

    // Store TOTP secret (not verified until verify_2fa is called)
    let config = crate::auth_2fa::TwoFactorConfig {
        user_id: body.user_id.clone(),
        totp: Some(crate::auth_2fa::TotpSecret {
            secret: secret.clone(),
            enabled_at: chrono::Utc::now().timestamp(),
            verified: false,
        }),
        backup_codes: backup_codes
            .iter()
            .map(|code| crate::auth_2fa::BackupCode {
                code: code.clone(),
                used: false,
                created_at: chrono::Utc::now().timestamp(),
            })
            .collect(),
        created_at: chrono::Utc::now().timestamp(),
        last_verified: None,
    };

    TWO_FACTOR_STORE.store_config(config).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: err }),
        )
    })?;

    Ok(Json(Enable2faResponse {
        secret,
        qr_code_uri,
        backup_codes,
    }))
}

/// #984: Verify 2FA code during login
/// Returns success if TOTP code is valid and marks 2FA as verified
#[utoipa::path(
    post,
    path = "/auth/2fa/verify",
    tag = "Authentication",
    request_body = Verify2faRequest,
    responses(
        (status = 200, description = "2FA code verified successfully", body = Verify2faResponse),
        (status = 401, description = "Invalid 2FA code", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "2FA not configured for user", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn verify_2fa(
    Json(body): Json<Verify2faRequest>,
) -> Result<Json<Verify2faResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.user_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "user_id must not be empty".to_string(),
            }),
        ));
    }

    if body.totp_code.len() != 6 || !body.totp_code.chars().all(|c| c.is_numeric()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "TOTP code must be 6 digits".to_string(),
            }),
        ));
    }

    // Get user's 2FA config
    let mut config = match TWO_FACTOR_STORE
        .get_config(&body.user_id)
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: err }),
            )
        })?
    {
        Some(cfg) => cfg,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "2FA not configured for this user".to_string(),
                }),
            ))
        }
    };

    // Get TOTP secret
    let secret = match &config.totp {
        Some(totp) => &totp.secret,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "2FA not configured for this user".to_string(),
                }),
            ))
        }
    };

    // Verify TOTP code
    match crate::auth_2fa::verify_totp_code(secret, &body.totp_code) {
        Ok(true) => {
            // Mark 2FA as verified
            if let Some(ref mut totp) = config.totp {
                totp.verified = true;
            }
            config.last_verified = Some(chrono::Utc::now().timestamp());

            TWO_FACTOR_STORE.store_config(config).map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: err }),
                )
            })?;

            Ok(Json(Verify2faResponse {
                success: true,
                message: "2FA verified successfully".to_string(),
            }))
        }
        Ok(false) => Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid TOTP code".to_string(),
            }),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: err }),
        )),
    }
}

/// #984: Use backup code for account recovery
/// Allows account access if backup code is valid (marks code as used)
#[utoipa::path(
    post,
    path = "/auth/2fa/backup-code",
    tag = "Authentication",
    request_body = UseBackupCodeRequest,
    responses(
        (status = 200, description = "Backup code accepted, account recovered", body = Verify2faResponse),
        (status = 401, description = "Invalid or already used backup code", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn use_backup_code(
    Json(body): Json<UseBackupCodeRequest>,
) -> Result<Json<Verify2faResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.user_id.is_empty() || body.backup_code.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "user_id and backup_code must not be empty".to_string(),
            }),
        ));
    }

    // Check if backup code exists and is valid
    let used = crate::auth_2fa::mark_backup_code_used(&TWO_FACTOR_STORE, &body.user_id, &body.backup_code)
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: err }),
            )
        })?;

    if !used {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid or already used backup code".to_string(),
            }),
        ));
    }

    Ok(Json(Verify2faResponse {
        success: true,
        message: "Account recovered with backup code. Please update your 2FA settings.".to_string(),
    }))
}

// ── #985: Session Management ───────────────────────────────────────────────

static SESSION_STORE: once_cell::sync::Lazy<crate::session::SessionStore> =
    once_cell::sync::Lazy::new(|| crate::session::SessionStore::new(crate::session::SessionConfig::default()));

/// #985: Check current session status
/// Returns session status including timeout warnings and grace period info
#[utoipa::path(
    post,
    path = "/auth/session/status",
    tag = "Authentication",
    request_body = CheckSessionRequest,
    responses(
        (status = 200, description = "Session status retrieved", body = CheckSessionResponse),
        (status = 400, description = "Invalid token", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn check_session(
    Json(body): Json<CheckSessionRequest>,
) -> Result<Json<CheckSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.token.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "token must not be empty".to_string(),
            }),
        ));
    }

    match SESSION_STORE.check_session_status(&body.token) {
        Ok(status) => Ok(Json(CheckSessionResponse {
            active: status.active,
            in_grace_period: status.in_grace_period,
            minutes_until_timeout: status.minutes_until_timeout,
            show_warning: status.show_warning,
            message: status.message,
        })),
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Session not found".to_string(),
            }),
        )),
    }
}

/// #985: Extend session expiration
/// Extends the current session by resetting the idle timeout counter.
/// Can be called during grace period after timeout.
#[utoipa::path(
    post,
    path = "/auth/extend-session",
    tag = "Authentication",
    request_body = ExtendSessionRequest,
    responses(
        (status = 200, description = "Session extended successfully", body = ExtendSessionResponse),
        (status = 400, description = "Invalid token", body = ErrorResponse),
        (status = 404, description = "Session not found or grace period expired", body = ErrorResponse),
    )
)]
#[instrument(skip(body))]
pub async fn extend_session(
    Json(body): Json<ExtendSessionRequest>,
) -> Result<Json<ExtendSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    if body.token.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "token must not be empty".to_string(),
            }),
        ));
    }

    // First check if session exists and get status
    match SESSION_STORE.check_session_status(&body.token) {
        Ok(status) => {
            // Allow extension if session is active OR in grace period
            if !status.active && !status.in_grace_period {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "Session expired and grace period ended. Please log in again.".to_string(),
                    }),
                ));
            }

            // Extend the session
            match SESSION_STORE.extend_session(&body.token) {
                Ok(new_expiry) => {
                    Ok(Json(ExtendSessionResponse {
                        success: true,
                        new_expiry,
                        message: "Session extended successfully. You have another 30 minutes.".to_string(),
                    }))
                }
                Err(err) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: err }),
                )),
            }
        }
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Session not found".to_string(),
            }),
        )),
    }
}
