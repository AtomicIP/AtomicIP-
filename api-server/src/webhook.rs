use std::time::Duration;
use axum::http::StatusCode;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::cache;

/// Maximum number of retries for webhook delivery.
const MAX_RETRIES: u32 = 10;

/// Maximum backoff delay (24 hours in seconds).
const MAX_BACKOFF_SECS: u64 = 86400;

/// Initial backoff delay in seconds.
const INITIAL_BACKOFF_SECS: u64 = 1;

/// Retention period for completed event records (7 days in seconds).
const EVENT_RETENTION_SECS: u64 = 604800;

/// Configuration for a registered webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub created_at: u64,
}

/// Webhook delivery payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event: String,
    pub swap_id: u64,
    pub old_status: Option<String>,
    pub new_status: String,
    pub timestamp: u64,
    pub signature: Option<String>,
}

/// Delivery status for a webhook event (#627).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
    Retrying,
}

/// Webhook event record with delivery tracking (#627).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEventRecord {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: DeliveryStatus,
    pub attempt_count: u32,
    pub max_retries: u32,
    pub last_attempt: Option<u64>,
    pub next_retry: Option<u64>,
    pub created_at: u64,
    pub last_error: Option<String>,
}

/// In-memory webhook registry.
static REGISTRY: Lazy<DashMap<Uuid, WebhookConfig>> = Lazy::new(DashMap::new);

/// #627: Persistent event store for delivery tracking.
static EVENT_STORE: Lazy<DashMap<Uuid, WebhookEventRecord>> = Lazy::new(DashMap::new);

/// #627: Retry queue (event IDs pending retry).
static RETRY_QUEUE: Lazy<DashMap<Uuid, WebhookEventRecord>> = Lazy::new(DashMap::new);

/// HTTP client for webhook delivery.
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build reqwest client")
});

