use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::models::{
    MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_core::search::SearchResponse;
use kadr_server::api::create_router;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::playback::SessionRegistry;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_search_router() -> (axum::Router, String) {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let alice_user = User {
        id: "u-alice".into(),
        username: "alice".into(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo.create(&alice_user).await.unwrap();

    let jwt_svc = JwtService::new("search-test-secret-key-at-least-32-bytes!", 3600);
    let alice_jwt = jwt_svc.generate_token(&alice_user).unwrap();

    lib_repo
        .create(
            "lib-pub",
            "Public Films",
            "/media/public",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    lib_repo
        .create(
            "lib-shows",
            "TV Archive",
            "/media/shows",
            MediaType::Show,
            false,
            None,
        )
        .await
        .unwrap();

    lib_repo
        .create(
            "lib-private",
            "Private Vault",
            "/media/vault",
            MediaType::Movie,
            true,
            Some(&hash_pin("9999").unwrap()),
        )
        .await
        .unwrap();

    media_repo
        .upsert_batch(&[
            MediaItem {
                library_id: "lib-pub".into(),
                item_type: MediaType::Movie,
                title: "Dune: Part Two".into(),
                release_year: Some(2024),
                added_at: 1_700_000_100,
                file_path: "/media/public/dune2.mkv".into(),
                file_name: "Dune.Part.Two.2024.2160p.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 10000,
                    resolution: Some("2160p".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("Denis Villeneuve".into()),
                    actors: vec!["Timothée Chalamet".into(), "Zendaya".into()],
                    genres: vec!["Sci-Fi".into(), "Adventure".into()],
                    overview: Some("Paul Atreides unites with Chani and the Fremen.".into()),
                    poster_path: Some("/art/dune2.jpg".into()),
                    rating: Some(8.6),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "lib-shows".into(),
                item_type: MediaType::Episode,
                title: "The Rains of Castamere".into(),
                release_year: Some(2013),
                added_at: 1_700_000_200,
                file_path: "/media/shows/got_s03e09.mkv".into(),
                file_name: "got_s03e09.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 3100,
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    series_title: Some("Game of Thrones".into()),
                    season: Some(3),
                    episode: Some(9),
                    director: Some("David Nutter".into()),
                    genres: vec!["Fantasy".into(), "Drama".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "lib-private".into(),
                item_type: MediaType::Movie,
                title: "Dune: Classified Vault Reel".into(),
                release_year: Some(2024),
                added_at: 1_700_000_300,
                file_path: "/media/vault/dune_secret.mkv".into(),
                file_name: "dune_secret.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 5400,
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("Denis Villeneuve".into()),
                    actors: vec!["VaultExclusiveActor".into()],
                    genres: vec!["Sci-Fi".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    let dune = media_repo
        .find_by_path("/media/public/dune2.mkv")
        .await
        .unwrap()
        .unwrap();
    playback_repo
        .upsert_progress(
            "u-alice",
            dune.id.unwrap(),
            2500,
            WatchState::InProgress,
            1_700_001_000,
        )
        .await
        .unwrap();

    let limiter = RateLimiter::new(10, Duration::from_secs(60), Duration::from_secs(60));
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

    (app, alice_jwt)
}

async fn get_json(
    app: &axum::Router,
    uri: &str,
    jwt: Option<&str>,
    unlocked_header: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(tok) = jwt {
        builder = builder.header("Authorization", format!("Bearer {tok}"));
    }
    if let Some(uh) = unlocked_header {
        builder = builder.header("X-Kadr-Unlocked", uh);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, val)
}

#[tokio::test]
async fn search_route_requires_authentication_and_handles_empty_query() {
    let (app, jwt) = setup_search_router().await;

    // Missing JWT -> 401
    let (status, _) = get_json(&app, "/api/v1/search?q=Dune", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Empty / missing / whitespace q -> 200 OK empty payload
    for uri in [
        "/api/v1/search",
        "/api/v1/search?q=",
        "/api/v1/search?q=%20%20%20",
    ] {
        let (status, body) = get_json(&app, uri, Some(&jwt), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "query": "", "total": 0, "items": [] }));
    }
}

#[tokio::test]
async fn search_route_filters_hydrates_and_enforces_private_library_unlock_tokens() {
    let (app, jwt) = setup_search_router().await;

    // 1. Search "Dune: Part" while private vault is locked -> ONLY public Dune returned!
    let (status, body) = get_json(&app, "/api/v1/search?q=Dune", Some(&jwt), None).await;
    assert_eq!(status, StatusCode::OK);
    let parsed: SearchResponse = serde_json::from_value(body).unwrap();
    assert_eq!(parsed.query, "Dune");
    assert_eq!(parsed.total, 1);
    assert_eq!(parsed.items.len(), 1);
    assert_eq!(parsed.items[0].title, "Dune: Part Two");
    assert_eq!(parsed.items[0].library_name, "Public Films");
    assert!((parsed.items[0].playback_progress.unwrap() - 0.25).abs() < 0.01);
    assert_eq!(parsed.items[0].badge.as_deref(), Some("RESUME"));

    // 2. Forged X-Kadr-Unlocked header -> STILL zero leak!
    let (status, body) = get_json(
        &app,
        "/api/v1/search?q=VaultExclusiveActor",
        Some(&jwt),
        Some("forged.token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);

    // 3. Unlock private library via POST /api/v1/libraries/lib-private/unlock
    let unlock_req = Request::builder()
        .method("POST")
        .uri("/api/v1/libraries/lib-private/unlock")
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "pin": "9999" }).to_string()))
        .unwrap();
    let unlock_resp = app.clone().oneshot(unlock_req).await.unwrap();
    assert_eq!(unlock_resp.status(), StatusCode::OK);
    let unlock_bytes = unlock_resp.into_body().collect().await.unwrap().to_bytes();
    let unlock_json: Value = serde_json::from_slice(&unlock_bytes).unwrap();
    let unlock_token = unlock_json["token"].as_str().unwrap();

    // 4. With valid X-Kadr-Unlocked token -> both public & private Dune items returned!
    let (status, body) = get_json(
        &app,
        "/api/v1/search?q=Dune",
        Some(&jwt),
        Some(unlock_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let unlocked_parsed: SearchResponse = serde_json::from_value(body).unwrap();
    assert_eq!(unlocked_parsed.total, 2);
    assert_eq!(unlocked_parsed.items.len(), 2);

    // 5. Filter by item_type=episode and S03E09
    let (status, ep_body) = get_json(
        &app,
        "/api/v1/search?q=S03E09&item_type=episode",
        Some(&jwt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ep_parsed: SearchResponse = serde_json::from_value(ep_body).unwrap();
    assert_eq!(ep_parsed.total, 1);
    assert_eq!(ep_parsed.items[0].title, "The Rains of Castamere");
    assert_eq!(
        ep_parsed.items[0].series_title.as_deref(),
        Some("Game of Thrones")
    );
    assert_eq!(ep_parsed.items[0].season, Some(3));
    assert_eq!(ep_parsed.items[0].episode, Some(9));

    // 6. Invalid item_type -> 400 Bad Request
    let (status, _) = get_json(
        &app,
        "/api/v1/search?q=Dune&item_type=not_a_type",
        Some(&jwt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
