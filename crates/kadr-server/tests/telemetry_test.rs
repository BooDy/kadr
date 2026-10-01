use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;

use kadr_core::events::SystemEvent;
use kadr_core::models::PlaybackSession;
use kadr_server::events::EventBus;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::telemetry::TelemetryCollector;

#[tokio::test]
async fn test_collect_snapshot_metrics() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("kadr.db");
    let wal_path = dir.path().join("kadr.db-wal");

    // Write known sizes to db and wal files
    let mut db_file = tokio::fs::File::create(&db_path).await.expect("create db file");
    db_file.write_all(&vec![0u8; 2048]).await.expect("write db file");
    db_file.flush().await.expect("flush db");

    let mut wal_file = tokio::fs::File::create(&wal_path).await.expect("create wal file");
    wal_file.write_all(&vec![0u8; 512]).await.expect("write wal file");
    wal_file.flush().await.expect("flush wal");

    let session_registry = Arc::new(SessionRegistry::new());
    let session1 = PlaybackSession {
        session_id: "session-1".to_string(),
        user_id: "user-1".to_string(),
        media_item_id: 101,
        duration_seconds: 3600,
        started_at: 1_700_000_000,
        last_heartbeat_at: 1_700_000_000,
        current_position_seconds: 120,
    };
    let session2 = PlaybackSession {
        session_id: "session-2".to_string(),
        user_id: "user-2".to_string(),
        media_item_id: 102,
        duration_seconds: 7200,
        started_at: 1_700_000_000,
        last_heartbeat_at: 1_700_000_000,
        current_position_seconds: 300,
    };
    session_registry.insert(session1).await;
    session_registry.insert(session2).await;

    let event_bus = Arc::new(EventBus::default_bus());
    let collector = TelemetryCollector::new(db_path, session_registry, event_bus);

    let snapshot = collector.collect_snapshot().await;

    assert_eq!(snapshot.active_sessions_count, 2);
    assert_eq!(snapshot.db_size_bytes, 2048);
    assert_eq!(snapshot.wal_size_bytes, 512);
    assert!(snapshot.timestamp > 0);

    #[cfg(target_os = "linux")]
    {
        assert!(snapshot.rss_memory_bytes > 0, "RSS memory should be non-zero on Linux");
    }
}

#[tokio::test]
async fn test_collect_snapshot_missing_files() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("nonexistent.db");

    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let collector = TelemetryCollector::new(db_path, session_registry, event_bus);

    let snapshot = collector.collect_snapshot().await;

    assert_eq!(snapshot.active_sessions_count, 0);
    assert_eq!(snapshot.db_size_bytes, 0);
    assert_eq!(snapshot.wal_size_bytes, 0);
    assert!(snapshot.timestamp > 0);
}

#[tokio::test]
async fn test_spawn_periodic_broadcaster() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("broadcast_test.db");
    tokio::fs::write(&db_path, b"dummy data").await.expect("write db");

    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let mut rx = event_bus.subscribe();

    let collector = Arc::new(TelemetryCollector::new(db_path, session_registry, event_bus));
    let handle = collector.spawn_periodic_broadcaster(Duration::from_millis(50));

    // Wait for first event
    let event = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timed out waiting for telemetry event")
        .expect("channel recv error");

    match event {
        SystemEvent::SystemTelemetry(snapshot) => {
            assert_eq!(snapshot.db_size_bytes, 10);
            assert_eq!(snapshot.active_sessions_count, 0);
            assert!(snapshot.timestamp > 0);
        }
        other => panic!("expected SystemTelemetry event, got: {:?}", other),
    }

    // Wait for a second tick to ensure periodic execution
    let event2 = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timed out waiting for 2nd telemetry event")
        .expect("channel recv error");

    match event2 {
        SystemEvent::SystemTelemetry(snapshot) => {
            assert_eq!(snapshot.db_size_bytes, 10);
        }
        other => panic!("expected SystemTelemetry event, got: {:?}", other),
    }

    handle.abort();
}
