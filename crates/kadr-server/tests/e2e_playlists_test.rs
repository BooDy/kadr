use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::ast::{QueryMacro, ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding};
use kadr_core::events::SystemEvent;
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
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
    layout_registry: LayoutRegistry,
    unlock_svc: UnlockTokenService,
    alice_token: String,
    bob_token: String,
    admin_token: String,
    movie_a_id: i64,
    movie_b_id: i64,
    season_item_id: i64,
    show_id: i64,
    ep_s1e1_id: i64,
    ep_s1e2_id: i64,
    ep_s2e1_id: i64,
    private_movie_id: i64,
    folder_ep1_id: i64,
    folder_ep2_id: i64,
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
    let sub_season = folder_root.join("Season 01");
    let nested_bonus = sub_season.join("Bonus");
    let hidden_dir = sub_season.join(".hidden");
    std::fs::create_dir_all(&nested_bonus).unwrap();
    std::fs::create_dir_all(&hidden_dir).unwrap();

    let ep1_file = sub_season.join("01_pilot.mkv");
    let ep2_file = nested_bonus.join("02_making_of.mp4");
    let hidden_file = hidden_dir.join("secret_dot.mkv");
    std::fs::write(&ep1_file, b"fake-video-1").unwrap();
    std::fs::write(&ep2_file, b"fake-video-2").unwrap();
    std::fs::write(&hidden_file, b"hidden-video").unwrap();

    let db_path = temp_dir.path().join("playlists_e2e.db");
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

    let jwt_secret = "super-secret-playlist-test-key-32b!";
    let jwt_svc = JwtService::new(jwt_secret, 3600);
    let unlock_svc = UnlockTokenService::new(jwt_secret.as_bytes(), 3600);

    let alice_token = jwt_svc.generate_token(&alice).unwrap();
    let bob_token = jwt_svc.generate_token(&bob).unwrap();
    let admin_token = jwt_svc.generate_token(&admin).unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public Movies".to_string(),
            path: PathBuf::from("/media/pub"),
            media_type: MediaType::Movie,
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
            name: "Folder Shows".to_string(),
            path: canon_folder_root.clone(),
            paths: vec![canon_folder_root],
            media_type: MediaType::Episode,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let mk_item =
        |lib: &str, kind: MediaType, title: &str, path: PathBuf, dur: i64, meta: MediaMetadata| {
            MediaItem {
                id: None,
                library_id: lib.to_string(),
                item_type: kind,
                title: title.to_string(),
                original_title: None,
                release_year: Some(2024),
                added_at: 2000,
                file_path: path.clone(),
                file_name: path.file_name().unwrap().to_string_lossy().to_string(),
                file_size: 1024,
                technical: TechnicalInfo {
                    duration_seconds: dur,
                    resolution: Some("4K".to_string()),
                    ..Default::default()
                },
                metadata: meta,
            }
        };

    let movie_a_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Movie,
            "Arrival",
            PathBuf::from("/media/pub/arrival.mkv"),
            7000,
            MediaMetadata {
                poster_path: Some("/art/arrival.jpg".to_string()),
                ..Default::default()
            },
        ),
    )
    .await;

    let movie_b_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Movie,
            "Dune",
            PathBuf::from("/media/pub/dune.mkv"),
            9000,
            MediaMetadata {
                poster_path: Some("/art/dune.jpg".to_string()),
                ..Default::default()
            },
        ),
    )
    .await;

    let season_item_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Season,
            "Severance Season 1",
            PathBuf::from("/media/pub/Severance/Season 1"),
            0,
            MediaMetadata::default(),
        ),
    )
    .await;

    let show_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Show,
            "Severance",
            PathBuf::from("/media/pub/Severance"),
            0,
            MediaMetadata::default(),
        ),
    )
    .await;

    let ep_s1e2_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Episode,
            "Half Loop",
            PathBuf::from("/media/pub/Severance/S01E02.mkv"),
            3000,
            MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(2),
                ..Default::default()
            },
        ),
    )
    .await;

    let ep_s1e1_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Episode,
            "Good News About Hell",
            PathBuf::from("/media/pub/Severance/S01E01.mkv"),
            3200,
            MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(1),
                ..Default::default()
            },
        ),
    )
    .await;

    let ep_s2e1_id = insert_item(
        &media_repo,
        mk_item(
            "lib-pub",
            MediaType::Episode,
            "Hello Ms. Cobel",
            PathBuf::from("/media/pub/Severance/S02E01.mkv"),
            3100,
            MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(2),
                episode: Some(1),
                ..Default::default()
            },
        ),
    )
    .await;

    let private_movie_id = insert_item(
        &media_repo,
        mk_item(
            &private_lib_id,
            MediaType::Movie,
            "Classified Cut",
            PathBuf::from("/media/priv/classified.mkv"),
            5400,
            MediaMetadata {
                poster_path: Some("/art/classified.jpg".to_string()),
                ..Default::default()
            },
        ),
    )
    .await;

    let folder_ep1_id = insert_item(
        &media_repo,
        mk_item(
            &folder_lib_id,
            MediaType::Episode,
            "Folder Pilot",
            std::fs::canonicalize(&ep1_file).unwrap(),
            1800,
            MediaMetadata::default(),
        ),
    )
    .await;

    let folder_ep2_id = insert_item(
        &media_repo,
        mk_item(
            &folder_lib_id,
            MediaType::Episode,
            "Folder Bonus",
            std::fs::canonicalize(&ep2_file).unwrap(),
            900,
            MediaMetadata::default(),
        ),
    )
    .await;

    // Also ingest the hidden dot-directory file into DB to prove folder expansion skips hidden '.' entries
    let _hidden_id = insert_item(
        &media_repo,
        mk_item(
            &folder_lib_id,
            MediaType::Episode,
            "Hidden Dot File",
            std::fs::canonicalize(&hidden_file).unwrap(),
            500,
            MediaMetadata::default(),
        ),
    )
    .await;

    // Seed Bob's independent playback progress: movie_a InProgress (1400s), movie_b Completed (9000s)
    playback_repo
        .upsert_progress(&bob.id, movie_a_id, 1400, WatchState::InProgress, 5000)
        .await
        .unwrap();
    playback_repo
        .upsert_progress(&bob.id, movie_b_id, 9000, WatchState::Completed, 5000)
        .await
        .unwrap();

    let limiter = RateLimiter::new(5, Duration::from_secs(60), Duration::from_secs(60));
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
        layout_registry.clone(),
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry,
    );

    TestEnv {
        app,
        event_bus,
        layout_registry,
        unlock_svc,
        alice_token,
        bob_token,
        admin_token,
        movie_a_id,
        movie_b_id,
        season_item_id,
        show_id,
        ep_s1e1_id,
        ep_s1e2_id,
        ep_s2e1_id,
        private_movie_id,
        folder_ep1_id,
        folder_ep2_id,
        private_lib_id,
        folder_lib_id,
        _temp_dir: temp_dir,
    }
}

