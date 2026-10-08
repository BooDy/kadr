use std::sync::Arc;
use std::time::Duration;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use kadr_core::ast::{ScreenId, ScreenLayout, WidgetNode};
use kadr_core::models::{Library, MediaType, User, UserRole};
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

#[tokio::test]
async fn test_rename_library_with_named_or_disk_screen() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr.db");

    let pool = create_pool(&db_path, 2).expect("failed to create pool");
    initialize_database(&pool).await.expect("failed to initialize db");

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

    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let admin_token = jwt_svc.generate_token(&admin_user).expect("generate admin token");

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

    let mut config_val = AppConfig::default();
    config_val.server.data_dir = dir.path().to_path_buf();
    let config = Arc::new(RwLock::new(config_val));

    // Create a library named "Porn"
    let lib = Library {
        id: "lib-uuid-1234".to_string(),
        name: "Porn".to_string(),
        path: std::path::PathBuf::from("/media/porn"),
        media_type: MediaType::Movie,
        created_at: 1000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    // Register a screen layout named "Porn" on disk (exactly like what happened with Porn.json)
    let screens_dir = dir.path().join("screens");
    std::fs::create_dir_all(&screens_dir).unwrap();
    let porn_screen = ScreenLayout::new(
        ScreenId::Custom("Porn".to_string()),
        "Porn",
        vec![WidgetNode::Grid {
            id: "porn_grid".to_string(),
            title: "All Porn".to_string(),
            binding: kadr_core::ast::WidgetQueryBinding::new(kadr_core::ast::QueryMacro::RecentlyAdded),
            columns: 4,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    );
    layout_registry.save_screen(porn_screen, &screens_dir).unwrap();

    let (dummy_tx, _) = tokio::sync::mpsc::channel(1);
    let dummy_pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));
    let default_identity = Arc::new(kadr_server::identity::ServerIdentity {
        id: "00000000-0000-0000-0000-000000000000".to_string(),
    });

    let app = kadr_server::api::create_router_with_ingest(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo.clone(),
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry.clone(),
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus,
        telemetry_collector,
        dummy_tx,
        dummy_pipeline,
        config,
        default_identity,
    );

    // 1. Rename library from "Porn" to "Private"
    let patch_payload = json!({ "name": "Private" });
    let req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/libraries/lib-uuid-1234")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Fetch screens list
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let screens: Vec<Value> = serde_json::from_slice(&body_bytes).unwrap();
    let titles: Vec<&str> = screens.iter().map(|s| s["title"].as_str().unwrap()).collect();

    println!("Current screens titles: {:?}", titles);

    // Assert that "Private" exists
    assert!(titles.contains(&"Private"), "Screens must contain 'Private'");
    // Assert that old screen "Porn" DOES NOT exist
    assert!(!titles.contains(&"Porn"), "Old screen 'Porn' must NOT remain in screens: {:?}", titles);
    // Assert Porn.json file was deleted from disk
    assert!(!screens_dir.join("Porn.json").exists(), "Porn.json must not exist on disk");
}

#[tokio::test]
async fn test_rename_builtin_movies_library() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr.db");

    let pool = create_pool(&db_path, 2).expect("failed to create pool");
    initialize_database(&pool).await.expect("failed to initialize db");

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

    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let admin_token = jwt_svc.generate_token(&admin_user).expect("generate admin token");

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

    let mut config_val = AppConfig::default();
    config_val.server.data_dir = dir.path().to_path_buf();
    let config = Arc::new(RwLock::new(config_val));

    // Create a library named "Movies"
    let lib = Library {
        id: "lib-movie-123".to_string(),
        name: "Movies".to_string(),
        path: std::path::PathBuf::from("/media/movies"),
        media_type: MediaType::Movie,
        created_at: 1000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    let app = create_router_with_events(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo.clone(),
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry.clone(),
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus,
        telemetry_collector,
    )
    .layer(axum::extract::Extension(config));

    // 1. Rename library from "Movies" to "Cinema"
    let patch_payload = json!({ "name": "Cinema" });
    let req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/libraries/lib-movie-123")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&patch_payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Fetch screens list
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let screens: Vec<Value> = serde_json::from_slice(&body_bytes).unwrap();
    let titles: Vec<&str> = screens.iter().map(|s| s["title"].as_str().unwrap()).collect();

    println!("Current screens after renaming Movies: {:?}", titles);

    // Assert that "Cinema" exists
    assert!(titles.contains(&"Cinema"), "Screens must contain 'Cinema'");
    // Assert that old screen "Movies" DOES NOT exist
    assert!(!titles.contains(&"Movies"), "Old screen 'Movies' must NOT remain in screens: {:?}", titles);
}

#[tokio::test]
async fn test_delete_library_cleans_up_screen() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr.db");

    let pool = create_pool(&db_path, 2).expect("failed to create pool");
    initialize_database(&pool).await.expect("failed to initialize db");

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

    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let admin_token = jwt_svc.generate_token(&admin_user).expect("generate admin token");

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

    let mut config_val = AppConfig::default();
    config_val.server.data_dir = dir.path().to_path_buf();
    let config = Arc::new(RwLock::new(config_val));

    // Create a library named "Anime"
    let lib = Library {
        id: "lib-anime-999".to_string(),
        name: "Anime".to_string(),
        path: std::path::PathBuf::from("/media/anime"),
        media_type: MediaType::Show,
        created_at: 1000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    let screens_dir = dir.path().join("screens");
    std::fs::create_dir_all(&screens_dir).unwrap();
    let anime_screen = ScreenLayout::new(
        ScreenId::Custom("lib-anime-999".to_string()),
        "Anime",
        vec![],
    );
    layout_registry.save_screen(anime_screen, &screens_dir).unwrap();

    let (dummy_tx, _) = tokio::sync::mpsc::channel(1);
    let dummy_pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));
    let default_identity = Arc::new(kadr_server::identity::ServerIdentity {
        id: "00000000-0000-0000-0000-000000000000".to_string(),
    });

    let app = kadr_server::api::create_router_with_ingest(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo.clone(),
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry.clone(),
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus,
        telemetry_collector,
        dummy_tx,
        dummy_pipeline,
        config,
        default_identity,
    );

    // 1. Delete library
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/libraries/lib-anime-999")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // 2. Fetch screens list
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let screens: Vec<Value> = serde_json::from_slice(&body_bytes).unwrap();
    let titles: Vec<&str> = screens.iter().map(|s| s["title"].as_str().unwrap()).collect();

    // Assert Anime is deleted from screens and disk
    assert!(!titles.contains(&"Anime"), "Screens must NOT contain 'Anime': {:?}", titles);
    assert!(!screens_dir.join("lib-anime-999.json").exists(), "lib-anime-999.json must be deleted from disk");
}


