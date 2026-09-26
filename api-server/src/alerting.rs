//! #1065: Alert fatigue reduction.
//!
//! A small in-process alert pipeline that sits between signal producers
//! (e.g. [`crate::commitment_monitoring`]) and notification sinks. Every alert
//! passes through four stages, in order:
//!
//! 1. **Silencing** – alerts matching an active [`Silence`] are dropped.
//! 2. **Deduplication** – alerts with the same fingerprint (name + labels)
//!    arriving inside the dedup window are folded into the existing alert and
//!    only bump its `occurrences` counter.
//! 3. **Correlation** – [`CorrelationRule`]s group related alerts (e.g. many
//!    per-path error alerts caused by one RPC outage) under a single parent
//!    incident so on-call receives one page instead of N.
//! 4. **Escalation** – alerts that stay unacknowledged past their
//!    [`EscalationPolicy`] step deadline, or that keep recurring, are promoted
//!    to the next severity / notification tier.
//!
//! See `docs/alerting-best-practices.md` for operational guidance.
//!
//! Design rationale: docs/adr/0004-alert-fatigue-reduction-pipeline.md

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use metrics::counter;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// Alert severity. Ordered so `Critical > Warning > Info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }

    fn escalate(self) -> Self {
        match self {
            Severity::Info => Severity::Warning,
            _ => Severity::Critical,
        }
    }
}

/// A raw alert emitted by a signal producer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub name: String,
    pub severity: Severity,
    pub summary: String,
    /// Identifying labels. Participate in the dedup fingerprint.
    pub labels: BTreeMap<String, String>,
}

impl Alert {
    pub fn new(name: impl Into<String>, severity: Severity, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            severity,
            summary: summary.into(),
            labels: BTreeMap::new(),
        }
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Stable fingerprint over `name` + sorted labels. Summary text is
    /// deliberately excluded so that alerts differing only in a counter value
    /// ("12 errors" vs "13 errors") still deduplicate.
    pub fn fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.labels.hash(&mut hasher);
        hasher.finish()
    }
}

/// Lifecycle state of an alert tracked by the manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertState {
    Firing,
    Acknowledged,
    Resolved,
}

/// An alert as tracked by the manager after dedup/correlation.
#[derive(Debug, Clone, Serialize)]
pub struct TrackedAlert {
    pub fingerprint: u64,
    pub alert: Alert,
    pub state: AlertState,
    pub first_seen: u64,
    pub last_seen: u64,
    pub occurrences: u64,
    /// Escalation step already reached (0 = initial notification).
    pub escalation_level: usize,
    pub last_escalated_at: u64,
    /// Fingerprint of the parent incident when correlated.
    pub correlated_into: Option<u64>,
    /// Child fingerprints folded into this alert by correlation.
    pub correlated_children: Vec<u64>,
}

/// Matcher on a single label. `value == "*"` matches any value (presence only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelMatcher {
    pub key: String,
    pub value: String,
}

impl LabelMatcher {
    fn matches(&self, labels: &BTreeMap<String, String>, name: &str) -> bool {
        let actual = if self.key == "alertname" {
            Some(name)
        } else {
            labels.get(&self.key).map(String::as_str)
        };
        match actual {
            Some(v) => self.value == "*" || self.value == v,
            None => false,
        }
    }
}

/// A time-bounded mute for alerts matching all `matchers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Silence {
    pub id: String,
    pub matchers: Vec<LabelMatcher>,
    pub starts_at: u64,
    pub ends_at: u64,
    pub created_by: String,
    pub comment: String,
}

impl Silence {
    fn is_active(&self, now: u64) -> bool {
        now >= self.starts_at && now < self.ends_at
    }

    fn matches(&self, alert: &Alert) -> bool {
        !self.matchers.is_empty()
            && self.matchers.iter().all(|m| m.matches(&alert.labels, &alert.name))
    }
}