#[tokio::test]
async fn multi_user_permissions_validation_sharing_and_sse_events() {
    let env = setup_env().await;
    let mut rx = env.event_bus.subscribe();

    // Unauthenticated rejected
    let (status, _) = send_json(&env.app, "GET", "/api/v1/playlists", None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Validation: empty name, overlong name, overlong description
    let (s_empty_name, e_empty_name) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({ "name": "   " })),
    )
    .await;
    assert_eq!(s_empty_name, StatusCode::BAD_REQUEST);
    assert_eq!(
        e_empty_name["error"],
        "Playlist name must be between 1 and 120 characters"
    );

    let (s_long_desc, _) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({ "name": "Valid", "description": "x".repeat(501) })),
    )
    .await;
    assert_eq!(s_long_desc, StatusCode::BAD_REQUEST);

    // Alice 1-step Create & Add private playlist
    let (status, created) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({
            "name": "Alice Sci-Fi",
            "description": "Director cuts",
            "is_public": false,
            "items": { "item_ids": [env.movie_a_id, env.movie_b_id] }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let pl_id = created["summary"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["summary"]["visible_item_count"], 2);
    assert_eq!(created["summary"]["owner_username"], "alice");
    assert_eq!(created["items"][0]["card"]["id"], env.movie_a_id);
    assert!(created["items"][0].get("resume_position_seconds").is_none());

    // Verify SSE created event has zero metadata leakage
    let ev = rx
        .try_recv()
        .expect("expected PlaylistUpdated created event");
    let ev_json = serde_json::to_string(&ev).unwrap();
    assert!(!ev_json.contains("Alice Sci-Fi"));
    assert!(!ev_json.contains("Director cuts"));
    assert!(matches!(
        ev,
        SystemEvent::PlaylistUpdated { ref action, ref playlist_id, .. }
        if action == "created" && playlist_id == &pl_id
    ));

    // Bob gets 404 on Alice's private playlist (GET, PATCH, DELETE, add, reorder, remove)
    for (method, suffix, payload) in [
        ("GET", String::new(), None),
        ("PATCH", String::new(), Some(json!({ "name": "Hijack" }))),
        ("DELETE", String::new(), None),
        (
            "POST",
            "/items".to_string(),
            Some(json!({ "item_ids": [env.movie_a_id] })),
        ),
        (
            "PUT",
            "/items/order".to_string(),
            Some(json!({ "item_ids": [env.movie_b_id, env.movie_a_id] })),
        ),
        ("DELETE", format!("/items/{}", env.movie_a_id), None),
    ] {
        let (st, err) = send_json(
            &env.app,
            method,
            &format!("/api/v1/playlists/{pl_id}{suffix}"),
            Some(&env.bob_token),
            None,
            payload,
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND, "failed on {method} {suffix}");
        assert_eq!(err["error"], "Playlist not found");
    }

    // Alice makes playlist public and clears description with ""
    let (s_pub, pub_sum) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        Some(json!({ "is_public": true, "description": "   " })),
    )
    .await;
    assert_eq!(s_pub, StatusCode::OK);
    assert_eq!(pub_sum["is_public"], true);
    assert!(pub_sum.get("description").is_none());

    let ev_upd = rx
        .try_recv()
        .expect("expected PlaylistUpdated updated event");
    assert!(matches!(
        ev_upd,
        SystemEvent::PlaylistUpdated { ref action, .. } if action == "updated"
    ));

    // Bob can now view & gets HIS OWN playback progress (movie_a InProgress -> 1400, movie_b Completed -> None)
    let (s_bob, bob_detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(s_bob, StatusCode::OK);
    assert_eq!(bob_detail["summary"]["is_editable"], false);
    assert_eq!(bob_detail["items"][0]["resume_position_seconds"], 1400);
    assert!(bob_detail["items"][1]
        .get("resume_position_seconds")
        .is_none());

    // Bob cannot mutate public playlist -> 403 Forbidden
    for (method, suffix, payload) in [
        ("PATCH", String::new(), Some(json!({ "name": "Nope" }))),
        ("DELETE", String::new(), None),
        (
            "POST",
            "/items".to_string(),
            Some(json!({ "item_ids": [env.movie_a_id] })),
        ),
        (
            "PUT",
            "/items/order".to_string(),
            Some(json!({ "item_ids": [env.movie_b_id, env.movie_a_id] })),
        ),
        ("DELETE", format!("/items/{}", env.movie_a_id), None),
    ] {
        let (st, err) = send_json(
            &env.app,
            method,
            &format!("/api/v1/playlists/{pl_id}{suffix}"),
            Some(&env.bob_token),
            None,
            payload,
        )
        .await;
        assert_eq!(
            st,
            StatusCode::FORBIDDEN,
            "expected 403 on {method} {suffix}"
        );
        assert_eq!(err["error"], "Forbidden");
    }

    // Admin sees is_editable: true and can delete Alice's playlist
    let (_, adm_detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(adm_detail["summary"]["is_editable"], true);

    let (s_adm_del, _) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(s_adm_del, StatusCode::NO_CONTENT);

    let ev_del = rx
        .try_recv()
        .expect("expected PlaylistUpdated deleted event");
    assert!(matches!(
        ev_del,
        SystemEvent::PlaylistUpdated { ref action, .. } if action == "deleted"
    ));
}

#[tokio::test]
async fn batch_show_season_and_sandboxed_folder_expansion() {
    let env = setup_env().await;

    let (_, created) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({ "name": "Binge Queue" })),
    )
    .await;
    let pl_id = created["summary"]["id"].as_str().unwrap().to_string();

    // Empty batch add rejected -> 400
    let (st_empty, err_empty) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({})),
    )
    .await;
    assert_eq!(st_empty, StatusCode::BAD_REQUEST);
    assert_eq!(err_empty["error"], "No playable media items to add");

    // Season item rejected -> 400
    let (st_season, err_season) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [env.season_item_id] })),
    )
    .await;
    assert_eq!(st_season, StatusCode::BAD_REQUEST);
    assert_eq!(err_season["error"], "Non-playable media item type");

    // Non-existent item rejected -> 404
    let (st_missing, _) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [999_999] })),
    )
    .await;
    assert_eq!(st_missing, StatusCode::NOT_FOUND);

    // Expand Season 1 only -> S01E01 then S01E02 (sorted even though S01E02 was inserted first)
    let (st, res) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({ "show": { "show_id": env.show_id, "season_number": 1 } })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["added_count"], 2);

    // Expand entire Show via item_ids -> skips S01E01 & S01E02, appends S02E01
    let (st2, res2) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [env.show_id] })),
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(res2["added_count"], 1);
    assert_eq!(res2["skipped_duplicate_count"], 2);

    // Non-recursive folder add -> adds Season 01/01_pilot.mkv only
    let (st3, res3) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({
            "folder": {
                "library_id": env.folder_lib_id,
                "path": "Season 01",
                "recursive": false
            }
        })),
    )
    .await;
    assert_eq!(st3, StatusCode::OK);
    assert_eq!(res3["added_count"], 1);

    // Recursive folder add -> skips pilot & hidden .hidden dir, adds Bonus/02_making_of.mp4
    let (st4, res4) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({
            "folder": {
                "library_id": env.folder_lib_id,
                "path": "Season 01",
                "recursive": true
            }
        })),
    )
    .await;
    assert_eq!(st4, StatusCode::OK);
    assert_eq!(res4["added_count"], 1);
    assert_eq!(res4["skipped_duplicate_count"], 1);

    // Path traversal & absolute path rejected with 400 Invalid folder path
    for bad_path in ["../outside", "/etc/passwd", "Season 01\0bad"] {
        let (st_trav, err_trav) = send_json(
            &env.app,
            "POST",
            &format!("/api/v1/playlists/{pl_id}/items"),
            Some(&env.alice_token),
            None,
            Some(json!({
                "folder": {
                    "library_id": env.folder_lib_id,
                    "path": bad_path,
                    "recursive": true
                }
            })),
        )
        .await;
        assert_eq!(st_trav, StatusCode::BAD_REQUEST);
        assert_eq!(err_trav["error"], "Invalid folder path");
    }

    let (_, detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    let ids: Vec<i64> = detail["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["card"]["id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            env.ep_s1e1_id,
            env.ep_s1e2_id,
            env.ep_s2e1_id,
            env.folder_ep1_id,
            env.folder_ep2_id
        ]
    );
}

