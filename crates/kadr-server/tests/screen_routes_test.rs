use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use kadr_core::ast::{
    ItemDetailsPayload, QueryMacro, ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding,
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_server::api::create_router_with_layout;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use serde_json::{json, Value};
use tower::ServiceExt;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

struct TestContext {
    app: axum::Router,
    token: String,
    admin_token: String,
    spotlight_id: i64,
    show_id: i64,
    lib_repo: LibraryRepository,
}

async fn setup_test_app() -> TestContext {
    let pool = create_in_memory_pool().expect("failed to create in-memory pool");
    initialize_database(&pool)
        .await
        .expect("failed to run migrations");

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());

    let user = User {
        id: "user-test".to_string(),
        username: "tester".to_string(),
        pin_hash: "hash".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    user_repo.create(&user).await.expect("create user failed");

    lib_repo
        .insert(&Library {
            id: "movies".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/media/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "shows".to_string(),
            name: "Shows".to_string(),
            path: PathBuf::from("/media/shows"),
            media_type: MediaType::Show,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let now = now_secs();
    let mut items = vec![
        // Spotlight Movie
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Interstellar".to_string(),
            original_title: None,
            release_year: Some(2014),
            added_at: now - 30 * 86400,
            file_path: PathBuf::from("/media/movies/interstellar.mkv"),
            file_name: "interstellar.mkv".to_string(),
            file_size: 20_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 10140,
                resolution: Some("4K".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(6),
                container: Some("mkv".to_string()),
            },
            metadata: MediaMetadata {
                overview: Some("Humanity explores the stars.".to_string()),
                backdrop_path: Some("/artwork/interstellar_backdrop.jpg".to_string()),
                poster_path: Some("/artwork/interstellar_poster.jpg".to_string()),
                rating: Some(9.5),
                genres: vec!["Sci-Fi".to_string(), "Adventure".to_string()],
                ..Default::default()
            },
        },
        // In-progress movie
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Dune".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: now - 200,
            file_path: PathBuf::from("/media/movies/dune.mkv"),
            file_name: "dune.mkv".to_string(),
            file_size: 15_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 9000,
                resolution: Some("1080p".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                overview: Some("Desert planet adventure.".to_string()),
                rating: Some(8.5),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
        },
        // Show
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Show,
            title: "Severance".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now - 300,
            file_path: PathBuf::from("/media/shows/Severance"),
            file_name: "Severance".to_string(),
            file_size: 0,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                overview: Some("Work-life balance taken literally.".to_string()),
                genres: vec!["Drama".to_string(), "Sci-Fi".to_string()],
                ..Default::default()
            },
        },
        // Episode 1
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Severance - S01E01 - Good News About Hell".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now - 290,
            file_path: PathBuf::from("/media/shows/Severance/S01E01.mkv"),
            file_name: "S01E01.mkv".to_string(),
            file_size: 2_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 3400,
                resolution: Some("1080p".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(1),
                ..Default::default()
            },
        },
        // Episode 2
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Severance - S01E02 - Half Loop".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now - 280,
            file_path: PathBuf::from("/media/shows/Severance/S01E02.mkv"),
            file_name: "S01E02.mkv".to_string(),
            file_size: 2_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 3200,
                resolution: Some("1080p".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(2),
                ..Default::default()
            },
        },
    ];

    // Additional 10 movies for pagination testing
    for i in 1..=10 {
        items.push(MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: format!("Extra Movie {i}"),
            original_title: None,
            release_year: Some(2020 + i),
            added_at: now - 1000 - (i as i64 * 10),
            file_path: PathBuf::from(format!("/media/movies/extra_{i}.mkv")),
            file_name: format!("extra_{i}.mkv"),
            file_size: 1_000_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                genres: vec!["Action".to_string()],
                rating: Some(7.0 + (i as f32 * 0.1)),
                ..Default::default()
            },
        });
    }

    media_repo
        .upsert_batch(&items)
        .await
        .expect("upsert failed");

    let spotlight = media_repo
        .find_spotlight_candidate(&[], None)
        .await
        .unwrap()
        .expect("spotlight item not found");
    let spotlight_id = spotlight.id.unwrap();

    let all_movies = media_repo.list_by_library("movies", 20, 0).await.unwrap();
    let dune = all_movies.iter().find(|m| m.title == "Dune").unwrap();
    let dune_id = dune.id.unwrap();

    let all_shows = media_repo.list_by_library("shows", 20, 0).await.unwrap();
    let show = all_shows.iter().find(|m| m.title == "Severance").unwrap();
    let show_id = show.id.unwrap();

    // Set playback in-progress for user-test on Dune (50%)
    playback_repo
        .upsert_progress("user-test", dune_id, 4500, WatchState::InProgress, now)
        .await
        .unwrap();

    let admin_user = User {
        id: "admin-test".to_string(),
        username: "admin".to_string(),
        pin_hash: "adminhash".to_string(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo.create(&admin_user).await.expect("create admin failed");

    let jwt_svc = JwtService::new("super-secret-jwt-key-with-at-least-32-bytes", 3600);
    let token = jwt_svc.generate_token(&user).unwrap();
    let admin_token = jwt_svc.generate_token(&admin_user).unwrap();

    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));
    let session_registry = Arc::new(SessionRegistry::new());
    let limiter = RateLimiter::new(10, Duration::from_secs(60), Duration::from_secs(60));

    let app = create_router_with_layout(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo.clone(),
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
    );

    TestContext {
        app,
        token,
        admin_token,
        spotlight_id,
        show_id,
        lib_repo: lib_repo.clone(),
    }
}

