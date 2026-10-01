use kadr_core::events::{SystemEvent, TelemetrySnapshot};
// Also test that re-exports at root work
use kadr_core::{SystemEvent as RootSystemEvent, TelemetrySnapshot as RootTelemetrySnapshot};
use serde_json::json;

#[test]
fn test_telemetry_snapshot_serde() {
    let snapshot = TelemetrySnapshot {
        active_sessions_count: 3,
        rss_memory_bytes: 42_000_000,
        db_size_bytes: 10_000_000,
        wal_size_bytes: 1_000_000,
        timestamp: 1700000000,
    };

    let serialized = serde_json::to_string(&snapshot).expect("serialize TelemetrySnapshot");
    let deserialized: TelemetrySnapshot =
        serde_json::from_str(&serialized).expect("deserialize TelemetrySnapshot");

    assert_eq!(snapshot, deserialized);

    // Also check through root re-export type
    let root_deserialized: RootTelemetrySnapshot =
        serde_json::from_str(&serialized).expect("deserialize via root re-export");
    assert_eq!(snapshot, root_deserialized);
}

#[test]
fn test_system_event_library_updated_serde() {
    let event = SystemEvent::LibraryUpdated {
        library_id: "lib-movies".to_string(),
        item_count: 150,
        timestamp: 1700000010,
    };

    assert_eq!(event.event_type(), "library:updated");

    let val = serde_json::to_value(&event).expect("to_value");
    assert_eq!(val["type"], "library:updated");
    assert_eq!(val["payload"]["library_id"], "lib-movies");
    assert_eq!(val["payload"]["item_count"], 150);
    assert_eq!(val["payload"]["timestamp"], 1700000010);

    let roundtrip: SystemEvent = serde_json::from_value(val).expect("from_value");
    assert_eq!(event, roundtrip);
}

#[test]
fn test_system_event_layout_changed_serde() {
    let event = SystemEvent::LayoutChanged {
        screen_id: "home".to_string(),
        timestamp: 1700000020,
    };

    assert_eq!(event.event_type(), "layout:changed");

    let val = serde_json::to_value(&event).expect("to_value");
    assert_eq!(val["type"], "layout:changed");
    assert_eq!(val["payload"]["screen_id"], "home");
    assert_eq!(val["payload"]["timestamp"], 1700000020);

    let roundtrip: SystemEvent = serde_json::from_value(val).expect("from_value");
    assert_eq!(event, roundtrip);
}

#[test]
fn test_system_event_subtitle_downloaded_serde() {
    let event = SystemEvent::SubtitleDownloaded {
        item_id: 101,
        subtitle_id: 502,
        language: "en".to_string(),
        timestamp: 1700000030,
    };

    assert_eq!(event.event_type(), "subtitle:downloaded");

    let val = serde_json::to_value(&event).expect("to_value");
    assert_eq!(val["type"], "subtitle:downloaded");
    assert_eq!(val["payload"]["item_id"], 101);
    assert_eq!(val["payload"]["subtitle_id"], 502);
    assert_eq!(val["payload"]["language"], "en");
    assert_eq!(val["payload"]["timestamp"], 1700000030);

    let roundtrip: SystemEvent = serde_json::from_value(val).expect("from_value");
    assert_eq!(event, roundtrip);
}

#[test]
fn test_system_event_session_synced_serde() {
    let event = SystemEvent::SessionSynced {
        session_id: "sess-abc-123".to_string(),
        item_id: 202,
        user_id: "user-42".to_string(),
        position_seconds: 3600,
        timestamp: 1700000040,
    };

    assert_eq!(event.event_type(), "session:synced");

    let val = serde_json::to_value(&event).expect("to_value");
    assert_eq!(val["type"], "session:synced");
    assert_eq!(val["payload"]["session_id"], "sess-abc-123");
    assert_eq!(val["payload"]["item_id"], 202);
    assert_eq!(val["payload"]["user_id"], "user-42");
    assert_eq!(val["payload"]["position_seconds"], 3600);
    assert_eq!(val["payload"]["timestamp"], 1700000040);

    let roundtrip: SystemEvent = serde_json::from_value(val).expect("from_value");
    assert_eq!(event, roundtrip);
}

#[test]
fn test_system_event_system_telemetry_serde() {
    let snapshot = TelemetrySnapshot {
        active_sessions_count: 7,
        rss_memory_bytes: 84_000_000,
        db_size_bytes: 25_000_000,
        wal_size_bytes: 2_000_000,
        timestamp: 1700000050,
    };
    let event = SystemEvent::SystemTelemetry(snapshot.clone());

    assert_eq!(event.event_type(), "system:telemetry");

    let val = serde_json::to_value(&event).expect("to_value");
    assert_eq!(val["type"], "system:telemetry");
    assert_eq!(val["payload"]["active_sessions_count"], 7);
    assert_eq!(val["payload"]["rss_memory_bytes"], 84_000_000);
    assert_eq!(val["payload"]["db_size_bytes"], 25_000_000);
    assert_eq!(val["payload"]["wal_size_bytes"], 2_000_000);
    assert_eq!(val["payload"]["timestamp"], 1700000050);

    let roundtrip: RootSystemEvent = serde_json::from_value(val).expect("from_value root");
    assert_eq!(event, roundtrip);
}

#[test]
fn test_deserialize_raw_json_payloads() {
    let raw_json = json!({
        "type": "library:updated",
        "payload": {
            "library_id": "lib-1",
            "item_count": 10,
            "timestamp": 12345
        }
    });

    let event: SystemEvent = serde_json::from_value(raw_json).expect("deserialize valid raw json");
    match event {
        SystemEvent::LibraryUpdated {
            library_id,
            item_count,
            timestamp,
        } => {
            assert_eq!(library_id, "lib-1");
            assert_eq!(item_count, 10);
            assert_eq!(timestamp, 12345);
        }
        _ => panic!("Expected LibraryUpdated variant"),
    }
}