/// Register a new webhook.
pub fn register(url: String, events: Vec<String>) -> WebhookConfig {
    let config = WebhookConfig {
        id: Uuid::new_v4(),
        url,
        events,
        secret: None,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    REGISTRY.insert(config.id, config.clone());
    info!(webhook_id = %config.id, url = %config.url, "Webhook registered");
    config
}

/// Register with an optional HMAC secret for signature verification (#627).
pub fn register_with_secret(url: String, events: Vec<String>, secret: Option<String>) -> WebhookConfig {
    let has_secret = secret.is_some();
    let config = WebhookConfig {
        id: Uuid::new_v4(),
        url,
        events,
        secret,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    REGISTRY.insert(config.id, config.clone());
    info!(webhook_id = %config.id, url = %config.url, has_secret, "Webhook registered with secret");
    config
}

/// Unregister a webhook by ID.
pub fn unregister(id: Uuid) -> bool {
    if REGISTRY.remove(&id).is_some() {
        info!(webhook_id = %id, "Webhook unregistered");
        true
    } else {
        false
    }
}

/// List all registered webhooks.
pub fn list_all() -> Vec<WebhookConfig> {
    REGISTRY.iter().map(|entry| entry.clone()).collect()
}

/// Trigger webhook delivery for a swap status change.
pub fn trigger_swap_status_changed(swap_id: u64, old_status: Option<String>, new_status: String) {
    let payload = WebhookPayload {
        event: "swap.status_changed".to_string(),
        swap_id,
        old_status,
        new_status,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        signature: None,
    };

    for entry in REGISTRY.iter() {
        let config = entry.value();
        if config.events.contains(&"swap.status_changed".to_string()) || config.events.contains(&"*".to_string()) {
            let config = config.clone();
            let payload = payload.clone();
            let mut signed_payload = payload.clone();

            // #627: Sign payload if webhook has a secret
            if let Some(ref secret) = config.secret {
                let signature = compute_hmac_signature(secret, &payload);
                signed_payload.signature = Some(signature);
            }

            // #627: Create event record for tracking
            let event_id = Uuid::new_v4();
            let event_record = WebhookEventRecord {
                id: event_id,
                webhook_id: config.id,
                event_type: "swap.status_changed".to_string(),
                payload: serde_json::to_value(&signed_payload).unwrap_or_default(),
                status: DeliveryStatus::Pending,
                attempt_count: 0,
                max_retries: MAX_RETRIES,
                last_attempt: None,
                next_retry: Some(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                last_error: None,
            };

            EVENT_STORE.insert(event_id, event_record.clone());
            RETRY_QUEUE.insert(event_id, event_record.clone());

            tokio::spawn(async move {
                deliver_with_backoff(&config, &signed_payload, event_id).await;
            });
        }
    }
}

/// #627: Compute HMAC-SHA256 signature for webhook payload.
fn compute_hmac_signature(secret: &str, payload: &WebhookPayload) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let message = serde_json::to_string(payload).unwrap_or_default();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// #627: Verify HMAC-SHA256 signature of incoming webhook request.
pub fn verify_signature(secret: &str, payload: &str, signature: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    // Constant-time comparison to prevent timing attacks
    expected.len() == signature.len()
        && expected.bytes().zip(signature.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Deliver a webhook payload with exponential backoff and jitter (#627).
async fn deliver_with_backoff(config: &WebhookConfig, payload: &WebhookPayload, event_id: Uuid) {
    let mut delay_secs = INITIAL_BACKOFF_SECS;

    for attempt in 1..=MAX_RETRIES {
        // Update event record status
        update_event_status(&event_id, DeliveryStatus::Retrying, attempt, None);

        match deliver(&config.url, payload).await {
            Ok(status) if status.is_success() => {
                info!(
                    webhook_id = %config.id,
                    url = %config.url,
                    event_id = %event_id,
                    attempt,
                    "Webhook delivered successfully"
                );
                update_event_status(&event_id, DeliveryStatus::Delivered, attempt, None);
                RETRY_QUEUE.remove(&event_id);
                return;
            }
            Ok(status) => {
                let error_msg = format!("HTTP {}", status.as_u16());
                warn!(
                    webhook_id = %config.id,
                    url = %config.url,
                    event_id = %event_id,
                    attempt,
                    status = status.as_u16(),
                    "Webhook delivery returned non-success status"
                );
                update_event_status(&event_id, DeliveryStatus::Failed, attempt, Some(error_msg));
            }
            Err(e) => {
                let error_msg = e.to_string();
                warn!(
                    webhook_id = %config.id,
                    url = %config.url,
                    event_id = %event_id,
                    attempt,
                    error = %e,
                    "Webhook delivery failed"
                );
                update_event_status(&event_id, DeliveryStatus::Failed, attempt, Some(error_msg));
            }
        }

        if attempt < MAX_RETRIES {
            // #627: Exponential backoff with jitter
            let jitter = rand::random::<f64>() * 0.5 + 0.75; // 0.75-1.25x multiplier
            let sleep_secs = ((delay_secs as f64) * jitter).min(MAX_BACKOFF_SECS as f64) as u64;

            // Schedule next retry
            let next_retry = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + sleep_secs;
            update_next_retry(&event_id, next_retry);

            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;

            // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 32s, 64s, 128s, 256s, 512s
            delay_secs = (delay_secs * 2).min(MAX_BACKOFF_SECS);
        }
    }

    error!(
        webhook_id = %config.id,
        url = %config.url,
        event_id = %event_id,
        max_retries = MAX_RETRIES,
        "Webhook delivery exhausted all retries"
    );
    update_event_status(&event_id, DeliveryStatus::Failed, MAX_RETRIES, Some("Max retries exhausted".to_string()));
    RETRY_QUEUE.remove(&event_id);

    // #627: Invalidate cache on final failure
    cache::invalidate(&format!("webhook:event:{}", event_id));
}

/// Update event status in the event store.
fn update_event_status(event_id: &Uuid, status: DeliveryStatus, attempt: u32, error: Option<String>) {
    if let Some(mut entry) = EVENT_STORE.get_mut(event_id) {
        entry.status = status;
        entry.attempt_count = attempt;
        entry.last_attempt = Some(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs());
        if let Some(err) = error {
            entry.last_error = Some(err);
        }
    }
}

/// Update next retry time for an event.
fn update_next_retry(event_id: &Uuid, next_retry: u64) {
    if let Some(mut entry) = EVENT_STORE.get_mut(event_id) {
        entry.next_retry = Some(next_retry);
    }
    if let Some(mut entry) = RETRY_QUEUE.get_mut(event_id) {
        entry.next_retry = Some(next_retry);
    }
}

/// Get delivery status for a specific event.
pub fn get_delivery_status(event_id: Uuid) -> Option<WebhookEventRecord> {
    EVENT_STORE.get(&event_id).map(|entry| entry.clone())
}

/// List all tracked webhook events.
pub fn list_all_events() -> Vec<WebhookEventRecord> {
    EVENT_STORE.iter().map(|entry| entry.clone()).collect()
}

/// Get pending retry events.
pub fn get_pending_retries() -> Vec<WebhookEventRecord> {
    RETRY_QUEUE.iter().map(|entry| entry.clone()).collect()
}

/// #627: Clean up old event records (call periodically).
pub fn expire_old_events() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cutoff = now.saturating_sub(EVENT_RETENTION_SECS);

    EVENT_STORE.retain(|_, event| {
        event.created_at >= cutoff || event.status == DeliveryStatus::Retrying || event.status == DeliveryStatus::Pending
    });
}

/// Single delivery attempt.
async fn deliver(url: &str, payload: &WebhookPayload) -> Result<StatusCode, reqwest::Error> {
    let response = CLIENT
        .post(url)
        .json(&json!(payload))
        .send()
        .await?;
    Ok(response.status())
}