#[tokio::test]
async fn private_library_isolation_reorder_and_widget_ast_resolution() {
    let env = setup_env().await;
    let (unlock_tok, _) = env.unlock_svc.generate_token(&env.private_lib_id).unwrap();

    // Adding locked item without unlock token fails with 403 LIBRARY_LOCKED and does NOT create orphan playlist
    let (st_fail, err_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({
            "name": "Should Not Exist",
            "items": { "item_ids": [env.private_movie_id] }
        })),
    )
    .await;
    assert_eq!(st_fail, StatusCode::FORBIDDEN);
    assert_eq!(err_body["error"], "LIBRARY_LOCKED");

    let (_, list_check) = send_json(
        &env.app,
        "GET",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(list_check["playlists"].as_array().unwrap().len(), 0);

    // Create with unlock token containing [movie_a, private_movie, movie_b]
    let (st_ok, created) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        Some(&unlock_tok),
        Some(json!({
            "name": "Mixed Vault",
            "is_public": true,
            "items": { "item_ids": [env.movie_a_id, env.private_movie_id, env.movie_b_id] }
        })),
    )
    .await;
    assert_eq!(st_ok, StatusCode::CREATED);
    let pl_id = created["summary"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["summary"]["visible_item_count"], 3);
    assert_eq!(created["summary"]["locked_item_count"], 0);

    // Without unlock token: locked item hidden from items, posters, duration, and contains_item_id
    let (_, locked_detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(locked_detail["summary"]["visible_item_count"], 2);
    assert_eq!(locked_detail["summary"]["locked_item_count"], 1);
    assert_eq!(locked_detail["summary"]["total_duration_seconds"], 16000);
    assert_eq!(locked_detail["items"].as_array().unwrap().len(), 2);
    assert_eq!(locked_detail["items"][0]["position"], 0);
    assert_eq!(locked_detail["items"][1]["position"], 1);
    let raw_str = locked_detail.to_string();
    assert!(!raw_str.contains("Classified Cut"));
    assert!(!raw_str.contains(&format!("/api/v1/artwork/{}/poster", env.private_movie_id)));

    // contains_item_id for locked item without token returns false; for visible item returns true
    let (_, contains_locked) = send_json(
        &env.app,
        "GET",
        &format!(
            "/api/v1/playlists?contains_item_id={}",
            env.private_movie_id
        ),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(
        contains_locked["playlists"][0]["contains_queried_item"],
        false
    );

    let (_, contains_visible) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists?contains_item_id={}", env.movie_a_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(
        contains_visible["playlists"][0]["contains_queried_item"],
        true
    );

    // Deleting locked item without token returns 403 LIBRARY_LOCKED
    let (st_del_locked, del_err) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}/items/{}", env.private_movie_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_del_locked, StatusCode::FORBIDDEN);
    assert_eq!(del_err["error"], "LIBRARY_LOCKED");

    // Invalid reorder rejected with exact spec message
    let (st_bad_reord, bad_reord_err) = send_json(
        &env.app,
        "PUT",
        &format!("/api/v1/playlists/{pl_id}/items/order"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [env.movie_b_id] })),
    )
    .await;
    assert_eq!(st_bad_reord, StatusCode::BAD_REQUEST);
    assert_eq!(
        bad_reord_err["error"],
        "item_ids must be an exact permutation of visible playlist items"
    );

    // Reorder visible items [movie_b, movie_a] while locked -> preserves private_movie_id
    let (st_reord, _) = send_json(
        &env.app,
        "PUT",
        &format!("/api/v1/playlists/{pl_id}/items/order"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [env.movie_b_id, env.movie_a_id] })),
    )
    .await;
    assert_eq!(st_reord, StatusCode::OK);

    // Unlock again -> all 3 items intact: [movie_b, movie_a, private_movie]
    let (_, unlocked_after) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        Some(&unlock_tok),
        None,
    )
    .await;
    let after_ids: Vec<i64> = unlocked_after["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["card"]["id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        after_ids,
        vec![env.movie_b_id, env.movie_a_id, env.private_movie_id]
    );

    // Pin playlist as Carousel on Home screen and verify AST hydration + pagination + privacy + graceful fallback
    let home = ScreenLayout::new(
        ScreenId::Home,
        "Home",
        vec![WidgetNode::Carousel {
            id: "pl_rail".to_string(),
            title: "Pinned Playlist".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::PlaylistItems {
                playlist_id: pl_id.clone(),
            })
            .with_limit(1),
            items: None,
            next_cursor: None,
        }],
    );
    env.layout_registry
        .save_screen(home, &env._temp_dir.path().join("screens"))
        .unwrap();

    let (st_screen, screen_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_screen, StatusCode::OK);
    assert_eq!(screen_val["widgets"][0]["items"][0]["id"], env.movie_b_id);
    assert_eq!(screen_val["widgets"][0]["next_cursor"], "1");

    let (st_page2, page2_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/widgets/pl_rail/data?screen_id=home&offset=1&limit=1",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_page2, StatusCode::OK);
    assert_eq!(page2_val["items"][0]["id"], env.movie_a_id);
    assert!(page2_val.get("next_cursor").is_none()); // locked item excluded so total = 2

    // Make playlist private -> Bob gets graceful empty rail on Home screen
    let _ = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        Some(json!({ "is_public": false })),
    )
    .await;
    let (st_bob_screen, bob_screen) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_bob_screen, StatusCode::OK);
    assert_eq!(
        bob_screen["widgets"][0]["items"].as_array().unwrap().len(),
        0
    );

    // Remove movie_b from playlist -> 200 OK, and repeating remove -> 404
    let (st_rem, rem_sum) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}/items/{}", env.movie_b_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_rem, StatusCode::OK);
    assert_eq!(rem_sum["visible_item_count"], 1);

    let (st_rem_again, _) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}/items/{}", env.movie_b_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_rem_again, StatusCode::NOT_FOUND);

    // Delete playlist -> Home screen gracefully resolves empty items instead of failing
    let _ = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    let (st_after_del, screen_after_del) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_after_del, StatusCode::OK);
    assert_eq!(
        screen_after_del["widgets"][0]["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
