use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::models::{User, UserRole};
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
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, String, String) {
    let pool = create_in_memory_pool().expect("failed to create pool");
    initialize_database(&pool)
        .await
        .expect("failed to initialize db");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let admin_user = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").expect("hash pin"),
        role: UserRole::Admin,
        created_at: 1_700_000_000,
    };
    user_repo.create(&admin_user).await.expect("create admin");

    let standard_user = User {
        id: "user-1".to_string(),
        username: "user".to_string(),
        pin_hash: hash_pin("5678").expect("hash pin"),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo
        .create(&standard_user)
        .await
        .expect("create standard");

    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let admin_token = jwt_svc
        .generate_token(&admin_user)
        .expect("generate admin token");
    let standard_token = jwt_svc
        .generate_token(&standard_user)
        .expect("generate standard token");

    let rate_limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    let temp_cache = std::env::temp_dir().join(format!("kadr-subtitles-{}", uuid::Uuid::new_v4()));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_cache,
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        std::path::PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));

    let app = create_router_with_events(
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
        event_bus,
        telemetry_collector,
    );

    (app, admin_token, standard_token)
}

#[tokio::test]
async fn test_create_library_with_paths_array() {
    let (app, admin_token, _) = setup_test_app().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir1 = tmp.path().join("dir1");
    let dir2 = tmp.path().join("dir2");
    std::fs::create_dir(&dir1).unwrap();
    std::fs::create_dir(&dir2).unwrap();

    let payload = json!({
        "name": "Multi Path Library",
        "paths": [dir1, dir2],
        "media_type": "Movie"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: Value = serde_json::from_slice(&body_bytes).unwrap();
    let paths = created["paths"].as_array().expect("paths array");
    assert_eq!(paths.len(), 2);

    // Verify GET /api/v1/libraries returns both paths
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/libraries")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let libs: Vec<Value> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0]["paths"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_create_library_fallback_to_path() {
    let (app, admin_token, _) = setup_test_app().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir1 = tmp.path().join("single_dir");
    std::fs::create_dir(&dir1).unwrap();

    let payload = json!({
        "name": "Single Path Library",
        "path": dir1,
        "media_type": "Movie"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: Value = serde_json::from_slice(&body_bytes).unwrap();
    let paths = created["paths"].as_array().expect("paths array");
    assert_eq!(paths.len(), 1);
}

#[tokio::test]
async fn test_create_library_validation_errors() {
    let (app, admin_token, _) = setup_test_app().await;

    // Non-existent path returns 400
    let payload = json!({
        "name": "Bad Library",
        "paths": ["/nonexistent/path/12345"],
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Empty paths and missing path returns 400
    let payload = json!({
        "name": "Empty Paths Library",
        "paths": [],
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Relative path returns 400
    let payload = json!({
        "name": "Relative Path Library",
        "path": "relative/path/to/media",
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_and_remove_library_paths() {
    let (app, admin_token, standard_token) = setup_test_app().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir1 = tmp.path().join("dir1");
    let dir2 = tmp.path().join("dir2");
    std::fs::create_dir(&dir1).unwrap();
    std::fs::create_dir(&dir2).unwrap();

    // 1. Create library with dir1
    let payload = json!({
        "name": "Growing Library",
        "path": dir1,
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let lib_id = created["id"].as_str().unwrap();

    // 2. Add dir2 without admin token returns 401/403
    let add_payload = json!({ "path": dir2 });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/paths"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {standard_token}"))
        .body(Body::from(add_payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 3. Add non-existent directory returns 400
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/paths"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(
            json!({ "path": "/nonexistent/xyz" }).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 3b. Add relative path returns 400
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/paths"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(
            json!({ "path": "relative/sub/dir" }).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 4. Add dir2 with admin token returns 200 (or 201)
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/paths"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(add_payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert!(res.status() == StatusCode::OK || res.status() == StatusCode::CREATED);

    // 5. Verify library now has 2 paths
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/libraries")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let libs: Vec<Value> =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(libs[0]["paths"].as_array().unwrap().len(), 2);

    // 6. Remove dir1 via query parameter DELETE /api/v1/libraries/{id}/paths?path=...
    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/v1/libraries/{lib_id}/paths?path={}",
            dir1.display()
        ))
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify only dir2 remains
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/libraries")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let libs: Vec<Value> =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(libs[0]["paths"].as_array().unwrap().len(), 1);

    // 7. Removing the last remaining path (dir2) returns 400 Bad Request
    let req = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/api/v1/libraries/{lib_id}/paths?path={}",
            dir2.display()
        ))
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_scan_library_multi_path() {
    let (app, admin_token, _) = setup_test_app().await;

    let tmp = tempfile::tempdir().unwrap();
    let dir1 = tmp.path().join("dir1");
    let dir2 = tmp.path().join("dir2");
    std::fs::create_dir(&dir1).unwrap();
    std::fs::create_dir(&dir2).unwrap();

    std::fs::write(dir1.join("movie1.mp4"), b"test").unwrap();
    std::fs::write(dir2.join("movie2.mkv"), b"test").unwrap();

    let payload = json!({
        "name": "Scan Library",
        "paths": [dir1, dir2],
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let created: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let lib_id = created["id"].as_str().unwrap();

    // Trigger scan
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/scan"))
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let scan_resp: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(scan_resp["files_scanned"], 2);
}
