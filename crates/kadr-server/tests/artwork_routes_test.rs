use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo,
};
use kadr_server::api::artwork_routes::resolve_artwork_mime;
use kadr_server::api::create_router;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::SessionRegistry;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use tempfile::tempdir;
use tower::ServiceExt;

struct ArtworkTestContext {
    app: axum::Router,
    valid_item_id: i64,
    missing_artwork_item_id: i64,
    not_on_disk_item_id: i64,
    poster_bytes: Vec<u8>,
    backdrop_bytes: Vec<u8>,
    _temp_dir: tempfile::TempDir,
}

async fn setup_test_context() -> ArtworkTestContext {
    let pool = create_in_memory_pool().expect("failed to create pool");
    initialize_database(&pool).await.expect("failed to initialize db");

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
        .expect("create lib failed");

    let dir = tempdir().expect("tempdir failed");

    // Poster file
    let poster_path = dir.path().join("poster.jpg");
    let mut poster_file = File::create(&poster_path).expect("create poster file");
    let poster_bytes: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    poster_file.write_all(&poster_bytes).expect("write poster bytes");

    // Backdrop file
    let backdrop_path = dir.path().join("backdrop.png");
    let mut backdrop_file = File::create(&backdrop_path).expect("create backdrop file");
    let backdrop_bytes: Vec<u8> = (0..8000).map(|i| ((i * 7) % 256) as u8).collect();
    backdrop_file.write_all(&backdrop_bytes).expect("write backdrop bytes");

    let dummy1 = dir.path().join("sample1.mp4");
    File::create(&dummy1).expect("create dummy1");
    let dummy2 = dir.path().join("sample2.mp4");
    File::create(&dummy2).expect("create dummy2");
    let dummy3 = dir.path().join("sample3.mp4");
    File::create(&dummy3).expect("create dummy3");

    // 1. Valid item with poster and backdrop
    media_repo
        .upsert_batch(&[
            MediaItem {
                id: None,
                library_id: "lib1".to_string(),
                item_type: MediaType::Movie,
                title: "Valid Movie".to_string(),
                original_title: None,
                release_year: Some(2022),
                added_at: 1000,
                file_path: dummy1,
                file_name: "sample1.mp4".to_string(),
                file_size: 100,
                technical: TechnicalInfo::default(),
                metadata: MediaMetadata {
                    poster_path: Some(poster_path.to_str().unwrap().to_string()),
                    backdrop_path: Some(backdrop_path.to_str().unwrap().to_string()),
                    ..Default::default()
                },
            },
            MediaItem {
                id: None,
                library_id: "lib1".to_string(),
                item_type: MediaType::Movie,
                title: "Missing Artwork Movie".to_string(),
                original_title: None,
                release_year: Some(2023),
                added_at: 1001,
                file_path: dummy2,
                file_name: "sample2.mp4".to_string(),
                file_size: 100,
                technical: TechnicalInfo::default(),
                metadata: MediaMetadata {
                    poster_path: None,
                    backdrop_path: Some("   ".to_string()),
                    ..Default::default()
                },
            },
            MediaItem {
                id: None,
                library_id: "lib1".to_string(),
                item_type: MediaType::Movie,
                title: "Nonexistent File Movie".to_string(),
                original_title: None,
                release_year: Some(2024),
                added_at: 1002,
                file_path: dummy3,
                file_name: "sample3.mp4".to_string(),
                file_size: 100,
                technical: TechnicalInfo::default(),
                metadata: MediaMetadata {
                    poster_path: Some("/nonexistent/path/to/poster.jpg".to_string()),
                    backdrop_path: Some("/nonexistent/path/to/backdrop.png".to_string()),
                    ..Default::default()
                },
            },
        ])
        .await
        .expect("upsert batch");

    let items = media_repo.list_by_library("lib1", 10, 0).await.expect("list items");
    let valid_item_id = items.iter().find(|i| i.title == "Valid Movie").unwrap().id.unwrap();
    let missing_artwork_item_id = items.iter().find(|i| i.title == "Missing Artwork Movie").unwrap().id.unwrap();
    let not_on_disk_item_id = items.iter().find(|i| i.title == "Nonexistent File Movie").unwrap().id.unwrap();

    let jwt = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        Arc::new(SessionRegistry::new()),
    );

    ArtworkTestContext {
        app,
        valid_item_id,
        missing_artwork_item_id,
        not_on_disk_item_id,
        poster_bytes,
        backdrop_bytes,
        _temp_dir: dir,
    }
}

