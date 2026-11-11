use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::create_router_with_ingest;
use kadr_server::api::unlock_token::UnlockTokenService;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::events::EventBus;
use kadr_server::identity::ServerIdentity;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::storyboard::StoryboardService;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::ServiceExt;

const TEST_JWT_SECRET: &str = "storyboard-test-secret-key-at-least-32-bytes!";

fn generate_test_video(path: &Path, duration_secs: u32, width: u32, height: u32) -> bool {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size={width}x{height}:rate=10"),
            "-c:v",
            "libx264",
            "-g",
            "10",
            "-pix_fmt",
            "yuv420p",
            path.to_str().unwrap(),
        ])
        .output();

    matches!(output, Ok(out) if out.status.success())
}

struct TestCtx {
    app: axum::Router,
    token: String,
    video_item_id: i64,
    locked_item_id: i64,
    audio_item_id: i64,
    missing_file_item_id: i64,
    storyboard_svc: Arc<StoryboardService>,
    _dir: tempfile::TempDir,
}

async fn setup_ctx(storyboard_enabled: bool) -> Option<TestCtx> {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("movie.mp4");
    if !generate_test_video(&video_path, 12, 640, 268) {
        return None;
    }
    let audio_path = dir.path().join("song.mp3");
    std::fs::write(&audio_path, b"not-a-video").unwrap();

    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let sub_repo = SubtitleRepository::new(pool.clone());

    let pub_dir = dir.path().join("pub");
    let priv_dir = dir.path().join("priv");
    std::fs::create_dir_all(&pub_dir).unwrap();
    std::fs::create_dir_all(&priv_dir).unwrap();

    lib_repo
        .insert(&Library {
            id: "pub-lib".to_string(),
            name: "Public Movies".to_string(),
            path: pub_dir,
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "priv-lib".to_string(),
            name: "Private Movies".to_string(),
            path: priv_dir,
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("hash".to_string()),
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    media_repo
        .upsert_batch(&[
            MediaItem {
                id: None,
                library_id: "pub-lib".to_string(),
                item_type: MediaType::Movie,
                title: "Test Movie".to_string(),
                file_path: video_path.clone(),
                file_name: "movie.mp4".to_string(),
                file_size: 1024,
                technical: TechnicalInfo {
                    duration_seconds: 12,
                    video_codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "priv-lib".to_string(),
                item_type: MediaType::Movie,
                title: "Locked Movie".to_string(),
                file_path: dir.path().join("locked_copy.mp4"),
                file_name: "locked_copy.mp4".to_string(),
                file_size: 1024,
                technical: TechnicalInfo {
                    duration_seconds: 12,
                    video_codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "pub-lib".to_string(),
                item_type: MediaType::Music,
                title: "Audio Track".to_string(),
                file_path: audio_path,
                file_name: "song.mp3".to_string(),
                file_size: 12,
                technical: TechnicalInfo {
                    duration_seconds: 180,
                    video_codec: None,
                    audio_codec: Some("mp3".to_string()),
                    container: Some("mp3".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "pub-lib".to_string(),
                item_type: MediaType::Movie,
                title: "Missing File Movie".to_string(),
                file_path: dir.path().join("does_not_exist.mp4"),
                file_name: "does_not_exist.mp4".to_string(),
                file_size: 100,
                technical: TechnicalInfo {
                    duration_seconds: 60,
                    video_codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    std::fs::copy(&video_path, dir.path().join("locked_copy.mp4")).unwrap();

    let pub_items = media_repo.list_by_library("pub-lib", 10, 0).await.unwrap();
    let priv_items = media_repo.list_by_library("priv-lib", 10, 0).await.unwrap();
    let video_item_id = pub_items
        .iter()
        .find(|i| i.title == "Test Movie")
        .unwrap()
        .id
        .unwrap();
    let audio_item_id = pub_items
        .iter()
        .find(|i| i.title == "Audio Track")
        .unwrap()
        .id
        .unwrap();
    let missing_file_item_id = pub_items
        .iter()
        .find(|i| i.title == "Missing File Movie")
        .unwrap()
        .id
        .unwrap();
    let locked_item_id = priv_items[0].id.unwrap();

    let jwt = JwtService::new(TEST_JWT_SECRET, 3600);
    let token = jwt
        .generate_token(&User {
            id: "u1".to_string(),
            username: "alice".to_string(),
            pin_hash: String::new(),
            role: UserRole::Standard,
            created_at: 1000,
        })
        .unwrap();

    let mut cfg = AppConfig::default();
    cfg.server.data_dir = dir.path().to_path_buf();
    cfg.storyboard.enabled = storyboard_enabled;
    cfg.storyboard.interval_secs = 5;
    let shared_cfg = Arc::new(tokio::sync::RwLock::new(cfg.clone()));

    let storyboard_svc = Arc::new(StoryboardService::new(
        dir.path().join("storyboards"),
        cfg.storyboard.clone(),
        "ffmpeg".to_string(),
    ));

    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry = Arc::new(TelemetryCollector::new(
        PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let (tx, _) = tokio::sync::mpsc::channel(1);
    let pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));

    let app = create_router_with_ingest(
        user_repo,
        playback_repo.clone(),
        media_repo.clone(),
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
        LayoutRegistry::new(),
        Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo)),
        Arc::new(SubtitleDeliveryService::new(
            dir.path().join("subs"),
            sub_repo,
            media_repo,
        )),
        Arc::new(OpenSubtitlesClient::new(None, None)),
        event_bus,
        telemetry,
        tx,
        pipeline,
        shared_cfg,
        Arc::new(ServerIdentity {
            id: "00000000-0000-0000-0000-000000000000".to_string(),
        }),
    )
    .layer(axum::Extension(storyboard_svc.clone()));

    Some(TestCtx {
        app,
        token,
        video_item_id,
        locked_item_id,
        audio_item_id,
        missing_file_item_id,
        storyboard_svc,
        _dir: dir,
    })
}

#[tokio::test]
async fn storyboard_202_then_200_flow_and_cache_reuse() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    // 1. Initial manifest request returns 202 Accepted with Retry-After: 5
    let req = Request::builder()
        .uri(format!("/api/v1/media/{}/storyboard", ctx.video_item_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    assert_eq!(res.headers().get("retry-after").unwrap(), "5");
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "generating");

    // 2. Poll until 200 OK
    let mut manifest_json = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let req = Request::builder()
            .uri(format!(
                "/api/v1/media/{}/storyboard?token={}",
                ctx.video_item_id, ctx.token
            ))
            .body(Body::empty())
            .unwrap();
        let res = ctx.app.clone().oneshot(req).await.unwrap();
        if res.status() == StatusCode::OK {
            let bytes = axum::body::to_bytes(res.into_body(), 16384).await.unwrap();
            manifest_json = Some(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap());
            break;
        }
    }

    let manifest = manifest_json.expect("manifest should become ready");
    assert_eq!(manifest["interval_ms"], 5000);
    assert_eq!(manifest["tile_width"], 320);
    assert_eq!(manifest["tile_height"], 134);
    assert_eq!(manifest["columns"], 10);
    assert_eq!(manifest["rows"], 10);
    let sheet_url = manifest["sheets"][0]["url"].as_str().unwrap();
    assert_eq!(
        sheet_url,
        format!("/api/v1/media/{}/storyboard/0.jpg", ctx.video_item_id)
    );
    assert!(!sheet_url.contains("token="));

    // 3. Fetch sheet 0.jpg via ?token= query param
    let req = Request::builder()
        .uri(format!("{sheet_url}?token={}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=604800"
    );
    let etag = res
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let jpeg_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(jpeg_bytes.starts_with(&[0xFF, 0xD8]));

    // 4. Conditional GET with If-None-Match returns 304 Not Modified
    let req = Request::builder()
        .uri(format!("{sheet_url}?token={}", ctx.token))
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);

    // 5. Out-of-range sheet returns 404
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard/99.jpg?token={}",
            ctx.video_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 6. Verify cache reuse: ffmpeg ran only once
    assert_eq!(ctx.storyboard_svc.storyboard_generation_count(), 1);
}

#[tokio::test]
async fn storyboard_enforces_auth_and_private_library_locks() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    // Unauthenticated -> 401
    let req = Request::builder()
        .uri(format!("/api/v1/media/{}/storyboard", ctx.video_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Locked private library -> 403 LIBRARY_LOCKED
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            ctx.locked_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "LIBRARY_LOCKED");

    // Unlocked via ?unlocked= query param -> 202 Accepted
    let unlock_svc = UnlockTokenService::new(TEST_JWT_SECRET, 3600);
    let (unlock_tok, _) = unlock_svc.generate_token("priv-lib").unwrap();
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}&unlocked={}",
            ctx.locked_item_id, ctx.token, unlock_tok
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn storyboard_error_statuses_404_and_503() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    // Nonexistent item -> 404 Media item not found
    let req = Request::builder()
        .uri(format!("/api/v1/media/999999/storyboard?token={}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Media item not found");

    // Missing disk file -> 404 Media file missing from disk
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            ctx.missing_file_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Media file missing from disk");

    // Audio-only item -> 404
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            ctx.audio_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Disabled storyboard -> 503 Storyboard generation unavailable
    let Some(disabled_ctx) = setup_ctx(false).await else {
        return;
    };
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            disabled_ctx.video_item_id, disabled_ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = disabled_ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Storyboard generation unavailable");
}

#[tokio::test]
async fn single_frame_fallback_endpoint_extracts_and_caches() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/frame?t=2.4&w=280&token={}",
            ctx.video_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=604800"
    );
    assert!(res.headers().get(header::ETAG).is_some());
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
    assert_eq!(ctx.storyboard_svc.frame_generation_count(), 1);

    // Second request at t=1.8 (snaps to same 2s) and w=280 hits disk cache
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/frame?t=1.8&w=280&token={}",
            ctx.video_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(ctx.storyboard_svc.frame_generation_count(), 1);
}
