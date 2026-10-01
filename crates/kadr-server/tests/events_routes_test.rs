use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures::StreamExt;
use kadr_core::events::{SystemEvent, TelemetrySnapshot};
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
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use tempfile::tempdir;
use tower::ServiceExt;

struct TestContext {
    app: axum::Router,
    event_bus: Arc<EventBus>,
    _telemetry_collector: Arc<TelemetryCollector>,
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
        username: "user".to_string(),
        pin_hash: hash_pin("5678").expect("hash pin"),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo
        .create(&standard_user)
        .await
        .expect("create standard user");

    let jwt_svc = JwtService::new("super-secret-jwt-key-for-testing-purposes-123456", 3600);
    let admin_token = jwt_svc.generate_token(&admin_user).expect("admin token");
    let standard_token = jwt_svc
        .generate_token(&standard_user)
        .expect("standard token");

    let limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    let subtitle_cache = dir.path().join("subtitles");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache,
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

    let app = create_router_with_events(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry_collector.clone(),
    );

    TestContext {
        app,
        event_bus,
        _telemetry_collector: telemetry_collector,
        admin_token,
        standard_token,
        _temp_dir: dir,
    }
}

#[tokio::test]
async fn test_public_sse_events_endpoint_headers() {
    let ctx = setup_test_context().await;

    // Public unauthenticated request
    let req = Request::builder()
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("content-type header")
        .to_str()
        .expect("content-type to_str");
    assert!(
        content_type.contains("text/event-stream"),
        "expected text/event-stream, got: {content_type}"
    );

    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .expect("cache-control header")
        .to_str()
        .expect("cache-control to_str");
    assert!(
        cache_control.contains("no-cache"),
        "expected no-cache, got: {cache_control}"
    );
}

#[tokio::test]
async fn test_sse_events_publishes_frames() {
    let ctx = setup_test_context().await;

    let req = Request::builder()
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::OK);

    let mut stream = response.into_body().into_data_stream();

    // Publish event on EventBus
    let event = SystemEvent::LibraryUpdated {
        library_id: "lib-movies-1".to_string(),
        item_count: 42,
        timestamp: 1_700_000_123,
    };
    ctx.event_bus.publish(event);

    // Read first chunk from the SSE stream
    let chunk = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting for SSE chunk")
        .expect("stream ended prematurely")
        .expect("error reading chunk");

    let chunk_str = String::from_utf8_lossy(&chunk);
    assert!(
        chunk_str.contains("event: library:updated"),
        "SSE chunk missing event line: {chunk_str}"
    );
    assert!(
        chunk_str.contains("data: "),
        "SSE chunk missing data line: {chunk_str}"
    );
    assert!(
        chunk_str.contains("\"library_id\":\"lib-movies-1\""),
        "SSE data missing library_id: {chunk_str}"
    );
    assert!(
        chunk_str.contains("\"item_count\":42"),
        "SSE data missing item_count: {chunk_str}"
    );
}

#[tokio::test]
async fn test_admin_telemetry_unauthenticated_returns_401() {
    let ctx = setup_test_context().await;

    let req = Request::builder()
        .uri("/api/v1/system/telemetry")
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_admin_telemetry_standard_user_returns_403() {
    let ctx = setup_test_context().await;

    let req = Request::builder()
        .uri("/api/v1/system/telemetry")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", ctx.standard_token),
        )
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_telemetry_admin_user_returns_200_snapshot() {
    let ctx = setup_test_context().await;

    let req = Request::builder()
        .uri("/api/v1/system/telemetry")
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body bytes");

    let snapshot: TelemetrySnapshot =
        serde_json::from_slice(&bytes).expect("deserialize TelemetrySnapshot");

    assert_eq!(snapshot.active_sessions_count, 0);
    assert!(snapshot.timestamp > 0);
}

#[tokio::test]
async fn test_admin_telemetry_token_in_query_param() {
    let ctx = setup_test_context().await;

    let req = Request::builder()
        .uri(format!(
            "/api/v1/system/telemetry?token={}",
            ctx.admin_token
        ))
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_sse_events_multiple_consecutive_events() {
    let ctx = setup_test_context().await;

    let req = Request::builder()
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("build request");

    let response = ctx.app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::OK);

    let mut stream = response.into_body().into_data_stream();

    // Publish event 1: LayoutChanged
    ctx.event_bus.publish(SystemEvent::LayoutChanged {
        screen_id: "home-screen".to_string(),
        timestamp: 1_700_000_100,
    });

    let chunk1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting for chunk 1")
        .expect("stream ended")
        .expect("error chunk 1");
    let text1 = String::from_utf8_lossy(&chunk1);
    assert!(text1.contains("event: layout:changed"));
    assert!(text1.contains("\"screen_id\":\"home-screen\""));

    // Publish event 2: SubtitleDownloaded
    ctx.event_bus.publish(SystemEvent::SubtitleDownloaded {
        item_id: 10,
        subtitle_id: 20,
        language: "spa".to_string(),
        timestamp: 1_700_000_200,
    });

    let chunk2 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting for chunk 2")
        .expect("stream ended")
        .expect("error chunk 2");
    let text2 = String::from_utf8_lossy(&chunk2);
    assert!(text2.contains("event: subtitle:downloaded"));
    assert!(text2.contains("\"language\":\"spa\""));

    // Publish event 3: SessionSynced
    ctx.event_bus.publish(SystemEvent::SessionSynced {
        session_id: "sess-abc".to_string(),
        item_id: 10,
        user_id: "user-123".to_string(),
        position_seconds: 450,
        timestamp: 1_700_000_300,
    });

    let chunk3 = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting for chunk 3")
        .expect("stream ended")
        .expect("error chunk 3");
    let text3 = String::from_utf8_lossy(&chunk3);
    assert!(text3.contains("event: session:synced"));
    assert!(text3.contains("\"position_seconds\":450"));
}

#[tokio::test]
async fn test_backward_compatible_create_router_with_subtitles() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr_compat.db");
    let pool = create_pool(&db_path, 2).expect("pool");
    initialize_database(&pool).await.expect("db init");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let jwt_svc = JwtService::new("super-secret-jwt-key-for-testing-purposes-123456", 3600);
    let limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    let subtitle_cache = dir.path().join("subtitles");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache,
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));

    // Calling backward-compatible constructor
    let app = kadr_server::api::create_router_with_subtitles(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
    );

    let req = Request::builder()
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_backward_compatible_create_router() {
    let pool = kadr_storage::pool::create_in_memory_pool().expect("in-memory pool");
    initialize_database(&pool).await.expect("db init");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let jwt_svc = JwtService::new("super-secret-jwt-key-for-testing-purposes-123456", 3600);
    let limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());

    // Calling legacy create_router
    let app = kadr_server::api::create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
    );

    let req = Request::builder()
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(req).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::OK);
}