#[tokio::test]
async fn test_unauthenticated_requests_return_401() {
    let ctx = setup_test_app().await;

    let unauth_endpoints = vec![
        ("/api/v1/screens", "GET"),
        ("/api/v1/screens/home", "GET"),
        ("/api/v1/widgets/recently_added/data", "GET"),
        ("/api/v1/items/1/details", "GET"),
    ];

    for (uri, method) in unauth_endpoints {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let res = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "Endpoint {uri} should require authentication"
        );
    }
}

#[tokio::test]
async fn test_get_screens_summary_list() {
    let ctx = setup_test_app().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.is_array());

    let screens = json.as_array().unwrap();
    assert_eq!(screens.len(), 3);

    assert_eq!(screens[0]["id"], "home");
    assert_eq!(screens[0]["title"], "Home");
    assert_eq!(screens[1]["id"], "movies");
    assert_eq!(screens[1]["title"], "Movies");
    assert_eq!(screens[2]["id"], "shows");
    assert_eq!(screens[2]["title"], "TV Shows");
}

#[tokio::test]
async fn test_get_screen_hydrated_and_unhydrated() {
    let ctx = setup_test_app().await;

    // 1. Authenticated GET /api/v1/screens/home (hydrated)
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64)
        .await
        .unwrap();
    let layout: ScreenLayout = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(layout.id, ScreenId::Home);
    assert_eq!(layout.title, "Home");
    assert!(!layout.widgets.is_empty());

    // Verify hero banner is hydrated
    let hero = layout
        .widgets
        .iter()
        .find(|w| w.id() == "home_spotlight")
        .expect("hero banner missing");
    match hero {
        WidgetNode::HeroBanner { data, .. } => {
            let card = data.as_ref().expect("hero card not populated");
            assert_eq!(card.id, ctx.spotlight_id);
            assert_eq!(card.title, "Interstellar");
            assert_eq!(card.badge.as_deref(), Some("4K"));
        }
        _ => panic!("home_spotlight is not HeroBanner"),
    }

    // Verify continue_watching carousel is hydrated
    let cont = layout
        .widgets
        .iter()
        .find(|w| w.id() == "continue_watching")
        .expect("continue watching missing");
    match cont {
        WidgetNode::Carousel { items, .. } => {
            let cards = items.as_ref().expect("continue items not populated");
            assert_eq!(cards.len(), 1);
            assert_eq!(cards[0].title, "Dune");
            assert_eq!(cards[0].badge.as_deref(), Some("RESUME"));
        }
        _ => panic!("continue_watching is not Carousel"),
    }

    // 2. Authenticated GET /api/v1/screens/home?unhydrated=true
    let req_unhydrated = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home?unhydrated=true")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res_unhydrated = ctx.app.clone().oneshot(req_unhydrated).await.unwrap();
    assert_eq!(res_unhydrated.status(), StatusCode::OK);

    let bytes_unhydrated = axum::body::to_bytes(res_unhydrated.into_body(), 1024 * 64)
        .await
        .unwrap();
    let raw_layout: ScreenLayout = serde_json::from_slice(&bytes_unhydrated).unwrap();

    // Verify all widgets are unhydrated
    for widget in &raw_layout.widgets {
        assert!(
            !widget.is_hydrated(),
            "Widget {} should not be hydrated when unhydrated=true",
            widget.id()
        );
    }
}

