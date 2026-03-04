use tokio::sync::broadcast;

use super::types::Event;

const BUS_CAPACITY: usize = 1024;

/// Wrapper around `tokio::sync::broadcast` for system-wide event distribution.
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(BUS_CAPACITY);
        Self { sender }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: Event) {
        // Ignore error (no receivers is fine during startup/shutdown)
        let _ = self.sender.send(event);
    }

    /// Subscribe to receive events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Get the number of active receivers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::EventPayload;

    #[tokio::test]
    async fn publish_and_receive() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(Event::new(EventPayload::CoreStarted));

        let event = rx.recv().await.unwrap();
        assert_eq!(event.kind(), "core_started");
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(Event::new(EventPayload::CoreStopping));

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.kind(), "core_stopping");
        assert_eq!(e2.kind(), "core_stopping");
    }

    #[test]
    fn receiver_count() {
        let bus = EventBus::new();
        assert_eq!(bus.receiver_count(), 0);
        let _rx = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);
    }
}