/// Groups alerts that share a root cause.
///
/// When a `source` alert fires, any alert whose name is in `targets` and that
/// shares all `group_by` label values with it, arriving within `window`, is
/// folded into the source alert instead of notifying separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationRule {
    pub name: String,
    pub source: String,
    pub targets: Vec<String>,
    pub group_by: Vec<String>,
    pub window: Duration,
}

/// One step of an escalation policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationStep {
    /// Time since the previous step (or first_seen) before this step fires.
    pub after: Duration,
    /// Notification channel for this step (e.g. "slack", "pagerduty", "phone").
    pub channel: String,
}

/// Escalation policy applied to unacknowledged alerts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationPolicy {
    pub steps: Vec<EscalationStep>,
    /// Occurrence count at which a still-firing alert is bumped one severity
    /// level, regardless of time. Catches flapping / recurring problems.
    pub recurrence_threshold: u64,
}

impl Default for EscalationPolicy {
    fn default() -> Self {
        Self {
            steps: vec![
                EscalationStep { after: Duration::from_secs(0), channel: "slack".into() },
                EscalationStep { after: Duration::from_secs(15 * 60), channel: "pagerduty".into() },
                EscalationStep { after: Duration::from_secs(30 * 60), channel: "phone".into() },
            ],
            recurrence_threshold: 20,
        }
    }
}

/// A notification the manager wants delivered.
#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub fingerprint: u64,
    pub alert_name: String,
    pub severity: Severity,
    pub channel: String,
    pub summary: String,
    pub occurrences: u64,
    pub correlated_count: usize,
    pub escalation_level: usize,
}

/// Outcome of submitting an alert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    Silenced(String),
    Deduplicated(u64),
    Correlated { parent: u64 },
    Notified(u64),
}

#[derive(Debug, Clone)]
pub struct AlertManagerConfig {
    pub dedup_window: Duration,
    /// Resolved alerts older than this are purged.
    pub retention: Duration,
    pub escalation: EscalationPolicy,
}

impl Default for AlertManagerConfig {
    fn default() -> Self {
        Self {
            dedup_window: Duration::from_secs(5 * 60),
            retention: Duration::from_secs(24 * 60 * 60),
            escalation: EscalationPolicy::default(),
        }
    }
}

#[derive(Default)]
struct Inner {
    alerts: HashMap<u64, TrackedAlert>,
    silences: Vec<Silence>,
    rules: Vec<CorrelationRule>,
    outbox: Vec<Notification>,
}

