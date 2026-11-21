use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::ast::{
    QueryMacro, ScreenId, ScreenLayout, WidgetFilterConfig, WidgetNode, WidgetQueryBinding,
};
use kadr_core::filter_ast::CatalogFacets;
use kadr_core::models::{
    MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_server::api::create_router_with_ingest;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::events::EventBus;
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
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::RwLock;
use tower::ServiceExt;

struct FilterTestEnv {
    _temp_dir: TempDir,
    app: axum::Router,
    admin_jwt: String,
    user_jwt: String,
    lib_repo: LibraryRepository,
    media_repo: MediaItemRepository,
    playback_repo: PlaybackRepository,
}

async fn setup_filter_test_env() -> FilterTestEnv {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let pool = create_in_memory_pool().expect("in-memory pool");
    initialize_database(&pool).await.expect("init db");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let admin_user = User {
        id: "u-admin".into(),
        username: "admin".into(),
        pin_hash: hash_pin("1111").unwrap(),
        role: UserRole::Admin,
        created_at: 1_700_000_000,
    };
    let standard_user = User {
        id: "u-standard".into(),
        username: "alice".into(),
        pin_hash: hash_pin("2222").unwrap(),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo.create(&admin_user).await.unwrap();
    user_repo.create(&standard_user).await.unwrap();

    let jwt_svc = JwtService::new("filter-ast-e2e-secret-key-at-least-32-bytes!", 3600);
    let admin_jwt = jwt_svc.generate_token(&admin_user).unwrap();
    let user_jwt = jwt_svc.generate_token(&standard_user).unwrap();

    lib_repo
        .create(
            "movies",
            "Movies",
            "/media/movies",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    lib_repo
        .create(
            "shows",
            "TV Shows",
            "/media/shows",
            MediaType::Show,
            false,
            None,
        )
        .await
        .unwrap();

    lib_repo
        .create(
            "lib-vault",
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
                library_id: "movies".into(),
                item_type: MediaType::Movie,
                title: "Arrival".into(),
                release_year: Some(2016),
                added_at: 1_700_000_100,
                file_path: PathBuf::from("/media/movies/arrival.mkv"),
                file_name: "arrival.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 6960,
                    resolution: Some("4K".into()),
                    video_codec: Some("hevc".into()),
                    audio_codec: Some("eac3".into()),
                    audio_channels: Some(6),
                    container: Some("mkv".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("Denis Villeneuve".into()),
                    studio: Some("Paramount".into()),
                    country: Some("US".into()),
                    language: Some("en".into()),
                    genres: vec!["Sci-Fi".into(), "Drama".into()],
                    tags: vec!["aliens".into(), "linguistics".into()],
                    rating: Some(8.0),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "movies".into(),
                item_type: MediaType::Movie,
                title: "Parasite".into(),
                release_year: Some(2019),
                added_at: 1_700_000_200,
                file_path: PathBuf::from("/media/movies/parasite.mp4"),
                file_name: "parasite.mp4".into(),
                technical: TechnicalInfo {
                    duration_seconds: 7920,
                    resolution: Some("1080p".into()),
                    video_codec: Some("h264".into()),
                    audio_codec: Some("aac".into()),
                    audio_channels: Some(6),
                    container: Some("mp4".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("Bong Joon-ho".into()),
                    studio: Some("Barunson".into()),
                    country: Some("KR".into()),
                    language: Some("ko".into()),
                    genres: vec!["Thriller".into(), "Drama".into()],
                    tags: vec!["satire".into()],
                    rating: Some(8.5),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "shows".into(),
                item_type: MediaType::Show,
                title: "Dark".into(),
                release_year: Some(2017),
                added_at: 1_700_000_300,
                file_path: PathBuf::from("/media/shows/dark"),
                file_name: "dark".into(),
                technical: TechnicalInfo {
                    duration_seconds: 3600,
                    resolution: Some("1080p".into()),
                    video_codec: Some("hevc".into()),
                    audio_codec: Some("eac3".into()),
                    audio_channels: Some(6),
                    container: Some("mkv".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("Baran bo Odar".into()),
                    studio: Some("Netflix".into()),
                    country: Some("DE".into()),
                    language: Some("de".into()),
                    genres: vec!["Sci-Fi".into(), "Mystery".into()],
                    tags: vec!["time-travel".into()],
                    rating: Some(8.8),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "lib-vault".into(),
                item_type: MediaType::Movie,
                title: "Classified Noir".into(),
                release_year: Some(1999),
                added_at: 1_700_000_400,
                file_path: PathBuf::from("/media/vault/classified.webm"),
                file_name: "classified.webm".into(),
                technical: TechnicalInfo {
                    duration_seconds: 5400,
                    resolution: Some("720p".into()),
                    video_codec: Some("av1".into()),
                    audio_codec: Some("flac".into()),
                    audio_channels: Some(2),
                    container: Some("webm".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("SecretDirector".into()),
                    studio: Some("VaultStudio".into()),
                    country: Some("FR".into()),
                    language: Some("fr".into()),
                    genres: vec!["VaultNoir".into()],
                    tags: vec!["top-secret".into()],
                    rating: Some(9.1),
                    ..Default::default()
                },
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    let limiter = RateLimiter::new(50, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_dir.path().join("subtitles"),
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let (dummy_tx, _) = tokio::sync::mpsc::channel(1);
    let dummy_pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));

    let mut cfg = AppConfig::default();
    cfg.server.data_dir = temp_dir.path().to_path_buf();
    let config = Arc::new(RwLock::new(cfg));
    let identity = Arc::new(ServerIdentity {
        id: "00000000-0000-0000-0000-000000000000".into(),
    });

    let app = create_router_with_ingest(
        user_repo,
        playback_repo.clone(),
        media_repo.clone(),
        lib_repo.clone(),
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

    FilterTestEnv {
        _temp_dir: temp_dir,
        app,
        admin_jwt,
        user_jwt,
        lib_repo,
        media_repo,
        playback_repo,
    }
}

async fn send_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    jwt: Option<&str>,
    unlocked: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = jwt {
        builder = builder.header("Authorization", format!("Bearer {tok}"));
    }
    if let Some(u) = unlocked {
        builder = builder.header("X-Kadr-Unlocked", u);
    }
    let req_body = if let Some(b) = body {
        builder = builder.header("Content-Type", "application/json");
        Body::from(serde_json::to_vec(&b).unwrap())
    } else {
        Body::empty()
    };
    let resp = app
        .clone()
        .oneshot(builder.body(req_body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

async fn unlock_vault(app: &axum::Router, jwt: &str) -> String {
    let (status, body) = send_json(
        app,
        "POST",
        "/api/v1/libraries/lib-vault/unlock",
        Some(jwt),
        None,
        Some(json!({ "pin": "9999" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn catalog_facets_returns_public_facets_and_isolates_private_libraries_until_unlocked() {
    let env = setup_filter_test_env().await;

    // Unauthenticated -> 401
    let (status, _) = send_json(&env.app, "GET", "/api/v1/catalog/facets", None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Public-only facets when private vault is locked
    let (status, body) = send_json(
        &env.app,
        "GET",
        "/api/v1/catalog/facets",
        Some(&env.user_jwt),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let facets: CatalogFacets = serde_json::from_value(body).unwrap();
    assert!(facets.genres.contains(&"Sci-Fi".to_string()));
    assert!(facets.genres.contains(&"Thriller".to_string()));
    assert!(!facets.genres.contains(&"VaultNoir".to_string()));
    assert!(!facets.countries.contains(&"FR".to_string()));
    assert!(!facets.directors.contains(&"SecretDirector".to_string()));
    assert!(!facets.studios.contains(&"VaultStudio".to_string()));
    assert!(!facets.tags.contains(&"top-secret".to_string()));
    assert!(!facets.video_codecs.contains(&"av1".to_string()));
    assert!(!facets.containers.contains(&"webm".to_string()));
    assert_eq!(facets.year_min, Some(2016));
    assert_eq!(facets.year_max, Some(2019));

    // Scoped to library_id=movies omits shows facets ("DE", "Mystery")
    let (status, movies_body) = send_json(
        &env.app,
        "GET",
        "/api/v1/catalog/facets?library_id=movies",
        Some(&env.user_jwt),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let movie_facets: CatalogFacets = serde_json::from_value(movies_body).unwrap();
    assert!(movie_facets.countries.contains(&"US".to_string()));
    assert!(movie_facets.countries.contains(&"KR".to_string()));
    assert!(!movie_facets.countries.contains(&"DE".to_string()));
    assert!(!movie_facets.genres.contains(&"Mystery".to_string()));

    // Unlock private vault and verify facets now include vault values
    let unlock_tok = unlock_vault(&env.app, &env.user_jwt).await;
    let (status, unlocked_body) = send_json(
        &env.app,
        "GET",
        "/api/v1/catalog/facets",
        Some(&env.user_jwt),
        Some(&unlock_tok),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let unlocked_facets: CatalogFacets = serde_json::from_value(unlocked_body).unwrap();
    assert!(unlocked_facets.genres.contains(&"VaultNoir".to_string()));
    assert!(unlocked_facets.countries.contains(&"FR".to_string()));
    assert!(unlocked_facets
        .directors
        .contains(&"SecretDirector".to_string()));
    assert!(unlocked_facets.tags.contains(&"top-secret".to_string()));
    assert_eq!(unlocked_facets.year_min, Some(1999));
    assert_eq!(unlocked_facets.year_max, Some(2019));
}

#[tokio::test]
async fn widget_preview_evaluates_nested_ast_enforces_rbac_and_validates_rules() {
    let env = setup_filter_test_env().await;

    let valid_preview_payload = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 20,
            "sort": "rating:desc",
            "filters": {
                "rule_tree": {
                    "kind": "group",
                    "op": "and",
                    "children": [
                        {
                            "kind": "group",
                            "op": "or",
                            "children": [
                                {
                                    "kind": "rule",
                                    "condition": {
                                        "field": "genres",
                                        "op": "contains_any",
                                        "values": ["Sci-Fi"]
                                    }
                                },
                                {
                                    "kind": "rule",
                                    "condition": {
                                        "field": "country",
                                        "op": "eq",
                                        "value": "KR"
                                    }
                                }
                            ]
                        },
                        {
                            "kind": "rule",
                            "negated": true,
                            "condition": {
                                "field": "country",
                                "op": "eq",
                                "value": "DE"
                            }
                        },
                        {
                            "kind": "rule",
                            "condition": {
                                "field": "release_year",
                                "op": "between",
                                "min": 2015,
                                "max": 2020
                            }
                        }
                    ]
                }
            }
        },
        "screen_id": "home",
        "limit": 10
    });

    // 1. Non-admin user receives 403 Forbidden
    let (status, _) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.user_jwt),
        None,
        Some(valid_preview_payload.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 2. Empty group children returns 400 Bad Request
    let empty_group_payload = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 10,
            "filters": {
                "rule_tree": {
                    "kind": "group",
                    "op": "and",
                    "children": []
                }
            }
        }
    });
    let (status, err_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(empty_group_payload),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(err_body["error"].as_str().is_some());

    // 3. Invalid range (min > max) returns 400 Bad Request
    let bad_range_payload = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 10,
            "filters": {
                "rule_tree": {
                    "kind": "rule",
                    "condition": {
                        "field": "release_year",
                        "op": "between",
                        "min": 2025,
                        "max": 2010
                    }
                }
            }
        }
    });
    let (status, _) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(bad_range_payload),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 4. Admin valid custom_query preview with nested AND / OR / NOT
    let (status, ok_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(valid_preview_payload),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ok_body["total_count"], 2);
    assert!(ok_body["execution_ms"].as_u64().is_some());
    let items = ok_body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "Parasite");
    assert_eq!(items[1]["title"], "Arrival");

    // 5. Private library isolation in preview (locked vs unlocked)
    let vault_preview = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 10,
            "filters": {
                "rule_tree": {
                    "kind": "rule",
                    "condition": {
                        "field": "genres",
                        "op": "contains_any",
                        "values": ["VaultNoir"]
                    }
                }
            }
        }
    });
    let (status, locked_res) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(vault_preview.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(locked_res["total_count"], 0);

    let unlock_tok = unlock_vault(&env.app, &env.admin_jwt).await;
    let (status, unlocked_res) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        Some(&unlock_tok),
        Some(vault_preview),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unlocked_res["total_count"], 1);
    assert_eq!(unlocked_res["items"][0]["title"], "Classified Noir");

    // 6. Canonical screen scoping (screen_id: "shows" scopes to shows library)
    let shows_preview = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 10,
            "filters": {
                "rule_tree": {
                    "kind": "rule",
                    "condition": {
                        "field": "rating",
                        "op": "gte",
                        "value": 7.0
                    }
                }
            }
        },
        "screen_id": "shows"
    });
    let (status, shows_res) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(shows_preview),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shows_res["total_count"], 1);
    assert_eq!(shows_res["items"][0]["title"], "Dark");
}

#[tokio::test]
async fn put_screen_validates_rule_tree_and_hydrates_paginated_custom_query_widget() {
    let env = setup_filter_test_env().await;

    // Mark "Arrival" as Completed for standard user (alice), unwatched for admin
    let arrival = env
        .media_repo
        .find_by_path("/media/movies/arrival.mkv")
        .await
        .unwrap()
        .unwrap();
    env.playback_repo
        .upsert_progress(
            "u-standard",
            arrival.id.unwrap(),
            6960,
            WatchState::Completed,
            1_700_001_000,
        )
        .await
        .unwrap();

    // 1. PUT /api/v1/screens/home with invalid rule_tree (empty group) -> 400 Bad Request
    let invalid_screen = json!({
        "id": "home",
        "title": "Home",
        "widgets": [
            {
                "type": "carousel",
                "id": "bad_widget",
                "title": "Broken Shelf",
                "binding": {
                    "macro_type": "custom_query",
                    "limit": 10,
                    "filters": {
                        "rule_tree": {
                            "kind": "group",
                            "op": "and",
                            "children": []
                        }
                    }
                }
            }
        ]
    });
    let (status, err_body) = send_json(
        &env.app,
        "PUT",
        "/api/v1/screens/home",
        Some(&env.admin_jwt),
        None,
        Some(invalid_screen),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(err_body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("bad_widget"));

    // 2. PUT /api/v1/screens/home with valid custom_query grid (limit 1 for pagination)
    //    and RecentlyAdded carousel filtered to Unwatched items
    let valid_screen = json!({
        "id": "home",
        "title": "Home",
        "widgets": [
            {
                "type": "grid",
                "id": "acclaimed_movies",
                "title": "Acclaimed Movies",
                "columns": 4,
                "binding": {
                    "macro_type": "custom_query",
                    "limit": 1,
                    "sort": "title:asc",
                    "filters": {
                        "rule_tree": {
                            "kind": "group",
                            "op": "and",
                            "children": [
                                {
                                    "kind": "rule",
                                    "condition": {
                                        "field": "item_type",
                                        "op": "in",
                                        "values": ["movie"]
                                    }
                                },
                                {
                                    "kind": "rule",
                                    "condition": {
                                        "field": "rating",
                                        "op": "gte",
                                        "value": 7.5
                                    }
                                }
                            ]
                        }
                    }
                }
            },
            {
                "type": "carousel",
                "id": "unwatched_recent",
                "title": "Unwatched Discoveries",
                "binding": {
                    "macro_type": "recently_added",
                    "limit": 10,
                    "filters": {
                        "rule_tree": {
                            "kind": "rule",
                            "condition": {
                                "field": "watch_state",
                                "op": "state_in",
                                "values": ["unwatched"]
                            }
                        }
                    }
                }
            }
        ]
    });
    let (status, _) = send_json(
        &env.app,
        "PUT",
        "/api/v1/screens/home",
        Some(&env.admin_jwt),
        None,
        Some(valid_screen),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 3. Hydrate GET /api/v1/screens/home as standard user
    let (status, hydrated_home) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.user_jwt),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let layout: ScreenLayout = serde_json::from_value(hydrated_home).unwrap();
    assert_eq!(layout.id, ScreenId::Home);
    assert_eq!(layout.widgets.len(), 2);

    match &layout.widgets[0] {
        WidgetNode::Grid {
            id,
            items,
            next_cursor,
            total_count,
            ..
        } => {
            assert_eq!(id, "acclaimed_movies");
            let cards = items.as_ref().unwrap();
            assert_eq!(cards.len(), 1);
            assert_eq!(cards[0].title, "Arrival");
            assert_eq!(*total_count, Some(2));
            assert_eq!(next_cursor.as_deref(), Some("1"));
        }
        other => panic!("Expected Grid widget, got {other:?}"),
    }

    // Standard user already completed "Arrival", so unwatched_recent contains only "Dark" & "Parasite"
    match &layout.widgets[1] {
        WidgetNode::Carousel { id, items, .. } => {
            assert_eq!(id, "unwatched_recent");
            let titles: Vec<&str> = items
                .as_ref()
                .unwrap()
                .iter()
                .map(|c| c.title.as_str())
                .collect();
            assert_eq!(titles, vec!["Dark", "Parasite"]);
        }
        other => panic!("Expected Carousel widget, got {other:?}"),
    }

    // 4. Paginate custom_query widget via GET /api/v1/widgets/acclaimed_movies/data?offset=1&limit=1&screen_id=home
    let (status, page2) = send_json(
        &env.app,
        "GET",
        "/api/v1/widgets/acclaimed_movies/data?offset=1&limit=1&screen_id=home",
        Some(&env.user_jwt),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page2["widget_id"], "acclaimed_movies");
    assert_eq!(page2["total_count"], 2);
    assert!(page2["next_cursor"].is_null());
    let page2_items = page2["items"].as_array().unwrap();
    assert_eq!(page2_items.len(), 1);
    assert_eq!(page2_items[0]["title"], "Parasite");

    // Verify WidgetQueryBinding round-trips cleanly
    let (status, raw_home) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home?unhydrated=true",
        Some(&env.user_jwt),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let raw_layout: ScreenLayout = serde_json::from_value(raw_home).unwrap();
    let binding: &WidgetQueryBinding = raw_layout.widgets[0].binding().unwrap();
    assert_eq!(binding.macro_type, QueryMacro::CustomQuery);
    let filter_cfg: &WidgetFilterConfig = binding.filters.as_ref().unwrap();
    assert!(filter_cfg.rule_tree.is_some());
}

#[tokio::test]
async fn widget_preview_1000_synthetic_items_executes_under_50ms() {
    let env = setup_filter_test_env().await;

    let genres_pool = ["Sci-Fi", "Drama", "Action", "Comedy", "Horror"];
    let countries_pool = ["US", "KR", "JP", "FR", "GB"];

    let mut synthetic = Vec::with_capacity(1000);
    for i in 0..1000usize {
        let g1 = genres_pool[i % genres_pool.len()];
        let g2 = genres_pool[(i + 2) % genres_pool.len()];
        let country = countries_pool[i % countries_pool.len()];
        let year = 1980 + (i % 46) as i32;
        let duration = 3600 + ((i % 120) as i64 * 60);
        let rating = 5.0 + ((i % 45) as f32 * 0.1);

        synthetic.push(MediaItem {
            library_id: "movies".into(),
            item_type: MediaType::Movie,
            title: format!("Synthetic Film {i:04}"),
            release_year: Some(year),
            added_at: 1_700_100_000 + i as i64,
            file_path: PathBuf::from(format!("/media/movies/synthetic_{i:04}.mkv")),
            file_name: format!("synthetic_{i:04}.mkv"),
            technical: TechnicalInfo {
                duration_seconds: duration,
                resolution: Some(if i % 3 == 0 { "4K" } else { "1080p" }.into()),
                video_codec: Some(if i % 2 == 0 { "hevc" } else { "h264" }.into()),
                container: Some("mkv".into()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                country: Some(country.into()),
                genres: vec![g1.into(), g2.into()],
                rating: Some(rating),
                ..Default::default()
            },
            ..Default::default()
        });
    }
    env.media_repo.upsert_batch(&synthetic).await.unwrap();

    let preview_payload = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 20,
            "sort": "rating:desc",
            "filters": {
                "rule_tree": {
                    "kind": "group",
                    "op": "and",
                    "children": [
                        {
                            "kind": "rule",
                            "condition": {
                                "field": "genres",
                                "op": "contains_any",
                                "values": ["Sci-Fi", "Action"]
                            }
                        },
                        {
                            "kind": "rule",
                            "condition": {
                                "field": "country",
                                "op": "in",
                                "values": ["US", "KR", "JP"]
                            }
                        },
                        {
                            "kind": "rule",
                            "condition": {
                                "field": "release_year",
                                "op": "between",
                                "min": 2000,
                                "max": 2024
                            }
                        },
                        {
                            "kind": "rule",
                            "condition": {
                                "field": "duration_seconds",
                                "op": "gte",
                                "value": 5400
                            }
                        },
                        {
                            "kind": "rule",
                            "negated": true,
                            "condition": {
                                "field": "genres",
                                "op": "contains_any",
                                "values": ["Horror"]
                            }
                        }
                    ]
                }
            }
        },
        "limit": 20
    });

    let (status, body) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(preview_payload),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let total_count = body["total_count"].as_u64().unwrap();
    let execution_ms = body["execution_ms"].as_u64().unwrap();
    let items = body["items"].as_array().unwrap();

    assert!(total_count > 0, "Expected matching synthetic items");
    assert!(!items.is_empty() && items.len() <= 20);
    assert!(
        execution_ms < 50,
        "Expected preview execution_ms < 50ms on 1,000 items in debug mode, got {execution_ms}ms"
    );
}

#[tokio::test]
async fn continue_watching_and_canonical_library_screen_preview_apply_rule_tree() {
    let env = setup_filter_test_env().await;

    let arrival = env
        .media_repo
        .find_by_path("/media/movies/arrival.mkv")
        .await
        .unwrap()
        .unwrap();
    let parasite = env
        .media_repo
        .find_by_path("/media/movies/parasite.mp4")
        .await
        .unwrap()
        .unwrap();

    // Admin has both Arrival (4K) and Parasite (1080p) in progress
    env.playback_repo
        .upsert_progress(
            "u-admin",
            arrival.id.unwrap(),
            2000,
            WatchState::InProgress,
            1_700_002_000,
        )
        .await
        .unwrap();
    env.playback_repo
        .upsert_progress(
            "u-admin",
            parasite.id.unwrap(),
            2000,
            WatchState::InProgress,
            1_700_003_000,
        )
        .await
        .unwrap();

    // Preview ContinueWatching filtered to resolution == "4K" -> only Arrival matches
    let cw_preview = json!({
        "binding": {
            "macro_type": "continue_watching",
            "limit": 10,
            "filters": {
                "rule_tree": {
                    "kind": "rule",
                    "condition": {
                        "field": "resolution",
                        "op": "eq",
                        "value": "4K"
                    }
                }
            }
        }
    });
    let (status, cw_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(cw_preview),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cw_items = cw_body["items"].as_array().unwrap();
    assert_eq!(cw_items.len(), 1);
    assert_eq!(cw_items[0]["title"], "Arrival");

    // SpotlightItem explicit item_id with rule_tree that excludes 1080p -> empty
    let spotlight_preview = json!({
        "binding": {
            "macro_type": {
                "spotlight_item": { "item_id": parasite.id.unwrap() }
            },
            "limit": 1,
            "filters": {
                "rule_tree": {
                    "kind": "rule",
                    "condition": {
                        "field": "resolution",
                        "op": "eq",
                        "value": "4K"
                    }
                }
            }
        }
    });
    let (status, sp_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_jwt),
        None,
        Some(spotlight_preview),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sp_body["items"].as_array().unwrap().len(), 0);

    // Custom canonical library screen mapping: library "lib-indie-uuid" ("Indie Cinema")
    // paired with custom screen "indie_hub" titled "Indie Cinema"
    env.lib_repo
        .create(
            "lib-indie-uuid",
            "Indie Cinema",
            "/media/indie",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();
    env.media_repo
        .upsert_batch(&[MediaItem {
            library_id: "lib-indie-uuid".into(),
            item_type: MediaType::Movie,
            title: "Past Lives".into(),
            release_year: Some(2023),
            added_at: 1_700_005_000,
            file_path: PathBuf::from("/media/indie/past_lives.mkv"),
            file_name: "past_lives.mkv".into(),
            technical: TechnicalInfo {
                duration_seconds: 6360,
                resolution: Some("4K".into()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                genres: vec!["Drama".into()],
                rating: Some(8.4),
                ..Default::default()
            },
            ..Default::default()
        }])
        .await
        .unwrap();

    let indie_binding = json!({
        "macro_type": "custom_query",
        "limit": 10,
        "filters": {
            "rule_tree": {
                "kind": "rule",
                "condition": {
                    "field": "genres",
                    "op": "contains_any",
                    "values": ["Drama"]
                }
            }
        }
    });

    let (status, _) = send_json(
        &env.app,
        "PUT",
        "/api/v1/screens/indie_hub",
        Some(&env.admin_jwt),
        None,
        Some(json!({
            "id": "indie_hub",
            "title": "Indie Cinema",
            "widgets": [
                {
                    "type": "grid",
                    "id": "indie_drama",
                    "title": "Indie Drama",
                    "columns": 4,
                    "binding": indie_binding
                }
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 1. Live preview matches total_count == 1 ("Past Lives") for both canonical and UUID screen_ids
    for target_sid in ["indie_hub", "lib-indie-uuid"] {
        let (status, indie_res) = send_json(
            &env.app,
            "POST",
            "/api/v1/widgets/preview",
            Some(&env.admin_jwt),
            None,
            Some(json!({
                "binding": indie_binding,
                "screen_id": target_sid
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(indie_res["total_count"], 1);
        assert_eq!(indie_res["items"][0]["title"], "Past Lives");
    }

    // 2. Hydrated GET /api/v1/screens/{screen_id} matches preview (total_count == 1, "Past Lives")
    for screen_path in [
        "/api/v1/screens/indie_hub",
        "/api/v1/screens/lib-indie-uuid",
    ] {
        let (status, screen_body) = send_json(
            &env.app,
            "GET",
            screen_path,
            Some(&env.user_jwt),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let screen_layout: ScreenLayout = serde_json::from_value(screen_body).unwrap();
        assert_eq!(screen_layout.widgets.len(), 1);
        match &screen_layout.widgets[0] {
            WidgetNode::Grid {
                id,
                items,
                total_count,
                ..
            } => {
                assert_eq!(id, "indie_drama");
                assert_eq!(
                    *total_count,
                    Some(1),
                    "GET {screen_path} must scope to Indie Cinema library"
                );
                let cards = items.as_ref().unwrap();
                assert_eq!(cards.len(), 1);
                assert_eq!(cards[0].title, "Past Lives");
            }
            other => panic!("Expected Grid widget, got {other:?}"),
        }
    }

    // 3. Paginated GET /api/v1/widgets/indie_drama/data matches preview across screen_id variants
    for data_uri in [
        "/api/v1/widgets/indie_drama/data?screen_id=indie_hub",
        "/api/v1/widgets/indie_drama/data?screen_id=lib-indie-uuid",
        "/api/v1/widgets/indie_drama/data",
    ] {
        let (status, data_body) =
            send_json(&env.app, "GET", data_uri, Some(&env.user_jwt), None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            data_body["total_count"], 1,
            "GET {data_uri} must scope to Indie Cinema library"
        );
        let data_items = data_body["items"].as_array().unwrap();
        assert_eq!(data_items.len(), 1);
        assert_eq!(data_items[0]["title"], "Past Lives");
    }
}