#[tokio::test]
async fn test_valid_poster_streaming() {
    let ctx = setup_test_context().await;

    // Standard unauthenticated request (e.g. <img> tag in browser)
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/poster", ctx.valid_item_id))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "image/jpeg"
    );
    assert_eq!(
        res.headers().get("cache-control").unwrap(),
        "public, max-age=86400"
    );
    assert_eq!(
        res.headers().get("accept-ranges").unwrap(),
        "bytes"
    );
    assert_eq!(
        res.headers().get("content-length").unwrap(),
        &ctx.poster_bytes.len().to_string()
    );

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body_bytes.as_ref(), &ctx.poster_bytes[..]);
}

#[tokio::test]
async fn test_valid_backdrop_streaming() {
    let ctx = setup_test_context().await;

    // Standard unauthenticated request
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/backdrop", ctx.valid_item_id))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "image/png"
    );
    assert_eq!(
        res.headers().get("cache-control").unwrap(),
        "public, max-age=86400"
    );
    assert_eq!(
        res.headers().get("accept-ranges").unwrap(),
        "bytes"
    );
    assert_eq!(
        res.headers().get("content-length").unwrap(),
        &ctx.backdrop_bytes.len().to_string()
    );

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body_bytes.as_ref(), &ctx.backdrop_bytes[..]);
}

#[tokio::test]
async fn test_artwork_with_query_token_and_auth_header() {
    let ctx = setup_test_context().await;

    // Query token param
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/poster?token=some_jwt_token", ctx.valid_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body_bytes.as_ref(), &ctx.poster_bytes[..]);

    // Authorization header
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/backdrop", ctx.valid_item_id))
        .header("authorization", "Bearer some_jwt_token")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body_bytes.as_ref(), &ctx.backdrop_bytes[..]);
}

#[tokio::test]
async fn test_missing_artwork_path_returns_404() {
    let ctx = setup_test_context().await;

    // Poster path is None
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/poster", ctx.missing_artwork_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "Artwork not found");

    // Backdrop path is whitespace-only
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/backdrop", ctx.missing_artwork_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "Artwork not found");
}

#[tokio::test]
async fn test_artwork_file_not_on_disk_returns_404() {
    let ctx = setup_test_context().await;

    // Poster file doesn't exist on disk
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/poster", ctx.not_on_disk_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "Artwork not found");

    // Backdrop file doesn't exist on disk
    let req = Request::builder()
        .uri(format!("/api/v1/artwork/{}/backdrop", ctx.not_on_disk_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "Artwork not found");
}

#[tokio::test]
async fn test_nonexistent_item_id_returns_404() {
    let ctx = setup_test_context().await;

    // Poster for nonexistent item
    let req = Request::builder()
        .uri("/api/v1/artwork/999999/poster")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "Media item not found");

    // Backdrop for nonexistent item
    let req = Request::builder()
        .uri("/api/v1/artwork/999999/backdrop")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"], "Media item not found");
}

#[test]
fn test_resolve_artwork_mime_types() {
    assert_eq!(resolve_artwork_mime("/path/to/poster.jpg"), "image/jpeg");
    assert_eq!(resolve_artwork_mime("/path/to/poster.JPG"), "image/jpeg");
    assert_eq!(resolve_artwork_mime("image.jpeg"), "image/jpeg");
    assert_eq!(resolve_artwork_mime("image.png"), "image/png");
    assert_eq!(resolve_artwork_mime("image.PNG"), "image/png");
    assert_eq!(resolve_artwork_mime("image.webp"), "image/webp");
    assert_eq!(resolve_artwork_mime("image.avif"), "image/avif");
    assert_eq!(resolve_artwork_mime("image.unknown"), "image/jpeg");
    assert_eq!(resolve_artwork_mime("image_no_ext"), "image/jpeg");
}