/// Deduplicating, correlating, silencing, escalating alert manager.
pub struct AlertManager {
    config: AlertManagerConfig,
    inner: Mutex<Inner>,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl AlertManager {
    pub fn new(config: AlertManagerConfig) -> Self {
        Self { config, inner: Mutex::new(Inner::default()) }
    }

    /// Manager pre-loaded with the project's default correlation rules.
    pub fn with_default_rules() -> Self {
        let mgr = Self::new(AlertManagerConfig::default());
        for rule in default_correlation_rules() {
            mgr.add_correlation_rule(rule);
        }
        mgr
    }

    pub fn add_correlation_rule(&self, rule: CorrelationRule) {
        self.inner.lock().unwrap().rules.push(rule);
    }

    pub fn add_silence(&self, silence: Silence) {
        self.inner.lock().unwrap().silences.push(silence);
    }

    /// Expire a silence early. Returns true if it existed.
    pub fn remove_silence(&self, id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.silences.len();
        inner.silences.retain(|s| s.id != id);
        inner.silences.len() != before
    }

    pub fn active_silences(&self, now: u64) -> Vec<Silence> {
        self.inner
            .lock()
            .unwrap()
            .silences
            .iter()
            .filter(|s| s.is_active(now))
            .cloned()
            .collect()
    }

    /// Submit an alert at the current wall-clock time.
    pub fn submit(&self, alert: Alert) -> Disposition {
        self.submit_at(alert, now_secs())
    }

    /// Submit an alert at an explicit timestamp (seconds since epoch).
    pub fn submit_at(&self, alert: Alert, now: u64) -> Disposition {
        let mut guard = self.inner.lock().unwrap();
        // Reborrow so disjoint fields (alerts / outbox) can be borrowed together.
        let inner = &mut *guard;
        inner.silences.retain(|s| now < s.ends_at);

        // 1. Silencing
        if let Some(s) = inner.silences.iter().find(|s| s.is_active(now) && s.matches(&alert)) {
            counter!("alerts_silenced_total", "alertname" => alert.name.clone()).increment(1);
            return Disposition::Silenced(s.id.clone());
        }

        let fp = alert.fingerprint();
        let dedup_window = self.config.dedup_window.as_secs();
        let recurrence_threshold = self.config.escalation.recurrence_threshold;

        // 2. Deduplication
        if let Some(existing) = inner.alerts.get_mut(&fp) {
            let within_window = now.saturating_sub(existing.last_seen) <= dedup_window;
            if existing.state != AlertState::Resolved && within_window {
                existing.last_seen = now;
                existing.occurrences += 1;
                existing.alert.summary = alert.summary;
                if alert.severity > existing.alert.severity {
                    existing.alert.severity = alert.severity;
                }
                // Recurrence-based escalation.
                if recurrence_threshold > 0
                    && existing.occurrences % recurrence_threshold == 0
                    && existing.state == AlertState::Firing
                {
                    existing.alert.severity = existing.alert.severity.escalate();
                    let n = notification_for(existing, "recurrence");
                    inner.outbox.push(n);
                }
                counter!("alerts_deduplicated_total", "alertname" => alert.name.clone()).increment(1);
                return Disposition::Deduplicated(fp);
            }
        }

        // 3. Correlation
        let parent = inner.rules.iter().find_map(|rule| {
            if !rule.targets.iter().any(|t| t == &alert.name) {
                return None;
            }
            inner.alerts.values().find_map(|candidate| {
                let related = candidate.alert.name == rule.source
                    && candidate.state != AlertState::Resolved
                    && now.saturating_sub(candidate.last_seen) <= rule.window.as_secs()
                    && rule.group_by.iter().all(|k| {
                        candidate.alert.labels.get(k) == alert.labels.get(k)
                    });
                related.then_some(candidate.fingerprint)
            })
        });

        let tracked = TrackedAlert {
            fingerprint: fp,
            alert: alert.clone(),
            state: AlertState::Firing,
            first_seen: now,
            last_seen: now,
            occurrences: 1,
            escalation_level: 0,
            last_escalated_at: now,
            correlated_into: parent,
            correlated_children: Vec::new(),
        };
        inner.alerts.insert(fp, tracked);

        if let Some(parent_fp) = parent {
            if let Some(p) = inner.alerts.get_mut(&parent_fp) {
                if !p.correlated_children.contains(&fp) {
                    p.correlated_children.push(fp);
                }
                p.last_seen = now;
            }
            counter!("alerts_correlated_total", "alertname" => alert.name.clone()).increment(1);
            return Disposition::Correlated { parent: parent_fp };
        }

        // 4. Initial notification (escalation step 0)
        let first_channel = self.first_channel();
        let n = notification_for(&inner.alerts[&fp], &first_channel);
        inner.outbox.push(n);
        counter!(
            "alerts_notified_total",
            "alertname" => alert.name.clone(),
            "severity" => alert.severity.as_str(),
        )
        .increment(1);
        Disposition::Notified(fp)
    }

    fn first_channel(&self) -> String {
        self.config
            .escalation
            .steps
            .first()
            .map(|s| s.channel.clone())
            .unwrap_or_else(|| "default".into())
    }

    /// Advance time-based escalation for unacknowledged alerts. Intended to be
    /// called periodically (e.g. every 30s) by a background task.
    pub fn tick(&self, now: u64) {
        let steps = &self.config.escalation.steps;
        let retention = self.config.retention.as_secs();
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;
        let mut pending = Vec::new();

        for a in inner.alerts.values_mut() {
            if a.state != AlertState::Firing || a.correlated_into.is_some() {
                continue;
            }
            let next = a.escalation_level + 1;
            if let Some(step) = steps.get(next) {
                if now.saturating_sub(a.last_escalated_at) >= step.after.as_secs() {
                    a.escalation_level = next;
                    a.last_escalated_at = now;
                    if next + 1 == steps.len() {
                        a.alert.severity = Severity::Critical;
                    }
                    counter!("alerts_escalated_total", "alertname" => a.alert.name.clone(), "channel" => step.channel.clone()).increment(1);
                    pending.push(notification_for(a, &step.channel));
                }
            }
        }

        inner
            .alerts
            .retain(|_, a| !(a.state == AlertState::Resolved && now.saturating_sub(a.last_seen) > retention));
        inner.silences.retain(|s| now < s.ends_at);
        inner.outbox.extend(pending);
    }

    /// Acknowledge an alert, halting escalation. Correlated children are
    /// acknowledged with their parent.
    pub fn acknowledge(&self, fingerprint: u64) -> bool {
        self.set_state(fingerprint, AlertState::Acknowledged)
    }

    /// Resolve an alert and its correlated children.
    pub fn resolve(&self, fingerprint: u64) -> bool {
        self.set_state(fingerprint, AlertState::Resolved)
    }

    fn set_state(&self, fingerprint: u64, state: AlertState) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let children = match inner.alerts.get_mut(&fingerprint) {
            Some(a) => {
                a.state = state;
                a.correlated_children.clone()
            }
            None => return false,
        };
        for c in children {
            if let Some(child) = inner.alerts.get_mut(&c) {
                child.state = state;
            }
        }
        true
    }

