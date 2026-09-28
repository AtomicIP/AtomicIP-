//! #1068: Incident management integration.
//!
//! Turns alerts into tracked incidents and mirrors them to an external paging
//! provider (PagerDuty or Opsgenie). Alerts reach this module from two places:
//!
//! * the in-process pipeline in [`crate::alerting`] — critical notifications
//!   drained by `commitment_monitoring::spawn_background_evaluator`;
//! * Prometheus Alertmanager, via the webhook receiver
//!   `POST /v1/admin/incidents/alertmanager` (see `monitoring/alertmanager`).
//!
//! Each incident keeps a timeline (trigger, re-trigger, acknowledge, resolve,
//! notes) and, once resolved, a postmortem. Acknowledging or resolving an
//! incident here is propagated to the provider and to the originating alert so
//! escalation stops.
//!
//! Configuration (environment):
//! * `INCIDENT_PROVIDER` – `pagerduty`, `opsgenie` or `none` (default).
//! * `PAGERDUTY_ROUTING_KEY` – Events API v2 integration key.
//! * `OPSGENIE_API_KEY` / `OPSGENIE_API_URL` – GenieKey and API base
//!   (default `https://api.opsgenie.com`, use `https://api.eu.opsgenie.com` for EU).
//!
//! Procedures: docs/incident-response.md. Postmortem template:
//! docs/postmortem-template.md.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use metrics::{counter, histogram};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::alerting::{self, Notification, Severity};

// ── Model ────────────────────────────────────────────────────────────────────

/// Incident severity. SEV1 is the most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncidentSeverity {
    Sev1,
    Sev2,
    Sev3,
}

impl IncidentSeverity {
    /// Maps alert severity (plus escalation level) to incident severity.
    pub fn from_alert(severity: Severity, escalation_level: usize) -> Self {
        match (severity, escalation_level) {
            (Severity::Critical, l) if l >= 2 => IncidentSeverity::Sev1,
            (Severity::Critical, _) => IncidentSeverity::Sev2,
            _ => IncidentSeverity::Sev3,
        }
    }

    fn from_label(label: Option<&str>) -> Self {
        match label {
            Some("critical") => IncidentSeverity::Sev2,
            Some("sev1") | Some("page") => IncidentSeverity::Sev1,
            _ => IncidentSeverity::Sev3,
        }
    }

