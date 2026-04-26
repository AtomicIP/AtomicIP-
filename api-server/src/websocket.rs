use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpEvent {
    pub event_type: String,
    pub ip_id: u64,
    pub owner: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapEvent {
    pub event_type: String,
    pub swap_id: u64,
    pub seller: String,
    pub buyer: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Event {
    IpEvent(IpEvent),
    SwapEvent(SwapEvent),
}

pub struct EventBroadcaster {
    ip_tx: broadcast::Sender<IpEvent>,
    swap_tx: broadcast::Sender<SwapEvent>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (ip_tx, _) = broadcast::channel(100);
        let (swap_tx, _) = broadcast::channel(100);
        Self { ip_tx, swap_tx }
    }

    pub fn broadcast_ip_event(&self, event: IpEvent) {
        let _ = self.ip_tx.send(event);
    }

    pub fn broadcast_swap_event(&self, event: SwapEvent) {
        let _ = self.swap_tx.send(event);
    }

    pub fn subscribe_ip(&self) -> broadcast::Receiver<IpEvent> {
        self.ip_tx.subscribe()
    }

    pub fn subscribe_swap(&self) -> broadcast::Receiver<SwapEvent> {
        self.swap_tx.subscribe()
    }
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionMessage {
    pub action: String,
    pub subscription_type: Option<String>,
}

pub async fn handle_socket(socket: WebSocket, broadcaster: Arc<EventBroadcaster>) {
    let (mut sender, mut receiver) = socket.split();

    let mut ip_rx = broadcaster.subscribe_ip();
    let mut swap_rx = broadcaster.subscribe_swap();

    let mut subscribed_ip = false;
    let mut subscribed_swap = false;

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(sub_msg) = serde_json::from_str::<SubscriptionMessage>(&text) {
                            match sub_msg.action.as_str() {
                                "subscribe_ip_events" => {
                                    subscribed_ip = true;
                                    let _ = sender.send(axum::extract::ws::Message::Text(
                                        r#"{"status":"subscribed","type":"ip_events"}"#.to_string()
                                    )).await;
                                }
                                "subscribe_swap_events" => {
                                    subscribed_swap = true;
                                    let _ = sender.send(axum::extract::ws::Message::Text(
                                        r#"{"status":"subscribed","type":"swap_events"}"#.to_string()
                                    )).await;
                                }
                                "unsubscribe_ip_events" => {
                                    subscribed_ip = false;
                                }
                                "unsubscribe_swap_events" => {
                                    subscribed_swap = false;
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
                        let _ = sender.send(axum::extract::ws::Message::Text(json)).await;
                    }
                }
            }
            event = swap_rx.recv(), if subscribed_swap => {
                if let Ok(event) = event {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let _ = sender.send(axum::extract::ws::Message::Text(json)).await;
                    }
                }
            }
        }
    }
}
