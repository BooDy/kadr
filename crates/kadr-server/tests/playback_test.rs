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
    i64,    // movie 1 id (duration 5000s)
    i64,    // movie 2 id (duration 3000s)
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
        .insert(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
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
                locked_fields: Vec::new(),
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
                locked_fields: Vec::new(),
            },
        ])
        .await
        .unwrap();

    let items = media_repo.list_by_library("lib1", 10, 0).await.unwrap();
    let item1_id = items
        .iter()
        .find(|i| i.title == "Cairo Station")
        .unwrap()
        .id
        .unwrap();
    let item2_id = items
        .iter()
        .find(|i| i.title == "The Land")
        .unwrap()
        .id
        .unwrap();

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

    let state = playback_repo
        .get_state("u1", item_id)
        .await
        .unwrap()
        .unwrap();
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

    let state = playback_repo
        .get_state("u1", item_id)
        .await
        .unwrap()
        .unwrap();
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

#[tokio::test]
async fn test_manual_set_playback_state_single_and_zero_duration() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item1_id, _, token, _) =
        setup_test_context().await;

    // Insert a zero-duration item and a live channel item in the same public library
    let item1 = media_repo.get_by_id(item1_id).await.unwrap().unwrap();
    let mut zero_item = item1.clone();
    zero_item.id = None;
    zero_item.file_path = PathBuf::from("/tmp/zero_duration_ep.mkv");
    zero_item.file_name = "zero_duration_ep.mkv".to_string();
    zero_item.title = "Zero Duration Ep".to_string();
    zero_item.technical.duration_seconds = 0;

    let mut live_item = item1.clone();
    live_item.id = None;
    live_item.item_type = MediaType::LiveChannel;
    live_item.file_path = PathBuf::from("/tmp/live_channel_1");
    live_item.file_name = "live_channel_1".to_string();
    live_item.title = "Live News".to_string();
    live_item.technical.duration_seconds = 0;
    live_item
        .metadata
        .extra
        .insert("live".to_string(), serde_json::json!(true));

    media_repo
        .batch_upsert(&[zero_item, live_item])
        .await
        .unwrap();
    let zero_id = media_repo
        .find_by_path("/tmp/zero_duration_ep.mkv")
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();
    let live_id = media_repo
        .find_by_path("/tmp/live_channel_1")
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();

    playback_repo
        .upsert_progress("u1", item1_id, 300, WatchState::InProgress, 1700000100)
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

    // Live item does not support manual watch state -> 400 BAD_REQUEST
    let live_req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/playback/states/{}", live_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"watched":true}"#))
        .unwrap();
    let live_res = app.clone().oneshot(live_req).await.unwrap();
    assert_eq!(live_res.status(), StatusCode::BAD_REQUEST);

    // 1. Mark item1 watched
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/playback/states/{}", item1_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"watched":true}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["watch_state"], "completed");
    assert_eq!(json["play_count"], 1);
    assert_eq!(json["playback_position_seconds"], 5000);

    // Continue watching should now be empty
    let cw_req = Request::builder()
        .method("GET")
        .uri("/api/v1/playback/continue-watching")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let cw_res = app.clone().oneshot(cw_req).await.unwrap();
    let cw_bytes = axum::body::to_bytes(cw_res.into_body(), 4096).await.unwrap();
    let cw_list: Vec<Value> = serde_json::from_slice(&cw_bytes).unwrap();
    assert!(cw_list.is_empty());

    // Item details should report playback_progress = 1.0 and resume_position_seconds = null
    let det_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/details", item1_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let det_res = app.clone().oneshot(det_req).await.unwrap();
    let det_bytes = axum::body::to_bytes(det_res.into_body(), 8192).await.unwrap();
    let det_json: Value = serde_json::from_slice(&det_bytes).unwrap();
    assert_eq!(det_json["card"]["playback_progress"], 1.0);
    assert!(det_json["resume_position_seconds"].is_null());

    // 2. Mark zero-duration item watched -> card.playback_progress == 1.0
    let z_req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/playback/states/{}", zero_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"watched":true}"#))
        .unwrap();
    let z_res = app.clone().oneshot(z_req).await.unwrap();
    assert_eq!(z_res.status(), StatusCode::OK);

    let z_det_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/details", zero_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let z_det_res = app.clone().oneshot(z_det_req).await.unwrap();
    let z_det_bytes = axum::body::to_bytes(z_det_res.into_body(), 8192).await.unwrap();
    let z_det_json: Value = serde_json::from_slice(&z_det_bytes).unwrap();
    assert_eq!(z_det_json["card"]["playback_progress"], 1.0);

    // 3. Mark item1 unwatched -> watch_state = unwatched, play_count preserved at 1, progress = null
    let unw_req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/playback/states/{}", item1_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"watched":false}"#))
        .unwrap();
    let unw_res = app.clone().oneshot(unw_req).await.unwrap();
    assert_eq!(unw_res.status(), StatusCode::OK);
    let unw_bytes = axum::body::to_bytes(unw_res.into_body(), 4096).await.unwrap();
    let unw_json: Value = serde_json::from_slice(&unw_bytes).unwrap();
    assert_eq!(unw_json["watch_state"], "unwatched");
    assert_eq!(unw_json["playback_position_seconds"], 0);
    assert_eq!(unw_json["play_count"], 1);

    let det2_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/details", item1_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let det2_res = app.clone().oneshot(det2_req).await.unwrap();
    let det2_bytes = axum::body::to_bytes(det2_res.into_body(), 8192).await.unwrap();
    let det2_json: Value = serde_json::from_slice(&det2_bytes).unwrap();
    assert!(det2_json["card"]["playback_progress"].is_null());
    assert!(det2_json["resume_position_seconds"].is_null());
}

