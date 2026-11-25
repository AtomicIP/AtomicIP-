use std::borrow::Borrow;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use axum::http::StatusCode;
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use tracing::{error, info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Event name emitted when a swap changes status.
pub const SWAP_STATUS_CHANGED_EVENT: &str = "swap.status_changed";

/// Header carrying the hex-encoded HMAC-SHA256 signature of `"{timestamp}.{body}"`.
pub const WEBHOOK_SIGNATURE_HEADER: &str = "X-Webhook-Signature";
/// Header carrying the Unix timestamp (seconds) that was signed.
pub const WEBHOOK_TIMESTAMP_HEADER: &str = "X-Webhook-Timestamp";
/// Header carrying the id of the webhook the payload was emitted for.
pub const WEBHOOK_ID_HEADER: &str = "X-Webhook-Id";

/// Number of random bytes in a generated webhook secret (256 bits).
const WEBHOOK_SECRET_BYTES: usize = 32;

/// Configuration for a registered webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>, // e.g., ["swap.status_changed"]
    pub created_at: u64,
    /// Owner (Stellar address) this webhook belongs to.
    pub owner: String,
    /// Per-webhook HMAC-SHA256 secret. Generated once at registration and
    /// returned to the caller exactly once (in the registration response);
    /// it is never included in listing responses.
    pub secret: String,
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

/// Errors produced while validating, registering, or delivering a webhook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookError {
    /// The URL could not be parsed.
    InvalidUrl(String),
    /// The URL scheme is not `http`/`https`.
    InvalidScheme(String),
    /// DNS resolution failed.
    DnsResolution { host: String, reason: String },
    /// The host resolved to an address the SSRF guard blocks.
    BlockedAddress { host: String, ip: IpAddr },
    /// The host did not resolve to any address.
    NoAddress(String),
    /// Delivery failed.
    Delivery(String),
}

impl fmt::Display for WebhookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebhookError::InvalidUrl(reason) => write!(f, "invalid URL: {reason}"),
            WebhookError::InvalidScheme(scheme) => write!(f, "unsupported scheme: {scheme}"),
            WebhookError::DnsResolution { host, reason } => {
                write!(f, "failed to resolve host {host}: {reason}")
            }
            WebhookError::BlockedAddress { host, ip } => {
                write!(f, "webhook target {host} resolves to blocked address {ip}")
            }
            WebhookError::NoAddress(host) => write!(f, "host {host} did not resolve to any address"),
            WebhookError::Delivery(reason) => write!(f, "webhook delivery failed: {reason}"),
        }
    }
}

impl std::error::Error for WebhookError {}

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
///
/// Accepts `IpAddr` or `&IpAddr` so callers can pass either.
pub fn is_blocked_ip(ip: impl Borrow<IpAddr>) -> bool {
    match *ip.borrow() {
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
pub async fn validate_webhook_url(url: &str) -> Result<(), WebhookError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| WebhookError::InvalidUrl(e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(WebhookError::InvalidScheme(other.to_string())),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| WebhookError::InvalidUrl("URL has no host".to_string()))?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| WebhookError::DnsResolution {
            host: host.to_string(),
            reason: e.to_string(),
        })?;
    let mut resolved_any = false;
    for addr in addrs {
        resolved_any = true;
        if is_blocked_ip(addr.ip()) {
            return Err(WebhookError::BlockedAddress {
                host: host.to_string(),
                ip: addr.ip(),
            });
        }
    }
    if !resolved_any {
        return Err(WebhookError::NoAddress(host.to_string()));
    }
    Ok(())
}

