use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, String) {
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

    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let admin_token = jwt_svc
        .generate_token(&admin_user)
        .expect("generate admin token");

    let rate_limiter = RateLimiter::new(5, Duration::from_secs(60), Duration::from_secs(60));
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

    (app, admin_token)
}

#[tokio::test]
async fn test_create_and_unlock_private_library() {
    let (app, admin_token) = setup_test_app().await;

    // 1. Create private library with PIN 4321
    let create_payload = serde_json::json!({
        "name": "Personal Home Videos",
        "path": "/tmp/kadr_test_private_lib",
        "media_type": "Movie",
        "is_private": true,
        "pin": "4321"
    });
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let lib_id = created["id"].as_str().unwrap();
    assert_eq!(created["is_private"], true);

    // 2. Unlock with wrong PIN -> 401 Unauthorized
    let wrong_req = Request::post(format!("/api/v1/libraries/{}/unlock", lib_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "0000"}).to_string()))
        .unwrap();
    let wrong_resp = app.clone().oneshot(wrong_req).await.unwrap();
    assert_eq!(wrong_resp.status(), StatusCode::UNAUTHORIZED);

    // 3. Unlock with correct PIN -> 200 OK + token
    let right_req = Request::post(format!("/api/v1/libraries/{}/unlock", lib_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "4321"}).to_string()))
        .unwrap();
    let right_resp = app.clone().oneshot(right_req).await.unwrap();
    assert_eq!(right_resp.status(), StatusCode::OK);
    let unlock_body: serde_json::Value =
        serde_json::from_slice(&right_resp.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert!(unlock_body["token"].is_string());
}

#[tokio::test]
async fn test_create_private_library_validates_pin() {
    let (app, admin_token) = setup_test_app().await;

    // Missing PIN when is_private = true -> 400 Bad Request
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "Secret",
                "path": "/tmp/secret",
                "media_type": "Movie",
                "is_private": true
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Non-4-digit PIN (letters) -> 400 Bad Request
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "Secret",
                "path": "/tmp/secret",
                "media_type": "Movie",
                "is_private": true,
                "pin": "abcd"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Non-4-digit PIN (wrong length) -> 400 Bad Request
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "Secret",
                "path": "/tmp/secret",
                "media_type": "Movie",
                "is_private": true,
                "pin": "123"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_unlock_rate_limiting_lockout() {
    let (app, admin_token) = setup_test_app().await;

    // Create private library
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "Restricted",
                "path": "/tmp/restricted",
                "media_type": "Movie",
                "is_private": true,
                "pin": "9999"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let lib_id = created["id"].as_str().unwrap();

    // 5 consecutive wrong PIN attempts -> 401 Unauthorized
    for _ in 0..5 {
        let wrong_req = Request::post(format!("/api/v1/libraries/{}/unlock", lib_id))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::json!({"pin": "1111"}).to_string()))
            .unwrap();
        let wrong_resp = app.clone().oneshot(wrong_req).await.unwrap();
        assert_eq!(wrong_resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 6th attempt -> 429 Too Many Requests
    let locked_req = Request::post(format!("/api/v1/libraries/{}/unlock", lib_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "9999"}).to_string()))
        .unwrap();
    let locked_resp = app.clone().oneshot(locked_req).await.unwrap();
    assert_eq!(locked_resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_get_libraries_includes_privacy_and_hides_pin_hash() {
    let (app, admin_token) = setup_test_app().await;

    // Create private library
    let create_payload = serde_json::json!({
        "name": "Secret Archive",
        "path": "/tmp/secret_archive",
        "media_type": "Movie",
        "is_private": true,
        "pin": "1234"
    });
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // GET /api/v1/libraries
    let req = Request::get("/api/v1/libraries")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let libs: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0]["name"], "Secret Archive");
    assert_eq!(libs[0]["is_private"], true);
    assert!(libs[0].get("pin_hash").is_none());
}

#[tokio::test]
async fn test_unlock_non_existent_and_public_library() {
    let (app, admin_token) = setup_test_app().await;

    // 1. Non-existent library -> 404
    let req = Request::post("/api/v1/libraries/non-existent-id/unlock")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "1234"}).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // 2. Create public library
    let create_payload = serde_json::json!({
        "name": "Public Movies",
        "path": "/tmp/public_movies",
        "media_type": "Movie",
        "is_private": false
    });
    let req = Request::post("/api/v1/libraries")
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let pub_id = created["id"].as_str().unwrap();

    // 3. Unlock public library -> 400 Bad Request
    let req = Request::post(format!("/api/v1/libraries/{}/unlock", pub_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({"pin": "1234"}).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_unlocked_libraries_extractor_with_tokens() {
    use kadr_server::api::unlock_token::{UnlockTokenService, UnlockedLibraries};

    let token_svc =
        UnlockTokenService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let (tok1, _) = token_svc.generate_token("lib-1").unwrap();
    let (tok2, _) = token_svc.generate_token("lib-2").unwrap();

    let router = axum::Router::new()
        .route(
            "/test-extractor",
            axum::routing::get(|unlocked: UnlockedLibraries| async move { axum::Json(unlocked.0) }),
        )
        .layer(axum::extract::Extension(token_svc));

    // Request with comma-separated tokens in header
    let req = Request::get("/test-extractor")
        .header(
            "X-Kadr-Unlocked",
            format!("{}, {}, invalid.token", tok1, tok2),
        )
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let unlocked_set: std::collections::HashSet<String> =
        serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(unlocked_set.len(), 2);
    assert!(unlocked_set.contains("lib-1"));
    assert!(unlocked_set.contains("lib-2"));

    // Request with no header
    let req = Request::get("/test-extractor").body(Body::empty()).unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let unlocked_set: std::collections::HashSet<String> =
        serde_json::from_slice(&body_bytes).unwrap();
    assert!(unlocked_set.is_empty());
}
