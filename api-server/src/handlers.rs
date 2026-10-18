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

/// #1209: Metric counter incremented whenever the audit log store falls back
/// to the in-memory sink because the configured store could not be opened.
static AUDIT_FALLBACK_ACTIVE: AtomicU64 = AtomicU64::new(0);

/// #1209: Typed error describing why the configured audit log store could not
/// be initialized. Returned instead of panicking so the process can degrade.
#[derive(Debug)]
pub struct AuditStoreInitError {
    pub message: String,
}

impl std::fmt::Display for AuditStoreInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audit store initialization failed: {}", self.message)
    }
}

impl std::error::Error for AuditStoreInitError {}

/// #1209: Number of times the audit store has fallen back to the in-memory
/// sink. Exposed for metrics scraping / tests.
pub fn audit_fallback_active_count() -> u64 {
    AUDIT_FALLBACK_ACTIVE.load(Ordering::Relaxed)
}

/// #1209: Build the audit log store, degrading to an in-memory sink instead of
/// panicking when the configured store cannot be opened.
///
/// Returns the store plus a flag indicating whether the fallback is active.
fn init_audit_log_store() -> (Arc<AuditLogStore>, bool) {
    let audit_key = std::env::var("AUDIT_HMAC_KEY")
        .unwrap_or_else(|_| "default_audit_key_for_testing".to_string());
    let audit_path = std::env::var("AUDIT_LOG_PATH")
        .unwrap_or_else(|_| "/tmp/api_audit.log".to_string());

    match AuditLogStore::open(audit_key.into_bytes(), audit_path) {
        Ok(store) => (Arc::new(store), false),
        Err(e) => {
            let err = AuditStoreInitError {
                message: e.to_string(),
            };
            tracing::warn!(
                error = %err,
                "Failed to initialize audit log store; falling back to in-memory audit sink"
            );
            AUDIT_FALLBACK_ACTIVE.fetch_add(1, Ordering::Relaxed);
            let store = AuditLogStore::in_memory("temp_key".into());
            (Arc::new(store), true)
        }
    }
}

/// Audit log store for tracking all API access and sensitive operations
static AUDIT_LOG_STORE: Lazy<Arc<AuditLogStore>> = Lazy::new(|| init_audit_log_store().0);

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

    // #1064: track the new commitment entering the Active lifecycle state.
    crate::commitment_monitoring::COMMITMENT_MONITOR.record_commit(1);

    Ok(Json(ip_id))
}

/// Return aggregate commitment and swap usage statistics.
#[utoipa::path(
    get,
    path = "/v1/analytics/commitments",
    tag = "Analytics",
    responses((status = 200, description = "Commitment and swap analytics"))
)]
#[instrument]
pub async fn commitment_analytics() -> impl IntoResponse {
    Json(crate::analytics::snapshot())
}

/// List IP commitment IDs organized under a tag.
#[utoipa::path(
    get,
    path = "/v1/ip/tag/{tag}",
    tag = "IP Registry",
    params(("tag" = String, Path, description = "Commitment tag")),
    responses((status = 200, description = "Commitments with the tag", body = Vec<u64>))
)]
#[instrument]
pub async fn list_commitments_by_tag(Path(tag): Path<String>) -> impl IntoResponse {
    Json(crate::commitments::list_by_tag(&tag))
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
            let mut watchlists = crate::lock_recover(&WATCHLISTS).map_err(|_| {
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
            let watchlists = crate::lock_recover(&WATCHLISTS).map_err(|_| {
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
            let mut watchlists = crate::lock_recover(&WATCHLISTS).map_err(|_| {
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
    let cursor_data = match pagination.cursor.as_deref() {
        Some(cursor) => match crate::schemas::cursor::decode(cursor) {
            Some(data) => Some(data),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    [(header::CACHE_CONTROL, cache::no_cache_header())],
                    Json(serde_json::json!({ "error": "invalid cursor" })),
                ).into_response();
            }
        },
        None => None,
    };
    let last_id = cursor_data.as_ref().map(|data| data.last_id);

    // #316: Check cache with cursor-based key
    let cache_key = cache::ip_list_key(
        &owner,
        limit,
        pagination.cursor.as_deref().unwrap_or("first"),
    );
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

    // IDs are appended monotonically by the contract, so a cursor can seek
    // directly by the last returned ID instead of skipping an offset.
    let mut page: Vec<u64> = all_ids
        .into_iter()
        .filter(|id| last_id.map_or(true, |last| *id > last))
        .take(limit as usize + 1)
        .collect();
    let has_more = page.len() > limit as usize;
    if has_more {
        page.truncate(limit as usize);
    }

    // Calculate next cursor
    let next_cursor = if has_more {
        let last_item_id = page.last().copied().unwrap_or(0);
        Some(crate::schemas::cursor::new(last_item_id))
    } else {
        None
    };

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