/// Generate a new per-webhook secret: 32 random bytes, hex-encoded.
pub fn generate_webhook_secret() -> String {
    let mut bytes = [0u8; WEBHOOK_SECRET_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Current Unix time in whole seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Register a new webhook for `owner`. The generated secret is returned once
/// inside the config; callers must surface it in the registration response and
/// must not include it in listings.
pub fn register(url: String, events: Vec<String>, owner: String) -> WebhookConfig {
    let config = WebhookConfig {
        id: Uuid::new_v4(),
        url,
        events,
        created_at: now_secs(),
        owner,
        secret: generate_webhook_secret(),
    };
    REGISTRY.insert(config.id, config.clone());
    info!(
        webhook_id = %config.id,
        url = %config.url,
        owner = %config.owner,
        "Webhook registered"
    );
    config
}

pub fn supports_event(events: &[String], event: &str) -> bool {
    events
        .iter()
        .any(|registered| registered == event || registered == "*")
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

/// Look up a webhook by ID.
pub fn get(id: Uuid) -> Option<WebhookConfig> {
    REGISTRY.get(&id).map(|entry| entry.clone())
}

/// List all registered webhooks.
pub fn list_all() -> Vec<WebhookConfig> {
    REGISTRY.iter().map(|entry| entry.clone()).collect()
}

/// Remove every registered webhook. Intended for tests.
pub fn clear_all() {
    REGISTRY.clear();
}

/// Build the canonical bytes that are signed: `"{timestamp}.{body}"`.
///
/// Binding the timestamp into the signature lets a receiver reject stale or
/// replayed notifications after checking the signature.
pub fn signing_message(timestamp: u64, body: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(body.len() + 24);
    message.extend_from_slice(timestamp.to_string().as_bytes());
    message.extend_from_slice(b".");
    message.extend_from_slice(body);
    message
}

/// Compute the hex-encoded HMAC-SHA256 signature over `"{timestamp}.{body}"`.
pub fn sign_payload(secret: &str, timestamp: u64, body: &[u8]) -> String {
    let message = signing_message(timestamp, body);
    hmac_sha256_hex(secret.as_bytes(), &message)
}

/// Alias for [`sign_payload`] for callers that prefer the longer name.
pub fn compute_webhook_signature(secret: &str, timestamp: u64, body: &[u8]) -> String {
    sign_payload(secret, timestamp, body)
}

/// Verify a hex-encoded HMAC-SHA256 signature over `"{timestamp}.{body}"`.
///
/// The comparison is constant-time so it does not leak the expected signature
/// through timing.
pub fn verify_payload_signature(
    secret: &str,
    timestamp: u64,
    body: &[u8],
    signature: &str,
) -> bool {
    let expected = sign_payload(secret, timestamp, body);
    constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

/// Alias for [`verify_payload_signature`].
pub fn verify_webhook_signature(secret: &str, timestamp: u64, body: &[u8], signature: &str) -> bool {
    verify_payload_signature(secret, timestamp, body, signature)
}

/// Parse a signature header value.
///
/// Accepts either a bare hex digest (the format this server sends) or the
/// Stripe-style `t=<timestamp>,v1=<hex>` envelope, returning `(timestamp, hex)`.
pub fn parse_signature_header(value: &str) -> Option<(Option<u64>, String)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(hex) = value.strip_prefix("v1=") {
        return Some((None, hex.trim().to_string()));
    }
    if value.contains(",v1=") || value.contains("&v1=") {
        let mut timestamp = None;
        let mut signature = None;
        for part in value.split([',', '&']) {
            let mut kv = part.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("t"), Some(ts)) => timestamp = ts.trim().parse::<u64>().ok(),
                (Some("v1"), Some(hex)) => signature = Some(hex.trim().to_string()),
                _ => {}
            }
        }
        return signature.map(|hex| (timestamp, hex));
    }
    // Bare hex digest.
    Some((None, value.to_string()))
}

fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(message);
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Outcome of a single webhook delivery attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Success { status: u16 },
    Failure { reason: String },
}

/// HTTP client that delivers signed webhook payloads.
pub struct WebhookClient {
    client: Client,
    /// Test-only marker: allows delivering to loopback addresses. Production
    /// delivery always runs the SSRF re-check in [`validate_webhook_url`].
    #[allow(dead_code)]
    allow_loopback: bool,
}

impl Default for WebhookClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WebhookClient {
    pub fn new() -> Self {
        Self {
            client: CLIENT.clone(),
            allow_loopback: false,
        }
    }

    /// Test constructor that permits loopback targets (the SSRF guard is
    /// exercised separately).
    pub fn for_tests_allow_loopback() -> Self {
        Self {
            client: CLIENT.clone(),
            allow_loopback: true,
        }
    }

    /// Deliver a payload to `config`, signing `"{timestamp}.{body}"` with the
    /// webhook's secret and sending the signature in [`WEBHOOK_SIGNATURE_HEADER`].
    pub async fn deliver(&self, config: &WebhookConfig, payload: &WebhookPayload) -> DeliveryOutcome {
        let body = match serde_json::to_vec(payload) {
            Ok(body) => body,
            Err(e) => {
                return DeliveryOutcome::Failure {
                    reason: format!("failed to serialize payload: {e}"),
                }
            }
        };
        let signature = sign_payload(&config.secret, payload.timestamp, &body);
        let response = self
            .client
            .post(&config.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(WEBHOOK_SIGNATURE_HEADER, signature)
            .header(WEBHOOK_TIMESTAMP_HEADER, payload.timestamp.to_string())
            .header(WEBHOOK_ID_HEADER, config.id.to_string())
            .body(body)
            .send()
            .await;
        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_redirection() {
                    DeliveryOutcome::Failure {
                        reason: format!(
                            "redirect response ({}) not followed",
                            status.as_u16()
                        ),
                    }
                } else if status.is_success() {
                    DeliveryOutcome::Success {
                        status: status.as_u16(),
                    }
                } else {
                    DeliveryOutcome::Failure {
                        reason: format!("non-success status {}", status.as_u16()),
                    }
                }
            }
            Err(e) => DeliveryOutcome::Failure {
                reason: e.to_string(),
            },
        }
    }

    /// Test helper: deliver a raw body without signing. Used to exercise
    /// redirect handling against a local server.
    pub async fn deliver_for_tests(&self, url: &str, body: &[u8]) -> DeliveryOutcome {
        let response = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_vec())
            .send()
            .await;
        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_redirection() {
                    DeliveryOutcome::Failure {
                        reason: format!(
                            "redirect response ({}) not followed",
                            status.as_u16()
                        ),
                    }
                } else if status.is_success() {
                    DeliveryOutcome::Success {
                        status: status.as_u16(),
                    }
                } else {
                    DeliveryOutcome::Failure {
                        reason: format!("non-success status {}", status.as_u16()),
                    }
                }
            }
            Err(e) => DeliveryOutcome::Failure {
                reason: e.to_string(),
            },
        }
    }
}

