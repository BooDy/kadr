use axum::http::{header, Request, StatusCode};
use std::process::Command;
use tower::ServiceExt;

mod test_helpers {
    use axum::Router;
    use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
    use kadr_server::api::create_router;
    use kadr_server::auth::jwt::JwtService;
    use kadr_server::auth::rate_limiter::RateLimiter;
    use kadr_server::config::TranscodingSettings;
    use kadr_server::playback::session::SessionRegistry;
    use kadr_server::streaming::hls::HlsSessionManager;
    use kadr_storage::pool::{create_in_memory_pool, initialize_database};
    use kadr_storage::repos::{
        LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
    };
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    pub async fn setup_e2e_with_media(video_path: &Path) -> (Router, i64) {
        let pool = create_in_memory_pool().expect("Failed to create in-memory pool");
        initialize_database(&pool)
            .await
            .expect("Failed to initialize database");

        let user_repo = UserRepository::new(pool.clone());
        let playback_repo = PlaybackRepository::new(pool.clone());
        let media_repo = MediaItemRepository::new(pool.clone());
        let lib_repo = LibraryRepository::new(pool.clone());

        let library_dir = video_path.parent().unwrap_or(video_path);
        lib_repo
            .insert(&Library {
                id: "e2e-lib".to_string(),
                name: "E2E Library".to_string(),
                path: library_dir.to_path_buf(),
                media_type: MediaType::Movie,
                created_at: 1000,
                is_private: false,
                ..Default::default()
            })
            .await
            .expect("Failed to insert library");

        let file_size = std::fs::metadata(video_path)
            .map(|m| m.len())
            .unwrap_or(1024);
        let item = MediaItem {
            id: None,
            library_id: "e2e-lib".to_string(),
            item_type: MediaType::Movie,
            title: "E2E Synthetic Video".to_string(),
            added_at: 1000,
            file_path: video_path.to_path_buf(),
            file_name: video_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            file_size,
            technical: TechnicalInfo {
                duration_seconds: 6,
                container: Some("mp4".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                resolution: Some("640x360".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
            ..Default::default()
        };

        media_repo
            .upsert_batch(&[item])
            .await
            .expect("Failed to upsert media item");
        let items = media_repo
            .list_by_library("e2e-lib", 1, 0)
            .await
            .expect("Failed to list media items");
        let media_id = items[0].id.expect("Media item missing ID");

        let jwt_svc = JwtService::new("test-secret-at-least-32-bytes-long", 3600);
        let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
        let session_registry = Arc::new(SessionRegistry::new());

        let app = create_router(
            user_repo,
            playback_repo,
            media_repo,
            lib_repo,
            jwt_svc,
            rate_limiter,
            session_registry,
        );

        let transcode_dir = library_dir.join("transcodes");
        let mut settings = TranscodingSettings::default();
        settings.temp_dir = transcode_dir;
        let session_mgr = Arc::new(HlsSessionManager::new(settings));
        let app = app.layer(axum::Extension(session_mgr));

        (app, media_id)
    }
}

#[tokio::test]
async fn test_e2e_hls_transcoding_and_segment_delivery() {
    // 1. Check if ffmpeg is available on the host
    let ffmpeg_available = Command::new("ffmpeg").arg("-version").output().is_ok();
    if !ffmpeg_available {
        eprintln!("Skipping test_e2e_hls_transcoding_and_segment_delivery: ffmpeg not available");
        return;
    }

    // 2. Generate a 6-second synthetic test video (H.264 + AAC)
    let temp_dir = tempfile::tempdir().unwrap();
    let video_path = temp_dir.path().join("sample.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=6:size=640x360:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:duration=6",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            video_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute ffmpeg generator");
    assert!(status.status.success());

    // 3. Initialize test server and seed item in database
    let (app, media_id) = test_helpers::setup_e2e_with_media(&video_path).await;

    // 4. Request master playlist
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{media_id}/hls/master.m3u8?video_codec=h264&audio_codec=aac"
        ))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/vnd.apple.mpegurl"
    );

    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let playlist_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(playlist_str.contains("#EXTM3U"));
    assert!(playlist_str.contains(".ts"));

    // 5. Extract first segment URL and fetch it
    let segment_line = playlist_str
        .lines()
        .find(|l| l.contains("segment_0000.ts"))
        .expect("Should contain segment_0000.ts");

    let seg_req = Request::builder()
        .uri(segment_line)
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let seg_resp = app.clone().oneshot(seg_req).await.unwrap();
    assert_eq!(seg_resp.status(), StatusCode::OK);
    assert_eq!(
        seg_resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "video/mp2t"
    );

    let seg_bytes = axum::body::to_bytes(seg_resp.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    assert!(!seg_bytes.is_empty());
    // MPEG-TS sync byte is 0x47
    assert_eq!(seg_bytes[0], 0x47);

    // 6. Verify session teardown
    // Segment URL format: /api/v1/media/{media_id}/hls/sessions/{session_id}/segment_0000.ts
    let parts: Vec<&str> = segment_line.split('/').collect();
    let session_idx = parts
        .iter()
        .position(|&p| p == "sessions")
        .expect("Segment URL must contain /sessions/");
    let session_id = parts[session_idx + 1];

    let delete_req = Request::builder()
        .uri(format!(
            "/api/v1/media/{media_id}/hls/sessions/{session_id}"
        ))
        .method("DELETE")
        .body(axum::body::Body::empty())
        .unwrap();

    let delete_resp = app.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    // After session teardown, requesting the segment again returns 404 NOT_FOUND
    let post_delete_req = Request::builder()
        .uri(segment_line)
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let post_delete_resp = app.clone().oneshot(post_delete_req).await.unwrap();
    assert_eq!(post_delete_resp.status(), StatusCode::NOT_FOUND);
}
