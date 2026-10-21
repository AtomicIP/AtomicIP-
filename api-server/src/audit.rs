use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::Sha256;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use utoipa::{IntoParams, ToSchema};
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;
const GENESIS_SIGNATURE: &str = "0";
const LOCK_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(2);

/// Threshold for suspicious access detection (failed attempts in 5 minutes)
const SUSPICIOUS_THRESHOLD: usize = 5;
/// Time window for tracking suspicious patterns (5 minutes)
const SUSPICIOUS_WINDOW_SECS: i64 = 300;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventCategory {
    Commitment,
    Swap,
    Admin,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct AuditEvent {
    pub sequence: u64,
    pub timestamp: i64,
    pub category: AuditEventCategory,
    pub event_type: String,
    pub actor_hash: Option<String>,
    pub ip_id: Option<u64>,
    pub swap_id: Option<u64>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub success: bool,
    pub status_code: Option<u16>,
    #[schema(value_type = Object)]
    pub details: Value,
    pub previous_signature: String,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct UnsignedAuditEvent {
    sequence: u64,
    timestamp: i64,
    category: AuditEventCategory,
    event_type: String,
    actor_hash: Option<String>,
    ip_id: Option<u64>,
    swap_id: Option<u64>,
    request_id: Option<String>,
    trace_id: Option<String>,
    success: bool,
    status_code: Option<u16>,
    details: Value,
    previous_signature: String,
}

impl From<&AuditEvent> for UnsignedAuditEvent {
    fn from(event: &AuditEvent) -> Self {
        Self {
            sequence: event.sequence,
            timestamp: event.timestamp,
            category: event.category.clone(),
            event_type: event.event_type.clone(),
            actor_hash: event.actor_hash.clone(),
            ip_id: event.ip_id,
            swap_id: event.swap_id,
            request_id: event.request_id.clone(),
            trace_id: event.trace_id.clone(),
            success: event.success,
            status_code: event.status_code,
            details: event.details.clone(),
            previous_signature: event.previous_signature.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventInput {
    pub category: AuditEventCategory,
    pub event_type: String,
    pub actor_hash: Option<String>,
    pub ip_id: Option<u64>,
    pub swap_id: Option<u64>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub success: bool,
    pub status_code: Option<u16>,
    pub details: Value,
}

impl AuditEventInput {
    pub fn new(category: AuditEventCategory, event_type: impl Into<String>) -> Self {
        Self {
            category,
            event_type: event_type.into(),
            actor_hash: None,
            ip_id: None,
            swap_id: None,
            request_id: None,
            trace_id: None,
            success: true,
            status_code: None,
            details: Value::Object(Map::new()),
        }
    }

    pub fn actor_hash(mut self, actor_hash: Option<String>) -> Self {
        self.actor_hash = actor_hash.filter(|value| !value.is_empty());
        self
    }

    pub fn ip_id(mut self, ip_id: Option<u64>) -> Self {
        self.ip_id = ip_id;
        self
    }

    pub fn swap_id(mut self, swap_id: Option<u64>) -> Self {
        self.swap_id = swap_id;
        self
    }

    pub fn request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id.filter(|value| !value.is_empty());
        self
    }

    pub fn trace_id(mut self, trace_id: Option<String>) -> Self {
        self.trace_id = trace_id.filter(|value| !value.is_empty());
        self
    }

    pub fn success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn status_code(mut self, status_code: Option<u16>) -> Self {
        self.status_code = status_code;
        self
    }

    pub fn detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        let mut details = match self.details {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        if let Ok(value) = serde_json::to_value(value) {
            details.insert(key.into(), value);
        }
        self.details = Value::Object(details);
        self
    }

    fn into_event(self, sequence: u64, previous_signature: String) -> AuditEvent {
        AuditEvent {
            sequence,
            timestamp: Utc::now().timestamp(),
            category: self.category,
            event_type: self.event_type,
            actor_hash: self.actor_hash,
            ip_id: self.ip_id,
            swap_id: self.swap_id,
            request_id: self.request_id,
            trace_id: self.trace_id,
            success: self.success,
            status_code: self.status_code,
            details: self.details,
            previous_signature,
            signature: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct AuditLogQuery {
    #[serde(default = "default_audit_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
    pub category: Option<AuditEventCategory>,
    pub event_type: Option<String>,
    pub actor_hash: Option<String>,
    pub ip_id: Option<u64>,
    pub swap_id: Option<u64>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub success: Option<bool>,
    pub min_sequence: Option<u64>,
    pub max_sequence: Option<u64>,
}

impl Default for AuditLogQuery {
    fn default() -> Self {
        Self {
            limit: default_audit_limit(),
            offset: 0,
            category: None,
            event_type: None,
            actor_hash: None,
            ip_id: None,
            swap_id: None,
            request_id: None,
            trace_id: None,
            success: None,
            min_sequence: None,
            max_sequence: None,
        }
    }
}

fn default_audit_limit() -> u64 {
    50
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditIntegrityStatus {
    Empty,
    Valid,
    Invalid,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct AuditLogQueryResponse {
    pub events: Vec<AuditEvent>,
    pub total_count: u64,
    pub has_more: bool,
    pub integrity: AuditIntegrityStatus,
}

/// Result of independently walking the durable, on-disk chain — usable by an
/// auditor with nothing more than read access to the backing file and the
/// HMAC key, without any cooperation from a running server instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuditChainVerification {
    pub event_count: u64,
    pub status: AuditIntegrityStatus,
    /// The sequence number of the first event whose linkage or signature
    /// doesn't check out, if the chain is invalid.
    pub broken_at_sequence: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SuspiciousPattern {
    pub actor_hash: String,
    pub ip_address: Option<String>,
    pub failed_attempts: usize,
    pub last_attempt_time: i64,
    pub pattern_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditLogError {
    MissingHmacKey,
    Io(String),
    Serialization,
    LockTimeout,
    CorruptChain { at_line: usize },
}

impl std::fmt::Display for AuditLogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHmacKey => formatter.write_str(
                "AUDIT_HMAC_KEY is not set; refusing to start with a randomly generated key",
            ),
            Self::Io(message) => write!(formatter, "audit log I/O error: {message}"),
            Self::Serialization => formatter.write_str("audit log serialization failed"),
            Self::LockTimeout => formatter.write_str("timed out waiting for audit log lock"),
            Self::CorruptChain { at_line } => {
                write!(formatter, "audit log corrupt at line {at_line}")
            }
        }
    }
}

impl std::error::Error for AuditLogError {}

impl From<io::Error> for AuditLogError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// Durably-backed, hash-chained audit log.
///
/// Every append is written to an append-only file under an exclusive lock,
/// fsynced before the call returns, and the chain's next `sequence` /
/// `previous_signature` are derived by reading that file's current tail
/// under the same lock — never from in-process state — so multiple
/// `AuditLogStore` instances (multiple process replicas, or a restarted
/// process) sharing one backing file always extend a single, consistent
/// chain instead of colliding or starting over.
pub struct AuditLogStore {
    key: Vec<u8>,
    path: PathBuf,
    events: RwLock<Vec<AuditEvent>>,
}

/// In-memory-only audit sink used as a degraded fallback when the durable
/// audit log cannot be initialized. Events are kept in process memory only
/// and are lost on restart, so this is strictly a "keep serving, keep
/// recording something" mode rather than a substitute for the durable store.
pub struct InMemoryAuditSink {
    key: Vec<u8>,
    events: RwLock<Vec<AuditEvent>>,
}

impl InMemoryAuditSink {
    /// Builds an in-memory sink from durable configuration. Mirrors
    /// [`AuditLogStore::from_env`]'s key requirement so the fallback never
    /// silently substitutes a random key.
    pub fn from_env() -> Result<Self, AuditLogError> {
        let key = std::env::var("AUDIT_HMAC_KEY")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or(AuditLogError::MissingHmacKey)?;

        Ok(Self::new(key.into_bytes()))
    }

    pub fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            events: RwLock::new(Vec::new()),
        }
    }

    pub fn hash_identifier(&self, identifier: &str) -> String {
        hmac_sha256_hex(&self.key, &format!("identifier:{identifier}"))
    }

    pub async fn append(&self, input: AuditEventInput) -> Result<AuditEvent, AuditLogError> {
        let mut events = self.events.write().await;
        let sequence = events.len() as u64 + 1;
        let previous_signature = events
            .last()
            .map(|event| event.signature.clone())
            .unwrap_or_else(|| GENESIS_SIGNATURE.to_string());

        let mut event = input.into_event(sequence, previous_signature);
        event.signature = compute_signature(&self.key, &event);
        events.push(event.clone());

        Ok(event)
    }

    pub async fn query(&self, filter: &AuditLogQuery) -> AuditLogQueryResponse {
        let events = self.events.read().await;
        let limit = filter.limit.clamp(1, 200);
        let offset = filter.offset as usize;
        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|event| event_matches_filter(event, filter))
            .cloned()
            .collect();
        let total_count = filtered.len() as u64;
        let page_end = offset.saturating_add(limit as usize).min(filtered.len());
        let page = filtered
            .iter()
            .skip(offset)
            .take(limit as usize)
            .cloned()
            .collect::<Vec<_>>();
        let has_more = page_end < filtered.len();
        let integrity = if events.is_empty() {
            AuditIntegrityStatus::Empty
        } else if verify_chain(&self.key, &events) {
            AuditIntegrityStatus::Valid
        } else {
            AuditIntegrityStatus::Invalid
        };

        AuditLogQueryResponse {
            events: page,
            total_count,
            has_more,
            integrity,
        }
    }

    pub async fn len(&self) -> usize {
        self.events.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.events.read().await.is_empty()
    }
}

impl AuditLogStore {
    /// Opens (or creates) the audit log backed by `path`, replaying any
    /// existing chain into memory so queries reflect history that predates
    /// this process.
    pub fn open(key: Vec<u8>, path: impl Into<PathBuf>) -> Result<Self, AuditLogError> {
        let path = path.into();
        let events = read_events(&path)?;
        Ok(Self {
            key,
            path,
            events: RwLock::new(events),
        })
    }

    /// Builds the store from durable configuration: `AUDIT_HMAC_KEY` must be
    /// set and non-empty. Startup fails loudly instead of silently
    /// substituting a random key, since a random key can never be
    /// reconstructed after a restart and would make the chain unverifiable.
    pub fn from_env(path: impl Into<PathBuf>) -> Result<Self, AuditLogError> {
        let key = std::env::var("AUDIT_HMAC_KEY")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or(AuditLogError::MissingHmacKey)?;

        Self::open(key.into_bytes(), path)
    }

    pub fn hash_identifier(&self, identifier: &str) -> String {
        hmac_sha256_hex(&self.key, &format!("identifier:{identifier}"))
    }

    pub async fn append(&self, input: AuditEventInput) -> Result<AuditEvent, AuditLogError> {
        let key = self.key.clone();
        let path = self.path.clone();

        let event = tokio::task::spawn_blocking(move || append_locked(&path, &key, input))
            .await
            .map_err(|error| AuditLogError::Io(format!("append task panicked: {error}")))??;

        self.events.write().await.push(event.clone());

        Ok(event)
    }

    pub async fn query(&self, filter: &AuditLogQuery) -> AuditLogQueryResponse {
        let events = self.events.read().await;
        let limit = filter.limit.clamp(1, 200);
        let offset = filter.offset as usize;
        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|event| event_matches_filter(event, filter))
            .cloned()
            .collect();
        let total_count = filtered.len() as u64;
        let page_end = offset.saturating_add(limit as usize).min(filtered.len());
        let page = filtered
            .iter()
            .skip(offset)
            .take(limit as usize)
            .cloned()
            .collect::<Vec<_>>();
        let has_more = page_end < filtered.len();
        let integrity = if events.is_empty() {
            AuditIntegrityStatus::Empty
        } else if verify_chain(&self.key, &events) {
            AuditIntegrityStatus::Valid
        } else {
            AuditIntegrityStatus::Invalid
        };

        AuditLogQueryResponse {
            events: page,
            total_count,
            has_more,
            integrity,
        }
    }

    pub async fn len(&self) -> usize {
        self.events.read().await.len()
    }

    /// Independently re-derives integrity by re-reading the durable file
    /// from disk (not the in-memory cache), so it reflects appends made by
    /// any other instance sharing this backing store.
    pub fn verify_persisted(&self) -> Result<AuditChainVerification, AuditLogError> {
        verify_persisted_chain(&self.path, &self.key)
    }

    /// Detects suspicious access patterns (brute force attempts, etc.)
    pub async fn detect_suspicious_patterns(&self) -> Vec<SuspiciousPattern> {
        let events = self.events.read().await;
        let now = Utc::now().timestamp();
        let window_start = now - SUSPICIOUS_WINDOW_SECS;

        let mut actor_failures: HashMap<String, Vec<i64>> = HashMap::new();

        for event in events.iter() {
            if !event.success {
                if let Some(actor) = &event.actor_hash {
                    let timestamps = actor_failures
                        .entry(actor.clone())
                        .or_insert_with(Vec::new);

                    if event.timestamp >= window_start {
                        timestamps.push(event.timestamp);
                    }
                }
            }
        }

        let mut suspicious = Vec::new();
        for (actor_hash, attempts) in actor_failures {
            if attempts.len() >= SUSPICIOUS_THRESHOLD {
                if let Some(last_attempt) = attempts.last() {
                    suspicious.push(SuspiciousPattern {
                        actor_hash,
                        ip_address: None,
                        failed_attempts: attempts.len(),
                        last_attempt_time: *last_attempt,
                        pattern_type: "brute_force".to_string(),
                    });
                }
            }
        }

        suspicious
    }

    /// Erases personal data recorded against `actor_hash` (#1107).
    ///
    /// Matching events keep their sequence, timestamp, category and outcome,
    /// but lose `actor_hash`, `request_id`, `trace_id` and `details`. Each
    /// signature covers the previous one, so the chain from the first affected
    /// event onward is re-signed with the server key and rewritten atomically.
    /// Returns the number of events anonymized.
    pub async fn anonymize_actor(&self, actor_hash: &str) -> Result<usize, AuditLogError> {
        let key = self.key.clone();
        let path = self.path.clone();
        let target = actor_hash.to_string();

        let (count, rewritten) =
            tokio::task::spawn_blocking(move || anonymize_actor_locked(&path, &key, &target))
                .await
                .map_err(|error| AuditLogError::Io(format!("anonymize task panicked: {error}")))??;

        if count > 0 {
            *self.events.write().await = rewritten;
        }

        Ok(count)
    }

    /// Counts persisted events still attributed to `actor_hash`. Used to
    /// verify that an erasure left no references behind.
    pub async fn count_actor_events(&self, actor_hash: &str) -> usize {
        self.events
            .read()
            .await
            .iter()
            .filter(|event| event.actor_hash.as_deref() == Some(actor_hash))
            .count()
    }
}

/// Walks the persisted chain at `path` and confirms every `signature`
/// matches its `previous_signature` linkage. Standalone by design: an
/// outside auditor can call this with only read access to the backing file
/// and the HMAC key — no running server or operator cooperation required.
pub fn verify_persisted_chain(
    path: &Path,
    key: &[u8],
) -> Result<AuditChainVerification, AuditLogError> {
    let events = read_events(path)?;
    let event_count = events.len() as u64;

    if events.is_empty() {
        return Ok(AuditChainVerification {
            event_count,
            status: AuditIntegrityStatus::Empty,
            broken_at_sequence: None,
        });
    }

    let broken_at_sequence = first_broken_sequence(key, &events);
    let status = if broken_at_sequence.is_none() {
        AuditIntegrityStatus::Valid
    } else {
        AuditIntegrityStatus::Invalid
    };

    Ok(AuditChainVerification {
        event_count,
        status,
        broken_at_sequence,
    })
}

fn event_matches_filter(event: &AuditEvent, filter: &AuditLogQuery) -> bool {
    if let Some(category) = &filter.category {
        if &event.category != category {
            return false;
        }
    }
    if let Some(event_type) = &filter.event_type {
        if &event.event_type != event_type {
            return false;
        }
    }
    if let Some(actor_hash) = &filter.actor_hash {
        if event.actor_hash.as_deref() != Some(actor_hash.as_str()) {
            return false;
        }
    }
    if let Some(ip_id) = filter.ip_id {
        if event.ip_id != Some(ip_id) {
            return false;
        }
    }
    if let Some(swap_id) = filter.swap_id {
        if event.swap_id != Some(swap_id) {
            return false;
        }
    }
    if let Some(request_id) = &filter.request_id {
        if event.request_id.as_deref() != Some(request_id.as_str()) {
            return false;
        }
    }
    if let Some(trace_id) = &filter.trace_id {
        if event.trace_id.as_deref() != Some(trace_id.as_str()) {
            return false;
        }
    }
    if let Some(success) = filter.success {
        if event.success != success {
            return false;
        }
    }
    if let Some(min_sequence) = filter.min_sequence {
        if event.sequence < min_sequence {
            return false;
        }
    }
    if let Some(max_sequence) = filter.max_sequence {
        if event.sequence > max_sequence {
            return false;
        }
    }

    true
}

fn compute_signature(key: &[u8], event: &AuditEvent) -> String {
    let unsigned = UnsignedAuditEvent::from(event);
    let message = serde_json::to_string(&unsigned).unwrap_or_else(|_| String::new());
    hmac_sha256_hex(key, &message)
}

fn verify_event_signature(key: &[u8], event: &AuditEvent) -> bool {
    let unsigned = UnsignedAuditEvent::from(event);
    let message = serde_json::to_string(&unsigned).unwrap_or_else(|_| String::new());
    let expected = hmac_sha256_hex(key, &message);
    constant_time_eq(event.signature.as_bytes(), expected.as_bytes())
}

fn first_broken_sequence(key: &[u8], events: &[AuditEvent]) -> Option<u64> {
    let mut expected_previous = GENESIS_SIGNATURE.to_string();

    for (index, event) in events.iter().enumerate() {
        let expected_sequence = index as u64 + 1;
        if event.sequence != expected_sequence
            || event.previous_signature != expected_previous
            || !verify_event_signature(key, event)
        {
            return Some(event.sequence);
        }
        expected_previous = event.signature.clone();
    }

    None
}

fn verify_chain(key: &[u8], events: &[AuditEvent]) -> bool {
    first_broken_sequence(key, events).is_none()
}

fn hmac_sha256_hex(key: &[u8], message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right.iter())
        .fold(0u8, |accumulator, (left, right)| accumulator | (left ^ right))
        == 0
}

/// #1106: Admin check for the audit log endpoints.
///
/// Fails closed. `expected` must be set and non-empty, and `provided` must match
/// it in constant time. An unset key therefore locks the endpoints rather than
/// opening them.
pub fn admin_key_matches(provided: Option<&str>, expected: Option<&str>) -> bool {
    match (provided, expected) {
        (Some(provided), Some(expected)) if !expected.is_empty() => {
            constant_time_eq(provided.as_bytes(), expected.as_bytes())
        }
        _ => false,
    }
}

fn lock_path_for(base_path: &Path) -> PathBuf {
    let mut os_string = base_path.as_os_str().to_owned();
    os_string.push(".lock");
    PathBuf::from(os_string)
}

/// Advisory, cross-instance exclusive lock implemented via atomic
/// create-if-absent on a sidecar `.lock` file. Held only across a single
/// read-tail-then-append critical section, and released (file removed) on
/// drop — including on the error/panic path that unwinds through `?`.
struct AppendLockGuard {
    lock_path: PathBuf,
}

impl AppendLockGuard {
    fn acquire(base_path: &Path) -> Result<Self, AuditLogError> {
        let lock_path = lock_path_for(base_path);
        let deadline = Instant::now() + LOCK_ACQUIRE_TIMEOUT;

        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(_) => return Ok(Self { lock_path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        return Err(AuditLogError::LockTimeout);
                    }
                    std::thread::sleep(LOCK_RETRY_INTERVAL);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl Drop for AppendLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn append_locked(
    path: &Path,
    key: &[u8],
    input: AuditEventInput,
) -> Result<AuditEvent, AuditLogError> {
    let _lock = AppendLockGuard::acquire(path)?;

    let existing = read_events(path)?;
    let sequence = existing.len() as u64 + 1;
    let previous_signature = existing
        .last()
        .map(|event| event.signature.clone())
        .unwrap_or_else(|| GENESIS_SIGNATURE.to_string());

    let mut event = input.into_event(sequence, previous_signature);
    event.signature = compute_signature(key, &event);

    let mut line = serde_json::to_string(&event).map_err(|_| AuditLogError::Serialization)?;
    line.push('\n');

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    file.sync_all()?;

    Ok(event)
}

/// Rewrites the backing file with personal data for `target` removed and the
/// chain re-signed from the first affected event. Runs under the append lock
/// so no concurrent append can interleave with the rewrite.
fn anonymize_actor_locked(
    path: &Path,
    key: &[u8],
    target: &str,
) -> Result<(usize, Vec<AuditEvent>), AuditLogError> {
    let _lock = AppendLockGuard::acquire(path)?;

    let mut events = read_events(path)?;
    let mut count = 0;
    let mut first_changed: Option<usize> = None;

    for (index, event) in events.iter_mut().enumerate() {
        if event.actor_hash.as_deref() == Some(target) {
            event.actor_hash = None;
            event.request_id = None;
            event.trace_id = None;
            event.details = Value::Object(Map::new());
            count += 1;
            first_changed.get_or_insert(index);
        }
    }

    let Some(start) = first_changed else {
        return Ok((0, events));
    };

    let mut previous_signature = if start == 0 {
        GENESIS_SIGNATURE.to_string()
    } else {
        events[start - 1].signature.clone()
    };
    for event in events.iter_mut().skip(start) {
        event.previous_signature = previous_signature.clone();
        event.signature = compute_signature(key, event);
        previous_signature = event.signature.clone();
    }

    let mut body = String::new();
    for event in &events {
        body.push_str(&serde_json::to_string(event).map_err(|_| AuditLogError::Serialization)?);
        body.push('\n');
    }

    let mut tmp_os = path.as_os_str().to_owned();
    tmp_os.push(".tmp");
    let tmp_path = PathBuf::from(tmp_os);
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;

    Ok((count, events))
}

/// Reads and parses every record in the backing file. A trailing line that
/// fails to parse is treated as an interrupted write (the process crashed
/// between `write_all` and `sync_all`) and is dropped rather than rejected,
/// so the store self-heals on the next append. A malformed line anywhere
/// else indicates real corruption of history and is a hard error.
fn read_events(path: &Path) -> Result<Vec<AuditEvent>, AuditLogError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .collect::<Result<_, _>>()?;

    let mut events = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<AuditEvent>(line) {
            Ok(event) => events.push(event),
            Err(_) if index == lines.len() - 1 => break,
            Err(_) => return Err(AuditLogError::CorruptChain { at_line: index + 1 }),
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempStorePath(PathBuf);

    impl TempStorePath {
        fn new(label: &str) -> Self {
            let unique = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let mut path = std::env::temp_dir();
            path.push(format!(
                "atomicip-audit-test-{label}-{}-{unique}.jsonl",
                std::process::id()
            ));
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempStorePath {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
            let _ = fs::remove_file(lock_path_for(&self.0));
        }
    }

    fn open_store(label: &str, key: Vec<u8>) -> (AuditLogStore, TempStorePath) {
        let temp = TempStorePath::new(label);
        let store = AuditLogStore::open(key, temp.path()).unwrap();
        (store, temp)
    }

    #[tokio::test]
    async fn test_append_creates_sequenced_hmac_signed_event() {
        let (store, _temp) = open_store("append-basic", vec![7; 32]);
        let event = store
            .append(
                AuditEventInput::new(AuditEventCategory::Commitment, "ip.commit.requested")
                    .ip_id(Some(1))
                    .detail("commitment_hash", "deadbeef"),
            )
            .await
            .unwrap();

        assert_eq!(event.sequence, 1);
        assert_eq!(event.previous_signature, GENESIS_SIGNATURE);
        assert!(!event.signature.is_empty());
        assert_eq!(event.details["commitment_hash"], "deadbeef");
    }

    #[tokio::test]
    async fn test_query_filters_by_category_ip_and_success() {
        let (store, _temp) = open_store("query-filter", vec![9; 32]);
        let actor_hash = store.hash_identifier("GOWNER");
        store
            .append(
                AuditEventInput::new(AuditEventCategory::Commitment, "ip.commit.requested")
                    .actor_hash(Some(actor_hash.clone()))
                    .ip_id(Some(1))
                    .success(false),
            )
            .await
            .unwrap();
        store
            .append(
                AuditEventInput::new(AuditEventCategory::Swap, "swap.accept.requested")
                    .swap_id(Some(2))
                    .success(true),
            )
            .await
            .unwrap();

        let response = store
            .query(&AuditLogQuery {
                category: Some(AuditEventCategory::Commitment),
                ip_id: Some(1),
                success: Some(false),
                ..AuditLogQuery::default()
            })
            .await;

        assert_eq!(response.total_count, 1);
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0].event_type, "ip.commit.requested");
        assert_eq!(response.events[0].actor_hash.as_deref(), Some(actor_hash.as_str()));
        assert_eq!(response.integrity, AuditIntegrityStatus::Valid);
    }

    #[tokio::test]
    async fn test_query_limits_and_reports_has_more() {
        let (store, _temp) = open_store("query-limit", vec![11; 32]);
        store
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();
        store
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();

        let response = store
            .query(&AuditLogQuery {
                limit: 1,
                offset: 0,
                ..AuditLogQuery::default()
            })
            .await;

        assert_eq!(response.total_count, 2);
        assert_eq!(response.events.len(), 1);
        assert!(response.has_more);
    }

    #[tokio::test]
    async fn test_tamper_detection_finds_modified_details() {
        let (store, _temp) = open_store("tamper-details", vec![13; 32]);
        let mut event = store
            .append(
                AuditEventInput::new(AuditEventCategory::Admin, "admin.action")
                    .detail("action", "register"),
            )
            .await
            .unwrap();

        if let Some(details) = event.details.as_object_mut() {
            details.insert("action".to_string(), json!("tamper"));
        }
        let events = vec![event];

        assert!(!verify_chain(store.key.as_slice(), &events));
    }

    #[tokio::test]
    async fn test_tamper_detection_finds_sequence_gap() {
        let (store, _temp) = open_store("tamper-gap", vec![17; 32]);
        let mut first = store
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();
        let second = store
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();

        first.sequence = 99;
        let events = vec![first, second];

        assert!(!verify_chain(store.key.as_slice(), &events));
    }

    #[tokio::test]
    async fn test_hash_identifier_is_stable_without_exposing_input() {
        let (store, _temp) = open_store("hash-identifier", vec![19; 32]);
        let hashed = store.hash_identifier("GOWNER@example.invalid");

        assert_eq!(hashed.len(), 64);
        assert!(!hashed.contains("GOWNER"));
        assert_eq!(hashed, store.hash_identifier("GOWNER@example.invalid"));
        assert_ne!(hashed, store.hash_identifier("GOTHER@example.invalid"));
    }

    #[tokio::test]
    async fn test_restart_continues_existing_chain_from_durable_store() {
        let temp = TempStorePath::new("restart-continuity");
        let key = vec![21; 32];

        {
            let store = AuditLogStore::open(key.clone(), temp.path()).unwrap();
            store
                .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
                .await
                .unwrap();
            store
                .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
                .await
                .unwrap();
        }

        // Simulate a process restart: a brand new store instance opened
        // against the same backing file should pick the chain up rather
        // than starting a fresh one.
        let restarted = AuditLogStore::open(key.clone(), temp.path()).unwrap();
        assert_eq!(restarted.len().await, 2);

        let third = restarted
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();

        assert_eq!(third.sequence, 3);
        assert_ne!(third.previous_signature, GENESIS_SIGNATURE);

        let verification = verify_persisted_chain(temp.path(), &key).unwrap();
        assert_eq!(verification.event_count, 3);
        assert_eq!(verification.status, AuditIntegrityStatus::Valid);
    }

    #[tokio::test]
    async fn test_from_env_requires_hmac_key_and_fails_loudly_when_unset() {
        let temp = TempStorePath::new("from-env-missing-key");

        // SAFETY: this test owns the AUDIT_HMAC_KEY var for its duration;
        // no other test reads or writes it.
        unsafe {
            std::env::remove_var("AUDIT_HMAC_KEY");
        }
        let missing = AuditLogStore::from_env(temp.path());
        assert_eq!(missing.err(), Some(AuditLogError::MissingHmacKey));

        unsafe {
            std::env::set_var("AUDIT_HMAC_KEY", "a-sufficiently-secret-value");
        }
        let store = AuditLogStore::from_env(temp.path())
            .expect("store should initialize once AUDIT_HMAC_KEY is set");
        assert_eq!(store.len().await, 0);

        unsafe {
            std::env::remove_var("AUDIT_HMAC_KEY");
        }
    }

    #[tokio::test]
    async fn test_concurrent_instances_never_collide_on_sequence() {
        let temp = TempStorePath::new("concurrent-instances");
        let key = vec![33; 32];

        let store_a = std::sync::Arc::new(AuditLogStore::open(key.clone(), temp.path()).unwrap());
        let store_b = std::sync::Arc::new(AuditLogStore::open(key.clone(), temp.path()).unwrap());

        let mut handles = Vec::new();
        for i in 0..25u32 {
            let store = if i % 2 == 0 { store_a.clone() } else { store_b.clone() };
            handles.push(tokio::spawn(async move {
                store
                    .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
                    .await
                    .unwrap()
            }));
        }

        let mut sequences: Vec<u64> = Vec::new();
        for handle in handles {
            sequences.push(handle.await.unwrap().sequence);
        }
        sequences.sort_unstable();

        let expected: Vec<u64> = (1..=25).collect();
        assert_eq!(sequences, expected, "no two events may share a sequence number");

        let verification = verify_persisted_chain(temp.path(), &key).unwrap();
        assert_eq!(verification.event_count, 25);
        assert_eq!(verification.status, AuditIntegrityStatus::Valid);
    }

    #[tokio::test]
    async fn test_in_memory_sink_accepts_appends_and_chains() {
        let sink = InMemoryAuditSink::new(vec![51; 32]);

        let first = sink
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();
        let second = sink
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();

        assert_eq!(first.sequence, 1);
        assert_eq!(first.previous_signature, GENESIS_SIGNATURE);
        assert_eq!(second.sequence, 2);
        assert_eq!(second.previous_signature, first.signature);
        assert_eq!(sink.len().await, 2);

        let response = sink.query(&AuditLogQuery::default()).await;
        assert_eq!(response.total_count, 2);
        assert_eq!(response.integrity, AuditIntegrityStatus::Valid);
    }

    #[tokio::test]
    async fn test_in_memory_sink_from_env_requires_hmac_key() {
        // SAFETY: this test owns the AUDIT_HMAC_KEY var for its duration;
        // no other test reads or writes it.
        unsafe {
            std::env::remove_var("AUDIT_HMAC_KEY");
        }
        let missing = InMemoryAuditSink::from_env();
        assert_eq!(missing.err(), Some(AuditLogError::MissingHmacKey));

        unsafe {
            std::env::set_var("AUDIT_HMAC_KEY", "a-sufficiently-secret-value");
        }
        let sink = InMemoryAuditSink::from_env()
            .expect("in-memory sink should initialize once AUDIT_HMAC_KEY is set");
        assert!(sink.is_empty().await);

        unsafe {
            std::env::remove_var("AUDIT_HMAC_KEY");
        }
    }

    #[tokio::test]
    async fn test_verify_persisted_chain_detects_on_disk_tampering() {
        let temp = TempStorePath::new("verify-tamper");
        let key = vec![41; 32];

        {
            let store = AuditLogStore::open(key.clone(), temp.path()).unwrap();
            store
                .append(
                    AuditEventInput::new(AuditEventCategory::Admin, "admin.action")
                        .detail("action", "register"),
                )
                .await
                .unwrap();
            store
                .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
                .await
                .unwrap();
        }

        let clean = verify_persisted_chain(temp.path(), &key).unwrap();
        assert_eq!(clean.status, AuditIntegrityStatus::Valid);
        assert_eq!(clean.event_count, 2);
        assert_eq!(clean.broken_at_sequence, None);

        let contents = fs::read_to_string(temp.path()).unwrap();
        let tampered = contents.replacen("\"register\"", "\"tampered\"", 1);
        fs::write(temp.path(), tampered).unwrap();

        let dirty = verify_persisted_chain(temp.path(), &key).unwrap();
        assert_eq!(dirty.status, AuditIntegrityStatus::Invalid);
        assert_eq!(dirty.broken_at_sequence, Some(1));
    }

    #[tokio::test]
    async fn anonymize_actor_redacts_personal_data_and_keeps_chain_valid() {
        let key = vec![9; 32];
        let (store, temp) = open_store("anonymize", key.clone());
        let target = store.hash_identifier("user-to-erase");
        let other = store.hash_identifier("bystander");

        store
            .append(
                AuditEventInput::new(AuditEventCategory::Commitment, "ip.commit.requested")
                    .actor_hash(Some(target.clone()))
                    .request_id(Some("req-1".to_string()))
                    .detail("owner", "user-to-erase"),
            )
            .await
            .unwrap();
        store
            .append(
                AuditEventInput::new(AuditEventCategory::Swap, "swap.accept.requested")
                    .actor_hash(Some(other.clone()))
                    .detail("buyer", "bystander"),
            )
            .await
            .unwrap();
        store
            .append(
                AuditEventInput::new(AuditEventCategory::Admin, "admin.action")
                    .actor_hash(Some(target.clone())),
            )
            .await
            .unwrap();

        let count = store.anonymize_actor(&target).await.unwrap();

        assert_eq!(count, 2);
        assert_eq!(store.count_actor_events(&target).await, 0);
        assert_eq!(store.count_actor_events(&other).await, 1);

        let persisted = read_events(temp.path()).unwrap();
        assert_eq!(persisted.len(), 3);
        assert_eq!(persisted[0].actor_hash, None);
        assert_eq!(persisted[0].request_id, None);
        assert_eq!(persisted[0].details, Value::Object(Map::new()));
        assert_eq!(persisted[0].event_type, "ip.commit.requested");
        assert_eq!(persisted[1].details["buyer"], "bystander");

        let verification = verify_persisted_chain(temp.path(), &key).unwrap();
        assert_eq!(verification.status, AuditIntegrityStatus::Valid);
    }

    #[tokio::test]
    async fn anonymize_actor_is_noop_for_unknown_actor() {
        let key = vec![5; 32];
        let (store, temp) = open_store("anonymize-noop", key.clone());
        store
            .append(AuditEventInput::new(AuditEventCategory::Admin, "admin.action"))
            .await
            .unwrap();
        let before = fs::read_to_string(temp.path()).unwrap();

        let count = store.anonymize_actor(&store.hash_identifier("nobody")).await.unwrap();

        assert_eq!(count, 0);
        assert_eq!(fs::read_to_string(temp.path()).unwrap(), before);
    }
}

#[cfg(test)]
mod admin_key_tests {
    use super::admin_key_matches;

    #[test]
    fn matching_key_is_accepted() {
        assert!(admin_key_matches(Some("s3cret"), Some("s3cret")));
    }

    #[test]
    fn wrong_or_missing_key_is_rejected() {
        assert!(!admin_key_matches(Some("wrong"), Some("s3cret")));
        assert!(!admin_key_matches(Some("s3cre"), Some("s3cret")));
        assert!(!admin_key_matches(None, Some("s3cret")));
    }

    #[test]
    fn unset_expected_key_locks_endpoint() {
        assert!(!admin_key_matches(Some("anything"), None));
        assert!(!admin_key_matches(Some(""), None));
    }

    #[test]
    fn empty_expected_key_locks_endpoint() {
        assert!(!admin_key_matches(Some(""), Some("")));
        assert!(!admin_key_matches(None, Some("")));
    }
}
