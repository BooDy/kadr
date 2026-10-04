use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::ast::{ScreenLayout, WidgetNode};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::unlock_token::UnlockTokenService;
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
use tempfile::tempdir;
use tower::ServiceExt;

async fn setup_app_with_private_item() -> (axum::Router, String, i64, String) {
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

    let jwt_secret = "test-secret-with-sufficient-entropy-for-hmac-sha256";
    let jwt_svc = JwtService::new(jwt_secret, 3600);
    let admin_token = jwt_svc
        .generate_token(&admin_user)
        .expect("generate admin token");

    // Create private library
    let private_lib_id = "private-lib-1".to_string();
    let private_lib = Library {
        id: private_lib_id.clone(),
        name: "Private Library".to_string(),
        path: PathBuf::from("/media/private"),
        media_type: MediaType::Movie,
        is_private: true,
        pin_hash: Some(hash_pin("4321").expect("pin hash")),
        created_at: 1_700_000_000,
    };
    lib_repo.insert(&private_lib).await.expect("insert private library");

    // Create dummy media file on disk
    let dir = tempdir().expect("tempdir");
    let dir_path = dir.keep();
    let video_path = dir_path.join("private_movie.mp4");
    let mut file = File::create(&video_path).expect("create dummy file");
    let dummy_data = vec![0xABu8; 1024];
    file.write_all(&dummy_data).expect("write dummy file");

    let private_item = MediaItem {
        id: None,
        library_id: private_lib_id.clone(),
        item_type: MediaType::Movie,
        title: "Confidential Video".to_string(),
        original_title: None,
        release_year: Some(2024),
        added_at: 1_700_000_100,
        file_path: video_path,
        file_name: "private_movie.mp4".to_string(),
        file_size: 1024,
        technical: TechnicalInfo {
            duration_seconds: 120,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mp4".to_string()),
        },
        metadata: MediaMetadata {
            overview: Some("Top secret film".to_string()),
            genres: vec!["Documentary".to_string()],
            ..Default::default()
        },
    };
    media_repo
        .upsert_batch(&[private_item])
        .await
        .expect("upsert private item");

    let items = media_repo
        .list_by_library(&private_lib_id, 1, 0)
        .await
        .expect("list private items");
    let private_item_id = items[0].id.expect("item id");

    // Generate unlock token for private library
    let unlock_service = UnlockTokenService::new(jwt_secret, 3600);
    let (unlock_token, _) = unlock_service
        .generate_token(&private_lib_id)
        .expect("generate unlock token");

    let rate_limiter = RateLimiter::new(5, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));

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

    (app, admin_token, private_item_id, unlock_token)
}

