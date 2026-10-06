use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use kadr_core::models::{User, UserRole};
use kadr_server::api::discovery_routes::DiscoveryResponse;
use kadr_server::api::{create_router, create_router_with_ingest};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::identity::ServerIdentity;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

#[tokio::test]
async fn test_discovery_endpoint_unauthenticated() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let jwt_svc = JwtService::new("test-secret-key-that-is-at-least-32-chars-long", 3600);
    let limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
    );

    // Unauthenticated GET /api/v1/discovery
    let req = Request::builder()
        .uri("/api/v1/discovery")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.contains("application/json"),
        "expected application/json Content-Type, got {}",
        content_type
    );

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Verify sensitive data is not leaked
    assert!(!body_str.contains(".db"));
    assert!(!body_str.contains("secret"));

    let discovery: DiscoveryResponse =
        serde_json::from_slice(&body_bytes).expect("Failed to deserialize DiscoveryResponse");

    assert_eq!(discovery.app, "kadr");
    assert_eq!(discovery.server_id, "00000000-0000-0000-0000-000000000000");
    assert_eq!(discovery.name, "Kadr Media Server");
    assert_eq!(discovery.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(discovery.protocol_version, 1);
    assert_eq!(discovery.port, 8492);
    assert!(!discovery.setup_completed);
    assert_eq!(discovery.status, "online");
}

#[tokio::test]
async fn test_discovery_setup_completed_when_user_exists() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    // Insert an initial user
    let user = User {
        id: "admin-user-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };
    user_repo.create(&user).await.unwrap();

    let jwt_svc = JwtService::new("test-secret-key-that-is-at-least-32-chars-long", 3600);
    let limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
    );

    let req = Request::builder()
        .uri("/api/v1/discovery")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let discovery: DiscoveryResponse =
        serde_json::from_slice(&body_bytes).expect("Failed to deserialize DiscoveryResponse");

    assert!(discovery.setup_completed);
    assert_eq!(discovery.status, "online");
}

#[tokio::test]
async fn test_discovery_reflects_updated_server_name() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };
    user_repo.create(&admin).await.unwrap();

    let jwt_svc = JwtService::new("test-secret-key-that-is-at-least-32-chars-long", 3600);
    let admin_token = jwt_svc.generate_token(&admin).unwrap();
    let limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    let temp_cache = std::env::temp_dir().join(format!("kadr-sub-disc-{}", uuid::Uuid::new_v4()));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_cache,
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(kadr_server::events::EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        std::path::PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let (dummy_tx, _) = tokio::sync::mpsc::channel(1);
    let dummy_pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));

    let config = Arc::new(tokio::sync::RwLock::new(
        kadr_server::config::AppConfig::default(),
    ));
    let identity = Arc::new(ServerIdentity {
        id: "custom-test-server-id-uuid".to_string(),
    });

    let app = create_router_with_ingest(
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
        event_bus,
        telemetry_collector,
        dummy_tx,
        dummy_pipeline,
        config,
        identity,
    );

    // Initial check: default name and custom server_id
    let req = Request::builder()
        .uri("/api/v1/discovery")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let discovery: DiscoveryResponse = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(discovery.name, "Kadr Media Server");
    assert_eq!(discovery.server_id, "custom-test-server-id-uuid");

    // Update server name via PUT /api/v1/system/config
    let update_payload = serde_json::json!({
        "name": "Living Room Kadr"
    });
    let put_req = Request::builder()
        .uri("/api/v1/system/config")
        .method("PUT")
        .header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&update_payload).unwrap()))
        .unwrap();

    let put_res = app.clone().oneshot(put_req).await.unwrap();
    assert_eq!(put_res.status(), StatusCode::OK);

    // Discovery should reflect updated name
    let req2 = Request::builder()
        .uri("/api/v1/discovery")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let body_bytes2 = axum::body::to_bytes(res2.into_body(), 1024 * 16)
        .await
        .unwrap();
    let discovery2: DiscoveryResponse = serde_json::from_slice(&body_bytes2).unwrap();
    assert_eq!(discovery2.name, "Living Room Kadr");
    assert_eq!(discovery2.server_id, "custom-test-server-id-uuid");
}
