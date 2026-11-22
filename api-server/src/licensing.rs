//! IP licensing module (#1094).
//!
//! Lets an IP owner (licensor) grant time-bounded usage rights to a licensee
//! in exchange for royalties on the licensee's revenue. Terms can be amended
//! by the licensor only before the license starts. Status is derived from the
//! clock: `Pending` before `start`, `Active` between `start` and `expiry`, and
//! `Expired` from `expiry` onward.

use axum::{extract::Path, http::StatusCode, Json};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::handlers::now_timestamp;
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
    pub exclusive: bool,
    pub royalties: Vec<RoyaltyPayment>,
    pub total_royalty_due: i128,
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
    /// When true, no other license may overlap on (ip_id, term) for any licensee.
    #[serde(default)]
    pub exclusive: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTermsRequest {
    pub royalty_bps: i128,
    pub expiry: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordRoyaltyRequest {
    pub gross_revenue: i128,
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
    let start = req.start.unwrap_or(now);
    validate_terms(req.royalty_bps, start, req.expiry)?;

    // #1395: Exclusivity overlap check on (ip_id, term).
    // Reject if the new license overlaps with any existing license for the same ip_id where
    // either the existing or the new license is exclusive.
    for entry in LICENSES.iter() {
        let l = entry.value();
        if l.ip_id != req.ip_id {
            continue;
        }
        // Intervals [l.start, l.expiry) and [start, req.expiry) overlap when start < l.expiry && l.start < req.expiry.
        let overlaps = start < l.expiry && l.start < req.expiry;
        if !overlaps {
            continue;
        }
        if l.exclusive {
            return Err(LicenseError::InvalidState(
                "an exclusive license already exists for this IP during the requested term".to_string(),
            ));
        }
        if req.exclusive {
            return Err(LicenseError::InvalidState(
                "an exclusive license cannot be granted when other licenses overlap the same term".to_string(),
            ));
        }
        // Non-exclusive duplicate licensee check.
        if l.licensee == req.licensee {
            return Err(LicenseError::InvalidState(
                "a license for this IP and licensee already exists during the requested term".to_string(),
            ));
        }
    }

    let license = License {
        id: NEXT_LICENSE_ID.fetch_add(1, Ordering::Relaxed),
        ip_id: req.ip_id,
        licensor: req.licensor,
        licensee: req.licensee,
        royalty_bps: req.royalty_bps,
        start,
        expiry: req.expiry,
        exclusive: req.exclusive,
        royalties: Vec::new(),
        total_royalty_due: 0,
    };
    LICENSES.insert(license.id, license.clone());
    Ok(license)
}

pub fn get(id: u64) -> Option<License> {
    LICENSES.get(&id).map(|l| l.clone())
}

/// Amend royalty rate and expiry. Only the licensor may do this, and only before the license starts.
pub fn update_terms(id: u64, req: UpdateTermsRequest, caller: &str, now: u64) -> Result<License, LicenseError> {
    let mut entry = LICENSES.get_mut(&id).ok_or(LicenseError::NotFound(id))?;
    if caller != entry.licensor {
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
    Ok(entry.clone())
}

/// Record licensee revenue and accrue the royalty due. Only allowed while the license is active.
/// Caller must be the licensee (reporting revenue) or the licensor (review).
pub fn record_royalty(id: u64, gross_revenue: i128, caller: &str, now: u64) -> Result<License, LicenseError> {
    let mut entry = LICENSES.get_mut(&id).ok_or(LicenseError::NotFound(id))?;
    if caller != entry.licensee && caller != entry.licensor {
        return Err(LicenseError::Forbidden(
            "only the licensee or licensor may record royalties".to_string(),
        ));
    }
    if gross_revenue <= 0 {
        return Err(LicenseError::Invalid("gross_revenue must be positive".to_string()));
    }
    match status_at(&entry, now) {
        LicenseStatus::Active => {}
        other => {
            return Err(LicenseError::InvalidState(format!(
                "royalties can only be recorded while the license is active (status is {other:?})"
            )))
        }
    }
    let royalty_due = gross_revenue * entry.royalty_bps / BPS_DENOMINATOR;
    entry.total_royalty_due += royalty_due;
    entry.royalties.push(RoyaltyPayment {
        gross_revenue,
        royalty_due,
        timestamp: now,
    });
    Ok(entry.clone())
}

fn respond(result: Result<License, LicenseError>, now: u64) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    result
        .map(|license| {
            let status = status_at(&license, now);
            Json(LicenseResponse { license, status })
        })
        .map_err(|err| (err.status(), Json(ErrorResponse { error: err.message() })))
}

/// `POST /v1/licenses` — create a license agreement.
pub async fn create_license(
    Json(body): Json<CreateLicenseRequest>,
) -> Result<(StatusCode, Json<LicenseResponse>), (StatusCode, Json<ErrorResponse>)> {
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
pub async fn get_license(
    crate::auth::AuthExtension(claims): crate::auth::AuthExtension,
    Path(id): Path<u64>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let now = now_timestamp();
    let license = get(id).ok_or_else(|| {
        let err = LicenseError::NotFound(id);
        (err.status(), Json(ErrorResponse { error: err.message() }))
    })?;
    if claims.sub != license.licensor && claims.sub != license.licensee {
        let err = LicenseError::Forbidden("access restricted to licensor and licensee".to_string());
        return Err((err.status(), Json(ErrorResponse { error: err.message() })));
    }
    let status = status_at(&license, now);
    Ok(Json(LicenseResponse { license, status }))
}

/// `POST /v1/licenses/{id}/terms` — amend terms before the license starts.
pub async fn update_license_terms(
    crate::auth::AuthExtension(claims): crate::auth::AuthExtension,
    Path(id): Path<u64>,
    Json(body): Json<UpdateTermsRequest>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let now = now_timestamp();
    respond(update_terms(id, body, &claims.sub, now), now)
}

/// `POST /v1/licenses/{id}/royalties` — record licensee revenue and accrue royalties.
pub async fn record_license_royalty(
    crate::auth::AuthExtension(claims): crate::auth::AuthExtension,
    Path(id): Path<u64>,
    Json(body): Json<RecordRoyaltyRequest>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let now = now_timestamp();
    respond(record_royalty(id, body.gross_revenue, &claims.sub, now), now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(start: Option<u64>, expiry: u64) -> CreateLicenseRequest {
        CreateLicenseRequest {
            ip_id: 7,
            licensor: "GLICENSOR".to_string(),
            licensee: "GLICENSEE".to_string(),
            royalty_bps: 1_000,
            start,
            expiry,
            exclusive: false,
        }
    }

    #[test]
    fn status_follows_the_clock() {
        let license = create(request(Some(100), 200), 0).unwrap();
        assert_eq!(status_at(&license, 99), LicenseStatus::Pending);
        assert_eq!(status_at(&license, 100), LicenseStatus::Active);
        assert_eq!(status_at(&license, 199), LicenseStatus::Active);
        assert_eq!(status_at(&license, 200), LicenseStatus::Expired);
    }

    #[test]
    fn start_defaults_to_creation_time() {
        let license = create(request(None, 500), 50).unwrap();
        assert_eq!(license.start, 50);
    }

    #[test]
    fn records_royalties_only_while_active() {
        let license = create(request(Some(100), 200), 0).unwrap();
        assert!(matches!(
            record_royalty(license.id, 1_000, "GLICENSEE", 50),
            Err(LicenseError::InvalidState(_))
        ));
        let updated = record_royalty(license.id, 1_000, "GLICENSEE", 150).unwrap();
        assert_eq!(updated.royalties.len(), 1);
        assert_eq!(updated.royalties[0].royalty_due, 100);
        assert_eq!(updated.total_royalty_due, 100);
        assert!(matches!(
            record_royalty(license.id, 1_000, "GLICENSEE", 200),
            Err(LicenseError::InvalidState(_))
        ));
    }

    #[test]
    fn only_licensor_can_amend_before_start() {
        let license = create(request(Some(100), 200), 0).unwrap();
        let by_licensee = UpdateTermsRequest {
            royalty_bps: 500,
            expiry: 300,
        };
        assert!(matches!(
            update_terms(license.id, by_licensee, "GLICENSEE", 0),
            Err(LicenseError::Forbidden(_))
        ));

        let by_licensor = UpdateTermsRequest {
            royalty_bps: 500,
            expiry: 300,
        };
        let amended = update_terms(license.id, by_licensor.clone(), "GLICENSOR", 0).unwrap();
        assert_eq!(amended.royalty_bps, 500);
        assert_eq!(amended.expiry, 300);

        // Once the license has started, terms are frozen.
        assert!(matches!(
            update_terms(license.id, by_licensor, "GLICENSOR", 150),
            Err(LicenseError::InvalidState(_))
        ));
    }

    #[test]
    fn rejects_invalid_terms() {
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
    fn unknown_license_is_not_found() {
        assert_eq!(record_royalty(u64::MAX, 1, "GLICENSEE", 0), Err(LicenseError::NotFound(u64::MAX)));
    }

    #[test]
    fn only_licensor_or_licensee_can_record_royalties() {
        let license = create(request(Some(100), 200), 0).unwrap();
        assert!(matches!(
            record_royalty(license.id, 1_000, "THIRD_PARTY", 150),
            Err(LicenseError::Forbidden(_))
        ));
        assert!(record_royalty(license.id, 1_000, "GLICENSEE", 150).is_ok());
        assert!(record_royalty(license.id, 1_000, "GLICENSOR", 150).is_ok());
    }

    #[test]
    fn exclusive_license_blocks_overlapping_licenses() {
        let exclusive_req = CreateLicenseRequest {
            ip_id: 42,
            licensor: "LICENSOR".to_string(),
            licensee: "LICENSEE_A".to_string(),
            royalty_bps: 500,
            start: Some(100),
            expiry: 200,
            exclusive: true,
        };
        create(exclusive_req, 0).unwrap();

        // Any overlapping license should be rejected.
        let overlap_req = CreateLicenseRequest {
            ip_id: 42,
            licensor: "LICENSOR".to_string(),
            licensee: "LICENSEE_B".to_string(),
            royalty_bps: 500,
            start: Some(150),
            expiry: 250,
            exclusive: false,
        };
        assert!(matches!(
            create(overlap_req, 0),
            Err(LicenseError::InvalidState(_))
        ));

        // Non-overlapping license is allowed.
        let non_overlap_req = CreateLicenseRequest {
            ip_id: 42,
            licensor: "LICENSOR".to_string(),
            licensee: "LICENSEE_B".to_string(),
            royalty_bps: 500,
            start: Some(200),
            expiry: 300,
            exclusive: false,
        };
        assert!(create(non_overlap_req, 0).is_ok());
    }

    #[test]
    fn non_exclusive_duplicate_licensee_is_rejected() {
        let req = CreateLicenseRequest {
            ip_id: 43,
            licensor: "LICENSOR".to_string(),
            licensee: "LICENSEE_A".to_string(),
            royalty_bps: 500,
            start: Some(100),
            expiry: 200,
            exclusive: false,
        };
        create(req.clone(), 0).unwrap();
        assert!(matches!(
            create(req, 0),
            Err(LicenseError::InvalidState(_))
        ));
    }

    #[test]
    fn new_exclusive_license_blocked_by_existing_non_exclusive() {
        let non_exclusive = CreateLicenseRequest {
            ip_id: 44,
            licensor: "LICENSOR".to_string(),
            licensee: "LICENSEE_A".to_string(),
            royalty_bps: 500,
            start: Some(100),
            expiry: 200,
            exclusive: false,
        };
        create(non_exclusive, 0).unwrap();

        let exclusive_req = CreateLicenseRequest {
            ip_id: 44,
            licensor: "LICENSOR".to_string(),
            licensee: "LICENSEE_B".to_string(),
            royalty_bps: 500,
            start: Some(100),
            expiry: 200,
            exclusive: true,
        };
        assert!(matches!(
            create(exclusive_req, 0),
            Err(LicenseError::InvalidState(_))
        ));
    }
}
