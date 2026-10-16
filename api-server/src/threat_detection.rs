//! Threat detection for suspicious activity (#1108).
//!
//! Three sliding-window detectors run over request activity:
//!
//! * **Rapid-fire commitments** — one owner registers `rapid_commit_threshold`
//!   commitments inside `window_secs`.
//! * **Unusual access** — one actor touches `unusual_access_threshold` distinct
//!   IP records inside `window_secs` (enumeration / scraping).
//! * **Suspicious verification chains** — `failed_verification_threshold`
//!   failed verifications against the same IP inside `window_secs`
//!   (guessing a commitment's secret or blinding factor).
//!
//! Each detector fires once when its threshold is crossed, then re-arms after
//! the window drains, so a sustained attack produces one alert per burst
//! rather than one per request. Time is passed in explicitly so the logic is
//! deterministic under test.

use crate::alerting::{Alert, Severity, ALERT_MANAGER};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatKind {
    RapidCommitments,
    UnusualAccess,
    SuspiciousVerification,
}

impl ThreatKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RapidCommitments => "rapid_commitments",
            Self::UnusualAccess => "unusual_access",
            Self::SuspiciousVerification => "suspicious_verification",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreatAlert {
    pub kind: ThreatKind,
    /// Owner address for commit/access threats, `ip:<id>` for verification threats.
    pub subject: String,
    pub count: usize,
    pub window_secs: u64,
    pub detected_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreatConfig {
    pub window_secs: u64,
    pub rapid_commit_threshold: usize,
    pub unusual_access_threshold: usize,
    pub failed_verification_threshold: usize,
}

impl Default for ThreatConfig {
    fn default() -> Self {
        Self {
            window_secs: 60,
            rapid_commit_threshold: 5,
            unusual_access_threshold: 50,
            failed_verification_threshold: 5,
        }
    }
}

/// Process-wide detector used by the request handlers.
pub static THREAT_DETECTOR: Lazy<ThreatDetector> =
    Lazy::new(|| ThreatDetector::new(ThreatConfig::default()));

pub struct ThreatDetector {
    config: ThreatConfig,
    commits: Mutex<HashMap<String, VecDeque<u64>>>,
    accesses: Mutex<HashMap<String, VecDeque<(u64, u64)>>>,
    failed_verifications: Mutex<HashMap<u64, VecDeque<u64>>>,
}

/// Drops timestamps that have aged out of the window ending at `now`.
fn prune_timestamps(entries: &mut VecDeque<u64>, now: u64, window_secs: u64) {
    let cutoff = now.saturating_sub(window_secs);
    while entries.front().is_some_and(|&ts| ts < cutoff) {
        entries.pop_front();
    }
}

impl ThreatDetector {
    pub fn new(config: ThreatConfig) -> Self {
        Self {
            config,
            commits: Mutex::new(HashMap::new()),
            accesses: Mutex::new(HashMap::new()),
            failed_verifications: Mutex::new(HashMap::new()),
        }
    }

    /// Records a commitment registration by `owner`.
    pub fn record_commit(&self, owner: &str, now: u64) -> Option<ThreatAlert> {
        let mut commits = self.commits.lock().ok()?;
        let entries = commits.entry(owner.to_string()).or_default();
        prune_timestamps(entries, now, self.config.window_secs);
        entries.push_back(now);

        if entries.len() == self.config.rapid_commit_threshold {
            return Some(ThreatAlert {
                kind: ThreatKind::RapidCommitments,
                subject: owner.to_string(),
                count: entries.len(),
                window_secs: self.config.window_secs,
                detected_at: now,
            });
        }
        None
    }

    /// Records that `actor` accessed IP record `ip_id`.
    pub fn record_access(&self, actor: &str, ip_id: u64, now: u64) -> Option<ThreatAlert> {
        let mut accesses = self.accesses.lock().ok()?;
        let entries = accesses.entry(actor.to_string()).or_default();
        let cutoff = now.saturating_sub(self.config.window_secs);
        while entries.front().is_some_and(|&(ts, _)| ts < cutoff) {
            entries.pop_front();
        }
        entries.push_back((now, ip_id));

        let distinct: HashSet<u64> = entries.iter().map(|&(_, id)| id).collect();
        if distinct.len() == self.config.unusual_access_threshold {
            return Some(ThreatAlert {
                kind: ThreatKind::UnusualAccess,
                subject: actor.to_string(),
                count: distinct.len(),
                window_secs: self.config.window_secs,
                detected_at: now,
            });
        }
        None
    }

    /// Records the outcome of verifying IP record `ip_id`. Successful
    /// verifications are not tracked; only failures count toward the chain.
    pub fn record_verification(&self, ip_id: u64, valid: bool, now: u64) -> Option<ThreatAlert> {
        if valid {
            return None;
        }

        let mut failures = self.failed_verifications.lock().ok()?;
        let entries = failures.entry(ip_id).or_default();
        prune_timestamps(entries, now, self.config.window_secs);
        entries.push_back(now);

        if entries.len() == self.config.failed_verification_threshold {
            return Some(ThreatAlert {
                kind: ThreatKind::SuspiciousVerification,
                subject: format!("ip:{ip_id}"),
                count: entries.len(),
                window_secs: self.config.window_secs,
                detected_at: now,
            });
        }
        None
    }
}

/// Reports a detected threat to logs and the alert manager.
pub fn dispatch(alert: &ThreatAlert) {
    tracing::warn!(
        kind = alert.kind.as_str(),
        subject = %alert.subject,
        count = alert.count,
        window_secs = alert.window_secs,
        "threat detected"
    );

    let summary = format!(
        "{} for {}: {} events within {}s",
        alert.kind.as_str(),
        alert.subject,
        alert.count,
        alert.window_secs
    );
    let _ = ALERT_MANAGER.submit(
        Alert::new(
            format!("threat_{}", alert.kind.as_str()),
            Severity::Critical,
            summary,
        )
        .with_label("subject", alert.subject.clone()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> ThreatDetector {
        ThreatDetector::new(ThreatConfig {
            window_secs: 60,
            rapid_commit_threshold: 3,
            unusual_access_threshold: 4,
            failed_verification_threshold: 3,
        })
    }

    #[test]
    fn rapid_commits_alert_when_threshold_reached_within_window() {
        let d = detector();
        assert!(d.record_commit("GOWNER", 100).is_none());
        assert!(d.record_commit("GOWNER", 101).is_none());

        let alert = d.record_commit("GOWNER", 102).expect("third commit in window alerts");
        assert_eq!(alert.kind, ThreatKind::RapidCommitments);
        assert_eq!(alert.subject, "GOWNER");
        assert_eq!(alert.count, 3);
    }

    #[test]
    fn spaced_commits_do_not_alert() {
        let d = detector();
        assert!(d.record_commit("GOWNER", 0).is_none());
        assert!(d.record_commit("GOWNER", 100).is_none());
        assert!(d.record_commit("GOWNER", 200).is_none());
    }

    #[test]
    fn commit_alert_fires_once_per_burst_then_rearms_after_window() {
        let d = detector();
        d.record_commit("A", 0);
        d.record_commit("A", 1);
        assert!(d.record_commit("A", 2).is_some());
        assert!(d.record_commit("A", 3).is_none(), "no alert per extra request");

        // Old events age out, so a fresh burst alerts again.
        assert!(d.record_commit("A", 500).is_none());
        assert!(d.record_commit("A", 501).is_none());
        assert!(d.record_commit("A", 502).is_some());
    }

    #[test]
    fn commit_counts_are_tracked_per_owner() {
        let d = detector();
        d.record_commit("A", 0);
        d.record_commit("B", 0);
        assert!(d.record_commit("A", 1).is_none());
        assert!(d.record_commit("B", 1).is_none());
    }

    #[test]
    fn unusual_access_counts_distinct_ips_not_repeat_requests() {
        let d = detector();
        for _ in 0..10 {
            assert!(d.record_access("scraper", 7, 0).is_none());
        }
        d.record_access("scraper", 8, 1);
        d.record_access("scraper", 9, 2);
        let alert = d.record_access("scraper", 10, 3).expect("fourth distinct IP alerts");
        assert_eq!(alert.kind, ThreatKind::UnusualAccess);
        assert_eq!(alert.count, 4);
    }

    #[test]
    fn unusual_access_forgets_ips_outside_the_window() {
        let d = detector();
        d.record_access("scraper", 1, 0);
        d.record_access("scraper", 2, 0);
        d.record_access("scraper", 3, 0);
        // Only the window-recent accesses should count.
        assert!(d.record_access("scraper", 4, 1000).is_none());
    }

    #[test]
    fn failed_verifications_alert_on_repeated_failures_for_same_ip() {
        let d = detector();
        assert!(d.record_verification(42, false, 0).is_none());
        assert!(d.record_verification(42, false, 1).is_none());
        let alert = d.record_verification(42, false, 2).expect("third failure alerts");
        assert_eq!(alert.kind, ThreatKind::SuspiciousVerification);
        assert_eq!(alert.subject, "ip:42");
    }

    #[test]
    fn successful_verifications_never_count_as_failures() {
        let d = detector();
        for t in 0..20 {
            assert!(d.record_verification(42, true, t).is_none());
        }
    }

    #[test]
    fn failures_are_isolated_per_ip() {
        let d = detector();
        d.record_verification(1, false, 0);
        d.record_verification(2, false, 0);
        assert!(d.record_verification(1, false, 1).is_none());
        assert!(d.record_verification(2, false, 1).is_none());
    }

    #[test]
    fn default_config_matches_documented_thresholds() {
        let config = ThreatConfig::default();
        assert_eq!(config.window_secs, 60);
        assert_eq!(config.rapid_commit_threshold, 5);
        assert_eq!(config.unusual_access_threshold, 50);
        assert_eq!(config.failed_verification_threshold, 5);
    }
}
