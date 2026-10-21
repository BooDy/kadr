use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

mod test_helpers {
    use axum::Router;
    use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
    use kadr_server::api::create_router;
    use kadr_server::auth::jwt::JwtService;
    use kadr_server::auth::rate_limiter::RateLimiter;
    use kadr_server::playback::SessionRegistry;
    use kadr_server::streaming::hls::session::HlsSessionManager;
    use kadr_storage::pool::{create_in_memory_pool, initialize_database};
    use kadr_storage::repos::{
        LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
    };
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    pub struct TestState {
        pub hls_manager: Arc<HlsSessionManager>,
        pub media_repo: MediaItemRepository,
        pub lib_repo: LibraryRepository,
        pub jwt_secret: String,
    }

    pub async fn create_test_app() -> (Router, TestState) {
        let pool = create_in_memory_pool().unwrap();
        initialize_database(&pool).await.unwrap();

        let user_repo = UserRepository::new(pool.clone());
        let playback_repo = PlaybackRepository::new(pool.clone());
        let media_repo = MediaItemRepository::new(pool.clone());
        let lib_repo = LibraryRepository::new(pool.clone());
        let jwt_secret = "test-secret-with-sufficient-length-for-hmac-sha256".to_string();
        let jwt_svc = JwtService::new(&jwt_secret, 3600);
        let limiter = RateLimiter::new(5, Duration::from_secs(60), Duration::from_secs(60));
        let session_registry = Arc::new(SessionRegistry::new());

        let app = create_router(
            user_repo,
            playback_repo,
            media_repo.clone(),
            lib_repo.clone(),
            jwt_svc,
            limiter,
            session_registry,
        );

        let hls_manager = Arc::new(HlsSessionManager::new(Default::default()));
        let app = app.layer(axum::Extension(hls_manager.clone()));

        (
            app,
            TestState {
                hls_manager,
                media_repo,
                lib_repo,
                jwt_secret,
            },
        )
    }

    pub async fn seed_media_item(
        media_repo: &MediaItemRepository,
        lib_repo: &LibraryRepository,
        is_private: bool,
        file_path: PathBuf,
    ) -> (Library, MediaItem) {
        let lib_id = format!("lib-{}", uuid::Uuid::new_v4());
        let lib = Library {
            id: lib_id.clone(),
            name: "Test Library".to_string(),
            path: PathBuf::from("/tmp/test"),
            paths: vec![PathBuf::from("/tmp/test")],
            media_type: MediaType::Movie,
            is_private,
            pin_hash: if is_private {
                Some(kadr_server::auth::pin::hash_pin("1234").unwrap())
            } else {
                None
            },
            created_at: 1_700_000_000,
        };
        lib_repo.insert(&lib).await.unwrap();

        let item = MediaItem {
            id: None,
            library_id: lib_id,
            item_type: MediaType::Movie,
            title: "Test Movie".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1_700_000_000,
            file_path,
            file_name: "test.mp4".to_string(),
            file_size: 1024,
            technical: TechnicalInfo {
                duration_seconds: 60,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mp4".to_string()),
            },
            metadata: MediaMetadata::default(),
        };
        media_repo.upsert_batch(&[item]).await.unwrap();
        let items = media_repo.list_by_library(&lib.id, 1, 0).await.unwrap();
        (lib, items[0].clone())
    }
}

