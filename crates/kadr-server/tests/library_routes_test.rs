use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use kadr_core::models::{User, UserRole};
use kadr_server::api::create_router_with_events;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::sync::RwLock;
use tower::ServiceExt;

struct TestContext {
    app: axum::Router,
    admin_token: String,
    standard_token: String,
    _temp_dir: tempfile::TempDir,
}

async fn setup_test_context() -> TestContext {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr.db");

    let pool = create_pool(&db_path, 2).expect("failed to create pool");
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
        username: "boody".to_string(),
        pin_hash: hash_pin("5678").expect("hash pin"),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo.create(&standard_user).await.expect("create user");

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
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));

    let temp_cache = dir.path().join("subtitles-cache");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_cache,
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        db_path,
        session_registry.clone(),
        event_bus.clone(),
    ));

    let config = Arc::new(RwLock::new(AppConfig::default()));

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
    )
    .layer(axum::extract::Extension(config));

    TestContext {
        app,
        admin_token,
        standard_token,
        _temp_dir: dir,
    }
}

#[tokio::test]
async fn test_libraries_crud_and_auth() {
    let ctx = setup_test_context().await;

    // 1. Initial GET /api/v1/libraries should be empty list
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/libraries")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let libs: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert!(libs.is_empty());

    // 2. POST /api/v1/libraries without token returns 401
    let create_payload = json!({
        "name": "Movies",
        "path": "/tmp/test-movies",
        "media_type": "Movie"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 3. POST /api/v1/libraries with Standard user returns 403 Forbidden
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.standard_token))
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 4. POST /api/v1/libraries with Admin token returns 201 Created
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let created_lib: Value = serde_json::from_slice(&body).unwrap();
    let lib_id = created_lib["id"].as_str().unwrap().to_string();
    assert_eq!(created_lib["name"], "Movies");

    // 5. GET /api/v1/libraries now contains the library
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/libraries")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let libs: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0]["id"], lib_id);

    // 6. POST /api/v1/libraries/{id}/scan returns 202 Accepted
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/scan"))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    // 7. DELETE /api/v1/libraries/{id} with Admin returns 204 No Content
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/libraries/{lib_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // 8. Subsequent DELETE returns 404
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/libraries/{lib_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_system_config_routes() {
    let ctx = setup_test_context().await;

    // 1. GET /api/v1/system/config without admin returns 401
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/config")
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. GET /api/v1/system/config with Admin returns 200 with port 8492
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/config")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let cfg: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(cfg["port"], 8492);
    assert_eq!(cfg["host"], "0.0.0.0");
    assert!(cfg["use_ffprobe"].as_bool().unwrap());

    // 3. PUT /api/v1/system/config updates settings
    let update_payload = json!({
        "port": 8492,
        "debounce_millis": 1000,
        "use_ffprobe": false
    });
    let req = Request::builder()
        .method("PUT")
        .uri("/api/v1/system/config")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::from(update_payload.to_string()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let updated: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["port"], 8492);
    assert_eq!(updated["debounce_millis"], 1000);
    assert!(!updated["use_ffprobe"].as_bool().unwrap());
}

#[tokio::test]
async fn test_scan_library_routes_to_ingest_worker() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = kadr_storage::pool::create_pool(&db_path, 2).unwrap();
    kadr_storage::pool::initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let admin_user = kadr_core::models::User {
        id: "admin-scan".to_string(),
        username: "admin".to_string(),
        pin_hash: kadr_server::auth::pin::hash_pin("1234").expect("hash pin"),
        role: kadr_core::models::UserRole::Admin,
        created_at: 1_700_000_000,
    };
    user_repo.create(&admin_user).await.expect("create admin");
    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let admin_token = jwt_svc.generate_token(&admin_user).expect("admin token");
    let rate_limiter = RateLimiter::new(10, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    let temp_cache = dir.path().join("subtitles-cache");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_cache,
        SubtitleRepository::new(pool.clone()),
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        db_path,
        session_registry.clone(),
        event_bus.clone(),
    ));

    // Create a real channel and pipeline
    let (ingest_tx, mut ingest_rx) = tokio::sync::mpsc::channel(10);
    let pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));
    let config = Arc::new(RwLock::new(AppConfig::default()));

    let app = kadr_server::api::create_router_with_ingest(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo.clone(),
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus,
        telemetry_collector,
        ingest_tx,
        pipeline,
        config,
    );

    // Create a media directory with a dummy movie file
    let media_dir = dir.path().join("movies");
    std::fs::create_dir_all(&media_dir).unwrap();
    let movie_file = media_dir.join("Inception (2010).mkv");
    std::fs::write(&movie_file, b"fake video content").unwrap();

    let lib = lib_repo
        .create_with_paths(
            "test-lib-scan",
            "Movies",
            &[media_dir],
            kadr_core::models::MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    // Trigger scan
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{}/scan", lib.id))
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    // Assert that the ingest worker channel receives the message
    let msg = tokio::time::timeout(Duration::from_secs(3), ingest_rx.recv())
        .await
        .expect("Timeout waiting for IngestMessage")
        .expect("Channel closed");

    match msg {
        kadr_ingest::watcher::IngestMessage::Upsert(item, _) => {
            assert_eq!(item.title, "Inception");
            assert_eq!(item.release_year, Some(2010));
        }
        _ => panic!("Expected Upsert message"),
    }
}
