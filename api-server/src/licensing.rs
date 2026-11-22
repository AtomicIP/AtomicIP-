//! IP licensing module (#1094).
//!
//! Lets an IP owner (licensor) grant time-bounded usage rights to a licensee
//! in exchange for royalties on the licensee's revenue. Terms can be amended
//! by the licensor only before the license starts. Status is derived from the
//! clock: `Pending` before `start`, `Active` between `start` and `expiry`, and
//! `Expired` from `expiry` onward.
//!
//! # Durability (#1401)
//!
//! Licenses are persisted to Redis (when `REDIS_URL` is set) so they survive
//! process restarts and are visible to every replica. The in-process
//! `DashMap` acts as a write-through cache: every write goes to Redis first
//! and then to the map; every read checks the map before falling back to
//! Redis. The ID counter is backed by a Redis `INCR` so replicas cannot
//! issue duplicate IDs.
//!
//! When Redis is unavailable the module falls back to in-process state only,
//! matching the previous behaviour. A log warning is emitted on each
//! degraded write so operators are alerted.

use axum::{extract::Path, http::{HeaderMap, StatusCode}, Json};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use r2d2::Pool;
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::handlers::now_timestamp;
use crate::handlers::indexed_record;
use crate::schemas::ErrorResponse;

const BPS_DENOMINATOR: i128 = 10_000;
/// Longest allowed license term: 10 years in seconds.
const MAX_TERM_SECS: u64 = 10 * 365 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LicenseStatus {
    Pending,
    Active,
    Expired,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoyaltyPayment {
    /// Licensee revenue reported for the period, in stroops.
    pub gross_revenue: i128,
    /// Royalty owed to the licensor on that revenue, in stroops.
    pub royalty_due: i128,
    pub timestamp: u64,
    /// On-chain settlement reference (transaction hash or swap ID) that
    /// proves the payment occurred. Required to prevent self-reporting.
    pub settlement_ref: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct License {
    pub id: u64,
    pub ip_id: u64,
    pub licensor: String,
    pub licensee: String,
    /// Royalty rate in basis points of licensee revenue.
    pub royalty_bps: i128,
    pub start: u64,
    pub expiry: u64,
    pub royalties: Vec<RoyaltyPayment>,
    pub total_royalty_due: i128,
    /// Accumulated remainder from prior royalty calculations. When the
    /// product `gross_revenue * royalty_bps` is not exactly divisible by
    /// `BPS_DENOMINATOR`, the leftover is carried forward so that rounding
    /// happens at most once across the life of the license.
    #[serde(default)]
    pub royalty_remainder: i128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLicenseRequest {
    pub ip_id: u64,
    pub licensor: String,
    pub licensee: String,
    pub royalty_bps: i128,
    /// Defaults to the creation time when omitted.
    pub start: Option<u64>,
    pub expiry: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTermsRequest {
    /// Must match the license's licensor.
    pub licensor: String,
    pub royalty_bps: i128,
    pub expiry: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordRoyaltyRequest {
    pub gross_revenue: i128,
    /// On-chain settlement reference (transaction hash or atomic-swap ID)
    /// that proves the payment occurred.
    pub settlement_ref: String,
}

#[derive(Debug, Serialize)]
pub struct LicenseResponse {
    #[serde(flatten)]
    pub license: License,
    pub status: LicenseStatus,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LicenseError {
    NotFound(u64),
    Invalid(String),
    InvalidState(String),
    Forbidden(String),
}

impl LicenseError {
    fn status(&self) -> StatusCode {
        match self {
            LicenseError::NotFound(_) => StatusCode::NOT_FOUND,
            LicenseError::Invalid(_) => StatusCode::BAD_REQUEST,
            LicenseError::InvalidState(_) => StatusCode::CONFLICT,
            LicenseError::Forbidden(_) => StatusCode::FORBIDDEN,
        }
    }

    fn message(&self) -> String {
        match self {
            LicenseError::NotFound(id) => format!("license {id} not found"),
            LicenseError::Invalid(msg)
            | LicenseError::InvalidState(msg)
            | LicenseError::Forbidden(msg) => msg.clone(),
        }
    }
}

static LICENSES: Lazy<DashMap<u64, License>> = Lazy::new(DashMap::new);
static NEXT_LICENSE_ID: AtomicU64 = AtomicU64::new(1);

// ── Redis-backed durability (#1401) ───────────────────────────────────────────

const REDIS_ID_KEY: &str = "license:next_id";

fn license_redis_key(id: u64) -> String {
    format!("license:{}", id)
}

/// Open a Redis connection from `REDIS_URL`, or return `None` when Redis is
/// not configured or not reachable. Errors are logged but not propagated so
/// the module degrades gracefully.
fn redis_pool() -> Option<Pool<redis::Client>> {
    static POOL: Lazy<Option<Pool<redis::Client>>> = Lazy::new(|| {
        let url = std::env::var("REDIS_URL").ok()?;
        let client = redis::Client::open(url)
            .map_err(|err| tracing::warn!(error = %err, "licensing: invalid REDIS_URL"))
            .ok()?;
        Pool::builder()
            .max_size(8)
            .connection_timeout(std::time::Duration::from_millis(300))
            .build_unchecked(client)
            .into()
    });
    POOL.clone()
}

/// Allocate the next license ID.
/// Uses Redis `INCR` when available so replicas cannot issue duplicate IDs;
/// falls back to the process-local `AtomicU64` otherwise.
fn next_id() -> u64 {
    if let Some(pool) = redis_pool() {
        if let Ok(mut conn) = pool.get() {
            match conn.incr::<_, _, i64>(REDIS_ID_KEY, 1_i64) {
                Ok(id) => return id as u64,
                Err(err) => tracing::warn!(error = %err, "licensing: Redis INCR failed, using local counter"),
            }
        }
    }
    NEXT_LICENSE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Persist a license to Redis. Logs a warning on failure (degraded mode).
fn redis_save(license: &License) {
    let pool = match redis_pool() {
        Some(p) => p,
        None => return,
    };
    let mut conn = match pool.get() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(error = %err, id = license.id, "licensing: Redis connection unavailable, license not persisted to Redis");
            return;
        }
    };
    match serde_json::to_string(license) {
        Ok(json) => {
            if let Err(err) = conn.set::<_, _, ()>(&license_redis_key(license.id), &json) {
                tracing::warn!(error = %err, id = license.id, "licensing: failed to persist license to Redis");
            }
        }
        Err(err) => tracing::warn!(error = %err, id = license.id, "licensing: failed to serialize license"),
    }
}

/// Load a license from Redis into the local map, returning it if found.
fn redis_load(id: u64) -> Option<License> {
    let mut conn = redis_pool()?.get().ok()?;
    let json: String = conn.get(&license_redis_key(id)).ok()?;
    let license: License = serde_json::from_str(&json).ok()?;
    LICENSES.insert(id, license.clone());
    Some(license)
}

/// Status of a license at time `now`.
pub fn status_at(license: &License, now: u64) -> LicenseStatus {
    if now < license.start {
        LicenseStatus::Pending
    } else if now < license.expiry {
        LicenseStatus::Active
    } else {
        LicenseStatus::Expired
    }
}

fn validate_terms(royalty_bps: i128, start: u64, expiry: u64) -> Result<(), LicenseError> {
    if !(0..=BPS_DENOMINATOR).contains(&royalty_bps) {
        return Err(LicenseError::Invalid("royalty_bps must be between 0 and 10000".to_string()));
    }
    if expiry <= start {
        return Err(LicenseError::Invalid("expiry must be after start".to_string()));
    }
    if expiry - start > MAX_TERM_SECS {
        return Err(LicenseError::Invalid("license term may not exceed 10 years".to_string()));
    }
    Ok(())
}

/// Create a license agreement for an IP.
pub fn create(req: CreateLicenseRequest, now: u64) -> Result<License, LicenseError> {
    if req.licensor.trim().is_empty() || req.licensee.trim().is_empty() {
        return Err(LicenseError::Invalid("licensor and licensee must not be empty".to_string()));
    }
    if req.licensor == req.licensee {
        return Err(LicenseError::Invalid("licensor and licensee must be different parties".to_string()));
    }
    let record = indexed_record(req.ip_id)
        .ok_or_else(|| LicenseError::Invalid(format!("IP {} does not exist", req.ip_id)))?;
    if record.owner != req.licensor {
        return Err(LicenseError::Forbidden("licensor does not own this IP".to_string()));
    }
    let start = req.start.unwrap_or(now);
    validate_terms(req.royalty_bps, start, req.expiry)?;

    let license = License {
        id: next_id(),
        ip_id: req.ip_id,
        licensor: req.licensor,
        licensee: req.licensee,
        royalty_bps: req.royalty_bps,
        start,
        expiry: req.expiry,
        royalties: Vec::new(),
        total_royalty_due: 0,
        royalty_remainder: 0,
    };
    redis_save(&license);
    LICENSES.insert(license.id, license.clone());
    Ok(license)
}

pub fn get(id: u64) -> Option<License> {
    LICENSES.get(&id).map(|l| l.clone()).or_else(|| redis_load(id))
}

/// Amend royalty rate and expiry. Only the licensor may do this, and only before the license starts.
pub fn update_terms(id: u64, req: UpdateTermsRequest, now: u64) -> Result<License, LicenseError> {
    let mut entry = LICENSES.get_mut(&id).ok_or(LicenseError::NotFound(id))?;
    if req.licensor != entry.licensor {
        return Err(LicenseError::Forbidden("only the licensor may amend terms".to_string()));
    }
    if now >= entry.start {
        return Err(LicenseError::InvalidState(
            "terms can only be amended before the license starts".to_string(),
        ));
    }
    validate_terms(req.royalty_bps, entry.start, req.expiry)?;
    entry.royalty_bps = req.royalty_bps;
    entry.expiry = req.expiry;
    let updated = entry.clone();
    drop(entry);
    redis_save(&updated);
    Ok(updated)
}

/// Record licensee revenue and accrue the royalty due. Only allowed while the license is active.
pub fn record_royalty(id: u64, gross_revenue: i128, settlement_ref: String, now: u64) -> Result<License, LicenseError> {
    let mut entry = LICENSES.get_mut(&id).ok_or(LicenseError::NotFound(id))?;
    if gross_revenue <= 0 {
        return Err(LicenseError::Invalid("gross_revenue must be positive".to_string()));
    }
    if settlement_ref.trim().is_empty() {
        return Err(LicenseError::Invalid(
            "settlement_ref must be a non-empty transaction hash or swap ID".to_string(),
        ));
    }
    match status_at(&entry, now) {
        LicenseStatus::Active => {}
        other => {
            return Err(LicenseError::InvalidState(format!(
                "royalties can only be recorded while the license is active (status is {other:?})"
            )))
        }
    }
    // Carry the remainder forward to avoid per-payment flooring (#1399).
    // numerator = gross_revenue * royalty_bps + leftover from previous payments
    let numerator = gross_revenue
        .checked_mul(entry.royalty_bps)
        .and_then(|n| n.checked_add(entry.royalty_remainder))
        .ok_or_else(|| LicenseError::Invalid("royalty calculation overflowed".to_string()))?;
    let royalty_due = numerator / BPS_DENOMINATOR;
    let new_remainder = numerator % BPS_DENOMINATOR;
    let new_total = entry
        .total_royalty_due
        .checked_add(royalty_due)
        .ok_or_else(|| LicenseError::Invalid("total_royalty_due overflowed".to_string()))?;
    entry.royalty_remainder = new_remainder;
    entry.total_royalty_due = new_total;
    entry.royalties.push(RoyaltyPayment {
        gross_revenue,
        royalty_due,
        timestamp: now,
        settlement_ref,
    });
    let updated = entry.clone();
    drop(entry);
    redis_save(&updated);
    Ok(updated)
}

fn respond(result: Result<License, LicenseError>, now: u64) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    result
        .map(|license| {
            let status = status_at(&license, now);
            Json(LicenseResponse { license, status })
        })
        .map_err(|err| (err.status(), Json(ErrorResponse { error: err.message() })))
}

/// Reject the request unless the signature-verified `X-Public-Key` equals `expected`.
///
/// The request-signing layer proves the caller holds the key named in
/// `X-Public-Key`; this binds that proven key to the party the action is for,
/// so a caller cannot act under someone else's licensor/licensee name.
fn require_signer(
    headers: &HeaderMap,
    expected: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let signer = headers
        .get("X-Public-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse { error: "missing X-Public-Key".to_string() }),
        ))?;
    if signer != expected {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse { error: "signer does not match the required party".to_string() }),
        ));
    }
    Ok(())
}