    /// Drain notifications produced since the last call.
    pub fn drain_notifications(&self) -> Vec<Notification> {
        std::mem::take(&mut self.inner.lock().unwrap().outbox)
    }

    /// Snapshot of alerts that are not resolved.
    pub fn open_alerts(&self) -> Vec<TrackedAlert> {
        self.inner
            .lock()
            .unwrap()
            .alerts
            .values()
            .filter(|a| a.state != AlertState::Resolved)
            .cloned()
            .collect()
    }
}

fn notification_for(a: &TrackedAlert, channel: &str) -> Notification {
    Notification {
        fingerprint: a.fingerprint,
        alert_name: a.alert.name.clone(),
        severity: a.alert.severity,
        channel: channel.to_string(),
        summary: a.alert.summary.clone(),
        occurrences: a.occurrences,
        correlated_count: a.correlated_children.len(),
        escalation_level: a.escalation_level,
    }
}

/// Correlation rules for known AtomicIP failure cascades.
pub fn default_correlation_rules() -> Vec<CorrelationRule> {
    vec![
        CorrelationRule {
            name: "rpc-outage-cascade".into(),
            source: "SorobanRpcUnavailable".into(),
            targets: vec![
                "HighErrorRate".into(),
                "HighLatency".into(),
                "CommitmentRateDrop".into(),
                "SwapCompletionStalled".into(),
            ],
            group_by: vec!["network".into()],
            window: Duration::from_secs(10 * 60),
        },
        CorrelationRule {
            name: "cache-outage-cascade".into(),
            source: "RedisUnavailable".into(),
            targets: vec!["HighLatency".into(), "RateLimiterDegraded".into()],
            group_by: vec![],
            window: Duration::from_secs(10 * 60),
        },
        CorrelationRule {
            name: "commitment-anomaly-cascade".into(),
            source: "CommitmentSpike".into(),
            targets: vec!["CommitmentRateLimitHits".into(), "HighErrorRate".into()],
            group_by: vec![],
            window: Duration::from_secs(5 * 60),
        },
    ]
}

/// Process-wide alert manager.
pub static ALERT_MANAGER: Lazy<AlertManager> = Lazy::new(AlertManager::with_default_rules);
