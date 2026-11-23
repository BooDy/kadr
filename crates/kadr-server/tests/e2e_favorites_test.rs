use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::ast::{
    ItemDetailsPayload, QueryMacro, ScreenId, ScreenLayout, WidgetFilterConfig, WidgetNode,
    WidgetQueryBinding,
};
use kadr_core::events::SystemEvent;
use kadr_core::favorites::{FavoriteStatusResponse, FavoritesListResponse};
use kadr_core::filter_ast::{BoolOp, FilterCondition, FilterNode};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_core::playlists::PlaylistDetail;
use kadr_core::search::SearchResponse;
use kadr_server::api::{create_router_with_events, UnlockTokenService};
use kadr_server::auth::jwt::JwtService;
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
use tempfile::TempDir;
use tower::ServiceExt;

struct TestEnv {
    app: axum::Router,
    event_bus: Arc<EventBus>,
    unlock_svc: UnlockTokenService,
    alice_id: String,
    alice_token: String,
    bob_token: String,
    admin_id: String,
    admin_token: String,
    spotlight_movie_id: i64,
    dune_movie_id: i64,
    show_id: i64,
    ep_s1e1_id: i64,
    ep_s1e2_id: i64,
    private_movie_id: i64,
    folder_ep_id: i64,
    pub_movies_lib_id: String,
    pub_shows_lib_id: String,
    private_lib_id: String,
    folder_lib_id: String,
    _temp_dir: TempDir,
}

async fn insert_item(repo: &MediaItemRepository, item: MediaItem) -> i64 {
    repo.upsert_batch(std::slice::from_ref(&item))
        .await
        .unwrap();
    repo.find_by_path(&item.file_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap()
}

async fn send_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    unlock_token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(jwt) = token {
        builder = builder.header("Authorization", format!("Bearer {jwt}"));
    }
    if let Some(ut) = unlock_token {
        builder = builder.header("X-Kadr-Unlocked", ut);
    }
    let req = if let Some(b) = body {
        builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, val)
}

