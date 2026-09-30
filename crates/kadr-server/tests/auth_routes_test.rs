use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
use kadr_core::models::{User, UserRole};
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::auth::jwt::JwtService;
use kadr_server::api::create_router;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{UserRepository, PlaybackRepository, MediaItemRepository, LibraryRepository};
use std::time::Duration;

#[tokio::test]
async fn test_auth_profile_flow_and_rate_limiting() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };
    user_repo.create(&admin).await.unwrap();

    let jwt = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt.clone(),
        rate_limiter,
    );

    // 1. GET /api/v1/users/profiles
    let req = Request::builder().uri("/api/v1/users/profiles").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let profiles: Value = serde_json::from_slice(&bytes).unwrap();
    let arr = profiles.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "admin-1");
    assert_eq!(arr[0]["username"], "admin");
    assert_eq!(arr[0]["role"], "admin");
    assert!(arr[0].get("pin_hash").is_none());

    // 2. Bad PIN -> 401
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-1","pin":"9999"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 3. Good PIN -> 200 with JWT
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-1","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let token = json["token"].as_str().unwrap();
    assert_eq!(json["user_id"], "admin-1");
    assert_eq!(json["username"], "admin");
    assert_eq!(json["role"], "admin");

    // 4. GET /api/v1/auth/me with Bearer token
    let req = Request::builder()
        .uri("/api/v1/auth/me")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let me: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(me["id"], "admin-1");
    assert_eq!(me["username"], "admin");
    assert_eq!(me["role"], "admin");
}

#[tokio::test]
async fn test_rate_limiting_lockout_and_headers() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };
    user_repo.create(&admin).await.unwrap();

    let jwt = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt.clone(),
        rate_limiter,
    );

    // 5 failed PIN attempts
    for _ in 0..5 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/profile-pin")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"user_id":"admin-1","pin":"0000"}"#))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    // 6th attempt -> 429 Too Many Requests with Retry-After header
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-1","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = res.headers().get("Retry-After").expect("missing Retry-After header");
    let retry_secs: u64 = retry_after.to_str().unwrap().parse().unwrap();
    assert!(retry_secs > 0 && retry_secs <= 300);
}

#[tokio::test]
async fn test_user_creation_admin_and_forbidden() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };
    user_repo.create(&admin).await.unwrap();

    let standard = User {
        id: "user-2".to_string(),
        username: "regular".to_string(),
        pin_hash: hash_pin("5678").unwrap(),
        role: UserRole::Standard,
        created_at: 1700000000,
    };
    user_repo.create(&standard).await.unwrap();

    let jwt = JwtService::new("super-secret-key-that-is-at-least-32-bytes-long", 3600);
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));

    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt.clone(),
        rate_limiter,
    );

    let admin_token = jwt.generate_token(&admin).unwrap();
    let standard_token = jwt.generate_token(&standard).unwrap();

    // 1. Unauthenticated creation -> 401
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"newuser","pin":"1111","role":"standard"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Standard user creation -> 403 Forbidden
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header("authorization", format!("Bearer {}", standard_token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"newuser","pin":"1111","role":"standard"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 3. Admin creation with invalid PIN format -> 400 Bad Request
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header("authorization", format!("Bearer {}", admin_token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"newuser","pin":"not-a-pin","role":"standard"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 4. Admin creation success -> 201 Created
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header("authorization", format!("Bearer {}", admin_token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"family1","pin":"4321","role":"standard"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let created: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(created["username"], "family1");
    assert_eq!(created["role"], "standard");
    let new_user_id = created["id"].as_str().unwrap();

    // 5. Verify newly created user can log in with their PIN
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"user_id":"{}","pin":"4321"}}"#, new_user_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
