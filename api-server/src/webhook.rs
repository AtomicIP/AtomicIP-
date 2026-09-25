use std::time::Duration;
use axum::http::StatusCode;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Configuration for a registered webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>, // e.g., ["swap.status_changed"]
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
}

pub const SWAP_STATUS_CHANGED_EVENT: &str = "swap.status_changed";

/// In-memory webhook registry.
static REGISTRY: Lazy<DashMap<Uuid, WebhookConfig>> = Lazy::new(DashMap::new);

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
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    REGISTRY.insert(config.id, config.clone());
    info!(webhook_id = %config.id, url = %config.url, "Webhook registered");
    config
}

pub fn supports_event(events: &[String], event: &str) -> bool {
    events.iter().any(|registered| registered == event || registered == "*")
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
        event: SWAP_STATUS_CHANGED_EVENT.to_string(),
        swap_id,
        old_status,
        new_status,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    for entry in REGISTRY.iter() {
        let config = entry.value();
        if supports_event(&config.events, SWAP_STATUS_CHANGED_EVENT) {
            let config = config.clone();
            let payload = payload.clone();
            tokio::spawn(async move {
                deliver_with_retry(&config, &payload).await;
            });
        }
    }
}

/// Deliver a webhook payload with exponential backoff retry.
async fn deliver_with_retry(config: &WebhookConfig, payload: &WebhookPayload) {
    let mut delay = Duration::from_secs(1);
    let max_retries = 3;

    for attempt in 1..=max_retries {
        match deliver(&config.url, payload).await {
            Ok(status) if status.is_success() => {
                info!(
                    webhook_id = %config.id,
                    url = %config.url,
                    attempt,
                    "Webhook delivered successfully"
                );
                return;
            }
            Ok(status) => {
                warn!(
                    webhook_id = %config.id,
                    url = %config.url,
                    attempt,
                    status = status.as_u16(),
                    "Webhook delivery returned non-success status"
                );
            }
            Err(e) => {
                warn!(
                    webhook_id = %config.id,
                    url = %config.url,
                    attempt,
                    error = %e,
                    "Webhook delivery failed"
                );
            }
        }

        if attempt < max_retries {
            tokio::time::sleep(delay).await;
            delay *= 2; // exponential backoff: 1s, 2s, 4s
        }
    }

    error!(
        webhook_id = %config.id,
        url = %config.url,
        "Webhook delivery exhausted all retries"
    );
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