async fn setup_env() -> TestEnv {
    let temp_dir = TempDir::new().unwrap();
    let folder_root = temp_dir.path().join("library_folders");
    std::fs::create_dir_all(&folder_root).unwrap();
    let folder_ep_file = folder_root.join("01_special.mkv");
    std::fs::write(&folder_ep_file, b"fake-video-bytes").unwrap();

    let db_path = temp_dir.path().join("favorites_e2e.db");
    let pool = create_pool(&db_path, 4).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let alice = User {
        id: "u-alice".to_string(),
        username: "alice".to_string(),
        pin_hash: "h".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let bob = User {
        id: "u-bob".to_string(),
        username: "bob".to_string(),
        pin_hash: "h".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let admin = User {
        id: "u-admin".to_string(),
        username: "admin".to_string(),
        pin_hash: "h".to_string(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo.create(&alice).await.unwrap();
    user_repo.create(&bob).await.unwrap();
    user_repo.create(&admin).await.unwrap();

    let jwt_secret = "super-secret-favorites-test-key-32b!";
    let jwt_svc = JwtService::new(jwt_secret, 3600);
    let unlock_svc = UnlockTokenService::new(jwt_secret.as_bytes(), 3600);

    let alice_token = jwt_svc.generate_token(&alice).unwrap();
    let bob_token = jwt_svc.generate_token(&bob).unwrap();
    let admin_token = jwt_svc.generate_token(&admin).unwrap();

    let pub_movies_lib_id = "lib-movies".to_string();
    lib_repo
        .insert(&Library {
            id: pub_movies_lib_id.clone(),
            name: "Movies".to_string(),
            path: PathBuf::from("/media/movies"),
            media_type: MediaType::Movie,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let pub_shows_lib_id = "lib-shows".to_string();
    lib_repo
        .insert(&Library {
            id: pub_shows_lib_id.clone(),
            name: "Shows".to_string(),
            path: PathBuf::from("/media/shows"),
            media_type: MediaType::Show,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let private_lib_id = "lib-priv".to_string();
    lib_repo
        .insert(&Library {
            id: private_lib_id.clone(),
            name: "Private Vault".to_string(),
            path: PathBuf::from("/media/priv"),
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("hash".to_string()),
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let folder_lib_id = "lib-folder".to_string();
    let canon_folder_root = std::fs::canonicalize(&folder_root).unwrap();
    lib_repo
        .insert(&Library {
            id: folder_lib_id.clone(),
            name: "Folder Extras".to_string(),
            path: canon_folder_root.clone(),
            paths: vec![canon_folder_root],
            media_type: MediaType::Episode,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let spotlight_movie_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: pub_movies_lib_id.clone(),
            item_type: MediaType::Movie,
            title: "Interstellar".to_string(),
            original_title: None,
            release_year: Some(2014),
            added_at: 5000,
            file_path: PathBuf::from("/media/movies/interstellar.mkv"),
            file_name: "interstellar.mkv".to_string(),
            file_size: 20_000_000,
            technical: TechnicalInfo {
                duration_seconds: 10140,
                resolution: Some("4K".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                overview: Some("Humanity explores the stars.".to_string()),
                poster_path: Some("/art/interstellar_poster.jpg".to_string()),
                backdrop_path: Some("/art/interstellar_backdrop.jpg".to_string()),
                rating: Some(9.6),
                genres: vec!["Sci-Fi".to_string(), "Action".to_string()],
                ..Default::default()
            },
            locked_fields: Vec::new(),
        },
    )
    .await;

    let dune_movie_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: pub_movies_lib_id.clone(),
            item_type: MediaType::Movie,
            title: "Dune".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: 4500,
            file_path: PathBuf::from("/media/movies/dune.mkv"),
            file_name: "dune.mkv".to_string(),
            file_size: 15_000_000,
            technical: TechnicalInfo {
                duration_seconds: 9000,
                resolution: Some("1080p".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                overview: Some("Desert planet epic.".to_string()),
                poster_path: Some("/art/dune_poster.jpg".to_string()),
                rating: Some(8.4),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
            locked_fields: Vec::new(),
        },
    )
    .await;

    let show_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: pub_shows_lib_id.clone(),
            item_type: MediaType::Show,
            title: "Severance".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 4000,
            file_path: PathBuf::from("/media/shows/Severance"),
            file_name: "Severance".to_string(),
            file_size: 0,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                overview: Some("Macrodata Refinement.".to_string()),
                rating: Some(8.9),
                genres: vec!["Sci-Fi".to_string(), "Drama".to_string()],
                ..Default::default()
            },
            locked_fields: Vec::new(),
        },
    )
    .await;

    let ep_s1e1_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: pub_shows_lib_id.clone(),
            item_type: MediaType::Episode,
            title: "Good News About Hell".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 3900,
            file_path: PathBuf::from("/media/shows/Severance/S01E01.mkv"),
            file_name: "S01E01.mkv".to_string(),
            file_size: 2_000_000,
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
            locked_fields: Vec::new(),
        },
    )
    .await;

    let ep_s1e2_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: pub_shows_lib_id.clone(),
            item_type: MediaType::Episode,
            title: "Half Loop".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 3800,
            file_path: PathBuf::from("/media/shows/Severance/S01E02.mkv"),
            file_name: "S01E02.mkv".to_string(),
            file_size: 2_000_000,
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
            locked_fields: Vec::new(),
        },
    )
    .await;

    let private_movie_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: private_lib_id.clone(),
            item_type: MediaType::Movie,
            title: "Classified Cut".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 4800,
            file_path: PathBuf::from("/media/priv/classified.mkv"),
            file_name: "classified.mkv".to_string(),
            file_size: 5_000_000,
            technical: TechnicalInfo {
                duration_seconds: 5400,
                resolution: Some("4K".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                poster_path: Some("/art/classified.jpg".to_string()),
                ..Default::default()
            },
            locked_fields: Vec::new(),
        },
    )
    .await;

    let folder_ep_id = insert_item(
        &media_repo,
        MediaItem {
            id: None,
            library_id: folder_lib_id.clone(),
            item_type: MediaType::Episode,
            title: "Folder Special Episode".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 3700,
            file_path: std::fs::canonicalize(&folder_ep_file).unwrap(),
            file_name: "01_special.mkv".to_string(),
            file_size: 16,
            technical: TechnicalInfo {
                duration_seconds: 1800,
                resolution: Some("1080p".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        },
    )
    .await;

    // Seed InProgress playback states for both alice and admin on Dune and Interstellar
    playback_repo
        .upsert_progress(&alice.id, dune_movie_id, 1800, WatchState::InProgress, 6000)
        .await
        .unwrap();
    playback_repo
        .upsert_progress(
            &alice.id,
            spotlight_movie_id,
            2500,
            WatchState::InProgress,
            5900,
        )
        .await
        .unwrap();
    playback_repo
        .upsert_progress(&admin.id, dune_movie_id, 3000, WatchState::InProgress, 6000)
        .await
        .unwrap();
    playback_repo
        .upsert_progress(
            &admin.id,
            spotlight_movie_id,
            1500,
            WatchState::InProgress,
            5900,
        )
        .await
        .unwrap();

    let limiter = RateLimiter::new(20, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_dir.path().join("subs"),
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry = Arc::new(TelemetryCollector::new(
        PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));

    let app = create_router_with_events(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry,
    );

    TestEnv {
        app,
        event_bus,
        unlock_svc,
        alice_id: alice.id,
        alice_token,
        bob_token,
        admin_id: admin.id,
        admin_token,
        spotlight_movie_id,
        dune_movie_id,
        show_id,
        ep_s1e1_id,
        ep_s1e2_id,
        private_movie_id,
        folder_ep_id,
        pub_movies_lib_id,
        pub_shows_lib_id,
        private_lib_id,
        folder_lib_id,
        _temp_dir: temp_dir,
    }
}

#[tokio::test]
async fn test_favorites_put_delete_auth_errors_privacy_isolation_and_events() {
    let env = setup_env().await;

    // 1. 401 Unauthorized when unauthenticated
    for (method, uri) in [
        ("GET", "/api/v1/favorites".to_string()),
        (
            "PUT",
            format!("/api/v1/favorites/{}", env.spotlight_movie_id),
        ),
        (
            "DELETE",
            format!("/api/v1/favorites/{}", env.spotlight_movie_id),
        ),
    ] {
        let (status, _) = send_json(&env.app, method, &uri, None, None, None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must reject unauthenticated requests"
        );
    }

    // 2. 404 Not Found on unknown item
    let (status, _) = send_json(
        &env.app,
        "PUT",
        "/api/v1/favorites/999999",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send_json(
        &env.app,
        "DELETE",
        "/api/v1/favorites/999999",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 3. 403 LIBRARY_LOCKED on locked private-library item (both PUT and DELETE)
    let priv_uri = format!("/api/v1/favorites/{}", env.private_movie_id);
    let (status, err_body) = send_json(
        &env.app,
        "PUT",
        &priv_uri,
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_body["error"], "LIBRARY_LOCKED");
    assert_eq!(err_body["code"], "LIBRARY_LOCKED");
    assert_eq!(err_body["library_id"], env.private_lib_id);

    let (status, err_body) = send_json(
        &env.app,
        "DELETE",
        &priv_uri,
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_body["error"], "LIBRARY_LOCKED");
    assert_eq!(err_body["code"], "LIBRARY_LOCKED");
    assert_eq!(err_body["library_id"], env.private_lib_id);

    // 4. Subscribe to EventBus and SSE /api/v1/events, and verify PUT & DELETE responses + favorite:updated events
    let sse_resp = env
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sse_resp.status(), StatusCode::OK);
    let mut sse_body = sse_resp.into_body();

    let mut rx = env.event_bus.subscribe();
    let put_uri = format!("/api/v1/favorites/{}", env.spotlight_movie_id);

    let (status, body) = send_json(
        &env.app,
        "PUT",
        &put_uri,
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp: FavoriteStatusResponse = serde_json::from_value(body).unwrap();
    assert_eq!(
        resp,
        FavoriteStatusResponse {
            item_id: env.spotlight_movie_id,
            is_favorite: true,
        }
    );

    let sse_frame = tokio::time::timeout(Duration::from_secs(1), sse_body.frame())
        .await
        .expect("timed out waiting for SSE frame")
        .expect("SSE stream ended")
        .expect("SSE frame error");
    let sse_text = String::from_utf8_lossy(sse_frame.data_ref().unwrap()).to_string();
    assert!(
        sse_text.contains("event: favorite:updated"),
        "expected SSE event header favorite:updated, got: {sse_text}"
    );
    assert!(
        sse_text.contains("\"type\":\"favorite:updated\""),
        "expected SSE JSON payload type favorite:updated, got: {sse_text}"
    );

    let evt = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timed out waiting for FavoriteUpdated event")
        .expect("event bus closed");
    assert_eq!(evt.event_type(), "favorite:updated");
    match evt {
        SystemEvent::FavoriteUpdated {
            user_id,
            item_id,
            is_favorite,
            timestamp,
        } => {
            assert_eq!(user_id, env.admin_id);
            assert_eq!(item_id, env.spotlight_movie_id);
            assert!(is_favorite);
            assert!(timestamp > 0);
        }
        other => panic!("expected FavoriteUpdated event, got {other:?}"),
    }

    // Idempotent repeat PUT succeeds with 200 OK
    let (status, body) = send_json(
        &env.app,
        "PUT",
        &put_uri,
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_favorite"], true);
    let _ = rx.try_recv();

    // 5. Per-user isolation: admin's favorite is NOT visible to standard user alice,
    // and alice's DELETE does NOT remove admin's favorite
    let (status, alice_list_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let alice_list: FavoritesListResponse = serde_json::from_value(alice_list_val).unwrap();
    assert!(alice_list.items.is_empty());
    assert_eq!(alice_list.total_count, 0);

    // Alice calls DELETE on spotlight_movie_id -> 200 OK { is_favorite: false } for alice, emits event for alice
    let (status, del_body) = send_json(
        &env.app,
        "DELETE",
        &put_uri,
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let del_resp: FavoriteStatusResponse = serde_json::from_value(del_body).unwrap();
    assert_eq!(
        del_resp,
        FavoriteStatusResponse {
            item_id: env.spotlight_movie_id,
            is_favorite: false,
        }
    );

    let alice_del_evt = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match alice_del_evt {
        SystemEvent::FavoriteUpdated {
            user_id,
            item_id,
            is_favorite,
            ..
        } => {
            assert_eq!(user_id, env.alice_id);
            assert_eq!(item_id, env.spotlight_movie_id);
            assert!(!is_favorite);
        }
        other => panic!("expected FavoriteUpdated event, got {other:?}"),
    }

    // Admin's favorite is still intact!
    let (status, admin_list_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_list: FavoritesListResponse = serde_json::from_value(admin_list_val).unwrap();
    assert_eq!(admin_list.items.len(), 1);
    assert_eq!(admin_list.items[0].card.id, env.spotlight_movie_id);

    // Admin unfavorites -> removed from admin's list and emits FavoriteUpdated(false)
    let (status, admin_del_val) = send_json(
        &env.app,
        "DELETE",
        &put_uri,
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin_del_val["is_favorite"], false);

    let (status, admin_after_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_after: FavoritesListResponse = serde_json::from_value(admin_after_val).unwrap();
    assert!(admin_after.items.is_empty());
}

#[tokio::test]
async fn test_list_favorites_resume_progress_filtering_pagination_and_locked_count() {
    let env = setup_env().await;
    let (unlock_tok, _) = env.unlock_svc.generate_token(&env.private_lib_id).unwrap();

    // Alice favorites Dune (Movie, InProgress at 1800s), Severance (Show), S01E01 (Episode),
    // and Classified Cut (Private Movie, while unlocked)
    for id in [env.dune_movie_id, env.show_id, env.ep_s1e1_id] {
        let (status, _) = send_json(
            &env.app,
            "PUT",
            &format!("/api/v1/favorites/{id}"),
            Some(&env.alice_token),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, _) = send_json(
        &env.app,
        "PUT",
        &format!("/api/v1/favorites/{}", env.private_movie_id),
        Some(&env.alice_token),
        Some(&unlock_tok),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Attempting DELETE on private item WITHOUT unlock token must still fail closed with 403 LIBRARY_LOCKED
    let (status, err_val) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/favorites/{}", env.private_movie_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_val["code"], "LIBRARY_LOCKED");

    // 1. Without unlock token: GET /api/v1/favorites hides private movie and reports locked_item_count == 1
    let (status, val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?sort=title:asc",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let locked_resp: FavoritesListResponse = serde_json::from_value(val).unwrap();
    assert_eq!(locked_resp.total_count, 3);
    assert_eq!(locked_resp.locked_item_count, 1);
    assert_eq!(locked_resp.items.len(), 3);
    assert!(locked_resp.items.iter().all(|it| it.card.is_favorite));
    assert!(locked_resp.items.iter().all(|it| it.favorited_at > 0));
    assert!(locked_resp
        .items
        .iter()
        .all(|it| it.card.id != env.private_movie_id));

    // Verify InProgress resume_position_seconds on Dune
    let dune_fav = locked_resp
        .items
        .iter()
        .find(|it| it.card.id == env.dune_movie_id)
        .expect("Dune should be in favorites");
    assert_eq!(dune_fav.duration_seconds, 9000);
    assert_eq!(dune_fav.resume_position_seconds, Some(1800));
    assert_eq!(dune_fav.card.badge.as_deref(), Some("RESUME"));

    // 2. Mark Dune completed via PUT /api/v1/playback/states/{id} -> resume_position_seconds MUST become None
    let (status, _) = send_json(
        &env.app,
        "PUT",
        &format!("/api/v1/playback/states/{}", env.dune_movie_id),
        Some(&env.alice_token),
        None,
        Some(json!({ "watched": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, after_complete_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?item_type=movie",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let movie_only: FavoritesListResponse = serde_json::from_value(after_complete_val).unwrap();
    assert_eq!(movie_only.total_count, 1);
    assert_eq!(movie_only.locked_item_count, 1); // private item is also a Movie!
    assert_eq!(movie_only.items.len(), 1);
    assert_eq!(movie_only.items[0].card.id, env.dune_movie_id);
    assert_eq!(
        movie_only.items[0].resume_position_seconds, None,
        "Completed item must NOT populate resume_position_seconds"
    );
    assert_eq!(movie_only.items[0].card.playback_progress, Some(1.0));

    // 3. item_type=episode -> 1 visible, 0 locked
    let (status, ep_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?item_type=episode",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ep_only: FavoritesListResponse = serde_json::from_value(ep_val).unwrap();
    assert_eq!(ep_only.total_count, 1);
    assert_eq!(ep_only.locked_item_count, 0);
    assert_eq!(ep_only.items[0].card.id, env.ep_s1e1_id);

    // Invalid item_type -> 400 Bad Request
    let (status, _) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?item_type=not_a_type",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // library_id filter -> only lib-shows items (Show + Episode = 2)
    let (status, shows_lib_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/favorites?library_id={}", env.pub_shows_lib_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let shows_lib_resp: FavoritesListResponse = serde_json::from_value(shows_lib_val).unwrap();
    assert_eq!(shows_lib_resp.total_count, 2);
    assert_eq!(shows_lib_resp.locked_item_count, 0);

    // 4. With unlock token: private movie becomes visible and locked_item_count drops to 0!
    let (status, unlocked_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?sort=title:asc&limit=2&offset=0",
        Some(&env.alice_token),
        Some(&unlock_tok),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page1: FavoritesListResponse = serde_json::from_value(unlocked_val).unwrap();
    assert_eq!(page1.total_count, 4);
    assert_eq!(page1.locked_item_count, 0);
    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.items[0].card.title, "Classified Cut");
    assert_eq!(page1.items[1].card.title, "Dune");

    let (status, page2_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?sort=title:asc&limit=2&offset=2",
        Some(&env.alice_token),
        Some(&unlock_tok),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page2: FavoritesListResponse = serde_json::from_value(page2_val).unwrap();
    assert_eq!(page2.total_count, 4);
    assert_eq!(page2.locked_item_count, 0);
    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.items[0].card.title, "Good News About Hell");
    assert_eq!(page2.items[1].card.title, "Severance");

    // Verify limit=0 clamps to 1
    let (status, clamped_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/favorites?limit=0",
        Some(&env.alice_token),
        Some(&unlock_tok),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let clamped: FavoritesListResponse = serde_json::from_value(clamped_val).unwrap();
    assert_eq!(clamped.total_count, 4);
    assert_eq!(clamped.items.len(), 1);
}

#[tokio::test]
async fn test_full_surface_hydration_screens_items_playlists_folders_search_and_preview() {
    let env = setup_env().await;

    // Admin favorites: spotlight_movie_id (Interstellar), show_id (Severance), ep_s1e1_id, and folder_ep_id.
    // Note: dune_movie_id and ep_s1e2_id remain UNFAVORITED for admin so we can verify both true and false!
    for id in [
        env.spotlight_movie_id,
        env.show_id,
        env.ep_s1e1_id,
        env.folder_ep_id,
    ] {
        let (status, _) = send_json(
            &env.app,
            "PUT",
            &format!("/api/v1/favorites/{id}"),
            Some(&env.admin_token),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // 1. GET /api/v1/screens/home -> includes "my_favorites" immediately after "continue_watching",
    //    hydrates "my_favorites" across libraries, and sets is_favorite: true on matching cards in
    //    home_spotlight, continue_watching, recently_added, top_rated, genre_action, genre_scifi.
    let (status, home_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let home: ScreenLayout = serde_json::from_value(home_val).unwrap();
    assert_eq!(home.id, ScreenId::Home);
    assert_eq!(home.widgets.len(), 7);
    assert_eq!(home.widgets[0].id(), "home_spotlight");
    assert_eq!(home.widgets[1].id(), "continue_watching");
    assert_eq!(home.widgets[2].id(), "my_favorites");
    assert_eq!(home.widgets[3].id(), "recently_added");

    // Check HeroBanner home_spotlight -> Interstellar has is_favorite == true
    match &home.widgets[0] {
        WidgetNode::HeroBanner { data, .. } => {
            let hero = data.as_ref().expect("hero card hydrated");
            assert_eq!(hero.id, env.spotlight_movie_id);
            assert!(
                hero.is_favorite,
                "HeroBanner spotlight must hydrate is_favorite"
            );
        }
        other => panic!("expected HeroBanner, got {other:?}"),
    }

    // Check continue_watching -> Dune is false, Interstellar is true
    match &home.widgets[1] {
        WidgetNode::Carousel { items, .. } => {
            let cards = items.as_ref().unwrap();
            let dune_card = cards.iter().find(|c| c.id == env.dune_movie_id).unwrap();
            let inter_card = cards
                .iter()
                .find(|c| c.id == env.spotlight_movie_id)
                .unwrap();
            assert!(!dune_card.is_favorite);
            assert!(inter_card.is_favorite);
        }
        other => panic!("expected continue_watching Carousel, got {other:?}"),
    }

    // Check my_favorites carousel -> contains all 4 favorited items across Movies, Shows, Folder libraries
    match &home.widgets[2] {
        WidgetNode::Carousel {
            id,
            title,
            binding,
            items,
            ..
        } => {
            assert_eq!(id, "my_favorites");
            assert_eq!(title, "My Favorites");
            assert_eq!(binding.macro_type, QueryMacro::Favorites);
            let cards = items.as_ref().expect("my_favorites items hydrated");
            assert_eq!(cards.len(), 4);
            assert!(cards.iter().all(|c| c.is_favorite));
        }
        other => panic!("expected my_favorites Carousel, got {other:?}"),
    }

    // Check recently_added -> Interstellar & Severance have is_favorite == true, Dune == false
    match &home.widgets[3] {
        WidgetNode::Carousel { items, .. } => {
            let cards = items.as_ref().unwrap();
            let inter = cards
                .iter()
                .find(|c| c.id == env.spotlight_movie_id)
                .unwrap();
            let show = cards.iter().find(|c| c.id == env.show_id).unwrap();
            let dune = cards.iter().find(|c| c.id == env.dune_movie_id).unwrap();
            assert!(inter.is_favorite);
            assert!(show.is_favorite);
            assert!(!dune.is_favorite);
        }
        other => panic!("expected recently_added Carousel, got {other:?}"),
    }

    // Bob (who favorited nothing) sees empty my_favorites and is_favorite == false on home_spotlight
    let (status, bob_home_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let bob_home: ScreenLayout = serde_json::from_value(bob_home_val).unwrap();
    if let WidgetNode::HeroBanner {
        data: Some(hero), ..
    } = &bob_home.widgets[0]
    {
        assert!(!hero.is_favorite);
    }
    if let WidgetNode::Carousel {
        items: Some(cards), ..
    } = &bob_home.widgets[2]
    {
        assert!(cards.is_empty());
    }

    // 2. GET /api/v1/widgets/my_favorites/data -> paginated widget data works
    let (status, widget_data_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/widgets/my_favorites/data?offset=0&limit=2",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(widget_data_val["total_count"], 4);
    assert_eq!(widget_data_val["items"].as_array().unwrap().len(), 2);
    assert!(widget_data_val["next_cursor"].is_string());

    // 3. GET /api/v1/items/{id} -> Movie card.is_favorite and Show card.is_favorite + child episodes
    let (status, item_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/items/{}", env.spotlight_movie_id),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let movie_details: ItemDetailsPayload = serde_json::from_value(item_val).unwrap();
    assert!(movie_details.card.is_favorite);

    let (status, show_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/items/{}", env.show_id),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let show_details: ItemDetailsPayload = serde_json::from_value(show_val).unwrap();
    assert!(show_details.card.is_favorite);
    let eps = show_details.episodes.expect("show episodes hydrated");
    let ep1 = eps.iter().find(|e| e.id == env.ep_s1e1_id).unwrap();
    let ep2 = eps.iter().find(|e| e.id == env.ep_s1e2_id).unwrap();
    assert!(ep1.is_favorite);
    assert!(!ep2.is_favorite);

    // 4. GET /api/v1/playlists/{id} -> items[0].card.is_favorite hydrated per requesting user
    let (status, pl_created_val) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.admin_token),
        None,
        Some(json!({
            "name": "Sci-Fi Picks",
            "is_public": true,
            "items": { "item_ids": [env.spotlight_movie_id, env.dune_movie_id] }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let pl_created: PlaylistDetail = serde_json::from_value(pl_created_val).unwrap();
    assert!(pl_created.items[0].card.is_favorite);
    assert!(!pl_created.items[1].card.is_favorite);

    let pl_id = &pl_created.summary.id;
    let (status, pl_get_admin_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pl_admin: PlaylistDetail = serde_json::from_value(pl_get_admin_val).unwrap();
    assert!(pl_admin.items[0].card.is_favorite);
    assert!(!pl_admin.items[1].card.is_favorite);

    // Bob views the same public playlist -> both cards have is_favorite == false for Bob
    let (status, pl_get_bob_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pl_bob: PlaylistDetail = serde_json::from_value(pl_get_bob_val).unwrap();
    assert!(!pl_bob.items[0].card.is_favorite);
    assert!(!pl_bob.items[1].card.is_favorite);

    // 5. GET /api/v1/libraries/{id}/folders -> folder cards hydrate is_favorite per user
    let (status, folders_admin_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/libraries/{}/folders", env.folder_lib_id),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(folders_admin_val["items"][0]["id"], env.folder_ep_id);
    assert_eq!(folders_admin_val["items"][0]["is_favorite"], true);

    let (status, folders_bob_val) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/libraries/{}/folders", env.folder_lib_id),
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(folders_bob_val["items"][0]["id"], env.folder_ep_id);
    assert!(!folders_bob_val["items"][0]
        .get("is_favorite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false));

    // 6. GET /api/v1/search?q=Interstellar -> items[0].is_favorite == true for admin, false for bob
    let (status, search_admin_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/search?q=Interstellar",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let search_admin: SearchResponse = serde_json::from_value(search_admin_val).unwrap();
    assert_eq!(search_admin.items.len(), 1);
    assert_eq!(search_admin.items[0].id, env.spotlight_movie_id);
    assert!(search_admin.items[0].is_favorite);

    let (status, search_bob_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/search?q=Interstellar",
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let search_bob: SearchResponse = serde_json::from_value(search_bob_val).unwrap();
    assert_eq!(search_bob.items.len(), 1);
    assert!(!search_bob.items[0].is_favorite);

    // 7. POST /api/v1/widgets/preview with QueryMacro::Favorites and FilterCondition::IsFavorite
    let (status, preview_fav_macro_val) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_token),
        None,
        Some(json!({
            "binding": WidgetQueryBinding::new(QueryMacro::Favorites),
            "screen_id": "movies",
            "limit": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Favorites macro spans ALL accessible libraries by default even when screen_id == "movies"!
    assert_eq!(preview_fav_macro_val["total_count"], 4);
    assert_eq!(preview_fav_macro_val["items"].as_array().unwrap().len(), 4);
    assert_eq!(preview_fav_macro_val["items"][0]["is_favorite"], true);

    // When filters.library_id is explicitly set on QueryMacro::Favorites, it scopes to that library!
    let scoped_fav_binding =
        WidgetQueryBinding::new(QueryMacro::Favorites).with_filters(WidgetFilterConfig {
            library_id: Some(env.pub_movies_lib_id.clone()),
            ..Default::default()
        });
    let (status, preview_fav_scoped_val) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_token),
        None,
        Some(json!({
            "binding": scoped_fav_binding,
            "limit": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview_fav_scoped_val["total_count"], 1);
    assert_eq!(
        preview_fav_scoped_val["items"][0]["id"],
        env.spotlight_movie_id
    );

    // Preview CustomQuery with FilterCondition::IsFavorite { value: true } scoped to Movies screen
    let fav_true_binding =
        WidgetQueryBinding::new(QueryMacro::CustomQuery).with_filters(WidgetFilterConfig {
            rule_tree: Some(FilterNode::Rule {
                negated: false,
                condition: FilterCondition::IsFavorite {
                    op: BoolOp::Eq,
                    value: true,
                },
            }),
            ..Default::default()
        });
    let (status, preview_custom_true) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_token),
        None,
        Some(json!({
            "binding": fav_true_binding,
            "screen_id": "movies",
            "limit": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview_custom_true["total_count"], 1);
    assert_eq!(
        preview_custom_true["items"][0]["id"],
        env.spotlight_movie_id
    );
    assert_eq!(preview_custom_true["items"][0]["is_favorite"], true);

    // Preview CustomQuery with FilterCondition::IsFavorite { value: false } scoped to Movies screen -> only Dune
    let fav_false_binding =
        WidgetQueryBinding::new(QueryMacro::CustomQuery).with_filters(WidgetFilterConfig {
            rule_tree: Some(FilterNode::Rule {
                negated: false,
                condition: FilterCondition::IsFavorite {
                    op: BoolOp::Eq,
                    value: false,
                },
            }),
            ..Default::default()
        });
    let (status, preview_custom_false) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_token),
        None,
        Some(json!({
            "binding": fav_false_binding,
            "screen_id": "movies",
            "limit": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview_custom_false["total_count"], 1);
    assert_eq!(preview_custom_false["items"][0]["id"], env.dune_movie_id);

    // Preview ContinueWatching (in-memory FilterNode::matches_with_context) with IsFavorite == true -> only Interstellar
    let cw_fav_binding =
        WidgetQueryBinding::new(QueryMacro::ContinueWatching).with_filters(WidgetFilterConfig {
            rule_tree: Some(FilterNode::Rule {
                negated: false,
                condition: FilterCondition::IsFavorite {
                    op: BoolOp::Eq,
                    value: true,
                },
            }),
            ..Default::default()
        });
    let (status, preview_cw_fav) = send_json(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.admin_token),
        None,
        Some(json!({
            "binding": cw_fav_binding,
            "limit": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cw_items = preview_cw_fav["items"].as_array().unwrap();
    assert_eq!(cw_items.len(), 1);
    assert_eq!(cw_items[0]["id"], env.spotlight_movie_id);
    assert_eq!(cw_items[0]["is_favorite"], true);
}