#[tokio::test]
async fn test_manual_set_playback_states_batch_and_private_library_guard() {
    let (user_repo, playback_repo, media_repo, lib_repo, jwt, item1_id, item2_id, token, _) =
        setup_test_context().await;

    // Create a private library with PIN "1234" and a private media item
    let pin_hash = kadr_server::auth::pin::hash_pin("1234").unwrap();
    lib_repo
        .create(
            "lib-priv-watch",
            "Private Vault",
            "/tmp/private-vault",
            MediaType::Movie,
            true,
            Some(&pin_hash),
        )
        .await
        .unwrap();

    let item1 = media_repo.get_by_id(item1_id).await.unwrap().unwrap();
    let mut priv_item = item1.clone();
    priv_item.id = None;
    priv_item.library_id = "lib-priv-watch".to_string();
    priv_item.file_path = PathBuf::from("/tmp/private-vault/secret.mkv");
    priv_item.file_name = "secret.mkv".to_string();
    priv_item.title = "Secret Film".to_string();

    let mut live_item = item1.clone();
    live_item.id = None;
    live_item.item_type = MediaType::LiveChannel;
    live_item.file_path = PathBuf::from("/tmp/live_batch_skip");
    live_item.file_name = "live_batch_skip".to_string();
    live_item.title = "Live Batch Skip".to_string();
    live_item.technical.duration_seconds = 0;
    live_item
        .metadata
        .extra
        .insert("live".to_string(), serde_json::json!(true));

    media_repo
        .batch_upsert(&[priv_item, live_item])
        .await
        .unwrap();
    let priv_id = media_repo
        .find_by_path("/tmp/private-vault/secret.mkv")
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();
    let live_id = media_repo
        .find_by_path("/tmp/live_batch_skip")
        .await
        .unwrap()
        .unwrap()
        .id
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

    // 1. Empty batch -> 400 BAD_REQUEST
    let empty_req = Request::builder()
        .method("PUT")
        .uri("/api/v1/playback/states")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"item_ids":[],"watched":true}"#))
        .unwrap();
    let empty_res = app.clone().oneshot(empty_req).await.unwrap();
    assert_eq!(empty_res.status(), StatusCode::BAD_REQUEST);

    // 2. Batch mark item1 + item2 + live_id watched -> live item is skipped, updated == 2
    let batch_req = Request::builder()
        .method("PUT")
        .uri("/api/v1/playback/states")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"item_ids":[{},{},{}],"watched":true}}"#,
            item1_id, item2_id, live_id
        )))
        .unwrap();
    let batch_res = app.clone().oneshot(batch_req).await.unwrap();
    assert_eq!(batch_res.status(), StatusCode::OK);
    let batch_bytes = axum::body::to_bytes(batch_res.into_body(), 4096).await.unwrap();
    let batch_json: Value = serde_json::from_slice(&batch_bytes).unwrap();
    assert_eq!(batch_json["updated"], 2);
    assert_eq!(batch_json["watched"], true);

    assert_eq!(
        playback_repo
            .get_state("u1", item1_id)
            .await
            .unwrap()
            .unwrap()
            .watch_state,
        WatchState::Completed
    );
    assert_eq!(
        playback_repo
            .get_state("u1", item2_id)
            .await
            .unwrap()
            .unwrap()
            .watch_state,
        WatchState::Completed
    );
    assert!(playback_repo.get_state("u1", live_id).await.unwrap().is_none());

    // 3. Locked private library item -> 403 LIBRARY_LOCKED on both single and batch PUT
    let locked_single = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/playback/states/{}", priv_id))
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"watched":true}"#))
        .unwrap();
    let locked_res = app.clone().oneshot(locked_single).await.unwrap();
    assert_eq!(locked_res.status(), StatusCode::FORBIDDEN);
    let locked_bytes = axum::body::to_bytes(locked_res.into_body(), 4096).await.unwrap();
    let locked_json: Value = serde_json::from_slice(&locked_bytes).unwrap();
    assert_eq!(locked_json["error"], "LIBRARY_LOCKED");

    let locked_batch = Request::builder()
        .method("PUT")
        .uri("/api/v1/playback/states")
        .header("authorization", format!("Bearer {}", token))
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"item_ids":[{}],"watched":true}}"#,
            priv_id
        )))
        .unwrap();
    let locked_batch_res = app.clone().oneshot(locked_batch).await.unwrap();
    assert_eq!(locked_batch_res.status(), StatusCode::FORBIDDEN);
    let locked_batch_bytes = axum::body::to_bytes(locked_batch_res.into_body(), 4096)
        .await
        .unwrap();
    let locked_batch_json: Value = serde_json::from_slice(&locked_batch_bytes).unwrap();
    assert_eq!(locked_batch_json["error"], "LIBRARY_LOCKED");

    // 4. Unlock private library and retry -> 200 OK
    let unlock_req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries/lib-priv-watch/unlock")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"pin":"1234"}"#))
        .unwrap();
    let unlock_res = app.clone().oneshot(unlock_req).await.unwrap();
    assert_eq!(unlock_res.status(), StatusCode::OK);
    let unlock_bytes = axum::body::to_bytes(unlock_res.into_body(), 4096).await.unwrap();
    let unlock_json: Value = serde_json::from_slice(&unlock_bytes).unwrap();
    let unlock_token = unlock_json["unlock_token"].as_str().unwrap();

    let unlocked_single = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/playback/states/{}", priv_id))
        .header("authorization", format!("Bearer {}", token))
        .header("x-kadr-unlocked", unlock_token)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"watched":true}"#))
        .unwrap();
    let unlocked_res = app.clone().oneshot(unlocked_single).await.unwrap();
    assert_eq!(unlocked_res.status(), StatusCode::OK);
}
