use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::ServiceExt;
use kadr_core::models::{Library, MediaItem, MediaType, TechnicalInfo, MediaMetadata, UserRole, WatchState};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::api::create_router;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, MediaItemRepository, LibraryRepository};

#[tokio::test]
async fn test_full_milestone_2_user_stream_and_scrobble_journey() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    // 1. Prepare dummy movie file on disk
    let dir = tempdir().unwrap();
    let video_path: PathBuf = dir.path().join("film.mkv");
    let mut file = File::create(&video_path).unwrap();
    let sample_bytes: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    file.write_all(&sample_bytes).unwrap();

    lib_repo.create(&Library {
        id: "lib1".to_string(),
        name: "Movies".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1000,
    }).await.unwrap();

    media_repo.upsert_batch(&[MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "The Nightingale's Prayer".to_string(),
        original_title: Some("Doaa al-Karawan".to_string()),
        release_year: Some(1959),
        added_at: 1000,
        file_path: video_path,
        file_name: "film.mkv".to_string(),
        file_size: 5000,
        technical: TechnicalInfo {
            duration_seconds: 6000,
            container: Some("mkv".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata::default(),
    }]).await.unwrap();

    let media_item_id = media_repo.list_by_library("lib1", 1, 0).await.unwrap()[0].id.unwrap();

    // 2. Setup user and server router
    let admin_user = kadr_core::models::User {
        id: "admin-uid".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo.create(&admin_user).await.unwrap();

    let jwt_svc = JwtService::new("test-secret-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt_svc,
        rate_limiter,
        session_registry,
    );

    // 3. User lists profiles
    let req = Request::builder().uri("/api/v1/users/profiles").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. User logs in with 4-digit PIN
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-uid","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let auth_data: Value = serde_json::from_slice(&body).unwrap();
    let token = auth_data["token"].as_str().unwrap();

    // 5. User requests HTTP 206 range stream: bytes=1000-1999
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", media_item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=1000-1999")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes 1000-1999/5000");
    assert_eq!(res.headers().get("content-type").unwrap(), "video/x-matroska");

    let chunk = axum::body::to_bytes(res.into_body(), 2048).await.unwrap();
    assert_eq!(chunk.len(), 1000);
    assert_eq!(&chunk[..], &sample_bytes[1000..2000]);

    // 6. User starts playback session
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, media_item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let sess_data: Value = serde_json::from_slice(&body).unwrap();
    let session_id = sess_data["session_id"].as_str().unwrap();

    // 7. Send heartbeat ping (>60s) -> marks as InProgress
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":300}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let state = playback_repo
        .get_state("admin-uid", media_item_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.watch_state, WatchState::InProgress);

    // 8. Verify Continue Watching returns this item
    let req = Request::builder()
        .uri("/api/v1/playback/continue-watching")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 2048).await.unwrap();
    let list: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["media_item_id"].as_i64().unwrap(), media_item_id);
    assert_eq!(list[0]["playback_position_seconds"].as_i64().unwrap(), 300);
}
