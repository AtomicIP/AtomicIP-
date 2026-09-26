//! #1064: Commitment lifecycle monitoring.
//!
//! Tracks IP commitments through their lifecycle and exposes:
//!
//! * **Per-state gauges** – `commitments_by_state{state="active|revealed|archived"}`.
//! * **Transition counters and rates** –
//!   `commitment_transitions_total{from,to}` plus an in-process sliding window
//!   used to compute per-minute transition rates.
//! * **Anomaly alerts** – z-score based spike / drop detection on the
//!   per-bucket transition counts, forwarded to [`crate::alerting`] so they get
//!   deduplicated, correlated and escalated like every other alert.
//! * **Trend analysis** – least-squares slope over recent buckets for each
//!   transition, exposed as `commitment_transition_trend{from,to}`.
//!
//! Lifecycle:
//!
//! ```text
//!   commit_ip / batch_commit_ip        unlock_commitment / reveal_partial
//!  ─────────────────────────▶ Active ───────────────────────────────▶ Revealed
//!                               │                                        │
//!                               │ revoke_ip / expiry                     │ revoke_ip / expiry
//!                               ▼                                        ▼
//!                            Archived ◀──────────────────────────────────┘
//! ```
//!
//! Dashboards and Prometheus alert rules live in `monitoring/`.
//!
//! Design rationale: docs/adr/0004-alert-fatigue-reduction-pipeline.md

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use metrics::{counter, gauge};
use once_cell::sync::Lazy;
use serde::Serialize;

use crate::alerting::{self, Alert, Severity};

/// Lifecycle state of a commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitmentState {
    /// Newly created (the implicit "from" state of a commit).
    New,
    Active,
    Revealed,
    Archived,
}

impl CommitmentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommitmentState::New => "new",
            CommitmentState::Active => "active",
            CommitmentState::Revealed => "revealed",
            CommitmentState::Archived => "archived",
        }
    }

    /// Whether `self -> to` is a legal lifecycle edge.
    pub fn can_transition_to(&self, to: CommitmentState) -> bool {
        use CommitmentState::*;
        matches!(
            (self, to),
            (New, Active) | (Active, Revealed) | (Active, Archived) | (Revealed, Archived)
        )
    }
}

const TRACKED_STATES: [CommitmentState; 3] = [
    CommitmentState::Active,
    CommitmentState::Revealed,
    CommitmentState::Archived,
];

