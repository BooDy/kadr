use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::models::{
    AudioStreamInfo, Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::audio_routes::AudioProbeService;
use kadr_server::api::create_router;
use kadr_server::api::unlock_token::UnlockTokenService;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::SessionRegistry;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use tempfile::tempdir;
use tower::ServiceExt;

struct TestEnv {
    app: axum::Router,
    media_repo: MediaItemRepository,
    lib_repo: LibraryRepository,
    jwt_secret: String,
    user_token: String,
}

async fn setup_test_env() -> TestEnv {
    let pool = create_in_memory_pool().expect("pool");
    initialize_database(&pool).await.expect("init db");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let user = User {
        id: "user-1".to_string(),
        username: "tester".to_string(),
        pin_hash: hash_pin("1234").expect("hash pin"),
        role: UserRole::Admin,
        created_at: 1_700_000_000,
    };
    user_repo.create(&user).await.expect("create user");

    let jwt_secret = "test-secret-with-sufficient-entropy-for-hmac-sha256".to_string();
    let jwt_svc = JwtService::new(&jwt_secret, 3600);
    let user_token = jwt_svc.generate_token(&user).expect("token");
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

    TestEnv {
        app,
        media_repo,
        lib_repo,
        jwt_secret,
        user_token,
    }
}

async fn seed_library(lib_repo: &LibraryRepository, id: &str, is_private: bool) {
    lib_repo
        .insert(&Library {
            id: id.to_string(),
            name: format!("Library {id}"),
            path: PathBuf::from(format!("/tmp/{id}")),
            paths: vec![PathBuf::from(format!("/tmp/{id}"))],
            media_type: MediaType::Movie,
            is_private,
            pin_hash: if is_private {
                Some(hash_pin("4321").expect("hash pin"))
            } else {
                None
            },
            created_at: 1_700_000_000,
        })
        .await
        .expect("insert library");
}

#[tokio::test]
async fn test_get_audio_tracks_returns_persisted_tracks_and_404_on_missing() {
    let env = setup_test_env().await;
    seed_library(&env.lib_repo, "lib-pub", false).await;

    let file_path = PathBuf::from("/tmp/lib-pub/anime.mkv");
    let expected_tracks = vec![
        AudioStreamInfo {
            audio_index: 0,
            stream_index: 1,
            codec: "aac".to_string(),
            channels: Some(2),
            language: "jpn".to_string(),
            title: Some("Japanese Original".to_string()),
            is_default: true,
            is_commentary: false,
        },
        AudioStreamInfo {
            audio_index: 1,
            stream_index: 2,
            codec: "dts".to_string(),
            channels: Some(6),
            language: "eng".to_string(),
            title: Some("English Dub".to_string()),
            is_default: false,
            is_commentary: false,
        },
    ];

    env.media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib-pub".to_string(),
            item_type: MediaType::Movie,
            title: "Anime Film".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1_700_000_100,
            file_path: file_path.clone(),
            file_name: "anime.mkv".to_string(),
            file_size: 2048,
            technical: TechnicalInfo {
                duration_seconds: 5400,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: expected_tracks.clone(),
            },
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .expect("upsert");

    let item = env
        .media_repo
        .find_by_path(&file_path)
        .await
        .unwrap()
        .unwrap();
    let item_id = item.id.unwrap();

    // 1. Missing auth -> 401
    let req_unauth = Request::get(format!("/api/v1/items/{item_id}/audio-tracks"))
        .body(Body::empty())
        .unwrap();
    let resp_unauth = env.app.clone().oneshot(req_unauth).await.unwrap();
    assert_eq!(resp_unauth.status(), StatusCode::UNAUTHORIZED);

    // 2. Missing item -> 404
    let req_404 = Request::get("/api/v1/items/999999/audio-tracks")
        .header("authorization", format!("Bearer {}", env.user_token))
        .body(Body::empty())
        .unwrap();
    let resp_404 = env.app.clone().oneshot(req_404).await.unwrap();
    assert_eq!(resp_404.status(), StatusCode::NOT_FOUND);

    // 3. Valid request -> 200 with persisted tracks
    let req = Request::get(format!("/api/v1/items/{item_id}/audio-tracks"))
        .header("authorization", format!("Bearer {}", env.user_token))
        .body(Body::empty())
        .unwrap();
    let resp = env.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let tracks: Vec<AudioStreamInfo> = serde_json::from_slice(&body).unwrap();
    assert_eq!(tracks, expected_tracks);
}

#[tokio::test]
async fn test_get_audio_tracks_private_library_lock_and_unlock() {
    let env = setup_test_env().await;
    seed_library(&env.lib_repo, "lib-priv", true).await;

    let file_path = PathBuf::from("/tmp/lib-priv/secret.mkv");
    let expected_tracks = vec![AudioStreamInfo {
        audio_index: 0,
        stream_index: 1,
        codec: "aac".to_string(),
        channels: Some(2),
        language: "eng".to_string(),
        title: None,
        is_default: true,
        is_commentary: false,
    }];

    env.media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib-priv".to_string(),
            item_type: MediaType::Movie,
            title: "Secret Film".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1_700_000_200,
            file_path: file_path.clone(),
            file_name: "secret.mkv".to_string(),
            file_size: 1024,
            technical: TechnicalInfo {
                duration_seconds: 120,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: expected_tracks.clone(),
            },
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .expect("upsert");

    let item_id = env
        .media_repo
        .find_by_path(&file_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();

    // Locked private library -> 403 LIBRARY_LOCKED
    let req_locked = Request::get(format!("/api/v1/items/{item_id}/audio-tracks"))
        .header("authorization", format!("Bearer {}", env.user_token))
        .body(Body::empty())
        .unwrap();
    let resp_locked = env.app.clone().oneshot(req_locked).await.unwrap();
    assert_eq!(resp_locked.status(), StatusCode::FORBIDDEN);
    let body_locked = resp_locked.into_body().collect().await.unwrap().to_bytes();
    let err_json: serde_json::Value = serde_json::from_slice(&body_locked).unwrap();
    assert_eq!(err_json["error"], "LIBRARY_LOCKED");

    // Unlocked via x-kadr-unlocked -> 200 OK
    let unlock_svc = UnlockTokenService::new(&env.jwt_secret, 3600);
    let (unlock_token, _) = unlock_svc.generate_token("lib-priv").unwrap();
    let req_unlocked = Request::get(format!("/api/v1/items/{item_id}/audio-tracks"))
        .header("authorization", format!("Bearer {}", env.user_token))
        .header("x-kadr-unlocked", unlock_token)
        .body(Body::empty())
        .unwrap();
    let resp_unlocked = env.app.clone().oneshot(req_unlocked).await.unwrap();
    assert_eq!(resp_unlocked.status(), StatusCode::OK);
    let body_unlocked = resp_unlocked
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let tracks: Vec<AudioStreamInfo> = serde_json::from_slice(&body_unlocked).unwrap();
    assert_eq!(tracks, expected_tracks);
}

#[tokio::test]
async fn test_get_audio_tracks_synthesizes_track_zero_for_synthetic_file() {
    let env = setup_test_env().await;
    seed_library(&env.lib_repo, "lib-synth", false).await;

    let temp_dir = tempdir().unwrap();
    let video_path = temp_dir.path().join("synthetic.mp4");
    tokio::fs::write(&video_path, b"not-a-real-mp4-container")
        .await
        .unwrap();

    env.media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib-synth".to_string(),
            item_type: MediaType::Movie,
            title: "Synthetic Movie".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1_700_000_300,
            file_path: video_path.clone(),
            file_name: "synthetic.mp4".to_string(),
            file_size: 24,
            technical: TechnicalInfo {
                duration_seconds: 120,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("ac3".to_string()),
                audio_channels: Some(6),
                container: Some("mp4".to_string()),
                audio_tracks: Vec::new(),
            },
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .expect("upsert");

    let item_id = env
        .media_repo
        .find_by_path(&video_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();

    let req = Request::get(format!("/api/v1/items/{item_id}/audio-tracks"))
        .header("authorization", format!("Bearer {}", env.user_token))
        .body(Body::empty())
        .unwrap();
    let resp = env.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let tracks: Vec<AudioStreamInfo> = serde_json::from_slice(&body).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].audio_index, 0);
    assert_eq!(tracks[0].codec, "ac3");
    assert_eq!(tracks[0].channels, Some(6));
    assert_eq!(tracks[0].language, "und");
    assert!(tracks[0].is_default);

    // Out-of-range HLS audio_track on synthetic item must return 400 BAD_REQUEST
    let hls_req = Request::get(format!(
        "/api/v1/media/{item_id}/hls/master.m3u8?audio_track=9"
    ))
    .body(Body::empty())
    .unwrap();
    let hls_resp = env.app.clone().oneshot(hls_req).await.unwrap();
    assert_eq!(hls_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_audio_tracks_lazy_ffprobe_backfill_and_hls_validation_on_real_mkv() {
    let ffmpeg_ok = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok();
    let ffprobe_ok = std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .is_ok();
    if !ffmpeg_ok || !ffprobe_ok {
        return;
    }

    let env = setup_test_env().await;
    seed_library(&env.lib_repo, "lib-real", false).await;

    let temp_dir = tempdir().unwrap();
    let mkv_path = temp_dir.path().join("dual_audio.mkv");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:d=1",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo:d=1",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=5.1:d=1",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:a:0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a:0",
            "aac",
            "-c:a:1",
            "ac3",
            "-metadata:s:a:0",
            "language=jpn",
            "-metadata:s:a:0",
            "title=Japanese 2.0",
            "-disposition:a:0",
            "default",
            "-metadata:s:a:1",
            "language=eng",
            "-metadata:s:a:1",
            "title=English 5.1",
            "-disposition:a:1",
            "0",
            mkv_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should create dual-audio mkv");
    assert!(status.success(), "ffmpeg dual-audio mux failed");

    env.media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib-real".to_string(),
            item_type: MediaType::Movie,
            title: "Dual Audio MKV".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1_700_000_400,
            file_path: mkv_path.clone(),
            file_name: "dual_audio.mkv".to_string(),
            file_size: 4096,
            technical: TechnicalInfo {
                duration_seconds: 1,
                resolution: Some("240p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: Vec::new(),
            },
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .expect("upsert");

    let item = env
        .media_repo
        .find_by_path(&mkv_path)
        .await
        .unwrap()
        .unwrap();
    let item_id = item.id.unwrap();
    assert!(item.technical.audio_tracks.is_empty());

    // First verify singleflight concurrent probe via AudioProbeService directly on a second service instance
    let probe_svc = AudioProbeService::new();
    let (r1, r2) = tokio::join!(
        probe_svc.list_or_probe_audio_tracks(&env.media_repo, &item),
        probe_svc.list_or_probe_audio_tracks(&env.media_repo, &item),
    );
    assert_eq!(r1.len(), 2);
    assert_eq!(r2.len(), 2);
    assert_eq!(probe_svc.in_flight_count(), 0);

    // Clear audio_tracks in DB to exercise the HTTP endpoint lazy backfill path
    env.media_repo
        .update_audio_tracks(item_id, &[])
        .await
        .unwrap();
    let cleared = env.media_repo.get_by_id(item_id).await.unwrap().unwrap();
    assert!(cleared.technical.audio_tracks.is_empty());

    let req = Request::get(format!("/api/v1/items/{item_id}/audio-tracks"))
        .header("authorization", format!("Bearer {}", env.user_token))
        .body(Body::empty())
        .unwrap();
    let resp = env.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let tracks: Vec<AudioStreamInfo> = serde_json::from_slice(&body).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].audio_index, 0);
    assert_eq!(tracks[0].codec, "aac");
    assert_eq!(tracks[0].language, "jpn");
    assert_eq!(tracks[0].title.as_deref(), Some("Japanese 2.0"));
    assert!(tracks[0].is_default);

    assert_eq!(tracks[1].audio_index, 1);
    assert_eq!(tracks[1].codec, "ac3");
    assert_eq!(tracks[1].channels, Some(6));
    assert_eq!(tracks[1].language, "eng");
    assert_eq!(tracks[1].title.as_deref(), Some("English 5.1"));
    assert!(!tracks[1].is_default);

    // Verify lazy backfill persisted into SQLite
    let persisted = env.media_repo.get_by_id(item_id).await.unwrap().unwrap();
    assert_eq!(persisted.technical.audio_tracks, tracks);

    // Out-of-range HLS audio_track=9 returns 400 BAD_REQUEST
    let hls_req = Request::get(format!(
        "/api/v1/media/{item_id}/hls/master.m3u8?audio_track=9"
    ))
    .body(Body::empty())
    .unwrap();
    let hls_resp = env.app.clone().oneshot(hls_req).await.unwrap();
    assert_eq!(hls_resp.status(), StatusCode::BAD_REQUEST);

    // Valid HLS audio_track=1 lazily probes if audio_tracks was empty and creates HLS playlist
    let hls_dir = temp_dir.path().join("hls");
    let hls_mgr = Arc::new(kadr_server::streaming::hls::HlsSessionManager::new(
        kadr_server::config::TranscodingSettings {
            temp_dir: hls_dir,
            ..Default::default()
        },
    ));
    let app_with_hls = env
        .app
        .clone()
        .layer(axum::Extension(hls_mgr.clone()))
        .layer(axum::Extension(Arc::new(AudioProbeService::new())));

    env.media_repo
        .update_audio_tracks(item_id, &[])
        .await
        .unwrap();

    let hls_valid_req = Request::get(format!(
        "/api/v1/media/{item_id}/hls/master.m3u8?audio_track=1"
    ))
    .body(Body::empty())
    .unwrap();
    let hls_valid_resp = app_with_hls.clone().oneshot(hls_valid_req).await.unwrap();
    assert_eq!(hls_valid_resp.status(), StatusCode::OK);
    let playlist_bytes = hls_valid_resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let playlist_str = String::from_utf8_lossy(&playlist_bytes);
    assert!(playlist_str.contains("#EXTM3U"));

    let persisted_via_hls = env.media_repo.get_by_id(item_id).await.unwrap().unwrap();
    assert_eq!(persisted_via_hls.technical.audio_tracks.len(), 2);

    // Clean up HLS session so no ffmpeg process outlives the test
    if let Some(session_line) = playlist_str.lines().find(|l| l.contains("/hls/sessions/")) {
        if let Some(rest) = session_line.split("/hls/sessions/").nth(1) {
            if let Some(session_id) = rest.split('/').next() {
                hls_mgr.terminate_session(session_id).await;
            }
        }
    }
}
