use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, PlaybackSession, TechnicalInfo, User, UserRole,
    WatchState,
};
use kadr_server::api::create_router;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::session::SessionRegistry;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

async fn setup_test_context() -> (
    UserRepository,
    PlaybackRepository,
    MediaItemRepository,
    LibraryRepository,
    JwtService,
    i64, // movie 1 id (duration 5000s)
    i64, // movie 2 id (duration 3000s)
    String, // user 1 token
    String, // user 2 token
) {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
        })
        .await
        .unwrap();

    let user1 = User {
        id: "u1".to_string(),
        username: "boody".to_string(),
        pin_hash: "hash1".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    user_repo.create(&user1).await.unwrap();

    let user2 = User {
        id: "u2".to_string(),
        username: "other".to_string(),
        pin_hash: "hash2".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    user_repo.create(&user2).await.unwrap();

    media_repo
        .upsert_batch(&[
            MediaItem {
                id: None,
                library_id: "lib1".to_string(),
                item_type: MediaType::Movie,
                title: "Cairo Station".to_string(),
                original_title: None,
                release_year: Some(1958),
                added_at: 1000,
                file_path: PathBuf::from("/media/cairo.mp4"),
                file_name: "cairo.mp4".to_string(),
                file_size: 1000,
                technical: TechnicalInfo {
                    duration_seconds: 5000,
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
            },
            MediaItem {
                id: None,
                library_id: "lib1".to_string(),
                item_type: MediaType::Movie,
                title: "The Land".to_string(),
                original_title: None,
                release_year: Some(1969),
                added_at: 2000,
                file_path: PathBuf::from("/media/the_land.mp4"),
                file_name: "the_land.mp4".to_string(),
                file_size: 2000,
                technical: TechnicalInfo {
                    duration_seconds: 3000,
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
            },
        ])
        .await
        .unwrap();

    let items = media_repo.list_by_library("lib1", 10, 0).await.unwrap();
    let item1_id = items.iter().find(|i| i.title == "Cairo Station").unwrap().id.unwrap();
    let item2_id = items.iter().find(|i| i.title == "The Land").unwrap().id.unwrap();

    let jwt = JwtService::new("my-secret-key-that-is-at-least-32-bytes-long", 3600);
    let token1 = jwt.generate_token(&user1).unwrap();
    let token2 = jwt.generate_token(&user2).unwrap();

    (
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt,
        item1_id,
        item2_id,
        token1,
        token2,
    )
}

#[tokio::test]
async fn test_playback_session_lifecycle_and_scrobble() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item_id, _, token, _) =
        setup_test_context().await;

    let session_registry = Arc::new(SessionRegistry::new());

    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
    );

    // 1. Create playback session
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = json["session_id"].as_str().unwrap();

    // 2. Send heartbeat (>60s) -> watch_state should become InProgress
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":150}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let state = playback_repo.get_state("u1", item_id).await.unwrap().unwrap();
    assert_eq!(state.watch_state, WatchState::InProgress);
    assert_eq!(state.playback_position_seconds, 150);

    // 3. Send heartbeat (>= 90% of 5000s = 4500s) -> Completed
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":4600}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let state = playback_repo.get_state("u1", item_id).await.unwrap().unwrap();
    assert_eq!(state.watch_state, WatchState::Completed);
    assert_eq!(state.play_count, 1);
}

#[tokio::test]
async fn test_session_resume_position_from_previous_state() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item_id, _, token, _) =
        setup_test_context().await;

    // Simulate pre-existing InProgress state at 750 seconds
    playback_repo
        .upsert_progress("u1", item_id, 750, WatchState::InProgress, 123456)
        .await
        .unwrap();

    let session_registry = Arc::new(SessionRegistry::new());
    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
    );

    // 1. Create session -> resume_position_seconds should be 750
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["resume_position_seconds"], 750);
    assert_eq!(json["duration_seconds"], 5000);

    // 2. Now complete the movie
    playback_repo
        .upsert_progress("u1", item_id, 4900, WatchState::Completed, 123999)
        .await
        .unwrap();

    // 3. Create another session -> resume_position_seconds should be 0 because it was completed
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["resume_position_seconds"], 0);
}

