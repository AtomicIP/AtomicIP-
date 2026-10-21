//! GDPR-style erasure of a user's off-chain data (#1107).
//!
//! `DELETE /users/{user_id}` removes everything this API holds about a user:
//! the commitment index entries and tags for IPs they own (cascading to every
//! watchlist that referenced them), their own watchlist, their 2FA
//! configuration and sessions, and personal data in the audit log.
//!
//! Commitments recorded on-chain are immutable and cannot be deleted by the
//! API. The report says so explicitly via `onchain_commitments_retained`.

use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::collections::BTreeSet;
use tracing::instrument;
use utoipa::ToSchema;

use crate::audit::{AuditEventCategory, AuditEventInput};
use crate::cache;
use crate::commitments;
use crate::handlers::{AUDIT_LOG_STORE, COMMITMENT_INDEX, SESSION_STORE, TWO_FACTOR_STORE, WATCHLISTS};
use crate::schemas::ErrorResponse;

/// Event type recorded in the audit log once an erasure completes. It carries
/// counts only, never the erased user's identifier.
const ERASURE_EVENT_TYPE: &str = "user.erased";

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
pub struct UserErasureReport {
    pub user_id: String,
    /// Commitments owned by the user that were removed from the off-chain index.
    pub commitments_removed: usize,
    /// Watchlist entries removed: the user's own list plus references to their IPs.
    pub watchlist_entries_removed: usize,
    pub tags_removed: usize,
    /// Cache entries invalidated for the user's IP records.
    pub cache_entries_invalidated: usize,
    pub two_factor_configs_removed: usize,
    pub sessions_removed: usize,
    /// Audit events whose personal data was redacted.
    pub audit_events_anonymized: usize,
    /// Number of references still found by the post-erasure scan. Zero means
    /// the erasure is complete.
    pub remaining_references: usize,
    pub verified: bool,
    /// Always `true`: on-chain commitments cannot be deleted through the API.
    pub onchain_commitments_retained: bool,
}

/// Removes all off-chain data for `user_id` and verifies that nothing remains.
pub async fn erase_user_data(user_id: &str) -> Result<UserErasureReport, String> {
    // Collect owned IPs first so the cascade has a fixed set to work from.
    let owned_ids: BTreeSet<u64> = COMMITMENT_INDEX
        .iter()
        .filter(|entry| entry.value().owner == user_id)
        .map(|entry| *entry.key())
        .collect();

    for ip_id in &owned_ids {
        COMMITMENT_INDEX.remove(ip_id);
    }

    let mut tags_removed = 0;
    let mut cache_entries_invalidated = 0;
    for ip_id in &owned_ids {
        if commitments::remove_tags(*ip_id) {
            tags_removed += 1;
        }
        cache::invalidate(&cache::ip_key(*ip_id));
        cache_entries_invalidated += 1;
    }

    let watchlist_entries_removed = {
        let mut watchlists = WATCHLISTS
            .lock()
            .map_err(|_| "watchlist store unavailable".to_string())?;
        let mut removed = 0;
        if let Some(own) = watchlists.remove(user_id) {
            removed += own.len();
        }
        for ids in watchlists.values_mut() {
            let before = ids.len();
            ids.retain(|ip_id| !owned_ids.contains(ip_id));
            removed += before - ids.len();
        }
        removed
    };

    let two_factor_configs_removed = usize::from(
        TWO_FACTOR_STORE
            .remove_config(user_id)
            .map_err(|e| format!("2FA store unavailable: {e}"))?,
    );

    let sessions_removed = SESSION_STORE
        .remove_user_sessions(user_id)
        .map_err(|e| format!("session store unavailable: {e}"))?;

    let actor_hash = AUDIT_LOG_STORE.hash_identifier(user_id);
    let audit_events_anonymized = AUDIT_LOG_STORE
        .anonymize_actor(&actor_hash)
        .await
        .map_err(|e| format!("audit log anonymization failed: {e}"))?;

    let remaining_references = count_remaining_references(user_id, &actor_hash, &owned_ids).await?;

    let report = UserErasureReport {
        user_id: user_id.to_string(),
        commitments_removed: owned_ids.len(),
        watchlist_entries_removed,
        tags_removed,
        cache_entries_invalidated,
        two_factor_configs_removed,
        sessions_removed,
        audit_events_anonymized,
        remaining_references,
        verified: remaining_references == 0,
        onchain_commitments_retained: true,
    };

    AUDIT_LOG_STORE
        .append(
            AuditEventInput::new(AuditEventCategory::Admin, ERASURE_EVENT_TYPE)
                .detail("commitments_removed", report.commitments_removed)
                .detail("audit_events_anonymized", report.audit_events_anonymized)
                .detail("verified", report.verified),
        )
        .await
        .map_err(|e| format!("failed to record erasure in audit log: {e}"))?;

    Ok(report)
}

