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
use serde_json::Value;
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
async fn test_browse_filesystem_requires_admin() {
    let (app, _admin_token, standard_token) = setup_test_app().await;

    // Unauthenticated request returns 401
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/fs")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Standard user request returns 403
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/fs")
        .header(header::AUTHORIZATION, format!("Bearer {standard_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_browse_filesystem_returns_directories_and_shortcuts() {
    let (app, admin_token, _) = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/fs")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["current_path"], "/");
    assert!(json["parent_path"].is_null());

    let shortcuts = json["shortcuts"].as_array().expect("shortcuts array");
    assert!(shortcuts
        .iter()
        .any(|s| s["name"] == "Root (/)" && s["path"] == "/"));

    let directories = json["directories"].as_array().expect("directories array");
    for dir in directories {
        let name = dir["name"].as_str().unwrap();
        assert!(
            !name.starts_with('.'),
            "hidden directory should not be listed"
        );
    }
}

#[tokio::test]
async fn test_browse_filesystem_specific_dir_and_filters() {
    let (app, admin_token, _) = setup_test_app().await;

    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path().canonicalize().unwrap();

    std::fs::create_dir(tmp_path.join("dir_b")).unwrap();
    std::fs::create_dir(tmp_path.join("dir_a")).unwrap();
    std::fs::create_dir(tmp_path.join(".hidden_dir")).unwrap();
    std::fs::write(tmp_path.join("regular_file.txt"), b"hello").unwrap();

    let uri = format!("/api/v1/system/fs?path={}", tmp_path.display());
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(
        json["current_path"].as_str().unwrap(),
        tmp_path.to_str().unwrap()
    );
    assert!(json["parent_path"].is_string());

    let directories = json["directories"].as_array().expect("directories array");
    assert_eq!(directories.len(), 2);
    assert_eq!(directories[0]["name"], "dir_a");
    assert_eq!(directories[1]["name"], "dir_b");
}

#[tokio::test]
async fn test_browse_filesystem_invalid_path() {
    let (app, admin_token, _) = setup_test_app().await;

    // Non-existent directory returns 400
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/fs?path=/nonexistent_directory_99999999")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // File path returns 400
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("file.txt");
    std::fs::write(&file_path, b"test").unwrap();

    let uri = format!("/api/v1/system/fs?path={}", file_path.display());
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
