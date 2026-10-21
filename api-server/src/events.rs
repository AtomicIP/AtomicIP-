use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::StreamExt as _;
use crate::event_topics;
use crate::websocket::{ConnectionGuard, ConnectionLimiter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContractEvent {
    #[serde(rename = "ip_revoked")]
    IpRevoked {
        ip_id: u64,
        owner: String,
        timestamp: u64,
    },
    #[serde(rename = "ip_transferred")]
    IpTransferred {
        ip_id: u64,
        from_owner: String,
        to_owner: String,
        timestamp: u64,
    },
    #[serde(rename = "batch_verified")]
    BatchVerified {
        ip_ids: Vec<u64>,
        verifier: String,
        timestamp: u64,
    },
    #[serde(rename = "ip_expired")]
    IpExpired {
        ip_id: u64,
        owner: String,
        expiry_timestamp: u64,
    },
    #[serde(rename = "swap_initiated")]
    SwapInitiated {
        swap_id: u64,
        ip_id: u64,
        seller: String,
        buyer: String,
        price: u64,
    },
    #[serde(rename = "swap_accepted")]
    SwapAccepted {
        swap_id: u64,
        buyer: String,
    },
    #[serde(rename = "swap_completed")]
    SwapCompleted {
        swap_id: u64,
    },
    #[serde(rename = "ip_committed")]
    IpCommitted {
        ip_id: u64,
        owner: String,
        timestamp: u64,
    },
    #[serde(rename = "key_revealed")]
    KeyRevealed {
        swap_id: u64,
        seller: String,
        timestamp: u64,
    },
    #[serde(rename = "swap_cancelled")]
    SwapCancelled {
        swap_id: u64,
        canceller: String,
        reason: String,
    },
    #[serde(rename = "protocol_fee")]
    ProtocolFee {
        amount: u64,
        token: String,
        timestamp: u64,
    },
    #[serde(rename = "dispute_raised")]
    DisputeRaised {
        swap_id: u64,
        initiator: String,
        reason: String,
    },
    #[serde(rename = "dispute_resolved")]
    DisputeResolved {
        swap_id: u64,
        resolution: String,
    },
    #[serde(rename = "unknown")]
    Unknown {
        topic: String,
        data: String,
    },
}

pub type EventBroadcaster = broadcast::Sender<ContractEvent>;

pub fn create_event_broadcaster() -> (EventBroadcaster, broadcast::Receiver<ContractEvent>) {
    broadcast::channel(1000)
}

/// Validates that a topic string matches one of the known canonical topics.
/// This ensures events from contracts are correctly recognized and parsed.
pub fn is_valid_contract_topic(topic: &str) -> bool {
    event_topics::all_known_topics().contains(&topic)
}

/// Validates all topics from a batch of events.
/// Returns the first invalid topic, if any.
pub fn validate_batch_topics(topics: &[&str]) -> Result<(), String> {
    for topic in topics {
        if !is_valid_contract_topic(topic) {
            return Err(format!(
                "Unknown event topic: '{}'. Known topics: {}",
                topic,
                event_topics::all_known_topics().join(", ")
            ));
        }
    }
    Ok(())
}

/// Parse an event by its topic string.
/// Returns Ok(ContractEvent) if topic is recognized, or Unknown variant if not.
pub fn parse_event_by_topic(topic: &str, data: String) -> ContractEvent {
    // Validate topic against canonical list
    if !is_valid_contract_topic(topic) {
        return ContractEvent::Unknown {
            topic: topic.to_string(),
            data,
        };
    }

    // Match against known topics
    match topic {
        t if t == event_topics::ip_registry::REVOKE_TOPIC => ContractEvent::IpRevoked {
            ip_id: 0,
            owner: String::new(),
            timestamp: 0,
        },
        t if t == event_topics::ip_registry::TRANSFER_TOPIC => ContractEvent::IpTransferred {
            ip_id: 0,
            from_owner: String::new(),
            to_owner: String::new(),
            timestamp: 0,
        },
        t if t == event_topics::ip_registry::BATCH_VERIFY_TOPIC => ContractEvent::BatchVerified {
            ip_ids: Vec::new(),
            verifier: String::new(),
            timestamp: 0,
        },
        t if t == event_topics::ip_registry::EXPIRY_TOPIC => ContractEvent::IpExpired {
            ip_id: 0,
            owner: String::new(),
            expiry_timestamp: 0,
        },
        t if t == event_topics::atomic_swap::SWAP_INITIATED_TOPIC => {
            ContractEvent::SwapInitiated {
                swap_id: 0,
                ip_id: 0,
                seller: String::new(),
                buyer: String::new(),
                price: 0,
            }
        }
        t if t == event_topics::atomic_swap::SWAP_ACCEPTED_TOPIC => ContractEvent::SwapAccepted {
            swap_id: 0,
            buyer: String::new(),
        },
        t if t == event_topics::atomic_swap::KEY_REVEALED_TOPIC => ContractEvent::KeyRevealed {
            swap_id: 0,
            seller: String::new(),
            timestamp: 0,
        },
        t if t == event_topics::atomic_swap::SWAP_CANCELLED_TOPIC => {
            ContractEvent::SwapCancelled {
                swap_id: 0,
                canceller: String::new(),
                reason: String::new(),
            }
        }
        t if t == event_topics::atomic_swap::PROTOCOL_FEE_TOPIC => ContractEvent::ProtocolFee {
            amount: 0,
            token: String::new(),
            timestamp: 0,
        },
        t if t == event_topics::atomic_swap::DISPUTE_RAISED_TOPIC => {
            ContractEvent::DisputeRaised {
                swap_id: 0,
                initiator: String::new(),
                reason: String::new(),
            }
        }
        t if t == event_topics::atomic_swap::DISPUTE_RESOLVED_TOPIC => {
            ContractEvent::DisputeResolved {
                swap_id: 0,
                resolution: String::new(),
            }
        }
        _ => ContractEvent::Unknown {
            topic: topic.to_string(),
            data,
        },
    }
}

/// Maximum time an SSE connection may remain idle (no events sent) before it is closed.
const SSE_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Extracts the client IP from `X-Forwarded-For` (first hop) or falls back to a
/// generic key when the header is absent.
fn client_ip_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|ip| ip.trim().to_string())
        .filter(|ip| !ip.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Server-Sent Events endpoint for real-time contract events
#[utoipa::path(
    get,
    path = "/events",
    tag = "Events",
    responses(
        (status = 200, description = "Event stream established", content_type = "text/event-stream"),
        (status = 429, description = "Too many connections"),
        (status = 503, description = "Connection limit reached"),
    )
)]
pub async fn events_handler(
    State(broadcaster): State<Arc<EventBroadcaster>>,
    State(limiter): State<Arc<ConnectionLimiter>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let client_ip = client_ip_from_headers(&headers);

    // Acquire a connection slot before establishing the stream. The guard is
    // moved into the stream so the slot is released when the stream is dropped.
    let guard = match limiter.try_acquire(&client_ip) {
        Ok(guard) => guard,
        Err(reason) => {
            let status = match reason {
                crate::websocket::LimitError::PerIp => StatusCode::TOO_MANY_REQUESTS,
                crate::websocket::LimitError::Global => StatusCode::SERVICE_UNAVAILABLE,
            };
            return (status, "connection limit reached").into_response();
        }
    };

    let receiver = broadcaster.subscribe();

    let stream = tokio_stream::wrappers::BroadcastStream::new(receiver)
        .filter_map(|result| match result {
            Ok(event) => Some(Ok::<axum::response::sse::Event, std::convert::Infallible>(Event::default().json_data(event).unwrap())),
            Err(_) => None, // Skip lagged messages
        })
        // Close the connection after the idle timeout so stale streams are
        // not kept open indefinitely.
        .take_until(tokio::time::sleep(SSE_IDLE_TIMEOUT))
        // Hold the guard for the lifetime of the stream so the slot is
        // released when the client disconnects.
        .map(move |item| {
            let _guard = &guard;
            item
        });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

pub async fn broadcast_event(broadcaster: &EventBroadcaster, event: ContractEvent) {
    let _ = broadcaster.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_broadcasting() {
        let (broadcaster, mut receiver) = create_event_broadcaster();

        let event = ContractEvent::IpRevoked {
            ip_id: 1,
            owner: "test_owner".to_string(),
            timestamp: 1234567890,
        };

        broadcast_event(&broadcaster, event.clone()).await;

        let received = receiver.recv().await.unwrap();
        match received {
            ContractEvent::IpRevoked { ip_id, .. } => assert_eq!(ip_id, 1),
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_all_ip_registry_topics_are_valid() {
        assert!(is_valid_contract_topic(event_topics::ip_registry::REVOKE_TOPIC));
        assert!(is_valid_contract_topic(event_topics::ip_registry::TRANSFER_TOPIC));
        assert!(is_valid_contract_topic(event_topics::ip_registry::BATCH_VERIFY_TOPIC));
        assert!(is_valid_contract_topic(event_topics::ip_registry::EXPIRY_TOPIC));
    }

    #[test]
    fn test_all_atomic_swap_topics_are_valid() {
        assert!(is_valid_contract_topic(event_topics::atomic_swap::SWAP_INITIATED_TOPIC));
        assert!(is_valid_contract_topic(event_topics::atomic_swap::SWAP_ACCEPTED_TOPIC));
        assert!(is_valid_contract_topic(event_topics::atomic_swap::KEY_REVEALED_TOPIC));
        assert!(is_valid_contract_topic(event_topics::atomic_swap::SWAP_CANCELLED_TOPIC));
        assert!(is_valid_contract_topic(event_topics::atomic_swap::PROTOCOL_FEE_TOPIC));
        assert!(is_valid_contract_topic(event_topics::atomic_swap::DISPUTE_RAISED_TOPIC));
        assert!(is_valid_contract_topic(event_topics::atomic_swap::DISPUTE_RESOLVED_TOPIC));
    }

    #[test]
    fn test_invalid_topic_rejected() {
        assert!(!is_valid_contract_topic("invalid_topic"));
        assert!(!is_valid_contract_topic(""));
        assert!(!is_valid_contract_topic("unknown_event"));
    }

    #[test]
    fn test_batch_topics_validation_success() {
        let topics = vec![
            event_topics::ip_registry::REVOKE_TOPIC,
            event_topics::ip_registry::TRANSFER_TOPIC,
            event_topics::atomic_swap::SWAP_INITIATED_TOPIC,
        ];
        assert!(validate_batch_topics(&topics).is_ok());
    }

    #[test]
    fn test_batch_topics_validation_failure() {
        let topics = vec![
            event_topics::ip_registry::REVOKE_TOPIC,
            "invalid_topic",
        ];
        assert!(validate_batch_topics(&topics).is_err());
    }

    #[test]
    fn test_parse_event_by_topic_ip_revoke() {
        let event = parse_event_by_topic(event_topics::ip_registry::REVOKE_TOPIC, String::new());
        match event {
            ContractEvent::IpRevoked { .. } => {}
            _ => panic!("Expected IpRevoked event"),
        }
    }

    #[test]
    fn test_parse_event_by_topic_swap_initiated() {
        let event = parse_event_by_topic(
            event_topics::atomic_swap::SWAP_INITIATED_TOPIC,
            String::new(),
        );
        match event {
            ContractEvent::SwapInitiated { .. } => {}
            _ => panic!("Expected SwapInitiated event"),
        }
    }

    #[test]
    fn test_parse_event_by_topic_unknown() {
        let event = parse_event_by_topic("unknown_topic", "test_data".to_string());
        match event {
            ContractEvent::Unknown { topic, data } => {
                assert_eq!(topic, "unknown_topic");
                assert_eq!(data, "test_data");
            }
            _ => panic!("Expected Unknown event"),
        }
    }

    #[test]
    fn test_event_topic_consistency_ip_registry() {
        // Verify IP Registry topics match Soroban symbol_short format
        let topics = vec![
            ("revoke", event_topics::ip_registry::REVOKE_TOPIC),
            ("ip_xfer", event_topics::ip_registry::TRANSFER_TOPIC),
            ("batch_vfy", event_topics::ip_registry::BATCH_VERIFY_TOPIC),
            ("ip_expiry", event_topics::ip_registry::EXPIRY_TOPIC),
        ];
        for (expected, actual) in topics {
            assert_eq!(
                actual, expected,
                "Topic mismatch. Contract defines '{}', but events.rs has '{}'",
                expected, actual
            );
        }
    }

    #[test]
    fn test_client_ip_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(client_ip_from_headers(&headers), "203.0.113.7");

        let empty = HeaderMap::new();
        assert_eq!(client_ip_from_headers(&empty), "unknown");
    }

    #[test]
    fn test_event_topic_consistency_atomic_swap() {
        // Verify Atomic Swap topics match contract definitions
        let topics = vec![
            ("swap_init", event_topics::atomic_swap::SWAP_INITIATED_TOPIC),
            ("swap_accept", event_topics::atomic_swap::SWAP_ACCEPTED_TOPIC),
            ("key_reveal", event_topics::atomic_swap::KEY_REVEALED_TOPIC),
            ("swap_cancel", event_topics::atomic_swap::SWAP_CANCELLED_TOPIC),
            ("protocol_fee", event_topics::atomic_swap::PROTOCOL_FEE_TOPIC),
            ("dispute_raised", event_topics::atomic_swap::DISPUTE_RAISED_TOPIC),
            ("dispute_resolved", event_topics::atomic_swap::DISPUTE_RESOLVED_TOPIC),
        ];
        for (expected, actual) in topics {
            assert_eq!(
                actual, expected,
                "Topic mismatch. Contract defines '{}', but events.rs has '{}'",
                expected, actual
            );
        }
    }
}