#[tokio::test]
async fn test_get_widget_data_pagination() {
    let ctx = setup_test_app().await;

    // Page 1: limit 5
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/widgets/recently_added/data?offset=0&limit=5")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 32)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["widget_id"], "recently_added");
    let items = json["items"].as_array().expect("items should be an array");
    assert_eq!(items.len(), 5);

    let next_cursor = json["next_cursor"]
        .as_str()
        .expect("next_cursor should be present");
    assert_eq!(
        next_cursor,
        "/api/v1/widgets/recently_added/data?offset=5&limit=5"
    );

    // Page 2: follow next_cursor
    let req_page2 = Request::builder()
        .method("GET")
        .uri(next_cursor)
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res_page2 = ctx.app.clone().oneshot(req_page2).await.unwrap();
    assert_eq!(res_page2.status(), StatusCode::OK);

    let bytes_page2 = axum::body::to_bytes(res_page2.into_body(), 1024 * 32)
        .await
        .unwrap();
    let json_page2: Value = serde_json::from_slice(&bytes_page2).unwrap();
    let items_page2 = json_page2["items"].as_array().expect("items array page 2");
    assert_eq!(items_page2.len(), 5);

    // Items from page 1 and page 2 must be disjoint
    let id1: i64 = items[0]["id"].as_i64().unwrap();
    let id2: i64 = items_page2[0]["id"].as_i64().unwrap();
    assert_ne!(id1, id2, "Pages should contain different items");
}

#[tokio::test]
async fn test_get_item_details_movie_and_show() {
    let ctx = setup_test_app().await;

    // 1. Movie item details
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/details", ctx.spotlight_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16)
        .await
        .unwrap();
    let details: ItemDetailsPayload = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(details.card.id, ctx.spotlight_id);
    assert_eq!(details.card.title, "Interstellar");
    assert_eq!(
        details.overview.as_deref(),
        Some("Humanity explores the stars.")
    );
    assert_eq!(details.genres, vec!["Sci-Fi", "Adventure"]);
    assert_eq!(details.duration_seconds, Some(10140));
    assert_eq!(
        details.stream_url,
        format!("/api/v1/stream/{}", ctx.spotlight_id)
    );
    assert!(details.episodes.is_none());
    assert!(details.technical.is_some());
    assert_eq!(
        details.technical.as_ref().unwrap().resolution.as_deref(),
        Some("4K")
    );

    // 2. Show item details (with child episodes)
    let req_show = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{}/details", ctx.show_id))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res_show = ctx.app.clone().oneshot(req_show).await.unwrap();
    assert_eq!(res_show.status(), StatusCode::OK);

    let bytes_show = axum::body::to_bytes(res_show.into_body(), 1024 * 16)
        .await
        .unwrap();
    let show_details: ItemDetailsPayload = serde_json::from_slice(&bytes_show).unwrap();

    assert_eq!(show_details.card.id, ctx.show_id);
    assert_eq!(show_details.card.title, "Severance");
    let episodes = show_details.episodes.expect("episodes should be populated");
    assert_eq!(episodes.len(), 2);
}

