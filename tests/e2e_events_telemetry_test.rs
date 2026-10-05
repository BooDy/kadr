use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use futures::StreamExt;
use kadr_core::events::TelemetrySnapshot;
use kadr_core::models::{Library, MediaItem, MediaType, User, UserRole};
use kadr_ingest::watcher::{scan_directory_recursive, IngestMessage, IngestPipeline, IngestWorker};
use kadr_server::api::create_full_router;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde_json::Value;
use tempfile::tempdir;
use tower::ServiceExt;

/// Helper function to read SSE events from a stream until a matching event type is found.
async fn recv_sse_event_matching<S>(
    stream: &mut S,
    expected_event: &str,
    timeout_duration: Duration,
) -> (String, Value)
where
    S: futures::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Unpin,
{
    let deadline = tokio::time::Instant::now() + timeout_duration;
    let mut buffer = String::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!(
                "Timed out waiting for SSE event '{}'. Buffer received so far:\n{}",
                expected_event, buffer
            );
        }

        let chunk = tokio::time::timeout(remaining, stream.next())
            .await
            .unwrap_or_else(|_| panic!("Timeout reading next chunk for event '{}'", expected_event))
            .unwrap_or_else(|| panic!("Stream ended before receiving event '{}'", expected_event))
            .unwrap_or_else(|e| panic!("Error reading chunk: {}", e));

        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        // Search for target event in accumulated buffer
        let target_prefix = format!("event: {}", expected_event);
        if let Some(pos) = buffer.find(&target_prefix) {
            let after_event = &buffer[pos..];
            if let Some(data_idx) = after_event.find("data: ") {
                let data_start = pos + data_idx + 6;
                if let Some(newline_idx) = buffer[data_start..].find('\n') {
                    let json_str = &buffer[data_start..data_start + newline_idx];
                    if let Ok(json_val) = serde_json::from_str::<Value>(json_str.trim()) {
                        return (expected_event.to_string(), json_val);
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_milestone_5a_realtime_events_and_telemetry_e2e() {
    // -------------------------------------------------------------------------
    // Step 1: Set up temporary test directory, SQLite pool, media files & repos
    // -------------------------------------------------------------------------
    let dir = tempdir().expect("Failed to create temporary directory");
    let db_path = dir.path().join("kadr_test.db");
    let pool = create_pool(&db_path, 4).expect("Failed to create SQLite connection pool");
    initialize_database(&pool)
        .await
        .expect("Failed to initialize database and migrations");

    let media_dir = dir.path().join("media");
    let movie_dir = media_dir.join("Interstellar (2014)");
    std::fs::create_dir_all(&movie_dir).expect("Failed to create movie directory");

    let movie_file = movie_dir.join("Interstellar (2014).mp4");
    std::fs::write(&movie_file, vec![0u8; 8192]).expect("Failed to write mock video file");

    let subtitle_cache_dir = dir.path().join("subtitles_cache");
    std::fs::create_dir_all(&subtitle_cache_dir)
        .expect("Failed to create subtitle cache directory");

    // Initialize repositories
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    // Seed admin user
    let admin_user = User {
        id: "admin-uid-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").expect("Failed to hash admin PIN"),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo
        .create(&admin_user)
        .await
        .expect("Failed to create admin user");

    // Seed standard viewer user
    let viewer_user = User {
        id: "viewer-uid-2".to_string(),
        username: "viewer".to_string(),
        pin_hash: hash_pin("5678").expect("Failed to hash viewer PIN"),
        role: UserRole::Standard,
        created_at: 1000,
    };
    user_repo
        .create(&viewer_user)
        .await
        .expect("Failed to create viewer user");

    // Seed library
    let library = Library {
        id: "lib-movies".to_string(),
        name: "Movies".to_string(),
        path: media_dir.clone(),
        media_type: MediaType::Movie,
        created_at: 1000,
        ..Default::default()
    };
    lib_repo
        .insert(&library)
        .await
        .expect("Failed to register library");

    // Seed initial media item for playback session testing
    let initial_item = MediaItem {
        id: None,
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        title: "Interstellar".to_string(),
        original_title: None,
        release_year: Some(2014),
        added_at: 1000,
        file_path: movie_file.clone(),
        file_name: "Interstellar (2014).mp4".to_string(),
        file_size: 8192,
        technical: kadr_core::models::TechnicalInfo {
            duration_seconds: 10140,
            container: Some("mp4".to_string()),
            ..Default::default()
        },
        metadata: kadr_core::models::MediaMetadata::default(),
    };
    media_repo
        .upsert_batch(&[initial_item])
        .await
        .expect("Failed to seed initial media item");
    let media_items = media_repo
        .list_by_library("lib-movies", 10, 0)
        .await
        .expect("Failed to list media items");
    assert_eq!(media_items.len(), 1);
    let seeded_item_id = media_items[0].id.expect("Expected item ID");

    // -------------------------------------------------------------------------
    // Step 2: Initialize EventBus, IngestWorker with callback, and TelemetryCollector
    // -------------------------------------------------------------------------
    let event_bus = Arc::new(EventBus::default_bus());
    let session_registry = Arc::new(SessionRegistry::new());

    let (ingest_tx, ingest_rx) = tokio::sync::mpsc::channel(200);
    let bus_for_worker = event_bus.clone();
    let worker = IngestWorker::new(ingest_rx, media_repo.clone())
        .with_subtitles(subtitle_repo.clone())
        .with_event_callback(move |ev| {
            bus_for_worker.publish(ev);
        });
    let worker_handle = tokio::spawn(worker.run());

    let telemetry_collector = Arc::new(TelemetryCollector::new(
        db_path.clone(),
        session_registry.clone(),
        event_bus.clone(),
    ));

    let jwt_svc = JwtService::new(
        "milestone-5a-integration-test-secret-at-least-32-bytes",
        3600,
    );
    let rate_limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache_dir,
        subtitle_repo.clone(),
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));

    // Assemble full router with real-time event streaming and system telemetry
    let app = create_full_router(
        user_repo.clone(),
        playback_repo.clone(),
        media_repo.clone(),
        lib_repo.clone(),
        jwt_svc.clone(),
        rate_limiter.clone(),
        session_registry.clone(),
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry_collector.clone(),
    );

    // -------------------------------------------------------------------------
    // Step 3: Connect to GET /api/v1/events as client SSE stream
    // -------------------------------------------------------------------------
    let sse_req = Request::builder()
        .method("GET")
        .uri("/api/v1/events")
        .body(Body::empty())
        .unwrap();
    let sse_resp = app
        .clone()
        .oneshot(sse_req)
        .await
        .expect("Failed to connect to /api/v1/events");
    assert_eq!(sse_resp.status(), StatusCode::OK);
    let ct = sse_resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        ct.contains("text/event-stream"),
        "Expected text/event-stream content-type"
    );
    let mut sse_stream = sse_resp.into_body().into_data_stream();

    // -------------------------------------------------------------------------
    // Step 4: Authenticate admin user with PIN (POST /api/v1/auth/pin) -> receives JWT token
    // -------------------------------------------------------------------------
    let auth_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/pin")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "user_id": "admin-uid-1",
                "pin": "1234",
            }))
            .unwrap(),
        ))
        .unwrap();
    let auth_resp = app.clone().oneshot(auth_req).await.unwrap();
    assert_eq!(auth_resp.status(), StatusCode::OK);
    let auth_bytes = axum::body::to_bytes(auth_resp.into_body(), 4096)
        .await
        .unwrap();
    let auth_json: Value = serde_json::from_slice(&auth_bytes).unwrap();
    let admin_token = auth_json["token"]
        .as_str()
        .expect("Expected JWT token string")
        .to_string();
    assert!(!admin_token.is_empty(), "Token must not be empty");

    // -------------------------------------------------------------------------
    // Step 5: Query GET /api/v1/system/telemetry with admin Bearer token
    // -------------------------------------------------------------------------
    // 5a. Unauthenticated request must return 401 Unauthorized
    let unauth_req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/telemetry")
        .body(Body::empty())
        .unwrap();
    let unauth_resp = app.clone().oneshot(unauth_req).await.unwrap();
    assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

    // 5b. Standard viewer token must return 403 Forbidden
    let viewer_token = jwt_svc
        .generate_token(&viewer_user)
        .expect("Generate viewer token");
    let forbidden_req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/telemetry")
        .header(header::AUTHORIZATION, format!("Bearer {}", viewer_token))
        .body(Body::empty())
        .unwrap();
    let forbidden_resp = app.clone().oneshot(forbidden_req).await.unwrap();
    assert_eq!(forbidden_resp.status(), StatusCode::FORBIDDEN);

    // 5c. Admin request returns 200 OK and valid TelemetrySnapshot JSON
    let telem_req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/telemetry")
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let telem_resp = app.clone().oneshot(telem_req).await.unwrap();
    assert_eq!(telem_resp.status(), StatusCode::OK);
    let telem_bytes = axum::body::to_bytes(telem_resp.into_body(), 4096)
        .await
        .unwrap();
    let snapshot: TelemetrySnapshot = serde_json::from_slice(&telem_bytes).unwrap();
    assert_eq!(
        snapshot.active_sessions_count, 0,
        "No active sessions initially"
    );
    assert!(
        snapshot.timestamp > 0,
        "Timestamp must be a valid positive epoch"
    );

    // -------------------------------------------------------------------------
    // Step 6: Create playback session & send progress heartbeat -> verify session:synced on SSE
    // -------------------------------------------------------------------------
    let session_req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "media_item_id": seeded_item_id,
            }))
            .unwrap(),
        ))
        .unwrap();
    let session_resp = app.clone().oneshot(session_req).await.unwrap();
    assert_eq!(session_resp.status(), StatusCode::CREATED);
    let session_bytes = axum::body::to_bytes(session_resp.into_body(), 4096)
        .await
        .unwrap();
    let session_json: Value = serde_json::from_slice(&session_bytes).unwrap();
    let session_id = session_json["session_id"]
        .as_str()
        .expect("Session ID")
        .to_string();

    // Send progress heartbeat
    let heartbeat_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "position_seconds": 1200,
            }))
            .unwrap(),
        ))
        .unwrap();
    let heartbeat_resp = app.clone().oneshot(heartbeat_req).await.unwrap();
    assert_eq!(heartbeat_resp.status(), StatusCode::OK);

    // Verify session:synced event received on SSE stream
    let (ev_type, ev_data) =
        recv_sse_event_matching(&mut sse_stream, "session:synced", Duration::from_secs(5)).await;
    assert_eq!(ev_type, "session:synced");
    assert_eq!(ev_data["type"], "session:synced");
    assert_eq!(ev_data["payload"]["session_id"], session_id);
    assert_eq!(ev_data["payload"]["item_id"], seeded_item_id);
    assert_eq!(ev_data["payload"]["user_id"], "admin-uid-1");
    assert_eq!(ev_data["payload"]["position_seconds"], 1200);

    // -------------------------------------------------------------------------
    // Step 7: Trigger ingestion via IngestWorker -> verify library:updated on SSE
    // -------------------------------------------------------------------------
    let pipeline = Arc::new(IngestPipeline::new(false, None));
    let scanned_files = scan_directory_recursive(&media_dir);
    for file in scanned_files {
        if let Ok(Some((item, subs))) = pipeline.process_file(&library, &file).await {
            ingest_tx
                .send(IngestMessage::Upsert(item, subs))
                .await
                .expect("Failed to send message to IngestWorker");
        }
    }

    // Verify library:updated event received on SSE stream
    let (ev_type, ev_data) =
        recv_sse_event_matching(&mut sse_stream, "library:updated", Duration::from_secs(5)).await;
    assert_eq!(ev_type, "library:updated");
    assert_eq!(ev_data["type"], "library:updated");
    assert_eq!(ev_data["payload"]["library_id"], "lib-movies");
    assert!(ev_data["payload"]["item_count"].as_u64().unwrap() >= 1);

    // -------------------------------------------------------------------------
    // Step 8: Spawn periodic broadcaster -> verify system:telemetry on SSE
    // -------------------------------------------------------------------------
    let broadcaster_handle = telemetry_collector
        .clone()
        .spawn_periodic_broadcaster(Duration::from_millis(50));

    // Verify system:telemetry event received on SSE stream
    let (ev_type, ev_data) =
        recv_sse_event_matching(&mut sse_stream, "system:telemetry", Duration::from_secs(5)).await;
    assert_eq!(ev_type, "system:telemetry");
    assert_eq!(ev_data["type"], "system:telemetry");
    let telem_payload = &ev_data["payload"];
    assert_eq!(
        telem_payload["active_sessions_count"], 1,
        "Active sessions count should reflect the 1 active session"
    );
    assert!(telem_payload["timestamp"].as_i64().unwrap() > 0);

    // Query REST telemetry endpoint again to confirm active session count is 1
    let telem_req2 = Request::builder()
        .method("GET")
        .uri("/api/v1/system/telemetry")
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let telem_resp2 = app.clone().oneshot(telem_req2).await.unwrap();
    assert_eq!(telem_resp2.status(), StatusCode::OK);
    let telem_bytes2 = axum::body::to_bytes(telem_resp2.into_body(), 4096)
        .await
        .unwrap();
    let snapshot2: TelemetrySnapshot = serde_json::from_slice(&telem_bytes2).unwrap();
    assert_eq!(snapshot2.active_sessions_count, 1);

    // -------------------------------------------------------------------------
    // Step 9: Clean shutdown of background tasks
    // -------------------------------------------------------------------------
    broadcaster_handle.abort();
    drop(ingest_tx);
    let _ = worker_handle.await;
}