#[tokio::test]
async fn test_streaming_private_item_requires_unlock_token() {
    let (app, _admin_token, private_item_id, unlock_token) = setup_app_with_private_item().await;

    // 1. Stream without unlock token -> 403 Forbidden
    let req = Request::get(format!("/api/v1/stream/{}", private_item_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 2. Stream with unlock token -> 206 Partial Content (or 200 OK)
    let req_with_token = Request::get(format!("/api/v1/stream/{}", private_item_id))
        .header("X-Kadr-Unlocked", &unlock_token)
        .header("Range", "bytes=0-100")
        .body(Body::empty())
        .unwrap();
    let resp_ok = app.clone().oneshot(req_with_token).await.unwrap();
    assert_eq!(resp_ok.status(), StatusCode::PARTIAL_CONTENT);
}

#[tokio::test]
async fn test_streaming_private_item_forbidden_payload_and_query_param() {
    let (app, admin_token, private_item_id, unlock_token) = setup_app_with_private_item().await;

    // 1. Verify 403 body contains {"error": "LIBRARY_LOCKED"} even with Authorization header
    let req = Request::get(format!("/api/v1/stream/{}", private_item_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"], "LIBRARY_LOCKED");

    // 2. Stream with query param unlocked token
    let req_with_query = Request::get(format!(
        "/api/v1/stream/{}?unlocked={}",
        private_item_id, unlock_token
    ))
    .header("Range", "bytes=0-50")
    .body(Body::empty())
    .unwrap();
    let resp_query = app.clone().oneshot(req_with_query).await.unwrap();
    assert_eq!(resp_query.status(), StatusCode::PARTIAL_CONTENT);
}

#[tokio::test]
async fn test_item_details_and_direct_routes_authorization() {
    let (app, admin_token, private_item_id, unlock_token) = setup_app_with_private_item().await;

    // 1. GET /api/v1/items/{id}/details without unlock token -> 403 Forbidden with {"error": "LIBRARY_LOCKED"}
    let req = Request::get(format!("/api/v1/items/{}/details", private_item_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"], "LIBRARY_LOCKED");

    // 2. GET /api/v1/items/{id} without unlock token -> 403 Forbidden
    let req_direct = Request::get(format!("/api/v1/items/{}", private_item_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let resp_direct = app.clone().oneshot(req_direct).await.unwrap();
    assert_eq!(resp_direct.status(), StatusCode::FORBIDDEN);

    // 3. GET /api/v1/items/{id}/details with unlock token -> 200 OK
    let req_unlocked = Request::get(format!("/api/v1/items/{}/details", private_item_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("X-Kadr-Unlocked", &unlock_token)
        .body(Body::empty())
        .unwrap();
    let resp_unlocked = app.clone().oneshot(req_unlocked).await.unwrap();
    assert_eq!(resp_unlocked.status(), StatusCode::OK);
    let bytes = resp_unlocked.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["card"]["title"], "Confidential Video");

    // 4. GET /api/v1/items/{id} with unlock token -> 200 OK
    let req_direct_unlocked = Request::get(format!("/api/v1/items/{}", private_item_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("X-Kadr-Unlocked", &unlock_token)
        .body(Body::empty())
        .unwrap();
    let resp_direct_unlocked = app.clone().oneshot(req_direct_unlocked).await.unwrap();
    assert_eq!(resp_direct_unlocked.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_screen_and_widget_routes_isolate_private_items_unless_unlocked() {
    let (app, admin_token, private_item_id, unlock_token) = setup_app_with_private_item().await;

    // 1. GET /api/v1/screens/home without unlock token
    let req_locked = Request::get("/api/v1/screens/home")
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let resp_locked = app.clone().oneshot(req_locked).await.unwrap();
    assert_eq!(resp_locked.status(), StatusCode::OK);
    let bytes = resp_locked.into_body().collect().await.unwrap().to_bytes();
    let screen_locked: ScreenLayout = serde_json::from_slice(&bytes).unwrap();

    // Verify private item does not appear in any widget
    for widget in &screen_locked.widgets {
        match widget {
            WidgetNode::HeroBanner { data: Some(card), .. } => {
                assert_ne!(card.id, private_item_id);
            }
            WidgetNode::Carousel { items: Some(items), .. } => {
                for card in items {
                    assert_ne!(card.id, private_item_id);
                }
            }
            WidgetNode::Grid { items: Some(items), .. } => {
                for card in items {
                    assert_ne!(card.id, private_item_id);
                }
            }
            _ => {}
        }
    }

    // 2. GET /api/v1/screens/home with X-Kadr-Unlocked token
    let req_unlocked = Request::get("/api/v1/screens/home")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("X-Kadr-Unlocked", &unlock_token)
        .body(Body::empty())
        .unwrap();
    let resp_unlocked = app.clone().oneshot(req_unlocked).await.unwrap();
    assert_eq!(resp_unlocked.status(), StatusCode::OK);
    let bytes = resp_unlocked.into_body().collect().await.unwrap().to_bytes();
    let screen_unlocked: ScreenLayout = serde_json::from_slice(&bytes).unwrap();

    // Verify private item IS present in the carousel/spotlight widgets
    let mut found_in_unlocked = false;
    for widget in &screen_unlocked.widgets {
        match widget {
            WidgetNode::HeroBanner {
                data: Some(card), ..
            } if card.id == private_item_id => {
                found_in_unlocked = true;
            }
            WidgetNode::Carousel {
                items: Some(items), ..
            } if items.iter().any(|c| c.id == private_item_id) => {
                found_in_unlocked = true;
            }
            WidgetNode::Grid {
                items: Some(items), ..
            } if items.iter().any(|c| c.id == private_item_id) => {
                found_in_unlocked = true;
            }
            _ => {}
        }
    }
    assert!(
        found_in_unlocked,
        "Private item should appear in screen layout when unlocked token is provided"
    );
}
