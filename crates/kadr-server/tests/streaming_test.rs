use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::create_router;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::SessionRegistry;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::ServiceExt;

#[tokio::test]
async fn test_http_206_range_streaming() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
        })
        .await
        .unwrap();

    let dir = tempdir().unwrap();
    let video_path = dir.path().join("sample.mp4");
    let mut file = File::create(&video_path).unwrap();
    // Write 1000 bytes (0..1000)
    let dummy_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    file.write_all(&dummy_data).unwrap();

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Sample Movie".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: 1000,
            file_path: video_path,
            file_name: "sample.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 120,
                container: Some("mp4".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
        }])
        .await
        .unwrap();

    let items = media_repo.list_by_library("lib1", 1, 0).await.unwrap();
    let item_id = items[0].id.unwrap();

    let jwt = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token = jwt
        .generate_token(&User {
            id: "u1".to_string(),
            username: "test".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Standard,
            created_at: 1000,
        })
        .unwrap();

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        Arc::new(SessionRegistry::new()),
    );

    // 1. Full Stream (no Range) -> 200 OK
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-length").unwrap(), "1000");

    // 2. Partial Content (bytes=100-199) -> 206 Partial Content
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=100-199")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes 100-199/1000");
    assert_eq!(res.headers().get("content-length").unwrap(), "100");

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(body_bytes.len(), 100);
    assert_eq!(&body_bytes[..], &dummy_data[100..200]);

    // 3. Query token param (?token=...)
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}?token={}", item_id, token))
        .header("range", "bytes=0-9")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
}

#[tokio::test]
async fn test_range_edge_cases_and_416() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
        })
        .await
        .unwrap();

    let dir = tempdir().unwrap();
    let video_path = dir.path().join("sample.mkv");
    let mut file = File::create(&video_path).unwrap();
    let dummy_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    file.write_all(&dummy_data).unwrap();

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Sample Movie".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: 1000,
            file_path: video_path,
            file_name: "sample.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 120,
                container: Some("mkv".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
        }])
        .await
        .unwrap();

    let items = media_repo.list_by_library("lib1", 1, 0).await.unwrap();
    let item_id = items[0].id.unwrap();

    let jwt = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token = jwt
        .generate_token(&User {
            id: "u1".to_string(),
            username: "test".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Standard,
            created_at: 1000,
        })
        .unwrap();

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        Arc::new(SessionRegistry::new()),
    );

    // Open-ended range (bytes=900-)
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=900-")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes 900-999/1000");
    assert_eq!(res.headers().get("content-length").unwrap(), "100");
    assert_eq!(res.headers().get("content-type").unwrap(), "video/x-matroska");
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(&body_bytes[..], &dummy_data[900..1000]);

    // Suffix range (bytes=-50)
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=-50")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes 950-999/1000");
    assert_eq!(res.headers().get("content-length").unwrap(), "50");
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(&body_bytes[..], &dummy_data[950..1000]);

    // Unsatisfiable range (bytes=2000-3000) -> 416
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=2000-3000")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes */1000");
    assert_eq!(res.headers().get("accept-ranges").unwrap(), "bytes");

    // Unsatisfiable open-ended range (bytes=9999-) -> 416
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=9999-")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(res.headers().get("content-range").unwrap(), "bytes */1000");
    assert_eq!(res.headers().get("accept-ranges").unwrap(), "bytes");

    // Unrecognized range unit (chars=0-100) -> RFC 7233 §3.1 Ignore fallback to 200 OK
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "chars=0-100")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-length").unwrap(), "1000");

    // Malformed range header -> RFC 7233 §3.1 Ignore fallback to 200 OK
    let req = Request::builder()
        .uri(format!("/api/v1/stream/{}", item_id))
        .header("authorization", format!("Bearer {}", token))
        .header("range", "bytes=invalid-syntax")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get("content-length").unwrap(), "1000");

    // Invalid item_id -> 404
    let req = Request::builder()
        .uri("/api/v1/stream/999999")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