type Transition = (CommitmentState, CommitmentState);

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Width of one aggregation bucket, in seconds.
    pub bucket_secs: u64,
    /// Number of buckets retained for rate, anomaly and trend computation.
    pub history_buckets: usize,
    /// Minimum number of completed buckets before anomaly detection runs.
    pub min_baseline_buckets: usize,
    /// |z-score| above which a bucket is considered anomalous.
    pub z_threshold: f64,
    /// Absolute floor so tiny baselines (e.g. 0,0,0,1) do not page.
    pub min_spike_count: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            bucket_secs: 60,
            history_buckets: 60,
            min_baseline_buckets: 10,
            z_threshold: 3.0,
            min_spike_count: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TransitionStats {
    pub from: CommitmentState,
    pub to: CommitmentState,
    pub total: u64,
    pub rate_per_minute: f64,
    /// Change in per-bucket count per bucket (least-squares slope).
    pub trend_slope: f64,
    pub trend: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct LifecycleSnapshot {
    pub counts: HashMap<&'static str, i64>,
    pub transitions: Vec<TransitionStats>,
    pub invalid_transitions: u64,
}

#[derive(Default)]
struct Series {
    total: u64,
    /// (bucket_start, count), oldest first.
    buckets: VecDeque<(u64, u64)>,
}

#[derive(Default)]
struct Inner {
    counts: HashMap<CommitmentState, i64>,
    series: HashMap<Transition, Series>,
    invalid_transitions: u64,
}

pub struct CommitmentLifecycleMonitor {
    config: MonitorConfig,
    inner: Mutex<Inner>,
}

impl CommitmentLifecycleMonitor {
    pub fn new(config: MonitorConfig) -> Self {
        Self { config, inner: Mutex::new(Inner::default()) }
    }

    /// Record a new commitment entering the Active state.
    pub fn record_commit(&self, count: u64) {
        for _ in 0..count {
            self.record_transition(CommitmentState::New, CommitmentState::Active);
        }
    }

    pub fn record_transition(&self, from: CommitmentState, to: CommitmentState) {
        self.record_transition_at(from, to, alerting::now_secs());
    }

    /// Record a lifecycle transition at an explicit timestamp (seconds).
    pub fn record_transition_at(&self, from: CommitmentState, to: CommitmentState, now: u64) {
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;

        if !from.can_transition_to(to) {
            inner.invalid_transitions += 1;
            counter!("commitment_invalid_transitions_total", "from" => from.as_str(), "to" => to.as_str())
                .increment(1);
            alerting::ALERT_MANAGER.submit(
                Alert::new(
                    "CommitmentInvalidTransition",
                    Severity::Warning,
                    format!("Illegal commitment transition {} -> {}", from.as_str(), to.as_str()),
                )
                .with_label("from", from.as_str())
                .with_label("to", to.as_str()),
            );
            return;
        }

        if from != CommitmentState::New {
            *inner.counts.entry(from).or_default() -= 1;
        }
        *inner.counts.entry(to).or_default() += 1;
        for s in TRACKED_STATES {
            gauge!("commitments_by_state", "state" => s.as_str())
                .set(*inner.counts.get(&s).unwrap_or(&0) as f64);
        }
        counter!("commitment_transitions_total", "from" => from.as_str(), "to" => to.as_str())
            .increment(1);

        let bucket = now - now % self.config.bucket_secs;
        let series = inner.series.entry((from, to)).or_default();
        series.total += 1;
        match series.buckets.back_mut() {
            Some((start, c)) if *start == bucket => *c += 1,
            _ => series.buckets.push_back((bucket, 1)),
        }
        while series.buckets.len() > self.config.history_buckets {
            series.buckets.pop_front();
        }
    }

    /// Evaluate anomaly detection and update trend gauges. Call periodically
    /// (once per bucket is sufficient) from a background task.
    pub fn evaluate(&self, now: u64) -> Vec<Alert> {
        let inner = self.inner.lock().unwrap();
        let current_bucket = now - now % self.config.bucket_secs;
        let mut alerts = Vec::new();

        for (&(from, to), series) in inner.series.iter() {
            let counts = densify(&series.buckets, current_bucket, self.config.bucket_secs, self.config.history_buckets);
            let slope = trend_slope(&counts);
            gauge!("commitment_transition_trend", "from" => from.as_str(), "to" => to.as_str()).set(slope);
            gauge!("commitment_transition_rate_per_minute", "from" => from.as_str(), "to" => to.as_str())
                .set(rate_per_minute(&counts, self.config.bucket_secs));

            // Compare the most recent *completed* bucket against the baseline
            // formed by the buckets before it.
            if counts.len() < self.config.min_baseline_buckets + 2 {
                continue;
            }
            let latest = counts[counts.len() - 2];
            let baseline = &counts[..counts.len() - 2];
            let (mean, stddev) = mean_stddev(baseline);
            let z = if stddev > 0.0 { (latest as f64 - mean) / stddev } else { 0.0 };
            let transition = format!("{}->{}", from.as_str(), to.as_str());

            if z >= self.config.z_threshold && latest >= self.config.min_spike_count {
                let name = if from == CommitmentState::New { "CommitmentSpike" } else { "CommitmentTransitionSpike" };
                alerts.push(
                    Alert::new(name, Severity::Warning, format!(
                        "{transition}: {latest} in last bucket vs baseline {mean:.1}±{stddev:.1} (z={z:.1})"
                    ))
                    .with_label("transition", transition.clone()),
                );
            } else if z <= -self.config.z_threshold && mean >= self.config.min_spike_count as f64 {
                let name = if from == CommitmentState::New { "CommitmentRateDrop" } else { "CommitmentTransitionDrop" };
                alerts.push(
                    Alert::new(name, Severity::Warning, format!(
                        "{transition}: {latest} in last bucket vs baseline {mean:.1}±{stddev:.1} (z={z:.1})"
                    ))
                    .with_label("transition", transition.clone()),
                );
            }
        }

        // Revealed/archived counts must never go negative; a negative value
        // means events were missed or double-counted by the indexer.
        for s in TRACKED_STATES {
            if *inner.counts.get(&s).unwrap_or(&0) < 0 {
                alerts.push(
                    Alert::new("CommitmentStateDrift", Severity::Critical, format!(
                        "commitments_by_state{{state=\"{}\"}} is negative; lifecycle events were lost",
                        s.as_str()
                    ))
                    .with_label("state", s.as_str()),
                );
            }
        }
        drop(inner);

        for a in &alerts {
            alerting::ALERT_MANAGER.submit_at(a.clone(), now);
        }
        alerts
    }

    pub fn snapshot(&self, now: u64) -> LifecycleSnapshot {
        let inner = self.inner.lock().unwrap();
        let current_bucket = now - now % self.config.bucket_secs;
        let transitions = inner
            .series
            .iter()
            .map(|(&(from, to), s)| {
                let counts = densify(&s.buckets, current_bucket, self.config.bucket_secs, self.config.history_buckets);
                let slope = trend_slope(&counts);
                TransitionStats {
                    from,
                    to,
                    total: s.total,
                    rate_per_minute: rate_per_minute(&counts, self.config.bucket_secs),
                    trend_slope: slope,
                    trend: classify_trend(slope, &counts),
                }
            })
            .collect();
        LifecycleSnapshot {
            counts: TRACKED_STATES
                .iter()
                .map(|s| (s.as_str(), *inner.counts.get(s).unwrap_or(&0)))
                .collect(),
            transitions,
            invalid_transitions: inner.invalid_transitions,
        }
    }
}

/// Expand sparse buckets into a dense, oldest-first vector ending at `current`.
fn densify(buckets: &VecDeque<(u64, u64)>, current: u64, width: u64, len: usize) -> Vec<u64> {
    let start = current.saturating_sub(width * (len as u64 - 1));
    let mut out = vec![0u64; len];
    for &(b, c) in buckets {
        if b >= start && b <= current {
            out[((b - start) / width) as usize] = c;
        }
    }
    out
}

/// Average per-minute rate over the last 5 completed buckets.
fn rate_per_minute(counts: &[u64], bucket_secs: u64) -> f64 {
    if counts.len() < 2 {
        return 0.0;
    }
    let completed = &counts[..counts.len() - 1];
    let window = &completed[completed.len().saturating_sub(5)..];
    let sum: u64 = window.iter().sum();
    let secs = (window.len() as u64 * bucket_secs) as f64;
    if secs == 0.0 { 0.0 } else { sum as f64 * 60.0 / secs }
}

fn mean_stddev(xs: &[u64]) -> (f64, f64) {
    let n = xs.len() as f64;
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let mean = xs.iter().sum::<u64>() as f64 / n;
    let var = xs.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n;
    (mean, var.sqrt())
}

/// Ordinary least-squares slope of counts against bucket index.
fn trend_slope(ys: &[u64]) -> f64 {
    let n = ys.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = ys.iter().sum::<u64>() as f64 / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, &y) in ys.iter().enumerate() {
        let dx = i as f64 - mean_x;
        num += dx * (y as f64 - mean_y);
        den += dx * dx;
    }
    if den == 0.0 { 0.0 } else { num / den }
}