#[tokio::test]
async fn test_playback_state_endpoint_and_continue_watching() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item1_id, item2_id, token, _) =
        setup_test_context().await;

    let session_registry = Arc::new(SessionRegistry::new());
    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
    );

    // 1. GET state for unwatched item1 -> returns default unwatched state
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/playback/states/{}", item1_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["watch_state"], "unwatched");
    assert_eq!(json["playback_position_seconds"], 0);
    assert_eq!(json["play_count"], 0);

    // 2. Set item1 to InProgress and item2 to Completed
    playback_repo
        .upsert_progress("u1", item1_id, 300, WatchState::InProgress, 1700000100)
        .await
        .unwrap();
    playback_repo
        .upsert_progress("u1", item2_id, 2900, WatchState::Completed, 1700000200)
        .await
        .unwrap();

    // 3. GET state for item1 -> returns recorded state
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/playback/states/{}", item1_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["watch_state"], "in_progress");
    assert_eq!(json["playback_position_seconds"], 300);

    // 4. GET /api/v1/playback/continue-watching -> only returns item1 (InProgress)
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/playback/continue-watching")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let list: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["media_item_id"], item1_id);
    assert_eq!(list[0]["watch_state"], "in_progress");
}

#[tokio::test]
async fn test_session_isolation_and_cross_user_protection() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item_id, _, token1, token2) =
        setup_test_context().await;

    let session_registry = Arc::new(SessionRegistry::new());
    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
    );

    // 1. User 1 creates session
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token1))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = json["session_id"].as_str().unwrap();

    // 2. User 2 attempts to send heartbeat to User 1's session -> 404 NOT_FOUND
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token2))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":200}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 3. User 2 attempts to close User 1's session -> 404 NOT_FOUND
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/playback/sessions/{}", session_id))
        .header("authorization", format!("Bearer {}", token2))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Verify User 1's state was not altered
    let u1_state = playback_repo.get_state("u1", item_id).await.unwrap();
    assert!(u1_state.is_none());
}

#[tokio::test]
async fn test_close_session_lifecycle() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item_id, _, token, _) =
        setup_test_context().await;

    let session_registry = Arc::new(SessionRegistry::new());
    let app = create_router(
        user_repo,
        playback_repo.clone(),
        media_repo,
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry.clone(),
    );

    // 1. Create session
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{}}}"#, item_id)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = json["session_id"].as_str().unwrap();

    assert_eq!(session_registry.len().await, 1);

    // 2. Close session -> 204 NO_CONTENT
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/playback/sessions/{}", session_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_eq!(session_registry.len().await, 0);

    // 3. Subsequent heartbeat on closed session -> 404 NOT_FOUND
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{}/progress", session_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":100}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 4. Closing already closed session -> 404 NOT_FOUND
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/playback/sessions/{}", session_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_session_registry_prune_stale() {
    let registry = SessionRegistry::new();
    assert!(registry.is_empty().await);

    let session1 = PlaybackSession {
        session_id: "s1".to_string(),
        user_id: "u1".to_string(),
        media_item_id: 1,
        duration_seconds: 1000,
        current_position_seconds: 50,
        started_at: 100,
        last_heartbeat_at: 100,
    };
    let session2 = PlaybackSession {
        session_id: "s2".to_string(),
        user_id: "u2".to_string(),
        media_item_id: 2,
        duration_seconds: 2000,
        current_position_seconds: 100,
        started_at: 100,
        last_heartbeat_at: 100,
    };

    registry.insert(session1).await;
    registry.insert(session2).await;
    assert_eq!(registry.len().await, 2);

    // Wait a tiny moment and prune with 0 duration -> all should be pruned
    tokio::time::sleep(Duration::from_millis(5)).await;
    let pruned = registry.prune_stale(Duration::from_millis(1)).await;
    assert_eq!(pruned, 2);
    assert_eq!(registry.len().await, 0);
}
