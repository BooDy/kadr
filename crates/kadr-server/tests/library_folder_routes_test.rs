use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, User, UserRole};
use kadr_server::api::create_router_with_events;
use kadr_server::api::library_routes::LibraryFolderResponse;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

#[allow(dead_code)]
struct TestContext {
    app: axum::Router,
    standard_token: String,
    lib_repo: LibraryRepository,
    media_repo: MediaItemRepository,
    playback_repo: PlaybackRepository,
    temp_dir: tempfile::TempDir,
    public_lib_id: String,
    public_lib_root: std::path::PathBuf,
    private_lib_id: String,
}

async fn setup_test_context() -> TestContext {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("kadr_test.db");

    let pool = create_pool(&db_path, 2).expect("failed to create pool");
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

    let standard_user = User {
        id: "user-1".to_string(),
        username: "boody".to_string(),
        pin_hash: hash_pin("5678").expect("hash pin"),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo.create(&standard_user).await.expect("create standard");

    let jwt_svc = JwtService::new("test-secret-with-sufficient-entropy-for-hmac-sha256", 3600);
    let standard_token = jwt_svc
        .generate_token(&standard_user)
        .expect("generate standard token");

    let rate_limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));

    let temp_cache = dir.path().join("subtitles-cache");
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_cache,
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        db_path,
        session_registry.clone(),
        event_bus.clone(),
    ));

    // Setup filesystem for public library
    let public_lib_root = dir.path().join("public_movies");
    std::fs::create_dir_all(&public_lib_root).expect("create public root");

    let action_dir = public_lib_root.join("Action");
    std::fs::create_dir_all(&action_dir).expect("create Action dir");
    let action_sub_dir = action_dir.join("2020s");
    std::fs::create_dir_all(&action_sub_dir).expect("create Action/2020s dir");

    let drama_dir = public_lib_root.join("Drama");
    std::fs::create_dir_all(&drama_dir).expect("create Drama dir");

    // Files in root
    let root_movie = public_lib_root.join("RootMovie.mkv");
    std::fs::write(&root_movie, b"root movie bytes").expect("write root movie");

    // Hidden file in Action
    let hidden_file = action_dir.join(".DS_Store");
    std::fs::write(&hidden_file, b"hidden content").expect("write hidden file");

    // Files in Action
    let action_movie = action_dir.join("DieHard.mkv");
    std::fs::write(&action_movie, b"action movie bytes").expect("write action movie");

    // Files in Action/2020s
    let topgun_movie = action_sub_dir.join("TopGunMaverick.mp4");
    std::fs::write(&topgun_movie, b"topgun bytes").expect("write topgun movie");

    // Files in Drama
    let drama_movie = drama_dir.join("Casablanca.mkv");
    std::fs::write(&drama_movie, b"drama movie bytes").expect("write drama movie");

    // Setup filesystem for private library
    let private_lib_root = dir.path().join("private_movies");
    std::fs::create_dir_all(&private_lib_root).expect("create private root");
    let private_movie = private_lib_root.join("SecretDoc.mkv");
    std::fs::write(&private_movie, b"private movie bytes").expect("write private movie");

    // Create public library in repo
    let public_lib = Library {
        id: "lib-public-1".to_string(),
        name: "Public Movies".to_string(),
        path: public_lib_root.clone(),
        paths: vec![public_lib_root.clone()],
        media_type: MediaType::Movie,
        is_private: false,
        pin_hash: None,
        created_at: 1_700_000_000,
    };
    lib_repo.insert(&public_lib).await.expect("create public lib");

    // Create private library in repo
    let private_lib = Library {
        id: "lib-private-1".to_string(),
        name: "Private Movies".to_string(),
        path: private_lib_root.clone(),
        paths: vec![private_lib_root.clone()],
        media_type: MediaType::Movie,
        is_private: true,
        pin_hash: Some(hash_pin("1234").expect("hash pin")),
        created_at: 1_700_000_000,
    };
    lib_repo.insert(&private_lib).await.expect("create private lib");

    // Insert MediaItems for metadata enrichment
    let now = 1_700_000_000;
    let items = vec![
        MediaItem {
            id: None,
            library_id: public_lib.id.clone(),
            item_type: MediaType::Movie,
            title: "Root Movie".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now,
            file_path: root_movie,
            file_name: "RootMovie.mkv".to_string(),
            file_size: 1024,
            technical: kadr_core::models::TechnicalInfo {
                duration_seconds: 1000,
                ..Default::default()
            },
            metadata: MediaMetadata {
                rating: Some(8.5),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: public_lib.id.clone(),
            item_type: MediaType::Movie,
            title: "Die Hard".to_string(),
            original_title: None,
            release_year: Some(1988),
            added_at: now,
            file_path: action_movie,
            file_name: "DieHard.mkv".to_string(),
            file_size: 2048,
            technical: Default::default(),
            metadata: MediaMetadata {
                rating: Some(9.0),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: public_lib.id.clone(),
            item_type: MediaType::Movie,
            title: "Top Gun: Maverick".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now,
            file_path: topgun_movie,
            file_name: "TopGunMaverick.mp4".to_string(),
            file_size: 3072,
            technical: Default::default(),
            metadata: MediaMetadata {
                rating: Some(8.8),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: public_lib.id.clone(),
            item_type: MediaType::Movie,
            title: "Casablanca".to_string(),
            original_title: None,
            release_year: Some(1942),
            added_at: now,
            file_path: drama_movie,
            file_name: "Casablanca.mkv".to_string(),
            file_size: 4096,
            technical: Default::default(),
            metadata: MediaMetadata {
                rating: Some(9.5),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: private_lib.id.clone(),
            item_type: MediaType::Movie,
            title: "Secret Documentary".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: now,
            file_path: private_movie,
            file_name: "SecretDoc.mkv".to_string(),
            file_size: 512,
            technical: Default::default(),
            metadata: MediaMetadata {
                rating: Some(7.5),
                ..Default::default()
            },
        },
    ];
    media_repo.upsert_batch(&items).await.expect("upsert media items");

    let app = create_router_with_events(
        user_repo,
        playback_repo.clone(),
        media_repo.clone(),
        lib_repo.clone(),
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

    TestContext {
        app,
        standard_token,
        lib_repo,
        media_repo,
        playback_repo,
        temp_dir: dir,
        public_lib_id: public_lib.id,
        public_lib_root,
        private_lib_id: private_lib.id,
    }
}

#[tokio::test]
async fn test_browse_library_root() {
    let ctx = setup_test_context().await;

    // 1. GET /api/v1/libraries/{id}/folders returns root subdirectories and media items
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders", ctx.public_lib_id))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: LibraryFolderResponse = serde_json::from_slice(&body_bytes).expect("parse LibraryFolderResponse");

    assert_eq!(body.library_id, ctx.public_lib_id);
    assert_eq!(body.library_name, "Public Movies");
    assert_eq!(body.current_path, "");
    assert_eq!(body.parent_path, None);
    assert!(body.breadcrumbs.is_empty());

    // Root should have Action and Drama directories
    let dir_names: Vec<&str> = body.directories.iter().map(|d| d.name.as_str()).collect();
    assert!(dir_names.contains(&"Action"));
    assert!(dir_names.contains(&"Drama"));

    let action_entry = body.directories.iter().find(|d| d.name == "Action").unwrap();
    assert_eq!(action_entry.path, "Action");
    // Action has 2020s (dir) and DieHard.mkv (file) - hidden .DS_Store is filtered, so item_count is 2
    assert_eq!(action_entry.item_count, 2);

    // Root should have RootMovie.mkv enriched with DB metadata
    assert_eq!(body.items.len(), 1);
    assert_eq!(body.items[0].title, "Root Movie");
    assert_eq!(body.items[0].release_year, Some(2022));
    assert_eq!(body.items[0].rating, Some(8.5));
}

#[tokio::test]
async fn test_browse_library_subfolder_and_nested() {
    let ctx = setup_test_context().await;

    // 2. GET /api/v1/libraries/{id}/folders?path=Action returns subfolder contents with breadcrumbs
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders?path=Action", ctx.public_lib_id))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: LibraryFolderResponse = serde_json::from_slice(&body_bytes).expect("parse LibraryFolderResponse");

    assert_eq!(body.current_path, "Action");
    assert_eq!(body.parent_path, Some("".to_string()));
    assert_eq!(body.breadcrumbs.len(), 1);
    assert_eq!(body.breadcrumbs[0].name, "Action");
    assert_eq!(body.breadcrumbs[0].path, "Action");

    // Action has subdirectory 2020s
    assert_eq!(body.directories.len(), 1);
    assert_eq!(body.directories[0].name, "2020s");
    assert_eq!(body.directories[0].path, "Action/2020s");
    assert_eq!(body.directories[0].item_count, 1);

    // Action has media item Die Hard
    assert_eq!(body.items.len(), 1);
    assert_eq!(body.items[0].title, "Die Hard");
    assert_eq!(body.items[0].release_year, Some(1988));
    assert_eq!(body.items[0].rating, Some(9.0));

    // Nested subfolder: GET /api/v1/libraries/{id}/folders?path=Action/2020s
    let req_nested = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders?path=Action/2020s", ctx.public_lib_id))
        .body(Body::empty())
        .unwrap();

    let res_nested = ctx.app.clone().oneshot(req_nested).await.unwrap();
    assert_eq!(res_nested.status(), StatusCode::OK);

    let body_bytes = res_nested.into_body().collect().await.unwrap().to_bytes();
    let body_nested: LibraryFolderResponse = serde_json::from_slice(&body_bytes).expect("parse LibraryFolderResponse");

    assert_eq!(body_nested.current_path, "Action/2020s");
    assert_eq!(body_nested.parent_path, Some("Action".to_string()));
    assert_eq!(body_nested.breadcrumbs.len(), 2);
    assert_eq!(body_nested.breadcrumbs[0].name, "Action");
    assert_eq!(body_nested.breadcrumbs[0].path, "Action");
    assert_eq!(body_nested.breadcrumbs[1].name, "2020s");
    assert_eq!(body_nested.breadcrumbs[1].path, "Action/2020s");

    assert!(body_nested.directories.is_empty());
    assert_eq!(body_nested.items.len(), 1);
    assert_eq!(body_nested.items[0].title, "Top Gun: Maverick");
}

#[tokio::test]
async fn test_browse_library_path_traversal_rejected() {
    let ctx = setup_test_context().await;

    // 3. Path traversal attempts return 400 Bad Request
    let traversal_paths = [
        "../../etc",
        "../",
        "..",
        "/etc",
        "/etc/passwd",
        "Action/../../",
        "Action/../",
        "Action%00hidden",
    ];

    for path in traversal_paths {
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/libraries/{}/folders?path={}", ctx.public_lib_id, path))
            .body(Body::empty())
            .unwrap();

        let res = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "Expected 400 Bad Request for path: {path}"
        );
    }
}

#[tokio::test]
async fn test_browse_library_non_existent_path_returns_404() {
    let ctx = setup_test_context().await;

    // 4. Non-existent path returns 404 Not Found
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders?path=NonExistentFolder", ctx.public_lib_id))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Non-existent library ID returns 404 Not Found
    let req_lib = Request::builder()
        .method("GET")
        .uri("/api/v1/libraries/non-existent-lib-id/folders")
        .body(Body::empty())
        .unwrap();

    let res_lib = ctx.app.clone().oneshot(req_lib).await.unwrap();
    assert_eq!(res_lib.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_browse_private_library_auth_and_token() {
    let ctx = setup_test_context().await;

    // 5. Private library without token returns 403 Forbidden
    let req_locked = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders", ctx.private_lib_id))
        .body(Body::empty())
        .unwrap();

    let res_locked = ctx.app.clone().oneshot(req_locked).await.unwrap();
    assert_eq!(res_locked.status(), StatusCode::FORBIDDEN);

    let body_bytes = res_locked.into_body().collect().await.unwrap().to_bytes();
    let err_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(err_json["error"], "LIBRARY_LOCKED");

    // Unlock the private library via POST /api/v1/libraries/{id}/unlock
    let unlock_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{}/unlock", ctx.private_lib_id))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "pin": "1234" }).to_string()))
        .unwrap();

    let unlock_res = ctx.app.clone().oneshot(unlock_req).await.unwrap();
    assert_eq!(unlock_res.status(), StatusCode::OK);
    let unlock_bytes = unlock_res.into_body().collect().await.unwrap().to_bytes();
    let unlock_data: Value = serde_json::from_slice(&unlock_bytes).unwrap();
    let unlock_token = unlock_data["token"].as_str().unwrap();

    // With unlock token via x-kadr-unlocked header -> 200 OK
    let req_unlocked = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders", ctx.private_lib_id))
        .header("x-kadr-unlocked", unlock_token)
        .body(Body::empty())
        .unwrap();

    let res_unlocked = ctx.app.clone().oneshot(req_unlocked).await.unwrap();
    assert_eq!(res_unlocked.status(), StatusCode::OK);

    let body_bytes = res_unlocked.into_body().collect().await.unwrap().to_bytes();
    let body: LibraryFolderResponse = serde_json::from_slice(&body_bytes).expect("parse LibraryFolderResponse");
    assert_eq!(body.library_id, ctx.private_lib_id);
    assert_eq!(body.items.len(), 1);
    assert_eq!(body.items[0].title, "Secret Documentary");
}

#[tokio::test]
async fn test_browse_library_authenticated_watch_progress() {
    let ctx = setup_test_context().await;

    // Find Root Movie in DB
    let movies = ctx
        .media_repo
        .list_by_library(&ctx.public_lib_id, 10, 0)
        .await
        .unwrap();
    let root_movie = movies.iter().find(|m| m.title == "Root Movie").unwrap();
    let root_movie_id = root_movie.id.unwrap();

    // Upsert 50% watch progress for user-1 (500s out of 1000s)
    ctx.playback_repo
        .upsert_progress(
            "user-1",
            root_movie_id,
            500,
            kadr_core::models::WatchState::InProgress,
            1_700_000_100,
        )
        .await
        .unwrap();

    // Call GET /api/v1/libraries/{id}/folders with standard_token (authenticated as user-1)
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders", ctx.public_lib_id))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", ctx.standard_token),
        )
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: LibraryFolderResponse =
        serde_json::from_slice(&body_bytes).expect("parse LibraryFolderResponse");

    assert_eq!(body.items.len(), 1);
    let item = &body.items[0];
    assert_eq!(item.title, "Root Movie");
    assert_eq!(item.playback_progress, Some(0.5));
    assert_eq!(item.badge, Some("RESUME".to_string()));
}

#[tokio::test]
async fn test_browse_library_multi_root_aggregates_counts_dedups_and_inherits_media_type() {
    let ctx = setup_test_context().await;

    let root1 = ctx.temp_dir.path().join("tv_root_1");
    let root2 = ctx.temp_dir.path().join("tv_root_2");
    std::fs::create_dir_all(&root1).expect("create root1");
    std::fs::create_dir_all(&root2).expect("create root2");

    // Both roots have a folder called "Sitcoms"
    let sitcoms1 = root1.join("Sitcoms");
    let sitcoms2 = root2.join("Sitcoms");
    std::fs::create_dir_all(&sitcoms1).expect("create sitcoms1");
    std::fs::create_dir_all(&sitcoms2).expect("create sitcoms2");

    // sitcoms1 has 2 files
    std::fs::write(sitcoms1.join("ep1.mkv"), b"video1").expect("write ep1");
    std::fs::write(sitcoms1.join("ep2.mkv"), b"video2").expect("write ep2");

    // sitcoms2 has 3 files
    std::fs::write(sitcoms2.join("ep3.mkv"), b"video3").expect("write ep3");
    std::fs::write(sitcoms2.join("ep4.mkv"), b"video4").expect("write ep4");
    std::fs::write(sitcoms2.join("ep5.mkv"), b"video5").expect("write ep5");

    // Untracked video file in root1 (not inserted into media_repo)
    let untracked_tv = root1.join("UntrackedShow.mkv");
    std::fs::write(&untracked_tv, b"untracked bytes").expect("write untracked");

    // Create a Show library with multiple roots, including a duplicate root entry
    let tv_lib = Library {
        id: "lib-tv-multiroot".to_string(),
        name: "TV MultiRoot".to_string(),
        path: root1.clone(),
        paths: vec![root1.clone(), root2.clone(), root1.clone()],
        media_type: MediaType::Show,
        is_private: false,
        pin_hash: None,
        created_at: 1_700_000_000,
    };
    ctx.lib_repo.insert(&tv_lib).await.expect("insert tv_lib");

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/libraries/{}/folders", tv_lib.id))
        .body(Body::empty())
        .unwrap();

    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: LibraryFolderResponse =
        serde_json::from_slice(&body_bytes).expect("parse LibraryFolderResponse");

    // "Sitcoms" should aggregate item_count: 2 + 3 = 5
    let sitcoms_dir = body
        .directories
        .iter()
        .find(|d| d.name == "Sitcoms")
        .expect("Sitcoms directory exists");
    assert_eq!(sitcoms_dir.item_count, 5);

    // Untracked item should inherit MediaType::Show ("show") from library
    let untracked_card = body
        .items
        .iter()
        .find(|i| i.title == "UntrackedShow")
        .expect("Untracked card exists");
    assert_eq!(untracked_card.media_type, "show");

    // Also verify deduplication: only 1 instance of UntrackedShow card (even though root1 was duplicated in paths)
    let untracked_count = body
        .items
        .iter()
        .filter(|i| i.title == "UntrackedShow")
        .count();
    assert_eq!(untracked_count, 1);
}