/// Classify a slope relative to the series mean (±5% of mean per bucket).
fn classify_trend(slope: f64, counts: &[u64]) -> &'static str {
    let (mean, _) = mean_stddev(counts);
    let threshold = (mean * 0.05).max(0.01);
    if slope > threshold {
        "increasing"
    } else if slope < -threshold {
        "decreasing"
    } else {
        "stable"
    }
}

/// Process-wide lifecycle monitor.
pub static COMMITMENT_MONITOR: Lazy<CommitmentLifecycleMonitor> =
    Lazy::new(|| CommitmentLifecycleMonitor::new(MonitorConfig::default()));

/// Spawn the periodic evaluation + escalation loop.
pub fn spawn_background_evaluator() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let now = alerting::now_secs();
            COMMITMENT_MONITOR.evaluate(now);
            alerting::ALERT_MANAGER.tick(now);
            for n in alerting::ALERT_MANAGER.drain_notifications() {
                tracing::warn!(
                    alert = %n.alert_name,
                    severity = ?n.severity,
                    channel = %n.channel,
                    occurrences = n.occurrences,
                    correlated = n.correlated_count,
                    escalation_level = n.escalation_level,
                    summary = %n.summary,
                    "alert notification"
                );
            }
        }
    });
}

/// `GET /v1/admin/commitments/lifecycle` – lifecycle snapshot with rates and trends.
pub async fn lifecycle_handler() -> axum::Json<LifecycleSnapshot> {
    axum::Json(COMMITMENT_MONITOR.snapshot(alerting::now_secs()))
}

/// `GET /v1/admin/alerts` – open (unresolved) alerts after dedup/correlation.
pub async fn open_alerts_handler() -> axum::Json<Vec<alerting::TrackedAlert>> {
    axum::Json(alerting::ALERT_MANAGER.open_alerts())
}
