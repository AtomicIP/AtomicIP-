//! WebSocket event streaming.
//!
//! Two WebSocket endpoints are available:
//!
//! ## `GET /graphql/ws` — GraphQL subscriptions (`graphql-transport-ws`)
//!
//! Standard GraphQL subscription protocol supported by Apollo Client,
//! `graphql-ws`, urql, etc.  The handler is registered in `main.rs` using
//! `async_graphql_axum::GraphQLWebSocket` and `GraphQLProtocol`.
//!
//! Supported subscriptions and their filters are documented in [`crate::graphql`].
//!
//! ## `GET /ws` — Raw JSON WebSocket (legacy & real-time push)
//!
//! A pub/sub protocol for IP, swap, and swap state change events. Clients send JSON
//! messages with an `"action"` field to subscribe/unsubscribe:
//!
//! ```json
//! { "action": "subscribe_ip_events" }
//! { "action": "subscribe_swap_events" }
//! { "action": "subscribe_swap_status", "swap_id": 123 }
//! { "action": "subscribe_swap_status" }
//! { "action": "unsubscribe_swap_status", "swap_id": 123 }
//! { "action": "unsubscribe_swap_status" }
//! ```

use axum::extract::ws::WebSocket;
use futures::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpEvent {
    pub event_type: String,
    pub ip_id: u64,
    pub owner: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwapEvent {
    pub event_type: String,
    pub swap_id: u64,
    pub seller: String,
    pub buyer: String,
    pub timestamp: u64,
}

/// Real-time swap status change event pushed over WebSocket (#861)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwapStatusChangeEvent {
    pub event_type: String, // "swap_status_changed"
    pub swap_id: u64,
    pub old_status: Option<String>,
    pub new_status: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Event {
    IpEvent(IpEvent),
    SwapEvent(SwapEvent),
    SwapStatusChangeEvent(SwapStatusChangeEvent),
}

pub struct EventBroadcaster {
    ip_tx: broadcast::Sender<IpEvent>,
    swap_tx: broadcast::Sender<SwapEvent>,
    swap_status_tx: broadcast::Sender<SwapStatusChangeEvent>,
}

static DEFAULT_BROADCASTER: Lazy<Arc<EventBroadcaster>> = Lazy::new(|| Arc::new(EventBroadcaster::new()));

pub fn get_default_broadcaster() -> Arc<EventBroadcaster> {
    DEFAULT_BROADCASTER.clone()
}

pub fn trigger_swap_status_changed(swap_id: u64, old_status: Option<String>, new_status: String) {
    DEFAULT_BROADCASTER.trigger_swap_status_changed(swap_id, old_status, new_status);
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (ip_tx, _) = broadcast::channel(200);
        let (swap_tx, _) = broadcast::channel(200);
        let (swap_status_tx, _) = broadcast::channel(200);
        Self { ip_tx, swap_tx, swap_status_tx }
    }

    pub fn broadcast_ip_event(&self, event: IpEvent) {
        let _ = self.ip_tx.send(event);
    }

    pub fn broadcast_swap_event(&self, event: SwapEvent) {
        let _ = self.swap_tx.send(event);
    }

    pub fn broadcast_swap_status_change(&self, event: SwapStatusChangeEvent) {
        let _ = self.swap_status_tx.send(event);
    }

    pub fn trigger_swap_status_changed(&self, swap_id: u64, old_status: Option<String>, new_status: String) {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let event = SwapStatusChangeEvent {
            event_type: "swap_status_changed".to_string(),
            swap_id,
            old_status,
            new_status,
            timestamp,
        };
        self.broadcast_swap_status_change(event);
    }

    /// Bridge contract events from events.rs into WebSocket pushes (#861)
    pub fn handle_contract_event(&self, event: &crate::events::ContractEvent) {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match event {
            crate::events::ContractEvent::SwapInitiated { swap_id, ip_id: _, seller, buyer, price: _ } => {
                self.broadcast_swap_event(SwapEvent {
                    event_type: "swap_initiated".to_string(),
                    swap_id: *swap_id,
                    seller: seller.clone(),
                    buyer: buyer.clone(),
                    timestamp,
                });
                self.trigger_swap_status_changed(*swap_id, None, "Pending".to_string());
            }
            crate::events::ContractEvent::SwapAccepted { swap_id, .. } => {
                self.trigger_swap_status_changed(*swap_id, Some("Pending".to_string()), "Accepted".to_string());
            }
            crate::events::ContractEvent::SwapCompleted { swap_id, .. } => {
                self.trigger_swap_status_changed(*swap_id, Some("Accepted".to_string()), "Completed".to_string());
            }
            crate::events::ContractEvent::IpCommitted { ip_id, owner, .. } => {
                self.broadcast_ip_event(IpEvent {
                    event_type: "ip_committed".to_string(),
                    ip_id: *ip_id,
                    owner: owner.clone(),
                    timestamp,
                });
            }
        }
    }

    pub fn subscribe_ip(&self) -> broadcast::Receiver<IpEvent> {
        self.ip_tx.subscribe()
    }

    pub fn subscribe_swap(&self) -> broadcast::Receiver<SwapEvent> {
        self.swap_tx.subscribe()
    }

    pub fn subscribe_swap_status(&self) -> broadcast::Receiver<SwapStatusChangeEvent> {
        self.swap_status_tx.subscribe()
    }
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionMessage {
    pub action: String,
    pub subscription_type: Option<String>,
    pub swap_id: Option<u64>,
}

pub async fn handle_socket(socket: WebSocket, broadcaster: Arc<EventBroadcaster>) {
    let (mut sender, mut receiver) = socket.split();

    let mut ip_rx = broadcaster.subscribe_ip();
    let mut swap_rx = broadcaster.subscribe_swap();
    let mut swap_status_rx = broadcaster.subscribe_swap_status();

    let mut subscribed_ip = false;
    let mut subscribed_swap = false;
    let mut subscribed_all_swap_status = false;
    let mut subscribed_swap_ids: HashSet<u64> = HashSet::new();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(sub_msg) = serde_json::from_str::<SubscriptionMessage>(text.as_str()) {
                            match sub_msg.action.as_str() {
                                "subscribe_ip_events" => {
                                    subscribed_ip = true;
                                    let _ = sender.send(axum::extract::ws::Message::Text(
                                        r#"{"status":"subscribed","type":"ip_events"}"#.into()
                                    )).await;
                                }
                                "subscribe_swap_events" => {
                                    subscribed_swap = true;
                                    let _ = sender.send(axum::extract::ws::Message::Text(
                                        r#"{"status":"subscribed","type":"swap_events"}"#.into()
                                    )).await;
                                }
                                "subscribe_swap_status" => {
                                    if let Some(id) = sub_msg.swap_id {
                                        subscribed_swap_ids.insert(id);
                                        let ack = serde_json::json!({
                                            "status": "subscribed",
                                            "type": "swap_status",
                                            "swap_id": id
                                        });
                                        let _ = sender.send(axum::extract::ws::Message::Text(ack.to_string().into())).await;
                                    } else {
                                        subscribed_all_swap_status = true;
                                        let _ = sender.send(axum::extract::ws::Message::Text(
                                            r#"{"status":"subscribed","type":"swap_status"}"#.into()
                                        )).await;
                                    }
                                }
                                "unsubscribe_ip_events" => {
                                    subscribed_ip = false;
                                }
                                "unsubscribe_swap_events" => {
                                    subscribed_swap = false;
                                }
                                "unsubscribe_swap_status" => {
                                    if let Some(id) = sub_msg.swap_id {
                                        subscribed_swap_ids.remove(&id);
                                    } else {
                                        subscribed_all_swap_status = false;
                                        subscribed_swap_ids.clear();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) => break,
                    Some(Err(_)) => break,
                    None => break,
                    _ => {}
                }
            }
            event = ip_rx.recv(), if subscribed_ip => {
                if let Ok(event) = event {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let _ = sender.send(axum::extract::ws::Message::Text(json.into())).await;
                    }
                }
            }
            event = swap_rx.recv(), if subscribed_swap => {
                if let Ok(event) = event {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let _ = sender.send(axum::extract::ws::Message::Text(json.into())).await;
                    }
                }
            }
            event = swap_status_rx.recv(), if (subscribed_all_swap_status || !subscribed_swap_ids.is_empty()) => {
                if let Ok(event) = event {
                    if subscribed_all_swap_status || subscribed_swap_ids.contains(&event.swap_id) {
                        if let Ok(json) = serde_json::to_string(&event) {
                            let _ = sender.send(axum::extract::ws::Message::Text(json.into())).await;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_swap_status_change_broadcast_and_receive() {
        let broadcaster = Arc::new(EventBroadcaster::new());
        let mut rx = broadcaster.subscribe_swap_status();

        broadcaster.trigger_swap_status_changed(42, Some("Pending".to_string()), "Accepted".to_string());

        let received = timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("Receive timed out")
            .expect("Failed to receive swap status change");

        assert_eq!(received.swap_id, 42);
        assert_eq!(received.old_status, Some("Pending".to_string()));
        assert_eq!(received.new_status, "Accepted".to_string());
        assert_eq!(received.event_type, "swap_status_changed");
    }

    #[tokio::test]
    async fn test_contract_event_bridge_to_swap_status() {
        let broadcaster = Arc::new(EventBroadcaster::new());
        let mut status_rx = broadcaster.subscribe_swap_status();
        let mut swap_rx = broadcaster.subscribe_swap();

        let contract_event = crate::events::ContractEvent::SwapAccepted {
            swap_id: 100,
            buyer: "GBUYER123".to_string(),
        };

        broadcaster.handle_contract_event(&contract_event);

        let status_event = timeout(Duration::from_millis(500), status_rx.recv())
            .await
            .expect("Receive timed out")
            .unwrap();

        assert_eq!(status_event.swap_id, 100);
        assert_eq!(status_event.new_status, "Accepted");

        let initiate_contract_event = crate::events::ContractEvent::SwapInitiated {
            swap_id: 200,
            ip_id: 1,
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 5000,
        };

        broadcaster.handle_contract_event(&initiate_contract_event);

        let swap_event = timeout(Duration::from_millis(500), swap_rx.recv())
            .await
            .expect("Receive timed out")
            .unwrap();

        assert_eq!(swap_event.swap_id, 200);
        assert_eq!(swap_event.seller, "GSELLER");
    }

    #[tokio::test]
    async fn test_multiple_swap_status_subscribers_bounded_time() {
        let broadcaster = Arc::new(EventBroadcaster::new());
        let mut rx1 = broadcaster.subscribe_swap_status();
        let mut rx2 = broadcaster.subscribe_swap_status();

        let start = std::time::Instant::now();
        broadcaster.trigger_swap_status_changed(777, Some("Accepted".to_string()), "Completed".to_string());

        let ev1 = timeout(Duration::from_millis(200), rx1.recv()).await.unwrap().unwrap();
        let ev2 = timeout(Duration::from_millis(200), rx2.recv()).await.unwrap().unwrap();

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(100), "Delivery took too long: {:?}", elapsed);
        assert_eq!(ev1.swap_id, 777);
        assert_eq!(ev2.swap_id, 777);
        assert_eq!(ev1.new_status, "Completed");
    }
}

