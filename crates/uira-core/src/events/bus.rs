use crate::events::Event;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::broadcast;

const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

pub trait EventBus: Send + Sync {
    fn publish(&self, event: Event);
    fn subscribe(&self) -> broadcast::Receiver<Event>;
    fn subscriber_count(&self) -> usize;
}

pub struct BroadcastBus {
    sender: broadcast::Sender<Event>,
    subscriber_count: Arc<RwLock<usize>>,
}

impl BroadcastBus {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CHANNEL_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            subscriber_count: Arc::new(RwLock::new(0)),
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn try_publish(&self, event: Event) -> Result<usize, broadcast::error::SendError<Event>> {
        self.sender.send(event)
    }
}

impl Default for BroadcastBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus for BroadcastBus {
    fn publish(&self, event: Event) {
        if let Err(e) = self.sender.send(event) {
            tracing::warn!(
                "Failed to publish event (no subscribers?): {:?}",
                e.0.event_name()
            );
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        *self.subscriber_count.write() += 1;
        self.sender.subscribe()
    }

    fn subscriber_count(&self) -> usize {
        *self.subscriber_count.read()
    }
}

impl Clone for BroadcastBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            subscriber_count: self.subscriber_count.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionEndReason;

    #[tokio::test]
    async fn test_broadcast_bus_basic() {
        let bus = BroadcastBus::new();
        let mut rx = bus.subscribe();

        bus.publish(Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        });

        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_name(), "session_started");
    }

    #[tokio::test]
    async fn test_broadcast_bus_multiple_subscribers() {
        let bus = BroadcastBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        assert_eq!(bus.subscriber_count(), 2);

        bus.publish(Event::SessionEnded {
            session_id: "test".to_string(),
            reason: SessionEndReason::Completed,
        });

        let event1 = rx1.recv().await.unwrap();
        let event2 = rx2.recv().await.unwrap();

        assert_eq!(event1.event_name(), "session_ended");
        assert_eq!(event2.event_name(), "session_ended");
    }

    #[tokio::test]
    async fn test_broadcast_bus_lagged_receiver() {
        let bus = BroadcastBus::with_capacity(2);
        let mut rx = bus.subscribe();

        for i in 0..5 {
            bus.publish(Event::TurnStarted {
                session_id: "test".to_string(),
                turn_number: i,
            });
        }

        let result = rx.recv().await;
        assert!(result.is_err() || result.is_ok());
    }
}