/// `POST /v1/licenses` — create a license agreement. The signer must be the licensor.
pub async fn create_license(
    headers: HeaderMap,
    Json(body): Json<CreateLicenseRequest>,
) -> Result<(StatusCode, Json<LicenseResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_signer(&headers, &body.licensor)?;
    let now = now_timestamp();
    match create(body, now) {
        Ok(license) => {
            let status = status_at(&license, now);
            Ok((StatusCode::CREATED, Json(LicenseResponse { license, status })))
        }
        Err(err) => Err((err.status(), Json(ErrorResponse { error: err.message() }))),
    }
}

/// `GET /v1/licenses/{id}` — license terms and current status.
pub async fn get_license(Path(id): Path<u64>) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    respond(get(id).ok_or(LicenseError::NotFound(id)), now_timestamp())
}

/// `POST /v1/licenses/{id}/terms` — amend terms before the license starts. The signer must be the licensor.
pub async fn update_license_terms(
    Path(id): Path<u64>,
    headers: HeaderMap,
    Json(body): Json<UpdateTermsRequest>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let license = get(id).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: format!("license {id} not found") }),
    ))?;
    require_signer(&headers, &license.licensor)?;
    let now = now_timestamp();
    respond(update_terms(id, body, now), now)
}

/// `POST /v1/licenses/{id}/royalties` — record licensee revenue and accrue royalties. The signer must be the licensee.
pub async fn record_license_royalty(
    Path(id): Path<u64>,
    headers: HeaderMap,
    Json(body): Json<RecordRoyaltyRequest>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let license = get(id).ok_or((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: format!("license {id} not found") }),
    ))?;
    require_signer(&headers, &license.licensee)?;
    let now = now_timestamp();
    respond(record_royalty(id, body.gross_revenue, body.settlement_ref, now), now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::index_record;
    use crate::schemas::IpRecord;

    fn register_test_ip() {
        index_record(IpRecord {
            ip_id: 7,
            owner: "GLICENSOR".to_string(),
            commitment_hash: "hash-7".to_string(),
            timestamp: 0,
            revoked: false,
        });
    }

    fn request(start: Option<u64>, expiry: u64) -> CreateLicenseRequest {
        CreateLicenseRequest {
            ip_id: 7,
            licensor: "GLICENSOR".to_string(),
            licensee: "GLICENSEE".to_string(),
            royalty_bps: 1_000,
            start,
            expiry,
        }
    }

    #[test]
    fn status_follows_the_clock() {
        register_test_ip();
        let license = create(request(Some(100), 200), 0).unwrap();
        assert_eq!(status_at(&license, 99), LicenseStatus::Pending);
        assert_eq!(status_at(&license, 100), LicenseStatus::Active);
        assert_eq!(status_at(&license, 199), LicenseStatus::Active);
        assert_eq!(status_at(&license, 200), LicenseStatus::Expired);
    }

    #[test]
    fn start_defaults_to_creation_time() {
        register_test_ip();
        let license = create(request(None, 500), 50).unwrap();
        assert_eq!(license.start, 50);
    }

    #[test]
    fn records_royalties_only_while_active() {
        register_test_ip();
        let license = create(request(Some(100), 200), 0).unwrap();
        assert!(matches!(
            record_royalty(license.id, 1_000, "tx_pending".to_string(), 50),
            Err(LicenseError::InvalidState(_))
        ));
        let updated = record_royalty(license.id, 1_000, "tx_active".to_string(), 150).unwrap();
        assert_eq!(updated.royalties.len(), 1);
        assert_eq!(updated.royalties[0].royalty_due, 100);
        assert_eq!(updated.total_royalty_due, 100);
        assert!(matches!(
            record_royalty(license.id, 1_000, "tx_expired".to_string(), 200),
            Err(LicenseError::InvalidState(_))
        ));
    }

    #[test]
    fn only_licensor_can_amend_before_start() {
        register_test_ip();
        let license = create(request(Some(100), 200), 0).unwrap();
        let by_licensee = UpdateTermsRequest {
            licensor: "GLICENSEE".to_string(),
            royalty_bps: 500,
            expiry: 300,
        };
        assert!(matches!(
            update_terms(license.id, by_licensee, 0),
            Err(LicenseError::Forbidden(_))
        ));

        let by_licensor = UpdateTermsRequest {
            licensor: "GLICENSOR".to_string(),
            royalty_bps: 500,
            expiry: 300,
        };
        let amended = update_terms(license.id, by_licensor.clone(), 0).unwrap();
        assert_eq!(amended.royalty_bps, 500);
        assert_eq!(amended.expiry, 300);

        // Once the license has started, terms are frozen.
        assert!(matches!(
            update_terms(license.id, by_licensor, 150),
            Err(LicenseError::InvalidState(_))
        ));
    }

    #[test]
    fn rejects_invalid_terms() {
        register_test_ip();
        let mut bad_rate = request(Some(1), 10);
        bad_rate.royalty_bps = 10_001;
        assert!(matches!(create(bad_rate, 0), Err(LicenseError::Invalid(_))));
        assert!(matches!(create(request(Some(10), 10), 0), Err(LicenseError::Invalid(_))));
        assert!(matches!(
            create(request(Some(1), 1 + MAX_TERM_SECS + 1), 0),
            Err(LicenseError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_nonexistent_ip() {
        let mut req = request(Some(1), 100);
        req.ip_id = 99999;
        assert!(matches!(create(req, 0), Err(LicenseError::Invalid(_))));
    }

    #[test]
    fn rejects_non_owner_licensor() {
        register_test_ip();
        let mut req = request(Some(1), 100);
        req.licensor = "GNOTOWNER".to_string();
        req.licensee = "GLICENSEE".to_string();
        assert!(matches!(create(req, 0), Err(LicenseError::Forbidden(_))));
    }

    #[test]
    fn unknown_license_is_not_found() {
        assert_eq!(
            record_royalty(u64::MAX, 1, "txhash".to_string(), 0),
            Err(LicenseError::NotFound(u64::MAX))
        );
    }

    #[test]
    fn royalty_remainder_is_carried_forward() {
        // royalty_bps = 1 (0.01%). Payment of 1 stroop: 1 * 1 / 10_000 = 0 with remainder 1.
        // After 10_000 such payments the accumulated numerator = 10_000, royalty_due = 1 total.
        let req = CreateLicenseRequest {
            ip_id: 1,
            licensor: "GLICENSOR".to_string(),
            licensee: "GLICENSEE".to_string(),
            royalty_bps: 1,
            start: Some(100),
            expiry: 200,
        };
        let license = create(req, 0).unwrap();
        // 9_999 payments of 1 stroop each — remainder accumulates, royalty_due stays 0 each time.
        for i in 0..9_999u64 {
            record_royalty(license.id, 1, format!("tx_{i}"), 150).unwrap();
        }
        // 10_000th payment tips the accumulated numerator over BPS_DENOMINATOR.
        let final_state = record_royalty(license.id, 1, "tx_final".to_string(), 150).unwrap();
        assert_eq!(final_state.total_royalty_due, 1, "remainder carry-forward must yield 1 stroop after 10_000 payments");
    }

    #[test]
    fn empty_settlement_ref_is_rejected() {
        let license = create(request(Some(100), 200), 0).unwrap();
        assert!(matches!(
            record_royalty(license.id, 1_000, "".to_string(), 150),
            Err(LicenseError::Invalid(_))
        ));
        assert!(matches!(
            record_royalty(license.id, 1_000, "   ".to_string(), 150),
            Err(LicenseError::Invalid(_))
        ));
    }

    fn headers_with_key(key: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("X-Public-Key", key.parse().unwrap());
        headers
    }

    #[test]
    fn require_signer_accepts_matching_key() {
        assert!(require_signer(&headers_with_key("GLICENSOR"), "GLICENSOR").is_ok());
    }

    #[test]
    fn require_signer_rejects_other_key() {
        // #1310: a caller signing with its own key cannot act for a different licensor/licensee.
        let err = require_signer(&headers_with_key("GATTACKER"), "GLICENSOR").unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn require_signer_rejects_missing_key_header() {
        let err = require_signer(&HeaderMap::new(), "GLICENSOR").unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }
}