/// Trigger webhook delivery for a swap status change.
pub fn trigger_swap_status_changed(swap_id: u64, old_status: Option<String>, new_status: String) {
    let payload = WebhookPayload {
        event: SWAP_STATUS_CHANGED_EVENT.to_string(),
        swap_id,
        old_status,
        new_status,
        timestamp: now_secs(),
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
    let client = WebhookClient::new();

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

        match client.deliver(config, payload).await {
            DeliveryOutcome::Success { status } => {
                info!(
                    webhook_id = %config.id,
                    url = %config.url,
                    attempt,
                    status,
                    "Webhook delivered successfully"
                );
                return;
            }
            DeliveryOutcome::Failure { reason } => {
                if reason.contains("redirect") {
                    warn!(
                        webhook_id = %config.id,
                        url = %config.url,
                        attempt,
                        "Webhook delivery failed: redirect response not followed"
                    );
                } else {
                    warn!(
                        webhook_id = %config.id,
                        url = %config.url,
                        attempt,
                        reason = %reason,
                        "Webhook delivery failed"
                    );
                }
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

/// Single unsigned delivery attempt (legacy helper used by unit tests).
async fn deliver(url: &str, payload: &WebhookPayload) -> Result<StatusCode, reqwest::Error> {
    let response = CLIENT.post(url).json(&json!(payload)).send().await?;
    Ok(response.status())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn blocked(s: &str) -> bool {
        is_blocked_ip(s.parse::<IpAddr>().unwrap())
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

    #[test]
    fn blocked_ip_accepts_reference_and_value() {
        let addr: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(is_blocked_ip(&addr));
        assert!(is_blocked_ip(addr));
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
    async fn validate_rejects_bad_scheme_with_structured_error() {
        match validate_webhook_url("ftp://example.com/hook").await {
            Err(WebhookError::InvalidScheme(scheme)) => assert_eq!(scheme, "ftp"),
            other => panic!("expected InvalidScheme, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn redirect_is_not_followed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
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
        assert!(status.is_redirection());
        assert_eq!(status.as_u16(), 302);
    }

    // ── HMAC signing ────────────────────────────────────────────────────────

    #[test]
    fn signing_message_binds_timestamp_and_body() {
        let message = signing_message(1700000000, b"{\"a\":1}");
        assert_eq!(message, b"1700000000.{\"a\":1}");
    }

    /// Known-answer test vector for the documented signature scheme.
    #[test]
    fn hmac_test_vector() {
        let secret = "whsec_test_secret_0123456789abcdef";
        let timestamp = 1_700_000_000u64;
        let body = br#"{"event":"swap.status_changed","swap_id":42,"old_status":"Pending","new_status":"Accepted","timestamp":1700000000}"#;
        assert_eq!(
            sign_payload(secret, timestamp, body),
            "51866deebfb87cff0f5dbe94110ab42e7a918259a2222d538d2b82e585e0854a"
        );
        assert!(verify_payload_signature(
            secret,
            timestamp,
            body,
            "51866deebfb87cff0f5dbe94110ab42e7a918259a2222d538d2b82e585e0854a"
        ));
    }

    #[test]
    fn hmac_test_vector_null_old_status() {
        let secret = "whsec_test_secret_0123456789abcdef";
        let timestamp = 1_700_000_123u64;
        let body = br#"{"event":"swap.status_changed","swap_id":7,"old_status":null,"new_status":"Completed","timestamp":1700000123}"#;
        assert_eq!(
            sign_payload(secret, timestamp, body),
            "df4c695930051364d051639db95a842f8e353c7f368f4ead8e6a996e7422720e"
        );
    }

    #[test]
    fn verify_rejects_tampered_body() {
        let secret = "whsec_test_secret_0123456789abcdef";
        let timestamp = 1_700_000_000u64;
        let body = br#"{"event":"swap.status_changed","swap_id":42,"old_status":"Pending","new_status":"Accepted","timestamp":1700000000}"#;
        let signature = sign_payload(secret, timestamp, body);
        let tampered = br#"{"event":"swap.status_changed","swap_id":43,"old_status":"Pending","new_status":"Accepted","timestamp":1700000000}"#;
        assert!(!verify_payload_signature(secret, timestamp, tampered, &signature));
    }

    #[test]
    fn verify_rejects_tampered_timestamp() {
        let secret = "whsec_test_secret_0123456789abcdef";
        let timestamp = 1_700_000_000u64;
        let body = br#"{"event":"swap.status_changed","swap_id":42,"old_status":"Pending","new_status":"Accepted","timestamp":1700000000}"#;
        let signature = sign_payload(secret, timestamp, body);
        assert!(!verify_payload_signature(secret, timestamp + 1, body, &signature));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let timestamp = 1_700_000_000u64;
        let body = br#"{"event":"swap.status_changed","swap_id":42,"old_status":"Pending","new_status":"Accepted","timestamp":1700000000}"#;
        let signature = sign_payload("correct-secret", timestamp, body);
        assert!(!verify_payload_signature("wrong-secret", timestamp, body, &signature));
    }

    #[test]
    fn generated_secrets_are_unique_and_long() {
        let a = generate_webhook_secret();
        let b = generate_webhook_secret();
        assert_eq!(a.len(), WEBHOOK_SECRET_BYTES * 2);
        assert_eq!(b.len(), WEBHOOK_SECRET_BYTES * 2);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn registration_generates_a_secret() {
        clear_all();
        let config = register(
            "https://example.com/hook".to_string(),
            vec![SWAP_STATUS_CHANGED_EVENT.to_string()],
            "GOWNER".to_string(),
        );
        assert_eq!(config.secret.len(), WEBHOOK_SECRET_BYTES * 2);
        assert_eq!(config.owner, "GOWNER");
        clear_all();
    }

    #[test]
    fn parse_signature_header_formats() {
        assert_eq!(
            parse_signature_header("abc123"),
            Some((None, "abc123".to_string()))
        );
        assert_eq!(
            parse_signature_header("v1=abc123"),
            Some((None, "abc123".to_string()))
        );
        assert_eq!(
            parse_signature_header("t=1700000000,v1=abc123"),
            Some((Some(1700000000), "abc123".to_string()))
        );
        assert_eq!(parse_signature_header("   "), None);
    }

    #[tokio::test]
    async fn deliver_sends_signature_header() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read until end of headers (and any body).
                loop {
                    match socket.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                // Give the body a brief moment to arrive.
                                tokio::time::sleep(Duration::from_millis(20)).await;
                                let _ = socket.read(&mut chunk).await;
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let request = String::from_utf8_lossy(&buf).to_string();
                let _ = tx.send(request);
                let response =
                    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        let payload = WebhookPayload {
            event: SWAP_STATUS_CHANGED_EVENT.to_string(),
            swap_id: 42,
            old_status: Some("Pending".to_string()),
            new_status: "Accepted".to_string(),
            timestamp: 1_700_000_000,
        };
        let body = serde_json::to_vec(&payload).unwrap();
        let secret = "whsec_test_secret_0123456789abcdef".to_string();
        let config = WebhookConfig {
            id: Uuid::new_v4(),
            url: format!("http://{addr}/hook"),
            events: vec![SWAP_STATUS_CHANGED_EVENT.to_string()],
            created_at: 0,
            owner: "GOWNER".to_string(),
            secret: secret.clone(),
        };
        let expected_signature = sign_payload(&secret, payload.timestamp, &body);

        let outcome = WebhookClient::for_tests_allow_loopback()
            .deliver(&config, &payload)
            .await;
        assert_eq!(outcome, DeliveryOutcome::Success { status: 200 });

        let request = rx.await.expect("server should capture request");
        let signature_header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-webhook-signature:"))
            .expect("signature header present");
        let sent_signature = signature_header.split(':').nth(1).unwrap().trim();
        assert_eq!(sent_signature, expected_signature);

        let timestamp_header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-webhook-timestamp:"))
            .expect("timestamp header present");
        assert_eq!(
            timestamp_header.split(':').nth(1).unwrap().trim(),
            "1700000000"
        );

        let id_header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-webhook-id:"))
            .expect("id header present");
        assert_eq!(
            id_header.split(':').nth(1).unwrap().trim(),
            config.id.to_string()
        );
    }
}