/// Re-scans every store for anything still attributable to `user_id`.
async fn count_remaining_references(
    user_id: &str,
    actor_hash: &str,
    owned_ids: &BTreeSet<u64>,
) -> Result<usize, String> {
    let mut remaining = 0;

    remaining += COMMITMENT_INDEX
        .iter()
        .filter(|entry| entry.value().owner == user_id)
        .count();

    remaining += owned_ids
        .iter()
        .filter(|ip_id| !commitments::get_tags(**ip_id).is_empty())
        .count();

    {
        let watchlists = WATCHLISTS
            .lock()
            .map_err(|_| "watchlist store unavailable".to_string())?;
        if watchlists.contains_key(user_id) {
            remaining += 1;
        }
        remaining += watchlists
            .values()
            .map(|ids| ids.iter().filter(|ip_id| owned_ids.contains(*ip_id)).count())
            .sum::<usize>();
    }

    remaining += usize::from(
        TWO_FACTOR_STORE
            .get_config(user_id)
            .map_err(|e| format!("2FA store unavailable: {e}"))?
            .is_some(),
    );
    remaining += SESSION_STORE
        .count_user_sessions(user_id)
        .map_err(|e| format!("session store unavailable: {e}"))?;
    remaining += AUDIT_LOG_STORE.count_actor_events(actor_hash).await;

    Ok(remaining)
}

/// Erase all off-chain data held for a user.
#[utoipa::path(
    delete,
    path = "/users/{user_id}",
    tag = "Users",
    params(("user_id" = String, Path, description = "User identifier to erase")),
    responses(
        (status = 200, description = "User data erased", body = UserErasureReport),
        (status = 400, description = "Invalid user ID", body = ErrorResponse),
        (status = 500, description = "Erasure failed", body = ErrorResponse),
    )
)]
#[instrument]
pub async fn delete_user_handler(
    Path(user_id): Path<String>,
) -> Result<Json<UserErasureReport>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "user_id must not be empty".to_string() }),
        ));
    }

    let report = erase_user_data(user_id).await.map_err(|error| {
        tracing::error!(error = %error, "user data erasure failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "user data erasure failed".to_string() }),
        )
    })?;

    if !report.verified {
        tracing::error!(remaining = report.remaining_references, "user data erasure incomplete");
    }

    Ok(Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::IpRecord;

    fn unique(label: &str) -> String {
        format!("erase-test-{label}-{}", uuid::Uuid::new_v4())
    }

    fn next_ip_id() -> u64 {
        uuid::Uuid::new_v4().as_u128() as u64
    }

    fn register_commitment_for_tests(owner: &str) -> u64 {
        let ip_id = next_ip_id();
        COMMITMENT_INDEX.insert(
            ip_id,
            IpRecord {
                ip_id,
                owner: owner.to_string(),
                commitment_hash: "ab".repeat(32),
                timestamp: 0,
                revoked: false,
            },
        );
        ip_id
    }

    fn add_to_watchlist_for_tests(user_id: &str, ip_id: u64) {
        WATCHLISTS
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_default()
            .insert(ip_id);
    }

    #[tokio::test]
    async fn erasure_cascades_to_owned_commitments_and_watchlists() {
        let owner = unique("owner");
        let viewer = unique("viewer");
        let owned_ip = register_commitment_for_tests(&owner);
        let other_ip = register_commitment_for_tests(&unique("other-owner"));
        add_to_watchlist_for_tests(&viewer, owned_ip);
        add_to_watchlist_for_tests(&viewer, other_ip);
        add_to_watchlist_for_tests(&owner, other_ip);
        commitments::set_tags(owned_ip, vec!["patent".into()]).unwrap();

        let report = erase_user_data(&owner).await.unwrap();

        assert_eq!(report.commitments_removed, 1);
        assert_eq!(report.tags_removed, 1);
        assert!(report.watchlist_entries_removed >= 2);
        assert!(report.verified, "remaining references: {}", report.remaining_references);
        assert!(report.onchain_commitments_retained);

        assert!(!COMMITMENT_INDEX.contains_key(&owned_ip));
        assert!(commitments::get_tags(owned_ip).is_empty());
        let watchlists = WATCHLISTS.lock().unwrap();
        assert!(!watchlists.get(&viewer).map_or(false, |ids| ids.contains(&owned_ip)));
        assert!(watchlists.get(&viewer).map_or(false, |ids| ids.contains(&other_ip)));
        assert!(!watchlists.contains_key(&owner));
    }

    #[tokio::test]
    async fn erasure_is_idempotent_for_unknown_user() {
        let report = erase_user_data(&unique("unknown")).await.unwrap();

        assert_eq!(report.commitments_removed, 0);
        assert_eq!(report.remaining_references, 0);
        assert!(report.verified);
    }

    #[tokio::test]
    async fn handler_rejects_blank_user_id() {
        let result = delete_user_handler(Path("   ".to_string())).await;

        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
