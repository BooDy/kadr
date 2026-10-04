use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kadr_core::models::{Library, MediaType, User, UserRole};
use kadr_ingest::watcher::{scan_directory_recursive, IngestMessage, IngestPipeline, IngestWorker};
use kadr_server::api::create_router_with_subtitles;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde_json::Value;
use tempfile::tempdir;
use tower::ServiceExt;

#[tokio::test]
async fn test_milestone_4_subtitles_end_to_end_journey() {
    // -------------------------------------------------------------------------
    // Setup temporary environment:
    // - SQLite database with migrations 001 through 004
    // - Media directory with movie file and sidecar subtitle file
    // - Subtitle cache directory
    // -------------------------------------------------------------------------
    let dir = tempdir().expect("Failed to create temporary directory");
    let db_path = dir.path().join("kadr_test.db");
    let pool = create_pool(&db_path, 4).expect("Failed to create SQLite connection pool");
    initialize_database(&pool)
        .await
        .expect("Failed to initialize database and run migrations 001-004");

    let media_dir = dir.path().join("media");
    let movie_dir = media_dir.join("Inception (2010)");
    std::fs::create_dir_all(&movie_dir).expect("Failed to create movie directory");

    let movie_file = movie_dir.join("Inception (2010).mp4");
    std::fs::write(&movie_file, vec![0u8; 8192]).expect("Failed to write mock movie file");

    let srt_file = movie_dir.join("Inception (2010).en.forced.srt");
    let srt_content = "1\n00:00:01,000 --> 00:00:04,000\nThey are sleeping.\n\n2\n00:00:05,000 --> 00:00:08,000\nWake them up.\n";
    std::fs::write(&srt_file, srt_content).expect("Failed to write mock srt subtitle file");

    let subtitle_cache_dir = dir.path().join("subtitles_cache");
    std::fs::create_dir_all(&subtitle_cache_dir)
        .expect("Failed to create subtitle cache directory");

    // Initialize repositories
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    // Register library
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
        .expect("Failed to create library");

    // -------------------------------------------------------------------------
    // Run IngestWorker with subtitles enabled to index media & sidecars
    // -------------------------------------------------------------------------
    let (ingest_tx, ingest_rx) = tokio::sync::mpsc::channel(200);
    let worker =
        IngestWorker::new(ingest_rx, media_repo.clone()).with_subtitles(subtitle_repo.clone());
    let worker_handle = tokio::spawn(worker.run());

    let pipeline = Arc::new(IngestPipeline::new(false));
    let scanned_files = scan_directory_recursive(&media_dir);
    for file in scanned_files {
        if let Ok(Some((item, subs))) = pipeline.process_file(&library, &file).await {
            ingest_tx
                .send(IngestMessage::Upsert(item, subs))
                .await
                .expect("Failed to send upsert message to ingest worker");
        }
    }
    drop(ingest_tx);
    worker_handle
        .await
        .expect("IngestWorker task panicked or failed");

    // Retrieve indexed media item
    let items = media_repo
        .list_by_library("lib-movies", 10, 0)
        .await
        .expect("Failed to list media items");
    assert_eq!(items.len(), 1, "Expected exactly 1 media item indexed");
    let media_item_id = items[0].id.expect("Media item must have an assigned id");

    // -------------------------------------------------------------------------
    // Setup Admin User and Assemble Full Axum Router
    // -------------------------------------------------------------------------
    let admin_user = User {
        id: "admin-uid".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").expect("Failed to hash pin"),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo
        .create(&admin_user)
        .await
        .expect("Failed to create admin user");

    let jwt_svc = JwtService::new("test-secret-must-be-at-least-32-chars-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache_dir.clone(),
        subtitle_repo.clone(),
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));

    let app = create_router_with_subtitles(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
    );

    // =========================================================================
    // Step 1: Authenticate admin user with PIN (POST /api/v1/auth/pin) -> receive JWT
    // =========================================================================
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-uid","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "POST /api/v1/auth/pin should succeed"
    );
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let auth_data: Value = serde_json::from_slice(&body).unwrap();
    let token = auth_data["token"]
        .as_str()
        .expect("Token should be present in auth response");

    // =========================================================================
    // Step 2: Query subtitle listing GET /api/v1/items/:id/subtitles with Bearer auth
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/subtitles", media_item_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "GET /api/v1/items/:id/subtitles should succeed"
    );

    let body = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let tracks: Value = serde_json::from_slice(&body).unwrap();
    let list = tracks
        .as_array()
        .expect("Expected array of subtitle tracks");
    assert_eq!(list.len(), 1, "Expected exactly 1 sidecar subtitle track");

    let track = &list[0];
    let subtitle_id = track["id"].as_i64().expect("Subtitle id should be an i64");
    let expected_stream_url = format!("/api/v1/subtitles/{}/stream.vtt", subtitle_id);

    // Verify sidecar track properties: language == en/eng, is_forced == true, format == srt, stream_url
    assert!(
        track["language"] == "en" || track["language"] == "eng",
        "Expected language 'en' or 'eng', got {:?}",
        track["language"]
    );
    assert_eq!(track["is_forced"], true, "Subtitle should be forced");
    assert_eq!(track["format"], "srt", "Subtitle format should be srt");
    assert_eq!(
        track["stream_url"], expected_stream_url,
        "stream_url mismatch"
    );

    // =========================================================================
    // Step 3: Stream WebVTT track GET /api/v1/subtitles/:id/stream.vtt WITHOUT auth
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(&expected_stream_url)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "GET /api/v1/subtitles/:id/stream.vtt should be public (200 OK without auth)"
    );

    let content_type = res
        .headers()
        .get("content-type")
        .expect("Missing Content-Type")
        .to_str()
        .unwrap();
    assert_eq!(content_type, "text/vtt; charset=utf-8");

    let cache_control = res
        .headers()
        .get("cache-control")
        .expect("Missing Cache-Control")
        .to_str()
        .unwrap();
    assert_eq!(cache_control, "public, max-age=86400");

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 64)
        .await
        .unwrap();
    let vtt_str = String::from_utf8(body_bytes.to_vec()).expect("VTT output should be valid UTF-8");
    assert!(
        vtt_str.starts_with("WEBVTT"),
        "WebVTT content must start with 'WEBVTT'"
    );
    assert!(
        vtt_str.contains("00:00:01.000 --> 00:00:04.000"),
        "WebVTT timestamps must have dots instead of commas"
    );
    assert!(
        !vtt_str.contains("00:00:01,000"),
        "WebVTT timestamps must not have commas"
    );
    assert!(
        vtt_str.contains("They are sleeping."),
        "WebVTT content must contain cue text"
    );

    // =========================================================================
    // Step 4: Stream WebVTT track a second time -> Verify cache hit on disk
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(&expected_stream_url)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "Second streaming request should return 200 OK"
    );

    let cached_vtt_path = subtitle_cache_dir.join(format!("{}.vtt", subtitle_id));
    assert!(
        cached_vtt_path.exists(),
        "Cached WebVTT file must exist on disk at {}",
        cached_vtt_path.display()
    );
    let cached_vtt_data = std::fs::read_to_string(&cached_vtt_path)
        .expect("Failed to read cached WebVTT file from disk");
    assert!(
        cached_vtt_data.starts_with("WEBVTT"),
        "Cached disk WebVTT must start with 'WEBVTT'"
    );

    // =========================================================================
    // Step 5: Test online search GET /api/v1/subtitles/:id/search
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/subtitles/{}/search", media_item_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "GET /api/v1/subtitles/:id/search should return 200 OK"
    );

    let body = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let search_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        search_json["configured"], false,
        "OpenSubtitles should not be configured"
    );
    let matches = search_json["matches"]
        .as_array()
        .expect("Matches should be an array");
    assert!(
        matches.is_empty(),
        "Matches should be empty when unconfigured"
    );

    // =========================================================================
    // Step 6: Test delete subtitle DELETE /api/v1/subtitles/:id
    // =========================================================================
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/subtitles/{}", subtitle_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "DELETE /api/v1/subtitles/:id should return 200 OK"
    );

    let body = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let del_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        del_json["deleted"], true,
        "Delete response should have deleted: true"
    );

    // Verify subsequent GET /api/v1/items/:id/subtitles returns empty array
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/subtitles", media_item_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let remaining_tracks: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        remaining_tracks.as_array().unwrap().len(),
        0,
        "Subtitle track listing must be empty after deletion"
    );

    // Verify subsequent GET /api/v1/subtitles/:id/stream.vtt returns 404 Not Found
    let req = Request::builder()
        .method("GET")
        .uri(&expected_stream_url)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "Streaming deleted subtitle must return 404 Not Found"
    );

    // Verify cached .vtt file on disk was removed
    assert!(
        !cached_vtt_path.exists(),
        "Cached WebVTT file on disk at {} should have been removed",
        cached_vtt_path.display()
    );
}
