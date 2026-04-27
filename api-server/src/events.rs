use axum::{
    extract::State,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContractEvent {
    #[serde(rename = "ip_committed")]
    IpCommitted {
        ip_id: u64,
        owner: String,
        commitment_hash: String,
        timestamp: u64,
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
        seller: String,
        buyer: String,
    },
}

pub type EventBroadcaster = broadcast::Sender<ContractEvent>;

pub fn create_event_broadcaster() -> (EventBroadcaster, broadcast::Receiver<ContractEvent>) {
    broadcast::channel(1000)
}

/// Server-Sent Events endpoint for real-time contract events
#[utoipa::path(
    get,
    path = "/events",
    tag = "Events",
    responses(
        (status = 200, description = "Event stream established", content_type = "text/event-stream"),
    )
)]
pub async fn events_handler(
    State(broadcaster): State<Arc<EventBroadcaster>>,
) -> impl IntoResponse {
    let receiver = broadcaster.subscribe();
    
    let stream = tokio_stream::wrappers::BroadcastStream::new(receiver)
        .filter_map(|result| match result {
            Ok(event) => Some(Ok(Event::default().json_data(event).unwrap())),
            Err(_) => None, // Skip lagged messages
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
        
        let event = ContractEvent::IpCommitted {
            ip_id: 1,
            owner: "test_owner".to_string(),
            commitment_hash: "test_hash".to_string(),
            timestamp: 1234567890,
        };
        
        broadcast_event(&broadcaster, event.clone()).await;
        
        let received = receiver.recv().await.unwrap();
        match received {
            ContractEvent::IpCommitted { ip_id, .. } => assert_eq!(ip_id, 1),
            _ => panic!("Wrong event type"),
        }
    }
}
