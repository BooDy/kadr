use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use futures::StreamExt;
use kadr_core::events::SystemEvent;
use kadr_core::models::{User, UserRole};
use kadr_server::api::{create_full_router, mount_web_serving};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use tempfile::tempdir;
use tower::ServiceExt;

#[tokio::test]
async fn test_milestone_5_web_serving_and_api_integration_e2e() {
    // -------------------------------------------------------------------------
    // Setup temporary directory with test SQLite DB and test web SPA assets
    // -------------------------------------------------------------------------
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let db_path = temp_dir.path().join("kadr_test.db");
    let pool = create_pool(&db_path, 2).expect("Failed to create SQLite connection pool");
    initialize_database(&pool)
        .await
        .expect("Failed to initialize database");

    let test_web_dir = temp_dir.path().join("web_dist");
    let assets_dir = test_web_dir.join("assets");
    std::fs::create_dir_all(&assets_dir).expect("Failed to create assets directory");

    let index_html = "<!doctype html><html><head><title>Kadr Web</title></head><body><div id=\"root\"></div></body></html>";
    let app_js = "console.log('kadr client initialized');";
    let app_css = "body { margin: 0; background: #000; }";

    std::fs::write(test_web_dir.join("index.html"), index_html)
        .expect("Failed to write index.html");
    std::fs::write(assets_dir.join("app.js"), app_js).expect("Failed to write app.js");
    std::fs::write(assets_dir.join("style.css"), app_css).expect("Failed to write style.css");

    // Initialize repositories and seed admin user
    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let admin_id = "admin-uid-milestone5";
    let admin_pin = "1234";
    let admin_user = User {
        id: admin_id.to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin(admin_pin).expect("Failed to hash admin PIN"),
        role: UserRole::Admin,
        created_at: 1_700_000_000,
    };
    user_repo
        .create(&admin_user)
        .await
        .expect("Failed to create admin user");

    let jwt_svc = JwtService::new("milestone-5-e2e-secret-key-at-least-32-bytes", 3600);
    let rate_limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    let subtitle_cache_dir = temp_dir.path().join("subtitles_cache");
    std::fs::create_dir_all(&subtitle_cache_dir)
        .expect("Failed to create subtitle cache directory");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache_dir,
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

    // Build router and mount static web serving pointing to test web assets
    let app = create_full_router(
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
        event_bus.clone(),
        telemetry_collector,
    );
    let app = mount_web_serving(app, &test_web_dir);

    // -------------------------------------------------------------------------
    // Step 1: GET / -> returns 200 OK, Content-Type: text/html, containing SPA HTML
    // -------------------------------------------------------------------------
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .expect("Failed to build GET / request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute GET /");
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("Missing Content-Type header on GET /")
        .to_str()
        .expect("Invalid Content-Type header");
    assert!(
        content_type.starts_with("text/html"),
        "Expected text/html, got {content_type}"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("Failed to read body bytes");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("Invalid UTF-8 body");
    assert!(
        body_str.contains("<div id=\"root\"></div>"),
        "SPA HTML does not contain root div: {body_str}"
    );

    // -------------------------------------------------------------------------
    // Step 2: GET /assets/app.js -> returns 200 OK, Content-Type: javascript
    // -------------------------------------------------------------------------
    let req = Request::builder()
        .method("GET")
        .uri("/assets/app.js")
        .body(Body::empty())
        .expect("Failed to build GET /assets/app.js request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute GET /assets/app.js");
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("Missing Content-Type header on GET /assets/app.js")
        .to_str()
        .expect("Invalid Content-Type header");
    assert!(
        content_type.contains("javascript"),
        "Expected javascript Content-Type, got {content_type}"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("Failed to read body bytes");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("Invalid UTF-8 body");
    assert!(
        body_str.contains("console.log('kadr client initialized');"),
        "Static JS file content mismatch: {body_str}"
    );

    // Also check GET /assets/style.css
    let req = Request::builder()
        .method("GET")
        .uri("/assets/style.css")
        .body(Body::empty())
        .expect("Failed to build GET /assets/style.css request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute GET /assets/style.css");
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("Missing Content-Type header on GET /assets/style.css")
        .to_str()
        .expect("Invalid Content-Type header");
    assert!(
        content_type.starts_with("text/css"),
        "Expected text/css Content-Type, got {content_type}"
    );

    // -------------------------------------------------------------------------
    // Step 3: GET /browse (unknown client route) -> falls back to 200 OK with index.html
    // -------------------------------------------------------------------------
    let req = Request::builder()
        .method("GET")
        .uri("/browse")
        .body(Body::empty())
        .expect("Failed to build GET /browse request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute GET /browse");
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("Missing Content-Type header on GET /browse")
        .to_str()
        .expect("Invalid Content-Type header");
    assert!(
        content_type.starts_with("text/html"),
        "Expected text/html, got {content_type}"
    );
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("Failed to read body bytes");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("Invalid UTF-8 body");
    assert!(
        body_str.contains("<div id=\"root\"></div>"),
        "SPA fallback does not contain root div: {body_str}"
    );

    // -------------------------------------------------------------------------
    // Step 4: GET /api/v1/screens -> returns 401 Unauthorized (proves /api/v1/* not swallowed)
    // -------------------------------------------------------------------------
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .body(Body::empty())
        .expect("Failed to build unauthenticated GET /api/v1/screens request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute GET /api/v1/screens");
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // -------------------------------------------------------------------------
    // Step 5: Authenticates admin user with PIN 1234, calls GET /api/v1/screens, verifies 200 OK JSON array
    // -------------------------------------------------------------------------
    let auth_payload = serde_json::json!({
        "user_id": admin_id,
        "pin": admin_pin,
    });
    let auth_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&auth_payload).unwrap()))
        .expect("Failed to build auth request");
    let auth_res = app
        .clone()
        .oneshot(auth_req)
        .await
        .expect("Failed to execute auth request");
    assert_eq!(auth_res.status(), StatusCode::OK);
    let auth_bytes = axum::body::to_bytes(auth_res.into_body(), usize::MAX)
        .await
        .expect("Failed to read auth response body");
    let auth_json: serde_json::Value =
        serde_json::from_slice(&auth_bytes).expect("Invalid JSON in auth response");
    let token = auth_json["token"]
        .as_str()
        .expect("Missing token string in auth response")
        .to_string();

    let screens_req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("Failed to build authenticated GET /api/v1/screens request");
    let screens_res = app
        .clone()
        .oneshot(screens_req)
        .await
        .expect("Failed to execute screens request");
    assert_eq!(screens_res.status(), StatusCode::OK);
    let screens_bytes = axum::body::to_bytes(screens_res.into_body(), usize::MAX)
        .await
        .expect("Failed to read screens response body");
    let screens_json: serde_json::Value =
        serde_json::from_slice(&screens_bytes).expect("Invalid JSON in screens response");
    assert!(
        screens_json.is_array(),
        "Expected JSON array for screens, got: {screens_json}"
    );

    // -------------------------------------------------------------------------
    // Step 6: Connects to GET /api/v1/events -> verifies SSE streaming response
    // -------------------------------------------------------------------------
    let sse_req = Request::builder()
        .method("GET")
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("Failed to build GET /api/v1/events request");
    let sse_res = app
        .clone()
        .oneshot(sse_req)
        .await
        .expect("Failed to connect to /api/v1/events");
    assert_eq!(sse_res.status(), StatusCode::OK);
    let sse_ct = sse_res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("Missing Content-Type header on SSE response")
        .to_str()
        .expect("Invalid SSE Content-Type header");
    assert!(
        sse_ct.contains("text/event-stream"),
        "Expected text/event-stream, got {sse_ct}"
    );

    let mut sse_stream = sse_res.into_body().into_data_stream();

    // Publish an event on EventBus and verify receiving it on the SSE data stream
    let test_event = SystemEvent::LibraryUpdated {
        library_id: "test-lib-milestone5".to_string(),
        item_count: 99,
        timestamp: 1_700_000_000,
    };
    event_bus.publish(test_event);

    let chunk = tokio::time::timeout(Duration::from_secs(5), sse_stream.next())
        .await
        .expect("Timed out waiting for SSE stream chunk")
        .expect("SSE stream ended prematurely")
        .expect("Error reading SSE chunk");
    let chunk_str = String::from_utf8_lossy(&chunk);
    assert!(
        chunk_str.contains("event: library:updated") || chunk_str.contains("library:updated"),
        "SSE chunk did not contain event name: {chunk_str}"
    );
    assert!(
        chunk_str.contains("test-lib-milestone5"),
        "SSE chunk did not contain library ID: {chunk_str}"
    );
}

#[tokio::test]
async fn test_web_serving_graceful_when_dir_does_not_exist() {
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let db_path = temp_dir.path().join("kadr_test_graceful.db");
    let pool = create_pool(&db_path, 2).expect("Failed to create SQLite connection pool");
    initialize_database(&pool)
        .await
        .expect("Failed to initialize database");

    let nonexistent_dir = temp_dir.path().join("does_not_exist");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let jwt_svc = JwtService::new("test-secret-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    let subtitle_cache_dir = temp_dir.path().join("subtitles_cache");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache_dir,
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

    let app = create_full_router(
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

    let app = mount_web_serving(app, &nonexistent_dir);

    // Unmatched route without static serving mounted falls back to standard 404
    let req = Request::builder()
        .method("GET")
        .uri("/unknown-route")
        .body(Body::empty())
        .expect("Failed to build request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // API route still works
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .body(Body::empty())
        .expect("Failed to build request");
    let res = app
        .clone()
        .oneshot(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
