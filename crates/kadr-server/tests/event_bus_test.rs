use kadr_core::events::SystemEvent;
use kadr_server::events::EventBus;
use kadr_server::EventBus as RootEventBus;
use tokio::sync::broadcast::error::RecvError;

#[test]
fn test_event_bus_creation() {
    let bus = EventBus::default_bus();
    assert_eq!(bus.receiver_count(), 0);

    let default_bus: RootEventBus = Default::default();
    assert_eq!(default_bus.receiver_count(), 0);

    let custom_bus = EventBus::new(64);
    assert_eq!(custom_bus.receiver_count(), 0);

    let rx1 = custom_bus.subscribe();
    assert_eq!(custom_bus.receiver_count(), 1);

    let rx2 = custom_bus.subscribe();
    assert_eq!(custom_bus.receiver_count(), 2);

    drop(rx1);
    assert_eq!(custom_bus.receiver_count(), 1);

    drop(rx2);
    assert_eq!(custom_bus.receiver_count(), 0);
}

#[test]
fn test_publish_zero_subscribers() {
    let bus = EventBus::default_bus();
    assert_eq!(bus.receiver_count(), 0);

    let event = SystemEvent::LayoutChanged {
        screen_id: "home".to_string(),
        timestamp: 1_700_000_000,
    };

    let delivered = bus.publish(event);
    assert_eq!(delivered, 0);
}

#[tokio::test]
async fn test_publish_multiple_subscribers() {
    let bus = EventBus::default_bus();
    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();
    let mut rx3 = bus.subscribe();

    assert_eq!(bus.receiver_count(), 3);

    let event = SystemEvent::LibraryUpdated {
        library_id: "lib-1".to_string(),
        item_count: 42,
        timestamp: 1_700_000_001,
    };

    let delivered = bus.publish(event.clone());
    assert_eq!(delivered, 3);

    assert_eq!(rx1.recv().await.unwrap(), event);
    assert_eq!(rx2.recv().await.unwrap(), event);
    assert_eq!(rx3.recv().await.unwrap(), event);
}

#[tokio::test]
async fn test_bounded_lag_handling() {
    // Ring buffer capacity of 2
    let bus = EventBus::new(2);
    let mut rx = bus.subscribe();

    for i in 0..5 {
        let event = SystemEvent::LayoutChanged {
            screen_id: format!("screen-{}", i),
            timestamp: i,
        };
        bus.publish(event);
    }

    // Subscriber lagged because 5 events were published to a buffer of capacity 2.
    // 3 events should have been dropped.
    match rx.recv().await {
        Err(RecvError::Lagged(skipped)) => {
            assert_eq!(skipped, 3);
        }
        other => panic!("expected RecvError::Lagged, got {:?}", other),
    }

    // After lagging, the receiver should receive the remaining buffered events: index 3 and 4
    let event3 = rx.recv().await.unwrap();
    assert_eq!(
        event3,
        SystemEvent::LayoutChanged {
            screen_id: "screen-3".to_string(),
            timestamp: 3,
        }
    );

    let event4 = rx.recv().await.unwrap();
    assert_eq!(
        event4,
        SystemEvent::LayoutChanged {
            screen_id: "screen-4".to_string(),
            timestamp: 4,
        }
    );
}

#[tokio::test]
async fn test_event_bus_clone_shares_channel() {
    let bus = EventBus::default_bus();
    let bus_clone = bus.clone();

    let mut rx = bus.subscribe();
    assert_eq!(bus_clone.receiver_count(), 1);

    let event = SystemEvent::SessionSynced {
        session_id: "sess-123".to_string(),
        item_id: 99,
        user_id: "user-1".to_string(),
        position_seconds: 350,
        timestamp: 1_700_000_002,
    };

    let delivered = bus_clone.publish(event.clone());
    assert_eq!(delivered, 1);

    assert_eq!(rx.recv().await.unwrap(), event);
}