#[tokio::test]
async fn test_not_found_cases() {
    let ctx = setup_test_app().await;

    // 1. Nonexistent screen
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/nonexistent_screen")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"], "Screen not found");

    // 2. Nonexistent widget
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/widgets/missing_widget/data")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"], "Widget not found");

    // 3. Nonexistent item
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/items/999999/details")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"], "Item not found");
}

#[tokio::test]
async fn test_save_and_reset_screen_layout() {
    let ctx = setup_test_app().await;

    // 1. Non-admin cannot mutate screens (403 Forbidden)
    let custom_home = ScreenLayout::new(
        ScreenId::Home,
        "Custom Home",
        vec![WidgetNode::Carousel {
            id: "my_carousel".to_string(),
            title: "My Carousel".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded),
            items: None,
            next_cursor: None,
        }],
    );

    let req_unauth = Request::builder()
        .method("PUT")
        .uri("/api/v1/screens/home")
        .header("authorization", format!("Bearer {}", ctx.token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&custom_home).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_unauth).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 2. Admin can PUT custom layout to /api/v1/screens/home
    let req_put = Request::builder()
        .method("PUT")
        .uri("/api/v1/screens/home")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&custom_home).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_put).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. GET /api/v1/screens/home to verify persisted changes
    let req_get = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home?unhydrated=true")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_get).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let updated: ScreenLayout = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(updated.title, "Custom Home");
    assert_eq!(updated.widgets.len(), 1);
    assert_eq!(updated.widgets[0].id(), "my_carousel");

    // 4. Admin DELETE /api/v1/screens/home resets to factory defaults
    let req_del = Request::builder()
        .method("DELETE")
        .uri("/api/v1/screens/home")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_del).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 5. GET /api/v1/screens/home verifies reset to default layout
    let req_get_reset = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home?unhydrated=true")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_get_reset).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let reset: ScreenLayout = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reset.title, "Home");
    assert_eq!(reset.widgets.len(), 6);

    // 6. Admin POST /api/v1/screens creates custom screen
    let create_payload = json!({
        "id": "anime",
        "title": "Anime Hub",
        "description": "Custom anime section"
    });
    let req_post = Request::builder()
        .method("POST")
        .uri("/api/v1/screens")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_post).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 7. GET /api/v1/screens lists the new custom screen
    let req_list = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_list).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let list: Value = serde_json::from_slice(&bytes).unwrap();
    let screens = list.as_array().unwrap();
    assert_eq!(screens.len(), 4);
    assert!(screens.iter().any(|s| s["id"] == "anime" && s["title"] == "Anime Hub"));

    // 8. Admin DELETE /api/v1/screens/anime deletes the custom screen
    let req_del_custom = Request::builder()
        .method("DELETE")
        .uri("/api/v1/screens/anime")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_del_custom).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 9. GET /api/v1/screens/anime returns 404
    let req_get_deleted = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/anime")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_get_deleted).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_custom_screen_put_serde_and_validation() {
    let ctx = setup_test_app().await;

    // 1. Path traversal / invalid screen ID validation on POST
    let invalid_post_ids = vec!["../bad_id", ".hidden", "bad id$", "bad/id", ""];
    for invalid_id in invalid_post_ids {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/screens")
            .header("authorization", format!("Bearer {}", ctx.admin_token))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "id": invalid_id,
                    "title": "Invalid ID"
                }))
                .unwrap(),
            ))
            .unwrap();
        let res = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "POST /api/v1/screens with id '{invalid_id}' should return 400"
        );
    }

    // 2. Invalid screen ID validation on PUT, GET, DELETE
    let req_put_invalid = Request::builder()
        .method("PUT")
        .uri("/api/v1/screens/.hidden")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "id": ".hidden",
                "title": "Invalid ID",
                "widgets": []
            }))
            .unwrap(),
        ))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_put_invalid).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let req_get_invalid = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/.hidden")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_get_invalid).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let req_del_invalid = Request::builder()
        .method("DELETE")
        .uri("/api/v1/screens/.hidden")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_del_invalid).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 3. PUT /api/v1/screens/anime with custom screen JSON
    let custom_anime = ScreenLayout::new(
        ScreenId::Custom("anime".to_string()),
        "Anime Hub",
        vec![WidgetNode::Carousel {
            id: "anime_spotlight".to_string(),
            title: "Trending Anime".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded),
            items: None,
            next_cursor: None,
        }],
    );

    // 4. Non-admin PUT returns 403
    let req_put_non_admin = Request::builder()
        .method("PUT")
        .uri("/api/v1/screens/anime")
        .header("authorization", format!("Bearer {}", ctx.token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&custom_anime).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_put_non_admin).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 5. Non-admin POST returns 403
    let req_post_non_admin = Request::builder()
        .method("POST")
        .uri("/api/v1/screens")
        .header("authorization", format!("Bearer {}", ctx.token))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "id": "anime",
                "title": "Anime Hub"
            }))
            .unwrap(),
        ))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_post_non_admin).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 6. Non-admin DELETE returns 403
    let req_del_non_admin = Request::builder()
        .method("DELETE")
        .uri("/api/v1/screens/anime")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_del_non_admin).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // 7. Admin PUT /api/v1/screens/anime succeeds (200 OK)
    let req_put_admin = Request::builder()
        .method("PUT")
        .uri("/api/v1/screens/anime")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&custom_anime).unwrap()))
        .unwrap();
    let res = ctx.app.clone().oneshot(req_put_admin).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 8. GET /api/v1/screens/anime returns string id "anime" and matching layout
    let req_get_anime = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/anime?unhydrated=true")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_get_anime).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let raw_json: Value = serde_json::from_slice(&bytes).unwrap();

    // Verify id is serializing as clean string "anime", NOT {"custom": "anime"}
    assert_eq!(raw_json["id"], "anime");
    assert_eq!(raw_json["title"], "Anime Hub");
    let layout: ScreenLayout = serde_json::from_value(raw_json).unwrap();
    assert_eq!(layout.id, ScreenId::Custom("anime".to_string()));
    assert_eq!(layout.widgets.len(), 1);
    assert_eq!(layout.widgets[0].id(), "anime_spotlight");

    // Clean up
    let req_del = Request::builder()
        .method("DELETE")
        .uri("/api/v1/screens/anime")
        .header("authorization", format!("Bearer {}", ctx.admin_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req_del).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dynamic_library_default_screen_generation() {
    let ctx = setup_test_app().await;

    let custom_lib_id = "c2033bde-ea09-481e-aa4a-eb4414d2820b";
    let custom_lib = Library {
        id: custom_lib_id.to_string(),
        name: "My Custom Library".to_string(),
        path: PathBuf::from("/media/custom"),
        media_type: MediaType::Movie,
        created_at: 1000,
        ..Default::default()
    };
    ctx.lib_repo.insert(&custom_lib).await.unwrap();

    // 1. GET /api/v1/screens/{custom_lib_id} should automatically resolve the default screen layout
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/screens/{custom_lib_id}"))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let layout: ScreenLayout = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(layout.id, ScreenId::Custom(custom_lib_id.to_string()));
    assert_eq!(layout.title, "My Custom Library");
    assert_eq!(layout.widgets.len(), 1);
    let widget_id = layout.widgets[0].id().to_string();
    assert_eq!(widget_id, format!("{custom_lib_id}_grid"));

    // 2. GET /api/v1/screens should now list the custom library screen
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let screens: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(screens
        .iter()
        .any(|s| s["id"] == custom_lib_id && s["title"] == "My Custom Library"));

    // 3. GET /api/v1/widgets/{widget_id}/data should return 200 OK
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/widgets/{widget_id}/data?screen_id={custom_lib_id}"
        ))
        .header("authorization", format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}



