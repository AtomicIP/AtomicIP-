use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
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
///
/// Redirects are disabled so a permitted public URL cannot redirect to an
/// internal address and bypass the SSRF guard.
static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build reqwest client")
});

/// Returns true if the given IP address is in a range that webhooks must not
/// be allowed to reach (loopback, private, link-local, unique-local,
/// unspecified, multicast).
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        // 100.64.0.0/10 (carrier-grade NAT)
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64)
        // 0.0.0.0/8
        || ip.octets()[0] == 0
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    let segments = ip.segments();
    // fc00::/7 unique-local
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // fe80::/10 link-local
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // IPv4-mapped addresses: re-check the embedded IPv4 address.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_ipv4(v4);
    }
    false
}

/// Validate a webhook URL for registration: the scheme must be http/https and
/// every resolved address must be public.
pub async fn validate_webhook_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("unsupported scheme: {other}")),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("failed to resolve host {host}: {e}"))?;
    let mut resolved_any = false;
    for addr in addrs {
        resolved_any = true;
        if is_blocked_ip(addr.ip()) {
            return Err(format!(
                "webhook target {host} resolves to blocked address {}",
                addr.ip()
            ));
        }
    }
    if !resolved_any {
        return Err(format!("host {host} did not resolve to any address"));
    }
    Ok(())
}

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
        // Re-check the resolved address at delivery time to defeat DNS
        // rebinding between registration and delivery.
        if let Err(e) = validate_webhook_url(&config.url).await {
            error!(
                webhook_id = %config.id,
                url = %config.url,
                attempt,
                error = %e,
                "Webhook delivery aborted: SSRF re-check failed"
            );
            return;
        }

        match deliver(&config.url, payload).await {
            Ok(status) if status.is_redirect() => {
                warn!(
                    webhook_id = %config.id,
                    url = %config.url,
                    attempt,
                    status = status.as_u16(),
                    "Webhook delivery failed: redirect response not followed"
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(s: &str) -> bool {
        is_blocked_ip(s.parse().unwrap())
    }

    #[test]
    fn blocks_loopback() {
        assert!(blocked("127.0.0.1"));
        assert!(blocked("127.255.255.254"));
        assert!(blocked("::1"));
    }

    #[test]
    fn blocks_rfc1918_private() {
        assert!(blocked("10.0.0.1"));
        assert!(blocked("10.255.255.255"));
        assert!(blocked("172.16.0.1"));
        assert!(blocked("172.31.255.255"));
        assert!(blocked("192.168.0.1"));
        assert!(blocked("192.168.255.255"));
    }

    #[test]
    fn blocks_link_local() {
        assert!(blocked("169.254.169.254"));
        assert!(blocked("fe80::1"));
    }

    #[test]
    fn blocks_unique_local() {
        assert!(blocked("fc00::1"));
        assert!(blocked("fd12:3456:789a::1"));
    }

    #[test]
    fn blocks_unspecified_and_multicast() {
        assert!(blocked("0.0.0.0"));
        assert!(blocked("::"));
        assert!(blocked("224.0.0.1"));
        assert!(blocked("ff02::1"));
    }

    #[test]
    fn allows_public_addresses() {
        assert!(!blocked("8.8.8.8"));
        assert!(!blocked("1.1.1.1"));
        assert!(!blocked("2606:4700:4700::1111"));
    }

    #[tokio::test]
    async fn validate_rejects_localhost() {
        assert!(validate_webhook_url("http://localhost:8080/hook").await.is_err());
        assert!(validate_webhook_url("http://127.0.0.1/hook").await.is_err());
        assert!(validate_webhook_url("http://[::1]/hook").await.is_err());
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data").await.is_err());
        assert!(validate_webhook_url("ftp://example.com/hook").await.is_err());
    }

    #[tokio::test]
    async fn redirect_is_not_followed() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
                let response = "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        let payload = WebhookPayload {
            event: SWAP_STATUS_CHANGED_EVENT.to_string(),
            swap_id: 1,
            old_status: None,
            new_status: "completed".to_string(),
            timestamp: 0,
        };
        let status = deliver(&format!("http://{addr}/hook"), &payload)
            .await
            .unwrap();
        assert!(status.is_redirect());
        assert_eq!(status.as_u16(), 302);
    }
}
