use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::ast::ItemDetailsPayload;
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole};
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

async fn send_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    jwt: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {jwt}"));

    let req = if let Some(payload) = body {
        builder = builder.header("Content-Type", "application/json");
        builder
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

#[tokio::test]
async fn patch_metadata_updates_fts_immediately_and_survives_upsert_batch_rescan() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    let admin = User {
        id: "u-admin".into(),
        username: "admin".into(),
        pin_hash: hash_pin("1234").unwrap(),
        role: UserRole::Admin,
        created_at: 1_700_000_000,
    };
    user_repo.create(&admin).await.unwrap();

    let jwt_svc = JwtService::new("cross-feature-integration-secret-key-32b!", 3600);
    let admin_jwt = jwt_svc.generate_token(&admin).unwrap();

    lib_repo
        .create(
            "lib-films",
            "Feature Films",
            "/media/films",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    // 1. Seed initial scanned item with placeholder title & empty credits
    media_repo
        .upsert_batch(&[MediaItem {
            library_id: "lib-films".into(),
            item_type: MediaType::Movie,
            title: "Scanner Placeholder Title".into(),
            release_year: Some(2021),
            added_at: 1_700_000_100,
            file_path: "/media/films/movie_01.mkv".into(),
            file_name: "movie_01.mkv".into(),
            technical: TechnicalInfo {
                duration_seconds: 9300,
                resolution: Some("2160p".into()),
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
            ..Default::default()
        }])
        .await
        .unwrap();

    let seeded = media_repo
        .find_by_path("/media/films/movie_01.mkv")
        .await
        .unwrap()
        .expect("seeded item should exist");
    let item_id = seeded.id.unwrap();

    let limiter = RateLimiter::new(10, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let app = create_router(
        user_repo,
        playback_repo,
        media_repo.clone(),
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
    );

    // Pre-check: placeholder title is searchable, edited credits are not yet indexed
    let (status, pre_body) = send_json(
        &app,
        "GET",
        "/api/v1/search?q=Placeholder",
        &admin_jwt,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pre_search: SearchResponse = serde_json::from_value(pre_body).unwrap();
    assert_eq!(pre_search.total, 1);

    let (_, pre_director_body) =
        send_json(&app, "GET", "/api/v1/search?q=Villeneuve", &admin_jwt, None).await;
    let pre_director: SearchResponse = serde_json::from_value(pre_director_body).unwrap();
    assert_eq!(pre_director.total, 0);

    // 2. Edit title, director, and actors via PATCH /api/v1/items/{id}/metadata
    let (patch_status, patch_body) = send_json(
        &app,
        "PATCH",
        &format!("/api/v1/items/{item_id}/metadata"),
        &admin_jwt,
        Some(json!({
            "title": "Dune: Part One",
            "director": "Denis Villeneuve",
            "actors": ["Timothée Chalamet", "Rebecca Ferguson"]
        })),
    )
    .await;
    assert_eq!(patch_status, StatusCode::OK);
    let updated_details: ItemDetailsPayload = serde_json::from_value(patch_body).unwrap();
    assert_eq!(updated_details.card.title, "Dune: Part One");
    assert_eq!(
        updated_details.director.as_deref(),
        Some("Denis Villeneuve")
    );
    assert!(updated_details.locked_fields.contains(&"title".to_string()));
    assert!(updated_details
        .locked_fields
        .contains(&"director".to_string()));
    assert!(updated_details
        .locked_fields
        .contains(&"actors".to_string()));

    // 3. Verify 011_search_fts.sql AFTER UPDATE trigger updated FTS5 index immediately
    for query in ["Dune", "Villeneuve", "Chalamet", "Ferguson"] {
        let (st, body) = send_json(
            &app,
            "GET",
            &format!("/api/v1/search?q={query}"),
            &admin_jwt,
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let res: SearchResponse = serde_json::from_value(body).unwrap();
        assert_eq!(res.total, 1, "expected FTS5 match for '{query}'");
        assert_eq!(res.items[0].id, item_id);
        assert_eq!(res.items[0].title, "Dune: Part One");
        assert_eq!(res.items[0].director.as_deref(), Some("Denis Villeneuve"));
    }

    // Old placeholder title token was purged from FTS5
    let (_, old_title_body) = send_json(
        &app,
        "GET",
        "/api/v1/search?q=Placeholder",
        &admin_jwt,
        None,
    )
    .await;
    let old_title_res: SearchResponse = serde_json::from_value(old_title_body).unwrap();
    assert_eq!(old_title_res.total, 0);

    // 4. Simulate a subsequent filesystem rescan via upsert_batch attempting to overwrite
    //    locked fields while supplying new unlocked metadata (`studio`).
    media_repo
        .upsert_batch(&[MediaItem {
            library_id: "lib-films".into(),
            item_type: MediaType::Movie,
            title: "Rescanned Noisy Filename".into(),
            release_year: Some(2021),
            added_at: 1_700_000_500,
            file_path: "/media/films/movie_01.mkv".into(),
            file_name: "movie_01.mkv".into(),
            technical: TechnicalInfo {
                duration_seconds: 9300,
                resolution: Some("2160p".into()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                director: Some("Scanner Wrong Director".into()),
                actors: vec!["Scanner Wrong Actor".into()],
                studio: Some("Legendary Pictures".into()),
                ..Default::default()
            },
            ..Default::default()
        }])
        .await
        .unwrap();

    // Verify media_items row preserved locked fields AND accepted unlocked `studio`
    let after_rescan = media_repo
        .get_by_id(item_id)
        .await
        .unwrap()
        .expect("item must still exist after rescan");
    assert_eq!(after_rescan.title, "Dune: Part One");
    assert_eq!(
        after_rescan.metadata.director.as_deref(),
        Some("Denis Villeneuve")
    );
    assert_eq!(
        after_rescan.metadata.actors,
        vec![
            "Timothée Chalamet".to_string(),
            "Rebecca Ferguson".to_string()
        ]
    );
    assert_eq!(
        after_rescan.metadata.studio.as_deref(),
        Some("Legendary Pictures")
    );

    // Verify GET /api/v1/search still finds locked title/director/actors AND newly scanned studio,
    // and never indexes the rejected rescan overwrite tokens!
    for query in ["Dune", "Villeneuve", "Ferguson", "Legendary"] {
        let (st, body) = send_json(
            &app,
            "GET",
            &format!("/api/v1/search?q={query}"),
            &admin_jwt,
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let res: SearchResponse = serde_json::from_value(body).unwrap();
        assert_eq!(
            res.total, 1,
            "expected FTS5 match after rescan for '{query}'"
        );
        assert_eq!(res.items[0].id, item_id);
        assert_eq!(res.items[0].title, "Dune: Part One");
        assert_eq!(res.items[0].director.as_deref(), Some("Denis Villeneuve"));
    }

    for rejected_query in ["Rescanned", "Wrong"] {
        let (st, body) = send_json(
            &app,
            "GET",
            &format!("/api/v1/search?q={rejected_query}"),
            &admin_jwt,
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let res: SearchResponse = serde_json::from_value(body).unwrap();
        assert_eq!(
            res.total, 0,
            "rejected rescan token '{rejected_query}' must not appear in FTS5"
        );
    }
}
