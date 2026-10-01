use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use kadr_core::ast::{ItemDetailsPayload, ScreenId, ScreenLayout, WidgetNode};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::create_router_with_layout;
use kadr_server::api::screen_routes::ScreenSummary;
use kadr_server::api::widget_routes::WidgetDataResponse;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};
use serde_json::Value;
use tempfile::tempdir;
use tower::ServiceExt;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[tokio::test]
async fn test_milestone_3_end_to_end_widget_ast_user_journey() {
    // 0. Clean test environment (temporary SQLite db with all migrations 001, 002, 003)
    let temp_dir = tempdir().expect("Failed to create temporary directory");
    let db_path = temp_dir.path().join("test_kadr_m3_e2e.db");
    let pool = create_pool(&db_path, 4).expect("Failed to create SQLite file pool");
    initialize_database(&pool)
        .await
        .expect("Failed to initialize database migrations");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());

    // Prepare dummy media files and artwork on disk
    let poster_path = temp_dir.path().join("dune_poster.jpg");
    let mut poster_file = File::create(&poster_path).expect("Failed to create poster file");
    let poster_bytes: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x01, 0x02];
    poster_file
        .write_all(&poster_bytes)
        .expect("Failed to write poster bytes");

    let backdrop_path = temp_dir.path().join("dune_backdrop.jpg");
    let mut backdrop_file = File::create(&backdrop_path).expect("Failed to create backdrop file");
    let backdrop_bytes: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    backdrop_file
        .write_all(&backdrop_bytes)
        .expect("Failed to write backdrop bytes");

    let movie_file = temp_dir.path().join("dune2.mkv");
    File::create(&movie_file).expect("Failed to create movie file");

    let ep_file = temp_dir.path().join("S01E01.mkv");
    File::create(&ep_file).expect("Failed to create episode file");

    let now = now_secs();

    // Register libraries
    lib_repo
        .create(&Library {
            id: "lib-movies".to_string(),
            name: "Movies".to_string(),
            path: temp_dir.path().join("movies"),
            media_type: MediaType::Movie,
            created_at: now - 1000,
        })
        .await
        .expect("Failed to create movies library");

    lib_repo
        .create(&Library {
            id: "lib-shows".to_string(),
            name: "Shows".to_string(),
            path: temp_dir.path().join("shows"),
            media_type: MediaType::Show,
            created_at: now - 1000,
        })
        .await
        .expect("Failed to create shows library");

    // Insert Movie item with poster/backdrop/rating/genres and 4K resolution
    // Note: added_at is set to 30 days ago so badge falls back to "4K" rather than "NEW"
    let movie_item = MediaItem {
        id: None,
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        title: "Dune: Part Two".to_string(),
        original_title: Some("Dune: Part Two".to_string()),
        release_year: Some(2024),
        added_at: now - 30 * 86400,
        file_path: movie_file,
        file_name: "dune2.mkv".to_string(),
        file_size: 15_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 7200,
            resolution: Some("4K".to_string()),
            video_codec: Some("hevc".to_string()),
            audio_codec: Some("eac3".to_string()),
            audio_channels: Some(6),
            container: Some("mkv".to_string()),
        },
        metadata: MediaMetadata {
            overview: Some(
                "Paul Atreides unites with Chani and the Fremen while seeking revenge.".to_string(),
            ),
            poster_path: Some(poster_path.to_str().unwrap().to_string()),
            backdrop_path: Some(backdrop_path.to_str().unwrap().to_string()),
            rating: Some(9.2),
            genres: vec!["Sci-Fi".to_string(), "Action".to_string()],
            ..Default::default()
        },
    };

    // Insert Series show item
    let show_item = MediaItem {
        id: None,
        library_id: "lib-shows".to_string(),
        item_type: MediaType::Show,
        title: "Severance".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: now - 200,
        file_path: temp_dir.path().join("shows/Severance"),
        file_name: "Severance".to_string(),
        file_size: 0,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            overview: Some(
                "Mark leads a team of office workers whose memories have been divided.".to_string(),
            ),
            rating: Some(8.7),
            genres: vec!["Drama".to_string(), "Sci-Fi".to_string()],
            ..Default::default()
        },
    };

    // Insert Series Episode item (added within last 14 days => badge "NEW")
    let episode_item = MediaItem {
        id: None,
        library_id: "lib-shows".to_string(),
        item_type: MediaType::Episode,
        title: "Good News About Hell".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: now - 100,
        file_path: ep_file,
        file_name: "S01E01.mkv".to_string(),
        file_size: 2_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 3600,
            resolution: Some("1080p".to_string()),
            container: Some("mkv".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata {
            series_title: Some("Severance".to_string()),
            season: Some(1),
            episode: Some(1),
            rating: Some(8.8),
            genres: vec!["Drama".to_string()],
            ..Default::default()
        },
    };

    media_repo
        .upsert_batch(&[movie_item, show_item, episode_item])
        .await
        .expect("Failed to upsert test media items");

    let movies = media_repo
        .list_by_library("lib-movies", 10, 0)
        .await
        .expect("Failed to list movies");
    let movie_id = movies[0].id.expect("Movie ID should be assigned");

    let shows = media_repo
        .list_by_library("lib-shows", 10, 0)
        .await
        .expect("Failed to list shows");
    let show_id = shows
        .iter()
        .find(|s| s.item_type == MediaType::Show)
        .unwrap()
        .id
        .unwrap();

    // Bootstrap initial admin user with PIN 1234
    let admin_user = User {
        id: "admin-uid".to_string(),
        username: "admin".to_string(),
        pin_hash: hash_pin("1234").expect("Failed to hash PIN"),
        role: UserRole::Admin,
        created_at: now,
    };
    user_repo
        .create(&admin_user)
        .await
        .expect("Failed to create admin user");

    // Initialize server dependencies and router
    let jwt_svc = JwtService::new("milestone-3-e2e-super-secret-jwt-key-32bytes", 3600);
    let rate_limiter = RateLimiter::new(10, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));

    let app = create_router_with_layout(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry,
        widget_resolver,
    );

    // =========================================================================
    // User Journey Step 1: Authenticate with PIN 1234 to acquire JWT token
    // =========================================================================
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/profile-pin")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"user_id":"admin-uid","pin":"1234"}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let auth_data: Value = serde_json::from_slice(&body).unwrap();
    let token = auth_data["token"].as_str().expect("Token should be returned").to_string();

    // =========================================================================
    // User Journey Step 2: GET /api/v1/screens -> verify screen list contains home, movies, shows
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 16).await.unwrap();
    let screens: Vec<ScreenSummary> = serde_json::from_slice(&body).unwrap();
    let screen_ids: Vec<String> = screens.into_iter().map(|s| s.id).collect();
    assert!(screen_ids.contains(&"home".to_string()));
    assert!(screen_ids.contains(&"movies".to_string()));
    assert!(screen_ids.contains(&"shows".to_string()));

    // =========================================================================
    // User Journey Step 3: GET /api/v1/screens/home -> verify hydrated AST
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let home_layout: ScreenLayout = serde_json::from_slice(&body).unwrap();

    assert_eq!(home_layout.id, ScreenId::Home);
    assert_eq!(home_layout.title, "Home");

    // Verify HeroBanner is populated with spotlight movie data
    let hero = home_layout
        .widgets
        .iter()
        .find(|w| w.id() == "home_spotlight")
        .expect("home_spotlight hero banner missing");
    match hero {
        WidgetNode::HeroBanner { data, .. } => {
            let hero_card = data.as_ref().expect("HeroBanner card should be populated");
            assert_eq!(hero_card.id, movie_id);
            assert_eq!(hero_card.title, "Dune: Part Two");
            assert_eq!(hero_card.badge.as_deref(), Some("4K"));
        }
        _ => panic!("home_spotlight is not a HeroBanner"),
    }

    // Verify Recently Added carousel contains media cards with badges (NEW, 4K)
    let recently_added = home_layout
        .widgets
        .iter()
        .find(|w| w.id() == "recently_added")
        .expect("recently_added carousel missing");
    match recently_added {
        WidgetNode::Carousel { items, .. } => {
            let cards = items
                .as_ref()
                .expect("recently_added carousel should be populated");
            assert!(!cards.is_empty());
            let badges: Vec<Option<String>> = cards.iter().map(|c| c.badge.clone()).collect();
            assert!(
                badges.contains(&Some("NEW".to_string())),
                "Recently Added should contain card with badge NEW"
            );
            assert!(
                badges.contains(&Some("4K".to_string())),
                "Recently Added should contain card with badge 4K"
            );
        }
        _ => panic!("recently_added is not a Carousel"),
    }

    // Verify Continue Watching carousel is initially empty
    let continue_watching = home_layout
        .widgets
        .iter()
        .find(|w| w.id() == "continue_watching")
        .expect("continue_watching carousel missing");
    match continue_watching {
        WidgetNode::Carousel { items, .. } => {
            let cards = items
                .as_ref()
                .expect("continue_watching carousel should be populated");
            assert!(cards.is_empty(), "Continue Watching should initially be empty");
        }
        _ => panic!("continue_watching is not a Carousel"),
    }

    // =========================================================================
    // User Journey Step 4: Post playback progress for the movie (120s out of 7200s, >60s => InProgress)
    // =========================================================================
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/playback/sessions")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"media_item_id":{movie_id}}}"#)))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let sess_data: Value = serde_json::from_slice(&body).unwrap();
    let session_id = sess_data["session_id"]
        .as_str()
        .expect("Session ID should be returned");

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/playback/{session_id}/progress"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"position_seconds":120}"#))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // =========================================================================
    // User Journey Step 5: Re-fetch GET /api/v1/screens/home -> verify Continue Watching has movie with progress & RESUME
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let updated_home: ScreenLayout = serde_json::from_slice(&body).unwrap();

    let updated_cw = updated_home
        .widgets
        .iter()
        .find(|w| w.id() == "continue_watching")
        .expect("continue_watching carousel missing");
    match updated_cw {
        WidgetNode::Carousel { items, .. } => {
            let cards = items
                .as_ref()
                .expect("continue_watching items should be populated");
            assert_eq!(cards.len(), 1, "Continue Watching should contain 1 movie");
            let movie_card = &cards[0];
            assert_eq!(movie_card.id, movie_id);
            assert_eq!(movie_card.title, "Dune: Part Two");
            assert_eq!(movie_card.badge.as_deref(), Some("RESUME"));
            let progress = movie_card
                .playback_progress
                .expect("playback_progress should be calculated");
            // 120 / 7200 = 0.016666...
            assert!(
                (progress - (120.0 / 7200.0)).abs() < 0.001,
                "Expected playback progress near 0.0166, got {progress}"
            );
        }
        _ => panic!("continue_watching is not a Carousel"),
    }

    // =========================================================================
    // User Journey Step 6: GET /api/v1/screens/home?unhydrated=true -> verify unhydrated AST returns widget structure without data
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/screens/home?unhydrated=true")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    let unhydrated: ScreenLayout = serde_json::from_slice(&body).unwrap();

    assert_eq!(unhydrated.id, ScreenId::Home);
    assert!(!unhydrated.widgets.is_empty());
    for widget in &unhydrated.widgets {
        assert!(
            !widget.is_hydrated(),
            "Widget '{}' should NOT be hydrated when unhydrated=true",
            widget.id()
        );
    }

    // =========================================================================
    // User Journey Step 7: GET /api/v1/widgets/recently_added/data?offset=0&limit=1 -> verify paginated widget response
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/widgets/recently_added/data?offset=0&limit=1")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 32).await.unwrap();
    let widget_resp: WidgetDataResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(widget_resp.widget_id, "recently_added");
    assert_eq!(widget_resp.items.len(), 1);
    assert!(
        widget_resp.next_cursor.is_some(),
        "next_cursor must be populated when there are more items"
    );
    let next_cursor = widget_resp.next_cursor.unwrap();
    assert!(
        next_cursor.contains("offset=1"),
        "next_cursor should point to offset=1, got {next_cursor}"
    );

    // =========================================================================
    // User Journey Step 8: GET /api/v1/items/{item_id}/details -> verify complete inspection payload
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{movie_id}/details"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 32).await.unwrap();
    let details: ItemDetailsPayload = serde_json::from_slice(&body).unwrap();

    assert_eq!(details.card.id, movie_id);
    assert_eq!(details.card.title, "Dune: Part Two");
    assert_eq!(
        details.overview.as_deref(),
        Some("Paul Atreides unites with Chani and the Fremen while seeking revenge.")
    );
    assert_eq!(details.genres, vec!["Sci-Fi", "Action"]);
    assert_eq!(details.duration_seconds, Some(7200));
    assert_eq!(details.resume_position_seconds, Some(120));
    assert_eq!(details.stream_url, format!("/api/v1/stream/{movie_id}"));
    let tech = details.technical.expect("Technical info should be present");
    assert_eq!(tech.resolution.as_deref(), Some("4K"));
    assert_eq!(tech.video_codec.as_deref(), Some("hevc"));

    // Also verify TV show details populate child episodes
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/items/{show_id}/details"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 32).await.unwrap();
    let show_details: ItemDetailsPayload = serde_json::from_slice(&body).unwrap();
    assert_eq!(show_details.card.id, show_id);
    assert_eq!(show_details.card.title, "Severance");
    let ep_cards = show_details
        .episodes
        .expect("Show episodes should be present");
    assert_eq!(ep_cards.len(), 1);
    assert_eq!(ep_cards[0].title, "Good News About Hell");

    // =========================================================================
    // User Journey Step 9: GET /api/v1/artwork/{item_id}/poster -> verify 200, Content-Type, Cache-Control
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/artwork/{movie_id}/poster"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get("content-type")
            .expect("content-type header"),
        "image/jpeg"
    );
    assert_eq!(
        res.headers()
            .get("cache-control")
            .expect("cache-control header"),
        "public, max-age=86400"
    );
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body.as_ref(), &poster_bytes[..]);

    // =========================================================================
    // User Journey Step 10: GET /api/v1/artwork/{item_id}/backdrop -> verify 200 with matching bytes
    // =========================================================================
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/artwork/{movie_id}/backdrop"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get("content-type")
            .expect("content-type header"),
        "image/jpeg"
    );
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(body.as_ref(), &backdrop_bytes[..]);
}
