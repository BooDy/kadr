use kadr_core::events::SystemEvent;
use tokio::sync::broadcast::{self, Receiver, Sender};

/// In-memory bounded pub/sub event bus powered by Tokio's broadcast channel.
///
/// Dispatches `SystemEvent` instances to all active subscribers. When subscribers
/// lag behind beyond channel capacity, `RecvError::Lagged` is returned to them
/// while newer messages remain intact, preserving bounded memory consumption.
#[derive(Clone, Debug)]
pub struct EventBus {
    sender: Sender<SystemEvent>,
}

impl EventBus {
    /// Creates a new `EventBus` with the specified ring buffer capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Creates an `EventBus` with default ring buffer capacity of 512 events.
    pub fn default_bus() -> Self {
        Self::new(512)
    }

    /// Publishes a `SystemEvent` to all active subscribers.
    ///
    /// Returns the number of active subscribers that received the event.
    /// If there are no active subscribers, returns 0 without error.
    pub fn publish(&self, event: SystemEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribes to the event stream, returning a broadcast `Receiver`.
    pub fn subscribe(&self) -> Receiver<SystemEvent> {
        self.sender.subscribe()
    }

    /// Returns the number of active receivers currently subscribed to the bus.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::default_bus()
    }
}