#[tokio::test]
async fn test_hls_invalid_uuid_segment_path_returns_bad_request() {
    let (app, _state) = test_helpers::create_test_app().await;

    let req = Request::builder()
        .uri("/api/v1/media/1/hls/sessions/not-a-valid-uuid/segment_0000.ts")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_hls_invalid_uuid_delete_session_returns_bad_request() {
    let (app, _state) = test_helpers::create_test_app().await;

    let req = Request::builder()
        .uri("/api/v1/media/1/hls/sessions/not-a-valid-uuid")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_hls_delete_session_success() {
    let (app, state) = test_helpers::create_test_app().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = temp_dir.path().join(&session_id);
    tokio::fs::create_dir_all(&session_path).await.unwrap();

    state
        .hls_manager
        .register_test_session(&session_id, 42, session_path.clone(), false)
        .await;

    assert_eq!(state.hls_manager.active_session_count().await, 1);

    let req = Request::builder()
        .uri(format!("/api/v1/media/42/hls/sessions/{}", session_id))
        .method("DELETE")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(state.hls_manager.active_session_count().await, 0);
}

#[tokio::test]
async fn test_hls_master_playlist_not_found_item() {
    let (app, _state) = test_helpers::create_test_app().await;

    let req = Request::builder()
        .uri("/api/v1/media/99999/hls/master.m3u8")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_hls_master_playlist_locked_library_forbidden() {
    let (app, state) = test_helpers::create_test_app().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let video_file = temp_dir.path().join("secret.mp4");
    tokio::fs::write(&video_file, b"content").await.unwrap();

    let (_lib, item) =
        test_helpers::seed_media_item(&state.media_repo, &state.lib_repo, true, video_file).await;
    let item_id = item.id.unwrap();

    // 1. Without unlock token: 403 Forbidden
    let req = Request::builder()
        .uri(format!("/api/v1/media/{}/hls/master.m3u8", item_id))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("LIBRARY_LOCKED"));
}

#[tokio::test]
async fn test_hls_master_playlist_rewrites_urls() {
    let (app, state) = test_helpers::create_test_app().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let video_file = temp_dir.path().join("source.mp4");
    tokio::fs::write(&video_file, b"sample-video-bytes")
        .await
        .unwrap();

    let (_lib, item) =
        test_helpers::seed_media_item(&state.media_repo, &state.lib_repo, false, video_file).await;
    let item_id = item.id.unwrap();

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = temp_dir.path().join(&session_id);
    tokio::fs::create_dir_all(&session_path).await.unwrap();

    let playlist_file = session_path.join("playlist.m3u8");
    let initial_m3u8 = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.000000,\nsegment_0000.ts\n#EXTINF:4.000000,\n/custom/path/segment_0001.ts\n#EXT-X-ENDLIST\n";
    tokio::fs::write(&playlist_file, initial_m3u8)
        .await
        .unwrap();

    state
        .hls_manager
        .register_test_session(&session_id, item_id, session_path, false)
        .await;

    let req = Request::builder()
        .uri(format!("/api/v1/media/{}/hls/master.m3u8", item_id))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/vnd.apple.mpegurl"
    );

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    assert!(body_str.contains(&format!(
        "/api/v1/media/{}/hls/sessions/{}/segment_0000.ts",
        item_id, session_id
    )));
    assert!(body_str.contains(&format!(
        "/api/v1/media/{}/hls/sessions/{}/segment_0001.ts",
        item_id, session_id
    )));
}

#[tokio::test]
async fn test_hls_stream_segment_invalid_segment_format() {
    let (app, _state) = test_helpers::create_test_app().await;
    let session_id = uuid::Uuid::new_v4().to_string();

    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/1/hls/sessions/{}/invalid_segment.txt",
            session_id
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_hls_stream_segment_not_found_session() {
    let (app, _state) = test_helpers::create_test_app().await;
    let session_id = uuid::Uuid::new_v4().to_string();

    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/1/hls/sessions/{}/segment_0000.ts",
            session_id
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_hls_stream_segment_media_id_mismatch() {
    let (app, state) = test_helpers::create_test_app().await;
    let session_id = uuid::Uuid::new_v4().to_string();
    let temp_dir = tempfile::tempdir().unwrap();
    let session_path = temp_dir.path().join(&session_id);
    tokio::fs::create_dir_all(&session_path).await.unwrap();

    state
        .hls_manager
        .register_test_session(&session_id, 10, session_path, false)
        .await;

    // Requested media_id is 99, but session.media_id is 10
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/99/hls/sessions/{}/segment_0000.ts",
            session_id
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_hls_stream_segment_success() {
    let (app, state) = test_helpers::create_test_app().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let video_file = temp_dir.path().join("movie.mp4");
    tokio::fs::write(&video_file, b"dummy-video").await.unwrap();

    let (_lib, item) =
        test_helpers::seed_media_item(&state.media_repo, &state.lib_repo, false, video_file).await;
    let item_id = item.id.unwrap();

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = temp_dir.path().join(&session_id);
    tokio::fs::create_dir_all(&session_path).await.unwrap();

    let segment_file = session_path.join("segment_0000.ts");
    let test_bytes = b"ts-transport-stream-data-chunk";
    tokio::fs::write(&segment_file, test_bytes).await.unwrap();

    state
        .hls_manager
        .register_test_session(&session_id, item_id, session_path, false)
        .await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/hls/sessions/{}/segment_0000.ts",
            item_id, session_id
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "video/mp2t"
    );
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=3600"
    );

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), test_bytes);
}

#[tokio::test]
async fn test_hls_stream_segment_locked_library_without_token() {
    let (app, state) = test_helpers::create_test_app().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let video_file = temp_dir.path().join("private_movie.mp4");
    tokio::fs::write(&video_file, b"private-video")
        .await
        .unwrap();

    let (lib, item) =
        test_helpers::seed_media_item(&state.media_repo, &state.lib_repo, true, video_file).await;
    let item_id = item.id.unwrap();

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = temp_dir.path().join(&session_id);
    tokio::fs::create_dir_all(&session_path).await.unwrap();

    let segment_file = session_path.join("segment_0000.ts");
    tokio::fs::write(&segment_file, b"bytes").await.unwrap();

    state
        .hls_manager
        .register_test_session(&session_id, item_id, session_path, false)
        .await;

    // 1. Without token: 403 Forbidden
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/hls/sessions/{}/segment_0000.ts",
            item_id, session_id
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 2. With valid unlock token: 200 OK
    let unlock_svc =
        kadr_server::api::unlock_token::UnlockTokenService::new(&state.jwt_secret, 3600);
    let (token, _) = unlock_svc.generate_token(&lib.id).unwrap();

    let req_unlocked = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/hls/sessions/{}/segment_0000.ts",
            item_id, session_id
        ))
        .header("X-Kadr-Unlocked", token)
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp_unlocked = app.oneshot(req_unlocked).await.unwrap();
    assert_eq!(resp_unlocked.status(), StatusCode::OK);
}
