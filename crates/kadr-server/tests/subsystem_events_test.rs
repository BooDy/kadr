use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use kadr_core::events::SystemEvent;
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_ingest::watcher::{IngestMessage, IngestWorker};
use kadr_server::api::create_router_with_events;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::tempdir;
use tokio::time::timeout;
use tower::ServiceExt;

const SAMPLE_SRT: &str = "\
1
00:00:01,000 --> 00:00:04,000
Hello, world!
";

struct TestContext {
    app: Router,
    event_bus: Arc<EventBus>,
    token: String,
    media_item_id: i64,
    media_repo: MediaItemRepository,
    _temp_dir: tempfile::TempDir,
}

#[derive(Deserialize)]
struct MockDownloadReq {
    file_id: u64,
}

#[derive(Serialize)]
struct MockDownloadResp {
    link: String,
    file_name: String,
}

async fn setup_test_context(mock_base_url: Option<String>) -> TestContext {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr.db");

    let pool = create_pool(&db_path, 2).expect("failed to create pool");
    initialize_database(&pool).await.expect("failed to initialize db");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let user = User {
        id: "user-sub-1".to_string(),
        username: "testuser".to_string(),
        pin_hash: hash_pin("1234").expect("hash pin"),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo.create(&user).await.expect("create user");

    let jwt_svc = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token = jwt_svc.generate_token(&user).expect("generate token");

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
        })
        .await
        .expect("create library");

    let media_dir = dir.path().join("media");
    std::fs::create_dir_all(&media_dir).unwrap();
    let video_path = media_dir.join("movie.mp4");
    File::create(&video_path).unwrap();

    let technical = TechnicalInfo {
        duration_seconds: 5000,
        ..Default::default()
    };

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Test Movie".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1000,
            file_path: video_path,
            file_name: "movie.mp4".to_string(),
            file_size: 1000,
            technical,
            metadata: MediaMetadata::default(),
        }])
        .await
        .unwrap();

    let items = media_repo.list_by_library("lib1", 10, 0).await.unwrap();
    let media_item_id = items[0].id.unwrap();

    let cache_dir = dir.path().join("cache");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();

    let subtitle_service = Arc::new(
        SubtitleDeliveryService::new(cache_dir, subtitle_repo, media_repo.clone())
            .with_data_dir(data_dir),
    );

    let opensubtitles_client = match mock_base_url {
        Some(url) => Arc::new(OpenSubtitlesClient::new(
            Some("test-api-key".to_string()),
            Some(url),
        )),
        None => Arc::new(OpenSubtitlesClient::new(None, None)),
    };

    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    let limiter = RateLimiter::new(100, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        db_path,
        session_registry.clone(),
        event_bus.clone(),
    ));

    let app = create_router_with_events(
        user_repo,
        playback_repo,
        media_repo.clone(),
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry_collector,
    );

    TestContext {
        app,
        event_bus,
        token,
        media_item_id,
        media_repo,
        _temp_dir: dir,
    }
}

#[tokio::test]
async fn test_playback_heartbeat_emits_session_synced_event() {
    let ctx = setup_test_context(None).await;
    let mut rx = ctx.event_bus.subscribe();

    // 1. Create playback session
    let create_payload = serde_json::json!({
        "media_item_id": ctx.media_item_id,
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let session_data: Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = session_data["session_id"].as_str().unwrap().to_string();

    // 2. Send progress heartbeat
    let heartbeat_payload = serde_json::json!({
        "position_seconds": 150,
    });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{session_id}/progress"))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&heartbeat_payload).unwrap()))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Verify SystemEvent::SessionSynced received on EventBus
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for SessionSynced event")
        .expect("channel recv error");

    match event {
        SystemEvent::SessionSynced {
            session_id: ev_session_id,
            item_id: ev_item_id,
            user_id: ev_user_id,
            position_seconds: ev_pos,
            timestamp: ev_ts,
        } => {
            assert_eq!(ev_session_id, session_id);
            assert_eq!(ev_item_id, ctx.media_item_id);
            assert_eq!(ev_user_id, "user-sub-1");
            assert_eq!(ev_pos, 150);
            assert!(ev_ts > 0);
        }
        other => panic!("expected SystemEvent::SessionSynced, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_subtitle_download_emits_subtitle_downloaded_event() {
    let mock_app = Router::new()
        .route(
            "/download",
            post(|_headers: HeaderMap, Json(body): Json<MockDownloadReq>| async move {
                assert_eq!(body.file_id, 12345);
                (
                    StatusCode::OK,
                    Json(MockDownloadResp {
                        link: "/files/downloaded.srt".to_string(),
                        file_name: "downloaded.srt".to_string(),
                    }),
                )
            }),
        )
        .route(
            "/files/downloaded.srt",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    SAMPLE_SRT,
                )
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, mock_app.into_make_service())
            .await
            .unwrap();
    });

    let mock_base_url = format!("http://{}", local_addr);
    let ctx = setup_test_context(Some(mock_base_url)).await;
    let mut rx = ctx.event_bus.subscribe();

    // Trigger download subtitle route
    let download_payload = serde_json::json!({
        "file_id": "12345",
        "language": "ara",
        "title": "Arabic Subtitle",
        "is_forced": false,
    });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/subtitles/{}/download", ctx.media_item_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&download_payload).unwrap()))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body_json: Value = serde_json::from_slice(&bytes).unwrap();
    let created_sub_id = body_json["id"].as_i64().expect("subtitle id");

    // Verify SystemEvent::SubtitleDownloaded received on EventBus
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for SubtitleDownloaded event")
        .expect("channel recv error");

    match event {
        SystemEvent::SubtitleDownloaded {
            item_id,
            subtitle_id,
            language,
            timestamp,
        } => {
            assert_eq!(item_id, ctx.media_item_id);
            assert_eq!(subtitle_id, created_sub_id);
            assert_eq!(language, "ara");
            assert!(timestamp > 0);
        }
        other => panic!("expected SystemEvent::SubtitleDownloaded, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_ingest_worker_flush_emits_library_updated_event() {
    let ctx = setup_test_context(None).await;
    let mut rx = ctx.event_bus.subscribe();

    let (tx, ingest_rx) = tokio::sync::mpsc::channel(10);
    let bus = ctx.event_bus.clone();
    let worker = IngestWorker::new(ingest_rx, ctx.media_repo.clone()).with_event_callback(move |ev| {
        bus.publish(ev);
    });

    let worker_handle = tokio::spawn(worker.run());

    let new_item = MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "Ingested Film".to_string(),
        original_title: None,
        release_year: Some(2025),
        added_at: 2000,
        file_path: PathBuf::from("/tmp/movies/film.mkv"),
        file_name: "film.mkv".to_string(),
        file_size: 4096,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata::default(),
    };

    tx.send(IngestMessage::Upsert(new_item, vec![]))
        .await
        .expect("send ingest msg");

    // Close tx so worker flushes batch and exits
    drop(tx);
    worker_handle.await.expect("worker task failed");

    // Verify SystemEvent::LibraryUpdated received on EventBus
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for LibraryUpdated event")
        .expect("channel recv error");

    match event {
        SystemEvent::LibraryUpdated {
            library_id,
            item_count,
            timestamp,
        } => {
            assert_eq!(library_id, "lib1");
            assert_eq!(item_count, 1);
            assert!(timestamp > 0);
        }
        other => panic!("expected SystemEvent::LibraryUpdated, got: {other:?}"),
    }
}
