//! Buyer-side due diligence for an IP commitment (#1095).
//!
//! `GET /v1/ip/{ip_id}/due-diligence` runs four groups of checks against the
//! indexed commitments and returns a report a buyer can review before paying:
//!
//! - **Provenance**: the commitment hash is a well-formed digest, the timestamp
//!   is plausible, and the record has not been revoked.
//! - **Ownership**: the owner is a valid Stellar public key and, when the caller
//!   supplies `claimed_owner`, matches it.
//! - **Prior art**: earlier commitments with the same or a near-identical hash,
//!   compared by Hamming distance over the 256-bit digest.
//! - **Legal risk**: a heuristic rating built from the findings above.
//!
//! The legal risk rating is decision support for a buyer, not legal advice.

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::schemas::{ErrorResponse, IpRecord};

/// Hamming distance (in bits, out of 256) at or below which two commitments
/// count as near-duplicates.
pub const NEAR_DUPLICATE_MAX_DISTANCE: u32 = 8;
/// Upper bound on prior-art matches returned in one report.
pub const MAX_PRIOR_ART_MATCHES: usize = 10;

const RISK_REVOKED: u32 = 50;
const RISK_MALFORMED_RECORD: u32 = 20;
const RISK_INVALID_OWNER: u32 = 40;
const RISK_OWNER_MISMATCH: u32 = 40;
const RISK_EXACT_DUPLICATE: u32 = 40;
const RISK_NEAR_DUPLICATE_EACH: u32 = 20;
const RISK_NEAR_DUPLICATE_CAP: u32 = 40;
const RISK_HIGH_THRESHOLD: u32 = 40;
const RISK_MEDIUM_THRESHOLD: u32 = 20;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    ExactDuplicate,
    NearDuplicate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorArtMatch {
    pub ip_id: u64,
    pub owner: String,
    pub timestamp: u64,
    /// Hamming distance in bits between the two commitment digests.
    pub distance: u32,
    pub kind: MatchKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegalRiskAssessment {
    pub level: RiskLevel,
    pub score: u32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DueDiligenceReport {
    pub ip_id: u64,
    pub provenance: Vec<Check>,
    pub ownership: Vec<Check>,
    pub prior_art: Vec<PriorArtMatch>,
    pub legal_risk: LegalRiskAssessment,
}

#[derive(Debug, Deserialize)]
pub struct DueDiligenceQuery {
    /// Owner the buyer expects. When set, ownership is checked against it.
    pub claimed_owner: Option<String>,
}

/// Run every due-diligence check for `record` against the commitments in `index`.
///
/// `now` is the current Unix time in seconds; it is a parameter so the
/// timestamp check is deterministic under test.
pub fn run(record: &IpRecord, claimed_owner: Option<&str>, index: &[IpRecord], now: u64) -> DueDiligenceReport {
    let provenance = provenance_checks(record, now);
    let ownership = ownership_checks(record, claimed_owner);
    let prior_art = find_prior_art(record, index);
    let legal_risk = assess_legal_risk(record, &provenance, &ownership, &prior_art);
    DueDiligenceReport {
        ip_id: record.ip_id,
        provenance,
        ownership,
        prior_art,
        legal_risk,
    }
}

fn check(name: &'static str, passed: bool, pass_detail: &str, fail_detail: &str) -> Check {
    Check {
        name,
        passed,
        detail: if passed { pass_detail } else { fail_detail }.to_string(),
    }
}

fn parse_digest(commitment_hash: &str) -> Option<[u8; 32]> {
    hex::decode(commitment_hash)
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
}

fn is_valid_owner(owner: &str) -> bool {
    stellar_strkey::ed25519::PublicKey::from_string(owner).is_ok()
}

fn hamming(left: &[u8; 32], right: &[u8; 32]) -> u32 {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| (a ^ b).count_ones())
        .sum()
}

/// Whether `other` was committed before `subject`. Equal timestamps break on
/// `ip_id` so that exactly one of two records is ever treated as the prior one.
fn is_earlier(other: &IpRecord, subject: &IpRecord) -> bool {
    (other.timestamp, other.ip_id) < (subject.timestamp, subject.ip_id)
}

fn provenance_checks(record: &IpRecord, now: u64) -> Vec<Check> {
    vec![
        check(
            "commitment_hash_format",
            parse_digest(&record.commitment_hash).is_some(),
            "commitment hash is a 32-byte hex digest",
            "commitment hash is not a 32-byte hex digest",
        ),
        check(
            "timestamp_present",
            record.timestamp > 0,
            "record carries a commitment timestamp",
            "record has no commitment timestamp",
        ),
        check(
            "timestamp_not_in_future",
            record.timestamp <= now,
            "commitment timestamp is not in the future",
            "commitment timestamp is in the future",
        ),
        check(
            "not_revoked",
            !record.revoked,
            "record is active",
            "record has been revoked",
        ),
    ]
}

fn ownership_checks(record: &IpRecord, claimed_owner: Option<&str>) -> Vec<Check> {
    let mut checks = vec![check(
        "owner_is_valid_stellar_address",
        is_valid_owner(&record.owner),
        "owner is a valid Stellar public key",
        "owner is not a valid Stellar public key",
    )];
    if let Some(claimed) = claimed_owner {
        checks.push(check(
            "owner_matches_claim",
            claimed == record.owner,
            "recorded owner matches the claimed owner",
            "recorded owner differs from the claimed owner",
        ));
    }
    checks
}

fn find_prior_art(record: &IpRecord, index: &[IpRecord]) -> Vec<PriorArtMatch> {
    let Some(digest) = parse_digest(&record.commitment_hash) else {
        return Vec::new();
    };
    let mut matches: Vec<PriorArtMatch> = index
        .iter()
        .filter(|other| other.ip_id != record.ip_id && is_earlier(other, record))
        .filter_map(|other| {
            let other_digest = parse_digest(&other.commitment_hash)?;
            let distance = hamming(&digest, &other_digest);
            let kind = if distance == 0 {
                MatchKind::ExactDuplicate
            } else if distance <= NEAR_DUPLICATE_MAX_DISTANCE {
                MatchKind::NearDuplicate
            } else {
                return None;
            };
            Some(PriorArtMatch {
                ip_id: other.ip_id,
                owner: other.owner.clone(),
                timestamp: other.timestamp,
                distance,
                kind,
            })
        })
        .collect();
    matches.sort_by_key(|found| (found.distance, found.timestamp, found.ip_id));
    matches.truncate(MAX_PRIOR_ART_MATCHES);
    matches
}

fn assess_legal_risk(
    record: &IpRecord,
    provenance: &[Check],
    ownership: &[Check],
    prior_art: &[PriorArtMatch],
) -> LegalRiskAssessment {
    let mut score = 0;
    let mut reasons = Vec::new();

    if record.revoked {
        score += RISK_REVOKED;
        reasons.push("record has been revoked".to_string());
    }
    if provenance.iter().any(|item| !item.passed && item.name != "not_revoked") {
        score += RISK_MALFORMED_RECORD;
        reasons.push("record provenance is incomplete or malformed".to_string());
    }
    if ownership
        .iter()
        .any(|item| !item.passed && item.name == "owner_is_valid_stellar_address")
    {
        score += RISK_INVALID_OWNER;
        reasons.push("owner is not a valid Stellar address".to_string());
    }
    if ownership
        .iter()
        .any(|item| !item.passed && item.name == "owner_matches_claim")
    {
        score += RISK_OWNER_MISMATCH;
        reasons.push("recorded owner differs from the claimed owner".to_string());
    }
    if prior_art.iter().any(|found| found.kind == MatchKind::ExactDuplicate) {
        score += RISK_EXACT_DUPLICATE;
        reasons.push("an identical commitment was registered earlier".to_string());
    }
    let near_duplicates = prior_art
        .iter()
        .filter(|found| found.kind == MatchKind::NearDuplicate)
        .count() as u32;
    if near_duplicates > 0 {
        score += (near_duplicates * RISK_NEAR_DUPLICATE_EACH).min(RISK_NEAR_DUPLICATE_CAP);
        reasons.push(format!(
            "{near_duplicates} near-duplicate commitment(s) were registered earlier"
        ));
    }
    if reasons.is_empty() {
        reasons.push("no risk factors identified".to_string());
    }

    let level = if score >= RISK_HIGH_THRESHOLD {
        RiskLevel::High
    } else if score >= RISK_MEDIUM_THRESHOLD {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    LegalRiskAssessment { level, score, reasons }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Due-diligence report for one indexed IP commitment.
pub async fn get_due_diligence(
    Path(ip_id): Path<u64>,
    Query(query): Query<DueDiligenceQuery>,
) -> Result<Json<DueDiligenceReport>, (StatusCode, Json<ErrorResponse>)> {
    let record = crate::handlers::indexed_record(ip_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("IP record {} not found", ip_id),
            }),
        )
    })?;
    let claimed_owner = query
        .claimed_owner
        .as_deref()
        .filter(|owner| !owner.trim().is_empty());
    let index = crate::handlers::indexed_records();
    Ok(Json(run(&record, claimed_owner, &index, now_secs())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 2_000_000_000;

    fn stellar_key(seed: u8) -> String {
        stellar_strkey::ed25519::PublicKey([seed; 32]).to_string()
    }

    fn digest(bytes: [u8; 32]) -> String {
        hex::encode(bytes)
    }

    fn record(ip_id: u64, bytes: [u8; 32], timestamp: u64) -> IpRecord {
        IpRecord {
            ip_id,
            owner: stellar_key(1),
            commitment_hash: digest(bytes),
            timestamp,
            revoked: false,
        }
    }

    fn has_failed(checks: &[Check], name: &str) -> bool {
        checks.iter().any(|item| item.name == name && !item.passed)
    }

    #[test]
    fn clean_record_is_low_risk() {
        let subject = record(1, [0xAA; 32], 100);
        let report = run(&subject, Some(&stellar_key(1)), &[], NOW);
        assert_eq!(report.legal_risk.level, RiskLevel::Low);
        assert_eq!(report.legal_risk.score, 0);
        assert!(report.provenance.iter().all(|item| item.passed));
        assert!(report.ownership.iter().all(|item| item.passed));
        assert!(report.prior_art.is_empty());
    }

    #[test]
    fn revoked_record_is_high_risk() {
        let mut subject = record(1, [0xAA; 32], 100);
        subject.revoked = true;
        let report = run(&subject, None, &[], NOW);
        assert!(has_failed(&report.provenance, "not_revoked"));
        assert_eq!(report.legal_risk.level, RiskLevel::High);
    }

    #[test]
    fn earlier_identical_commitment_is_exact_prior_art() {
        let subject = record(2, [0xAA; 32], 100);
        let index = vec![record(1, [0xAA; 32], 50)];
        let report = run(&subject, None, &index, NOW);
        assert_eq!(report.prior_art.len(), 1);
        assert_eq!(report.prior_art[0].kind, MatchKind::ExactDuplicate);
        assert_eq!(report.prior_art[0].distance, 0);
        assert_eq!(report.legal_risk.level, RiskLevel::High);
    }

    #[test]
    fn earlier_near_identical_commitment_is_medium_risk() {
        let mut near = [0xAA; 32];
        near[0] ^= 0b1;
        let subject = record(2, [0xAA; 32], 100);
        let index = vec![record(1, near, 50)];
        let report = run(&subject, None, &index, NOW);
        assert_eq!(report.prior_art.len(), 1);
        assert_eq!(report.prior_art[0].kind, MatchKind::NearDuplicate);
        assert_eq!(report.prior_art[0].distance, 1);
        assert_eq!(report.legal_risk.level, RiskLevel::Medium);
    }

    #[test]
    fn later_duplicate_is_not_prior_art() {
        let subject = record(1, [0xAA; 32], 100);
        let index = vec![record(2, [0xAA; 32], 200)];
        let report = run(&subject, None, &index, NOW);
        assert!(report.prior_art.is_empty());
        assert_eq!(report.legal_risk.level, RiskLevel::Low);
    }

    #[test]
    fn record_is_never_its_own_prior_art() {
        let subject = record(1, [0xAA; 32], 100);
        let index = vec![subject.clone()];
        let report = run(&subject, None, &index, NOW);
        assert!(report.prior_art.is_empty());
    }

    #[test]
    fn distance_beyond_threshold_is_ignored() {
        let mut far = [0xAA; 32];
        far[0] ^= 0xFF; // 8 bits
        far[1] ^= 0b10; // 9 bits total, past NEAR_DUPLICATE_MAX_DISTANCE
        let subject = record(2, [0xAA; 32], 100);
        let index = vec![record(1, far, 50)];
        let report = run(&subject, None, &index, NOW);
        assert!(report.prior_art.is_empty());
    }

    #[test]
    fn claimed_owner_mismatch_is_high_risk() {
        let subject = record(1, [0xAA; 32], 100);
        let report = run(&subject, Some(&stellar_key(9)), &[], NOW);
        assert!(has_failed(&report.ownership, "owner_matches_claim"));
        assert_eq!(report.legal_risk.level, RiskLevel::High);
    }

    #[test]
    fn invalid_owner_is_flagged() {
        let mut subject = record(1, [0xAA; 32], 100);
        subject.owner = "not-a-stellar-key".to_string();
        let report = run(&subject, None, &[], NOW);
        assert!(has_failed(&report.ownership, "owner_is_valid_stellar_address"));
        assert_eq!(report.legal_risk.level, RiskLevel::High);
    }

    #[test]
    fn malformed_hash_and_future_timestamp_are_medium_risk() {
        let mut subject = record(1, [0xAA; 32], NOW + 10);
        subject.commitment_hash = "zz".to_string();
        let report = run(&subject, None, &[], NOW);
        assert!(has_failed(&report.provenance, "commitment_hash_format"));
        assert!(has_failed(&report.provenance, "timestamp_not_in_future"));
        assert_eq!(report.legal_risk.level, RiskLevel::Medium);
    }
}
