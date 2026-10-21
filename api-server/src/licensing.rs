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

    let license = License {
        id: NEXT_LICENSE_ID.fetch_add(1, Ordering::Relaxed),
        ip_id: req.ip_id,
        licensor: req.licensor,
        licensee: req.licensee,
        royalty_bps: req.royalty_bps,
        start,
        expiry: req.expiry,
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
    Ok(entry.clone())
}

/// Record licensee revenue and accrue the royalty due. Only allowed while the license is active.
pub fn record_royalty(id: u64, gross_revenue: i128, now: u64) -> Result<License, LicenseError> {
    let mut entry = LICENSES.get_mut(&id).ok_or(LicenseError::NotFound(id))?;
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
pub async fn get_license(Path(id): Path<u64>) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    respond(get(id).ok_or(LicenseError::NotFound(id)), now_timestamp())
}

/// `POST /v1/licenses/{id}/terms` — amend terms before the license starts.
pub async fn update_license_terms(
    Path(id): Path<u64>,
    Json(body): Json<UpdateTermsRequest>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let now = now_timestamp();
    respond(update_terms(id, body, now), now)
}

/// `POST /v1/licenses/{id}/royalties` — record licensee revenue and accrue royalties.
pub async fn record_license_royalty(
    Path(id): Path<u64>,
    Json(body): Json<RecordRoyaltyRequest>,
) -> Result<Json<LicenseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let now = now_timestamp();
    respond(record_royalty(id, body.gross_revenue, now), now)
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
            record_royalty(license.id, 1_000, 50),
            Err(LicenseError::InvalidState(_))
        ));
        let updated = record_royalty(license.id, 1_000, 150).unwrap();
        assert_eq!(updated.royalties.len(), 1);
        assert_eq!(updated.royalties[0].royalty_due, 100);
        assert_eq!(updated.total_royalty_due, 100);
        assert!(matches!(
            record_royalty(license.id, 1_000, 200),
            Err(LicenseError::InvalidState(_))
        ));
    }

    #[test]
    fn only_licensor_can_amend_before_start() {
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
        assert_eq!(record_royalty(u64::MAX, 1, 0), Err(LicenseError::NotFound(u64::MAX)));
    }
}