    fn pagerduty(&self) -> &'static str {
        match self {
            IncidentSeverity::Sev1 | IncidentSeverity::Sev2 => "critical",
            IncidentSeverity::Sev3 => "warning",
        }
    }

    fn opsgenie(&self) -> &'static str {
        match self {
            IncidentSeverity::Sev1 => "P1",
            IncidentSeverity::Sev2 => "P2",
            IncidentSeverity::Sev3 => "P3",
        }
    }

    /// SEV1/SEV2 incidents require a postmortem (docs/incident-response.md §6).
    pub fn requires_postmortem(&self) -> bool {
        *self <= IncidentSeverity::Sev2
    }

    fn as_str(&self) -> &'static str {
        match self {
            IncidentSeverity::Sev1 => "sev1",
            IncidentSeverity::Sev2 => "sev2",
            IncidentSeverity::Sev3 => "sev3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncidentStatus {
    Triggered,
    Acknowledged,
    Resolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEntry {
    pub at: u64,
    pub kind: String,
    pub actor: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub description: String,
    pub owner: String,
    /// Issue / ticket URL tracking the follow-up.
    #[serde(default)]
    pub tracking_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Postmortem {
    pub author: String,
    pub summary: String,
    pub impact: String,
    pub root_cause: String,
    #[serde(default)]
    pub contributing_factors: Vec<String>,
    #[serde(default)]
    pub what_went_well: Vec<String>,
    #[serde(default)]
    pub what_went_wrong: Vec<String>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
    #[serde(default)]
    pub document_url: Option<String>,
    #[serde(default)]
    pub submitted_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub id: String,
    /// Stable key used to dedupe re-triggers and to address the incident at
    /// the provider (PagerDuty `dedup_key` / Opsgenie `alias`).
    pub dedup_key: String,
    pub title: String,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    pub source: String,
    /// Fingerprint of the originating in-process alert, if any.
    pub alert_fingerprint: Option<u64>,
    /// Whether this incident is mirrored to the external provider. Incidents
    /// from Alertmanager are not: Alertmanager pages the provider itself, and
    /// mirroring them would page twice.
    pub paged: bool,
    pub trigger_count: u64,
    pub created_at: u64,
    pub acknowledged_at: Option<u64>,
    pub acknowledged_by: Option<String>,
    pub resolved_at: Option<u64>,
    pub resolved_by: Option<String>,
    pub timeline: Vec<TimelineEntry>,
    pub postmortem: Option<Postmortem>,
}

impl Incident {
    fn push(&mut self, at: u64, kind: &str, actor: &str, message: impl Into<String>) {
        self.timeline.push(TimelineEntry {
            at,
            kind: kind.into(),
            actor: actor.into(),
            message: message.into(),
        });
    }

    pub fn postmortem_due(&self) -> bool {
        self.status == IncidentStatus::Resolved
            && self.severity.requires_postmortem()
            && self.postmortem.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct NewIncident {
    pub dedup_key: String,
    pub title: String,
    pub severity: IncidentSeverity,
    pub source: String,
    pub alert_fingerprint: Option<u64>,
    pub paged: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IncidentError {
    NotFound,
    InvalidState(&'static str),
}

// ── Provider integration ─────────────────────────────────────────────────────

/// External paging provider.
#[derive(Debug, Clone)]
pub enum Provider {
    None,
    PagerDuty { routing_key: String },
    Opsgenie { api_key: String, base_url: String },
}

/// Lifecycle event to mirror to the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAction {
    Trigger,
    Acknowledge,
    Resolve,
}

impl Provider {
    pub fn from_env() -> Self {
        match std::env::var("INCIDENT_PROVIDER").unwrap_or_default().to_lowercase().as_str() {
            "pagerduty" => match std::env::var("PAGERDUTY_ROUTING_KEY") {
                Ok(routing_key) if !routing_key.is_empty() => Provider::PagerDuty { routing_key },
                _ => {
                    tracing::warn!("INCIDENT_PROVIDER=pagerduty but PAGERDUTY_ROUTING_KEY unset; paging disabled");
                    Provider::None
                }
            },
            "opsgenie" => match std::env::var("OPSGENIE_API_KEY") {
                Ok(api_key) if !api_key.is_empty() => Provider::Opsgenie {
                    api_key,
                    base_url: std::env::var("OPSGENIE_API_URL")
                        .unwrap_or_else(|_| "https://api.opsgenie.com".into()),
                },
                _ => {
                    tracing::warn!("INCIDENT_PROVIDER=opsgenie but OPSGENIE_API_KEY unset; paging disabled");
                    Provider::None
                }
            },
            _ => Provider::None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Provider::None => "none",
            Provider::PagerDuty { .. } => "pagerduty",
            Provider::Opsgenie { .. } => "opsgenie",
        }
    }

    /// Build the HTTP request for `action`. Returns `None` when there is
    /// nothing to send (no provider configured).
    fn request(
        &self,
        client: &reqwest::Client,
        action: ProviderAction,
        incident: &Incident,
        details: &serde_json::Value,
        actor: &str,
    ) -> Option<reqwest::RequestBuilder> {
        match self {
            Provider::None => None,
            Provider::PagerDuty { routing_key } => {
                let event_action = match action {
                    ProviderAction::Trigger => "trigger",
                    ProviderAction::Acknowledge => "acknowledge",
                    ProviderAction::Resolve => "resolve",
                };
                let mut body = json!({
                    "routing_key": routing_key,
                    "event_action": event_action,
                    "dedup_key": incident.dedup_key,
                });
                if action == ProviderAction::Trigger {
                    body["payload"] = json!({
                        "summary": incident.title,
                        "source": incident.source,
                        "severity": incident.severity.pagerduty(),
                        "component": "atomicip-api",
                        "group": "atomicip",
                        "custom_details": details,
                    });
                    body["links"] = json!([{
                        "href": "https://github.com/AtomicIP/AtomicIP-/blob/main/docs/incident-response.md",
                        "text": "Incident response procedures",
                    }]);
                }
                Some(client.post("https://events.pagerduty.com/v2/enqueue").json(&body))
            }
            Provider::Opsgenie { api_key, base_url } => {
                let alias = urlencode(&incident.dedup_key);
                let req = match action {
                    ProviderAction::Trigger => client
                        .post(format!("{base_url}/v2/alerts"))
                        .json(&json!({
                            "message": truncate(&incident.title, 130),
                            "alias": incident.dedup_key,
                            "source": incident.source,
                            "priority": incident.severity.opsgenie(),
                            "tags": ["atomicip", incident.severity.as_str()],
                            "details": flatten_details(details),
                        })),
                    ProviderAction::Acknowledge => client
                        .post(format!("{base_url}/v2/alerts/{alias}/acknowledge?identifierType=alias"))
                        .json(&json!({ "user": actor, "source": "atomicip-api" })),
                    ProviderAction::Resolve => client
                        .post(format!("{base_url}/v2/alerts/{alias}/close?identifierType=alias"))
                        .json(&json!({ "user": actor, "source": "atomicip-api" })),
                };
                Some(req.header("Authorization", format!("GenieKey {api_key}")))
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Opsgenie `details` must be a flat string map.
fn flatten_details(details: &serde_json::Value) -> serde_json::Value {
    match details.as_object() {
        Some(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let s = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
                    (k.clone(), serde_json::Value::String(s))
                })
                .collect(),
        ),
        None => json!({}),
    }
}

// ── Manager ──────────────────────────────────────────────────────────────────

pub struct IncidentManager {
    provider: Provider,
    client: reqwest::Client,
    incidents: Mutex<HashMap<String, Incident>>,
    /// dedup_key -> id of the currently open incident with that key.
    open_by_key: Mutex<HashMap<String, String>>,
}

impl IncidentManager {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            incidents: Mutex::new(HashMap::new()),
            open_by_key: Mutex::new(HashMap::new()),
        }
    }

    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    /// Open a new incident, or record a re-trigger on the open incident with
    /// the same dedup key. Returns the incident id and whether it was new.
    pub fn trigger(&self, new: NewIncident, now: u64) -> (String, bool) {
        let mut open = self.open_by_key.lock().unwrap();
        let mut incidents = self.incidents.lock().unwrap();

        if let Some(id) = open.get(&new.dedup_key) {
            if let Some(inc) = incidents.get_mut(id) {
                inc.trigger_count += 1;
                // Severity only ever increases while the incident is open.
                if new.severity < inc.severity {
                    inc.severity = new.severity;
                    let msg = format!("severity raised to {}", new.severity.as_str());
                    inc.push(now, "severity_change", "system", msg);
                }
                inc.push(now, "retrigger", &new.source, new.title.clone());
                counter!("incidents_retriggered_total").increment(1);
                return (id.clone(), false);
            }
        }

        let id = format!("INC-{}", uuid::Uuid::new_v4().simple().to_string()[..10].to_uppercase());
        let mut incident = Incident {
            id: id.clone(),
            dedup_key: new.dedup_key.clone(),
            title: new.title,
            severity: new.severity,
            status: IncidentStatus::Triggered,
            source: new.source.clone(),
            alert_fingerprint: new.alert_fingerprint,
            paged: new.paged,
            trigger_count: 1,
            created_at: now,
            acknowledged_at: None,
            acknowledged_by: None,
            resolved_at: None,
            resolved_by: None,
            timeline: Vec::new(),
            postmortem: None,
        };
        incident.push(now, "triggered", &new.source, incident.title.clone());
        counter!("incidents_created_total", "severity" => incident.severity.as_str()).increment(1);

        self.notify(ProviderAction::Trigger, &incident, new.details, "atomicip-api");
        open.insert(new.dedup_key, id.clone());
        incidents.insert(id.clone(), incident);
        (id, true)
    }

    /// Create (or re-trigger) an incident from an in-process alert notification.
    /// Only critical notifications page; everything else stays in the alert
    /// pipeline.
    pub fn open_from_notification(&self, n: &Notification, now: u64) -> Option<String> {
        if n.severity != Severity::Critical {
            return None;
        }
        let (id, _) = self.trigger(
            NewIncident {
                dedup_key: format!("alert-{:016x}", n.fingerprint),
                title: format!("{}: {}", n.alert_name, n.summary),
                severity: IncidentSeverity::from_alert(n.severity, n.escalation_level),
                source: "atomicip-alerting".into(),
                alert_fingerprint: Some(n.fingerprint),
                paged: true,
                details: json!({
                    "alert": n.alert_name,
                    "occurrences": n.occurrences,
                    "correlated_alerts": n.correlated_count,
                    "escalation_level": n.escalation_level,
                    "channel": n.channel,
                }),
            },
            now,
        );
        Some(id)
    }

    pub fn acknowledge(&self, id: &str, actor: &str, now: u64) -> Result<Incident, IncidentError> {
        let mut incidents = self.incidents.lock().unwrap();
        let inc = incidents.get_mut(id).ok_or(IncidentError::NotFound)?;
        match inc.status {
            IncidentStatus::Resolved => return Err(IncidentError::InvalidState("incident already resolved")),
            IncidentStatus::Acknowledged => return Ok(inc.clone()),
            IncidentStatus::Triggered => {}
        }
        inc.status = IncidentStatus::Acknowledged;
        inc.acknowledged_at = Some(now);
        inc.acknowledged_by = Some(actor.to_owned());
        inc.push(now, "acknowledged", actor, "incident acknowledged");
        histogram!("incident_time_to_acknowledge_seconds", "severity" => inc.severity.as_str())
            .record(now.saturating_sub(inc.created_at) as f64);
        if let Some(fp) = inc.alert_fingerprint {
            // Stops escalation in the in-process alert pipeline.
            alerting::ALERT_MANAGER.acknowledge(fp);
        }
        self.notify(ProviderAction::Acknowledge, inc, json!({}), actor);
        Ok(inc.clone())
    }

    pub fn resolve(&self, id: &str, actor: &str, note: Option<String>, now: u64) -> Result<Incident, IncidentError> {
        // Lock order must match `trigger`: open_by_key before incidents.
        let mut open = self.open_by_key.lock().unwrap();
        let mut incidents = self.incidents.lock().unwrap();
        let inc = incidents.get_mut(id).ok_or(IncidentError::NotFound)?;
        if inc.status == IncidentStatus::Resolved {
            return Ok(inc.clone());
        }
        if inc.acknowledged_at.is_none() {
            inc.acknowledged_at = Some(now);
            inc.acknowledged_by = Some(actor.to_owned());
        }
        inc.status = IncidentStatus::Resolved;
        inc.resolved_at = Some(now);
        inc.resolved_by = Some(actor.to_owned());
        inc.push(now, "resolved", actor, note.unwrap_or_else(|| "incident resolved".into()));
        histogram!("incident_time_to_resolve_seconds", "severity" => inc.severity.as_str())
            .record(now.saturating_sub(inc.created_at) as f64);
        if inc.severity.requires_postmortem() {
            inc.push(now, "postmortem_required", "system", "SEV1/SEV2: postmortem due within 5 business days");
        }
        if let Some(fp) = inc.alert_fingerprint {
            alerting::ALERT_MANAGER.resolve(fp);
        }
        open.remove(&inc.dedup_key);
        self.notify(ProviderAction::Resolve, inc, json!({}), actor);
        Ok(inc.clone())
    }

    /// Resolve by dedup key (used when the upstream alert clears).
    pub fn resolve_by_key(&self, dedup_key: &str, actor: &str, now: u64) -> Option<Incident> {
        let id = { self.open_by_key.lock().unwrap().get(dedup_key).cloned()? };
        self.resolve(&id, actor, Some("source alert resolved".into()), now).ok()
    }

    pub fn add_note(&self, id: &str, actor: &str, message: String, now: u64) -> Result<Incident, IncidentError> {
        let mut incidents = self.incidents.lock().unwrap();
        let inc = incidents.get_mut(id).ok_or(IncidentError::NotFound)?;
        inc.push(now, "note", actor, message);
        Ok(inc.clone())
    }

    pub fn submit_postmortem(&self, id: &str, mut pm: Postmortem, now: u64) -> Result<Incident, IncidentError> {
        let mut incidents = self.incidents.lock().unwrap();
        let inc = incidents.get_mut(id).ok_or(IncidentError::NotFound)?;
        if inc.status != IncidentStatus::Resolved {
            return Err(IncidentError::InvalidState("postmortem can only be attached to a resolved incident"));
        }
        pm.submitted_at = now;
        let author = pm.author.clone();
        inc.postmortem = Some(pm);
        inc.push(now, "postmortem", &author, "postmortem submitted");
        counter!("incident_postmortems_total").increment(1);
        Ok(inc.clone())
    }

    pub fn get(&self, id: &str) -> Option<Incident> {
        self.incidents.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self, status: Option<IncidentStatus>) -> Vec<Incident> {
        let mut out: Vec<Incident> = self
            .incidents
            .lock()
            .unwrap()
            .values()
            .filter(|i| status.map_or(true, |s| i.status == s))
            .cloned()
            .collect();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    pub fn stats(&self) -> IncidentStats {
        let incidents = self.incidents.lock().unwrap();
        let mut s = IncidentStats { provider: self.provider.name().into(), ..Default::default() };
        let (mut tta, mut ttr) = (Vec::new(), Vec::new());
        for i in incidents.values() {
            match i.status {
                IncidentStatus::Triggered => s.triggered += 1,
                IncidentStatus::Acknowledged => s.acknowledged += 1,
                IncidentStatus::Resolved => s.resolved += 1,
            }
            if i.postmortem_due() {
                s.postmortems_due.push(i.id.clone());
            }
            if let Some(a) = i.acknowledged_at {
                tta.push(a.saturating_sub(i.created_at));
            }
            if let Some(r) = i.resolved_at {
                ttr.push(r.saturating_sub(i.created_at));
            }
        }
        let mean = |v: &[u64]| if v.is_empty() { None } else { Some(v.iter().sum::<u64>() / v.len() as u64) };
        s.mean_time_to_acknowledge_seconds = mean(&tta);
        s.mean_time_to_resolve_seconds = mean(&ttr);
        s
    }

    /// Fire-and-forget delivery to the provider. Failures are logged and
    /// counted but never block the caller; the incident is still tracked
    /// locally and Alertmanager's own PagerDuty route remains as a backstop.
    fn notify(&self, action: ProviderAction, incident: &Incident, details: serde_json::Value, actor: &str) {
        if !incident.paged {
            return;
        }
        let Some(req) = self.provider.request(&self.client, action, incident, &details, actor) else {
            return;
        };
        let provider = self.provider.name();
        let id = incident.id.clone();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(incident = %id, "no tokio runtime; skipping {provider} delivery");
            return;
        };
        handle.spawn(async move {
            let result = req.send().await.and_then(|r| r.error_for_status());
            let outcome = if result.is_ok() { "ok" } else { "error" };
            counter!("incident_provider_requests_total", "provider" => provider, "outcome" => outcome).increment(1);
            if let Err(e) = result {
                tracing::error!(incident = %id, provider, action = ?action, error = %e, "incident provider request failed");
            }
        });
    }
}

#[derive(Debug, Default, Serialize)]
pub struct IncidentStats {
    pub provider: String,
    pub triggered: usize,
    pub acknowledged: usize,
    pub resolved: usize,
    pub mean_time_to_acknowledge_seconds: Option<u64>,
    pub mean_time_to_resolve_seconds: Option<u64>,
    pub postmortems_due: Vec<String>,
}

/// Process-wide incident manager.
pub static INCIDENTS: Lazy<IncidentManager> = Lazy::new(|| IncidentManager::new(Provider::from_env()));

// ── HTTP handlers ────────────────────────────────────────────────────────────

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;

fn map_err(e: IncidentError) -> (StatusCode, Json<serde_json::Value>) {
    match e {
        IncidentError::NotFound => (StatusCode::NOT_FOUND, Json(json!({ "error": "incident not found" }))),
        IncidentError::InvalidState(msg) => (StatusCode::CONFLICT, Json(json!({ "error": msg }))),
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<IncidentStatus>,
}

/// `GET /v1/admin/incidents?status=triggered|acknowledged|resolved`
pub async fn list_handler(axum::extract::Query(q): axum::extract::Query<ListQuery>) -> Json<Vec<Incident>> {
    Json(INCIDENTS.list(q.status))
}

/// `GET /v1/admin/incidents/stats` – counts, MTTA/MTTR, overdue postmortems.
pub async fn stats_handler() -> Json<IncidentStats> {
    Json(INCIDENTS.stats())
}

/// `GET /v1/admin/incidents/{id}`
pub async fn get_handler(Path(id): Path<String>) -> ApiResult<Incident> {
    INCIDENTS.get(&id).map(Json).ok_or_else(|| map_err(IncidentError::NotFound))
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub title: String,
    pub severity: IncidentSeverity,
    pub reporter: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// `POST /v1/admin/incidents` – declare an incident manually.
pub async fn create_handler(Json(req): Json<CreateRequest>) -> (StatusCode, Json<Incident>) {
    let now = alerting::now_secs();
    let (id, _) = INCIDENTS.trigger(
        NewIncident {
            dedup_key: format!("manual-{}", uuid::Uuid::new_v4().simple()),
            title: req.title,
            severity: req.severity,
            source: format!("manual:{}", req.reporter),
            alert_fingerprint: None,
            paged: true,
            details: json!({ "reporter": req.reporter, "description": req.description }),
        },
        now,
    );
    (StatusCode::CREATED, Json(INCIDENTS.get(&id).expect("just inserted")))
}

#[derive(Debug, Deserialize)]
pub struct ActorRequest {
    pub actor: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /v1/admin/incidents/{id}/acknowledge`
pub async fn acknowledge_handler(Path(id): Path<String>, Json(req): Json<ActorRequest>) -> ApiResult<Incident> {
    INCIDENTS.acknowledge(&id, &req.actor, alerting::now_secs()).map(Json).map_err(map_err)
}

/// `POST /v1/admin/incidents/{id}/resolve`
pub async fn resolve_handler(Path(id): Path<String>, Json(req): Json<ActorRequest>) -> ApiResult<Incident> {
    INCIDENTS.resolve(&id, &req.actor, req.note, alerting::now_secs()).map(Json).map_err(map_err)
}

#[derive(Debug, Deserialize)]
pub struct NoteRequest {
    pub actor: String,
    pub message: String,
}

/// `POST /v1/admin/incidents/{id}/notes` – append to the incident timeline.
pub async fn note_handler(Path(id): Path<String>, Json(req): Json<NoteRequest>) -> ApiResult<Incident> {
    INCIDENTS.add_note(&id, &req.actor, req.message, alerting::now_secs()).map(Json).map_err(map_err)
}

/// `POST /v1/admin/incidents/{id}/postmortem`
pub async fn postmortem_handler(Path(id): Path<String>, Json(pm): Json<Postmortem>) -> ApiResult<Incident> {
    INCIDENTS.submit_postmortem(&id, pm, alerting::now_secs()).map(Json).map_err(map_err)
}

/// Subset of the Alertmanager webhook payload (version 4).
#[derive(Debug, Deserialize)]
pub struct AlertmanagerWebhook {
    pub alerts: Vec<AlertmanagerAlert>,
}

#[derive(Debug, Deserialize)]
pub struct AlertmanagerAlert {
    pub status: String,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default, rename = "generatorURL")]
    pub generator_url: String,
}

/// `POST /v1/admin/incidents/alertmanager` – Alertmanager webhook receiver.
/// Firing alerts open (or re-trigger) incidents; resolved alerts resolve them.
pub async fn alertmanager_webhook_handler(Json(hook): Json<AlertmanagerWebhook>) -> Json<serde_json::Value> {
    let now = alerting::now_secs();
    let (mut opened, mut resolved) = (Vec::new(), Vec::new());
    for a in hook.alerts {
        let name = a.labels.get("alertname").cloned().unwrap_or_else(|| "UnknownAlert".into());
        let dedup_key = format!("am-{}", if a.fingerprint.is_empty() { &name } else { &a.fingerprint });
        if a.status == "resolved" {
            if let Some(inc) = INCIDENTS.resolve_by_key(&dedup_key, "alertmanager", now) {
                resolved.push(inc.id);
            }
            continue;
        }
        let summary = a.annotations.get("summary").cloned().unwrap_or_default();
        let (id, _) = INCIDENTS.trigger(
            NewIncident {
                dedup_key,
                title: if summary.is_empty() { name.clone() } else { format!("{name}: {summary}") },
                severity: IncidentSeverity::from_label(a.labels.get("severity").map(String::as_str)),
                source: "alertmanager".into(),
                alert_fingerprint: None,
                paged: false,
                details: json!({
                    "labels": a.labels,
                    "annotations": a.annotations,
                    "generator_url": a.generator_url,
                }),
            },
            now,
        );
        opened.push(id);
    }
    Json(json!({ "opened_or_retriggered": opened, "resolved": resolved }))
}
