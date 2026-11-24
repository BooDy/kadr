use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures::StreamExt;
use http_body_util::BodyExt;
use kadr_core::ast::{
    QueryMacro, ResolvedLayoutScope, ScreenId, ScreenLayout, TargetDevice, TimeWindow, WidgetNode,
    WidgetQueryBinding,
};
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole};
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
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LayoutOverrideRepository, LibraryRepository, MediaItemRepository, PlaybackRepository,
    SubtitleRepository, UserRepository,
};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::RwLock;
use tower::ServiceExt;

const MOVIES_LIB_UUID: &str = "11111111-2222-3333-4444-555555555555";
const SHOWS_LIB_UUID: &str = "22222222-3333-4444-5555-666666666666";
const VAULT_LIB_UUID: &str = "33333333-4444-5555-6666-777777777777";

struct ScopedLayoutTestEnv {
    temp_dir: TempDir,
    app: axum::Router,
    admin_jwt: String,
    alice_jwt: String,
    bob_jwt: String,
    layout_registry: LayoutRegistry,
    layout_override_repo: LayoutOverrideRepository,
}

async fn setup_scoped_test_env() -> ScopedLayoutTestEnv {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("scoped_layouts_e2e.db");
    let pool = create_pool(&db_path, 4).expect("sqlite pool");
    initialize_database(&pool).await.expect("init db");

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());
    let layout_override_repo = LayoutOverrideRepository::new(pool.clone());

    let admin_user = User {
        id: "u-admin".into(),
        username: "admin".into(),
        pin_hash: hash_pin("1111").unwrap(),
        role: UserRole::Admin,
        created_at: 1_700_000_000,
    };
    let alice_user = User {
        id: "u-alice".into(),
        username: "alice".into(),
        pin_hash: hash_pin("2222").unwrap(),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    let bob_user = User {
        id: "u-bob".into(),
        username: "bob".into(),
        pin_hash: hash_pin("3333").unwrap(),
        role: UserRole::Standard,
        created_at: 1_700_000_000,
    };
    user_repo.create(&admin_user).await.unwrap();
    user_repo.create(&alice_user).await.unwrap();
    user_repo.create(&bob_user).await.unwrap();

    let jwt_svc = JwtService::new("scoped-layouts-e2e-secret-key-at-least-32-bytes!", 3600);
    let admin_jwt = jwt_svc.generate_token(&admin_user).unwrap();
    let alice_jwt = jwt_svc.generate_token(&alice_user).unwrap();
    let bob_jwt = jwt_svc.generate_token(&bob_user).unwrap();

    lib_repo
        .create(
            MOVIES_LIB_UUID,
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
            SHOWS_LIB_UUID,
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
            VAULT_LIB_UUID,
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
                library_id: MOVIES_LIB_UUID.into(),
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
                    container: Some("mkv".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    genres: vec!["Sci-Fi".into(), "Drama".into()],
                    rating: Some(8.2),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: MOVIES_LIB_UUID.into(),
                item_type: MediaType::Movie,
                title: "Blade Runner 2049".into(),
                release_year: Some(2017),
                added_at: 1_700_000_200,
                file_path: PathBuf::from("/media/movies/br2049.mkv"),
                file_name: "br2049.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 9840,
                    resolution: Some("4K".into()),
                    video_codec: Some("hevc".into()),
                    container: Some("mkv".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    genres: vec!["Sci-Fi".into(), "Neo-Noir".into()],
                    rating: Some(8.6),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: VAULT_LIB_UUID.into(),
                item_type: MediaType::Movie,
                title: "Secret Cut".into(),
                release_year: Some(2020),
                added_at: 1_700_000_300,
                file_path: PathBuf::from("/media/vault/secret_cut.mkv"),
                file_name: "secret_cut.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 5100,
                    resolution: Some("1080p".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    genres: vec!["VaultSecret".into()],
                    rating: Some(9.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    let limiter = RateLimiter::new(100, Duration::from_secs(60), Duration::from_secs(60));
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
        db_path,
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
        event_bus,
        telemetry_collector,
        dummy_tx,
        dummy_pipeline,
        config,
        identity,
    );

    ScopedLayoutTestEnv {
        temp_dir,
        app,
        admin_jwt,
        alice_jwt,
        bob_jwt,
        layout_registry,
        layout_override_repo,
    }
}

async fn send_req(
    app: &axum::Router,
    method: &str,
    uri: &str,
    jwt: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = jwt {
        builder = builder.header("Authorization", format!("Bearer {tok}"));
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
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

fn simple_carousel_screen(id: ScreenId, title: &str, widget_id: &str) -> ScreenLayout {
    ScreenLayout::new(
        id,
        title,
        vec![WidgetNode::Carousel {
            id: widget_id.to_string(),
            title: format!("{title} Shelf"),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded).with_limit(5),
            time_window: None,
            items: None,
            next_cursor: None,
        }],
    )
}

async fn recv_sse_event_matching<S>(
    stream: &mut S,
    expected_event: &str,
    timeout_duration: Duration,
) -> Value
where
    S: futures::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Unpin,
{
    let deadline = tokio::time::Instant::now() + timeout_duration;
    let mut buffer = String::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("Timed out waiting for SSE event '{expected_event}'. Buffer:\n{buffer}");
        }

        let chunk = tokio::time::timeout(remaining, stream.next())
            .await
            .expect("SSE chunk timeout")
            .expect("SSE stream ended early")
            .expect("SSE chunk error");

        buffer.push_str(&String::from_utf8_lossy(&chunk));
        let target_prefix = format!("event: {expected_event}");
        if let Some(pos) = buffer.find(&target_prefix) {
            let after_event = &buffer[pos..];
            if let Some(data_idx) = after_event.find("data: ") {
                let data_start = pos + data_idx + 6;
                if let Some(newline_idx) = buffer[data_start..].find('\n') {
                    let json_str = &buffer[data_start..data_start + newline_idx];
                    if let Ok(json_val) = serde_json::from_str::<Value>(json_str.trim()) {
                        return json_val;
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_four_tier_fallback_canonical_library_and_restart_reload() {
    let env = setup_scoped_test_env().await;

    // 0. Factory default Home has is_exact_override: false even for Global+All
    let (status, factory_home_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?unhydrated=true",
        Some(&env.admin_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let factory_home: ScreenLayout = serde_json::from_value(factory_home_val).unwrap();
    assert_eq!(factory_home.title, "Home");
    assert_eq!(
        factory_home.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: None,
            target_device: TargetDevice::All,
            is_exact_override: false,
        })
    );

    // 1. Admin saves Global TV override; Alice saves personal All + personal Mobile overrides
    let global_tv = simple_carousel_screen(ScreenId::Home, "Global TV Home", "global_tv_shelf");
    let (status, saved_gtv_val) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=global&target_device=tv",
        Some(&env.admin_jwt),
        &[],
        Some(serde_json::to_value(&global_tv).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saved_gtv: ScreenLayout = serde_json::from_value(saved_gtv_val).unwrap();
    assert_eq!(
        saved_gtv.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: None,
            target_device: TargetDevice::Tv,
            is_exact_override: true,
        })
    );

    let alice_all = simple_carousel_screen(ScreenId::Home, "Alice All Home", "alice_all_shelf");
    let (status, saved_aall_val) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=me&target_device=all",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_all).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saved_aall: ScreenLayout = serde_json::from_value(saved_aall_val).unwrap();
    assert_eq!(
        saved_aall.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: Some("u-alice".into()),
            target_device: TargetDevice::All,
            is_exact_override: true,
        })
    );

    let alice_mobile =
        simple_carousel_screen(ScreenId::Home, "Alice Mobile Home", "alice_mobile_shelf");
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=me&target_device=mobile",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_mobile).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 2a. Alice on mobile (both query param and X-Kadr-Device header) -> Tier 1 (u-alice, mobile), exact = true
    for (uri, hdrs) in [
        ("/api/v1/screens/home?target_device=mobile", &[][..]),
        ("/api/v1/screens/home", &[("X-Kadr-Device", "mobile")][..]),
    ] {
        let (status, val) = send_req(&env.app, "GET", uri, Some(&env.alice_jwt), hdrs, None).await;
        assert_eq!(status, StatusCode::OK);
        let screen: ScreenLayout = serde_json::from_value(val).unwrap();
        assert_eq!(screen.title, "Alice Mobile Home");
        assert_eq!(
            screen.resolved_scope,
            Some(ResolvedLayoutScope {
                user_id: Some("u-alice".into()),
                target_device: TargetDevice::Mobile,
                is_exact_override: true,
            })
        );
    }

    // 2b. Alice on tv -> Tier 2 (u-alice, all) beats Tier 3 Global TV! is_exact_override = false
    let (status, alice_tv_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?target_device=tv",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let alice_tv: ScreenLayout = serde_json::from_value(alice_tv_val).unwrap();
    assert_eq!(alice_tv.title, "Alice All Home");
    assert_eq!(
        alice_tv.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: Some("u-alice".into()),
            target_device: TargetDevice::All,
            is_exact_override: false,
        })
    );

    // 2c. Bob (no personal overrides) on tv -> Tier 3 (None, tv), is_exact_override = false
    for uri in [
        "/api/v1/screens/home?target_device=tv",
        "/api/v1/screens/home?unhydrated=true&target_device=tv",
    ] {
        let (status, bob_tv_val) =
            send_req(&env.app, "GET", uri, Some(&env.bob_jwt), &[], None).await;
        assert_eq!(status, StatusCode::OK);
        let bob_tv: ScreenLayout = serde_json::from_value(bob_tv_val).unwrap();
        assert_eq!(bob_tv.title, "Global TV Home");
        assert_eq!(
            bob_tv.resolved_scope,
            Some(ResolvedLayoutScope {
                user_id: None,
                target_device: TargetDevice::Tv,
                is_exact_override: false,
            })
        );
    }

    // Admin inspecting Global TV explicitly -> (None, tv), is_exact_override = true
    let (status, admin_gtv_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?unhydrated=true&scope_user_id=global&target_device=tv",
        Some(&env.admin_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let admin_gtv: ScreenLayout = serde_json::from_value(admin_gtv_val).unwrap();
    assert_eq!(admin_gtv.title, "Global TV Home");
    assert_eq!(
        admin_gtv.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: None,
            target_device: TargetDevice::Tv,
            is_exact_override: true,
        })
    );

    // 2d. Bob on web -> Tier 4 (None, all), is_exact_override = false
    let (status, bob_web_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?target_device=web",
        Some(&env.bob_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let bob_web: ScreenLayout = serde_json::from_value(bob_web_val).unwrap();
    assert_eq!(bob_web.title, "Home");
    assert_eq!(
        bob_web.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: None,
            target_device: TargetDevice::All,
            is_exact_override: false,
        })
    );

    // 3. Deleting Alice's mobile override falls back cleanly to (u-alice, all)
    let (status, del_msg) = send_req(
        &env.app,
        "DELETE",
        "/api/v1/screens/home?scope_user_id=me&target_device=mobile",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(del_msg["message"], "Scoped layout override reset");

    let (status, alice_mob_after_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?target_device=mobile",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let alice_mob_after: ScreenLayout = serde_json::from_value(alice_mob_after_val).unwrap();
    assert_eq!(alice_mob_after.title, "Alice All Home");
    assert_eq!(
        alice_mob_after.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: Some("u-alice".into()),
            target_device: TargetDevice::All,
            is_exact_override: false,
        })
    );

    // 4. Canonical library UUID resolution: override saved on "movies" applies to /api/v1/screens/{MOVIES_LIB_UUID}
    let alice_movies_tv = ScreenLayout::new(
        ScreenId::Movies,
        "Alice TV Cinema",
        vec![WidgetNode::DenseList {
            id: "alice_movies_list".into(),
            title: "Cinema List".into(),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded).with_limit(10),
            time_window: None,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    );
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/movies?scope_user_id=me&target_device=tv",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_movies_tv).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, uuid_movies_val) = send_req(
        &env.app,
        "GET",
        &format!("/api/v1/screens/{MOVIES_LIB_UUID}?target_device=tv"),
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let uuid_movies: ScreenLayout = serde_json::from_value(uuid_movies_val).unwrap();
    assert_eq!(uuid_movies.id, ScreenId::Movies);
    assert_eq!(uuid_movies.title, "Alice TV Cinema");
    assert_eq!(
        uuid_movies.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: Some("u-alice".into()),
            target_device: TargetDevice::Tv,
            is_exact_override: true,
        })
    );
    match &uuid_movies.widgets[0] {
        WidgetNode::DenseList { items, .. } => {
            assert_eq!(items.as_ref().unwrap().len(), 2);
        }
        other => panic!("Expected DenseList widget, got {other:?}"),
    }

    // 5. Global+All disk override sets is_exact_override: true and survives simulated restart along with SQLite overrides
    let global_all = simple_carousel_screen(ScreenId::Home, "Disk Custom Home", "disk_home_shelf");
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=global&target_device=all",
        Some(&env.admin_jwt),
        &[],
        Some(serde_json::to_value(&global_all).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let fresh_registry = LayoutRegistry::new();
    fresh_registry
        .load_overrides_from_dir(&env.temp_dir.path().join("screens"))
        .unwrap();
    let stored = env.layout_override_repo.list_all().await.unwrap();
    assert_eq!(stored.len(), 3); // global/tv home, alice/all home, alice/tv movies
    fresh_registry.load_scoped_overrides(stored);

    let reloaded_global = fresh_registry
        .resolve_scoped_screen(&ScreenId::Home, None, TargetDevice::All)
        .unwrap();
    assert_eq!(reloaded_global.title, "Disk Custom Home");
    assert_eq!(
        reloaded_global.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: None,
            target_device: TargetDevice::All,
            is_exact_override: true,
        })
    );

    let reloaded_alice_tv = fresh_registry
        .resolve_scoped_screen(&ScreenId::Home, Some("u-alice"), TargetDevice::Tv)
        .unwrap();
    assert_eq!(reloaded_alice_tv.title, "Alice All Home");

    let reloaded_movies = fresh_registry
        .resolve_scoped_screen(&ScreenId::Movies, Some("u-alice"), TargetDevice::Tv)
        .unwrap();
    assert_eq!(reloaded_movies.title, "Alice TV Cinema");
}

#[tokio::test]
async fn test_rbac_and_private_library_preview_isolation() {
    let env = setup_scoped_test_env().await;

    let sample = simple_carousel_screen(ScreenId::Home, "Unauthorized Edit", "unauth_shelf");
    let sample_val = serde_json::to_value(&sample).unwrap();

    // 1. Standard user (Alice) receives 403 Forbidden on global or other-user mutations & raw inspections
    for (method, uri, has_body) in [
        ("PUT", "/api/v1/screens/home", true),
        (
            "PUT",
            "/api/v1/screens/home?scope_user_id=global&target_device=tv",
            true,
        ),
        ("PUT", "/api/v1/screens/home?scope_user_id=u-bob", true),
        ("DELETE", "/api/v1/screens/home", false),
        ("DELETE", "/api/v1/screens/home?scope_user_id=global", false),
        (
            "DELETE",
            "/api/v1/screens/home?scope_user_id=u-bob&target_device=tv",
            false,
        ),
        (
            "GET",
            "/api/v1/screens/home?unhydrated=true&scope_user_id=global",
            false,
        ),
        (
            "GET",
            "/api/v1/screens/home?unhydrated=true&scope_user_id=u-bob",
            false,
        ),
    ] {
        let (status, _) = send_req(
            &env.app,
            method,
            uri,
            Some(&env.alice_jwt),
            &[],
            has_body.then(|| sample_val.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "Expected 403 Forbidden for Standard user on {method} {uri}"
        );
    }

    // 2. Standard user succeeds on personal unhydrated GET (defaults to me), personal PUT, and widget preview
    let alice_web = simple_carousel_screen(ScreenId::Home, "Alice Web Studio", "alice_web_shelf");
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=me&target_device=web",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_web).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, studio_me_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?unhydrated=true&target_device=web",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let studio_me: ScreenLayout = serde_json::from_value(studio_me_val).unwrap();
    assert_eq!(studio_me.title, "Alice Web Studio");
    assert_eq!(
        studio_me.resolved_scope,
        Some(ResolvedLayoutScope {
            user_id: Some("u-alice".into()),
            target_device: TargetDevice::Web,
            is_exact_override: true,
        })
    );

    // Private library preview fail-closed isolation for Standard user
    let vault_preview_req = json!({
        "binding": {
            "macro_type": "custom_query",
            "limit": 10,
            "filters": {
                "rule_tree": {
                    "kind": "rule",
                    "condition": {
                        "field": "genres",
                        "op": "contains_any",
                        "values": ["VaultSecret"]
                    }
                }
            }
        }
    });
    let (status, locked_preview) = send_req(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.alice_jwt),
        &[],
        Some(vault_preview_req.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(locked_preview["total_count"], 0);
    assert_eq!(locked_preview["items"].as_array().unwrap().len(), 0);

    let (status, unlock_body) = send_req(
        &env.app,
        "POST",
        &format!("/api/v1/libraries/{VAULT_LIB_UUID}/unlock"),
        Some(&env.alice_jwt),
        &[],
        Some(json!({ "pin": "9999" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let unlock_tok = unlock_body["token"].as_str().unwrap();

    let (status, unlocked_preview) = send_req(
        &env.app,
        "POST",
        "/api/v1/widgets/preview",
        Some(&env.alice_jwt),
        &[("X-Kadr-Unlocked", unlock_tok)],
        Some(vault_preview_req),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unlocked_preview["total_count"], 1);
    assert_eq!(unlocked_preview["items"][0]["title"], "Secret Cut");

    // 3. Admin succeeds on behalf of Bob, and receives 404 Not Found for nonexistent user ID
    let bob_curated = simple_carousel_screen(ScreenId::Home, "Bob Curated TV", "bob_tv_shelf");
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=u-bob&target_device=tv",
        Some(&env.admin_jwt),
        &[],
        Some(serde_json::to_value(&bob_curated).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, admin_inspect_bob) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?unhydrated=true&scope_user_id=u-bob&target_device=tv",
        Some(&env.admin_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin_inspect_bob["title"], "Bob Curated TV");
    assert_eq!(admin_inspect_bob["resolved_scope"]["user_id"], "u-bob");
    assert_eq!(
        admin_inspect_bob["resolved_scope"]["is_exact_override"],
        true
    );

    let (status, _) = send_req(
        &env.app,
        "DELETE",
        "/api/v1/screens/home?scope_user_id=u-bob&target_device=tv",
        Some(&env.admin_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for (method, uri, has_body) in [
        (
            "PUT",
            "/api/v1/screens/home?scope_user_id=no-such-user&target_device=tv",
            true,
        ),
        (
            "GET",
            "/api/v1/screens/home?unhydrated=true&scope_user_id=no-such-user",
            false,
        ),
        (
            "DELETE",
            "/api/v1/screens/home?scope_user_id=no-such-user&target_device=tv",
            false,
        ),
    ] {
        let (status, _) = send_req(
            &env.app,
            method,
            uri,
            Some(&env.admin_jwt),
            &[],
            has_body.then(|| sample_val.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "Expected 404 for nonexistent scope_user_id on {method} {uri}"
        );
    }

    // 4. Deleting a custom screen cleans up all scoped overrides for that screen
    let (status, _) = send_req(
        &env.app,
        "POST",
        "/api/v1/screens",
        Some(&env.admin_jwt),
        &[],
        Some(json!({ "id": "custom_scifi", "title": "Sci-Fi Hub" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let alice_scifi = simple_carousel_screen(
        ScreenId::Custom("custom_scifi".into()),
        "Alice Sci-Fi",
        "alice_scifi_shelf",
    );
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/custom_scifi?scope_user_id=me&target_device=tv",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_scifi).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(env
        .layout_override_repo
        .get("custom_scifi", Some("u-alice"), TargetDevice::Tv)
        .await
        .unwrap()
        .is_some());

    let (status, _) = send_req(
        &env.app,
        "DELETE",
        "/api/v1/screens/custom_scifi",
        Some(&env.admin_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(env
        .layout_override_repo
        .get("custom_scifi", Some("u-alice"), TargetDevice::Tv)
        .await
        .unwrap()
        .is_none());
    assert!(env
        .layout_registry
        .resolve_scoped_screen(
            &ScreenId::Custom("custom_scifi".into()),
            Some("u-alice"),
            TargetDevice::Tv,
        )
        .is_none());
}

#[tokio::test]
async fn test_time_window_hydration_and_validation() {
    let env = setup_scoped_test_env().await;

    let scheduled_home = ScreenLayout::new(
        ScreenId::Home,
        "Time Aware Home",
        vec![
            WidgetNode::Carousel {
                id: "daytime_picks".into(),
                title: "Daytime Picks".into(),
                binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded).with_limit(5),
                time_window: Some(TimeWindow {
                    start: "08:00".into(),
                    end: "20:00".into(),
                }),
                items: None,
                next_cursor: None,
            },
            WidgetNode::DenseList {
                id: "late_night_cinema".into(),
                title: "Late Night Cinema".into(),
                binding: WidgetQueryBinding::new(QueryMacro::TopRated).with_limit(5),
                time_window: Some(TimeWindow {
                    start: "23:00".into(),
                    end: "04:00".into(),
                }),
                items: None,
                next_cursor: None,
                total_count: None,
            },
        ],
    );

    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home",
        Some(&env.admin_jwt),
        &[],
        Some(serde_json::to_value(&scheduled_home).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 1. Daytime (?local_time=14:30) returns only daytime_picks
    let (status, day_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?local_time=14:30",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let day_screen: ScreenLayout = serde_json::from_value(day_val).unwrap();
    assert_eq!(day_screen.widgets.len(), 1);
    assert_eq!(day_screen.widgets[0].id(), "daytime_picks");

    // 2. Late-night (?local_time=23:45 and X-Kadr-Local-Time: 02:15) returns only late_night_cinema DenseList with duration_seconds
    for (uri, hdrs) in [
        ("/api/v1/screens/home?local_time=23:45", &[][..]),
        (
            "/api/v1/screens/home",
            &[("X-Kadr-Local-Time", "02:15")][..],
        ),
    ] {
        let (status, night_val) =
            send_req(&env.app, "GET", uri, Some(&env.alice_jwt), hdrs, None).await;
        assert_eq!(status, StatusCode::OK);
        let night_screen: ScreenLayout = serde_json::from_value(night_val).unwrap();
        assert_eq!(night_screen.widgets.len(), 1);
        match &night_screen.widgets[0] {
            WidgetNode::DenseList {
                id,
                items,
                time_window,
                ..
            } => {
                assert_eq!(id, "late_night_cinema");
                assert_eq!(time_window.as_ref().unwrap().start, "23:00");
                let cards = items.as_ref().unwrap();
                assert_eq!(cards.len(), 2);
                assert_eq!(cards[0].title, "Blade Runner 2049");
                assert_eq!(cards[0].duration_seconds, Some(9840));
                assert_eq!(cards[1].title, "Arrival");
                assert_eq!(cards[1].duration_seconds, Some(6960));
            }
            other => panic!("Expected DenseList late_night_cinema, got {other:?}"),
        }
    }

    // 3. Unhydrated GET preserves BOTH widgets with time_window intact
    let (status, raw_val) = send_req(
        &env.app,
        "GET",
        "/api/v1/screens/home?unhydrated=true",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let raw_screen: ScreenLayout = serde_json::from_value(raw_val).unwrap();
    assert_eq!(raw_screen.widgets.len(), 2);
    assert_eq!(
        raw_screen.widgets[0].time_window().unwrap(),
        &TimeWindow {
            start: "08:00".into(),
            end: "20:00".into(),
        }
    );
    assert_eq!(
        raw_screen.widgets[1].time_window().unwrap(),
        &TimeWindow {
            start: "23:00".into(),
            end: "04:00".into(),
        }
    );

    // 4. Invalid local_time, invalid target_device, and start == end time_window return 400 Bad Request
    for (uri, hdrs) in [
        ("/api/v1/screens/home?local_time=99:00", &[][..]),
        (
            "/api/v1/screens/home",
            &[("X-Kadr-Local-Time", "24:00")][..],
        ),
        ("/api/v1/screens/home?target_device=toaster", &[][..]),
        ("/api/v1/screens/home", &[("X-Kadr-Device", "fridge")][..]),
    ] {
        let (status, _) = send_req(&env.app, "GET", uri, Some(&env.alice_jwt), hdrs, None).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected 400 Bad Request for {uri} {hdrs:?}"
        );
    }

    let bad_window_screen = json!({
        "id": "home",
        "title": "Bad Window",
        "widgets": [
            {
                "type": "dense_list",
                "id": "zero_width",
                "title": "Zero Width",
                "binding": { "macro_type": "recently_added", "limit": 5 },
                "time_window": { "start": "12:00", "end": "12:00" }
            }
        ]
    });
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=me",
        Some(&env.alice_jwt),
        &[],
        Some(bad_window_screen),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_scoped_widget_pagination_and_layout_changed_sse() {
    let env = setup_scoped_test_env().await;

    // Subscribe to /api/v1/events SSE stream before saving scoped layouts
    let sse_resp = env
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/events")
                .header(header::AUTHORIZATION, format!("Bearer {}", env.alice_jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sse_resp.status(), StatusCode::OK);
    let mut sse_stream = sse_resp.into_body().into_data_stream();

    // 1. Alice saves personal mobile layout with paginated DenseList (limit = 1)
    let alice_mobile_dense = ScreenLayout::new(
        ScreenId::Home,
        "Alice Mobile Dense",
        vec![WidgetNode::DenseList {
            id: "personal_dense_movies".into(),
            title: "Compact Movies".into(),
            binding: WidgetQueryBinding::new(QueryMacro::LibraryItems {
                library_id: MOVIES_LIB_UUID.into(),
            })
            .with_limit(1)
            .with_sort("title:asc"),
            time_window: None,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    );
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=me&target_device=mobile",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_mobile_dense).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let first_event =
        recv_sse_event_matching(&mut sse_stream, "layout:changed", Duration::from_secs(3)).await;
    assert_eq!(first_event["payload"]["screen_id"], "home");
    assert_eq!(first_event["payload"]["user_id"], "u-alice");
    assert_eq!(first_event["payload"]["target_device"], "mobile");

    // 2. Paginate Alice's personal DenseList widget on mobile: page 1 -> next_cursor preserves target_device=mobile -> page 2
    let (status, page1) = send_req(
        &env.app,
        "GET",
        "/api/v1/widgets/personal_dense_movies/data?screen_id=home&target_device=mobile&offset=0&limit=1",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["items"][0]["title"], "Arrival");
    assert_eq!(page1["items"][0]["duration_seconds"], 6960);
    assert_eq!(page1["total_count"], 2);
    let next_cursor = page1["next_cursor"].as_str().expect("next_cursor URL");
    assert!(
        next_cursor.contains("target_device=mobile"),
        "Expected next_cursor to preserve target_device=mobile, got {next_cursor}"
    );

    let (status, page2) = send_req(
        &env.app,
        "GET",
        "/api/v1/widgets/personal_dense_movies/data?screen_id=home&target_device=mobile&offset=1&limit=1",
        Some(&env.alice_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page2["items"][0]["title"], "Blade Runner 2049");
    assert_eq!(page2["items"][0]["duration_seconds"], 9840);
    assert!(page2["next_cursor"].is_null());

    // Bob does not see Alice's personal widget
    let (status, _) = send_req(
        &env.app,
        "GET",
        "/api/v1/widgets/personal_dense_movies/data?screen_id=home&target_device=mobile&offset=0&limit=1",
        Some(&env.bob_jwt),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 3. Alice saves personal TV layout -> emits SystemEvent::LayoutChanged with user_id & target_device = "tv"
    let alice_tv = simple_carousel_screen(ScreenId::Home, "Alice TV", "alice_tv_shelf");
    let (status, _) = send_req(
        &env.app,
        "PUT",
        "/api/v1/screens/home?scope_user_id=me&target_device=tv",
        Some(&env.alice_jwt),
        &[],
        Some(serde_json::to_value(&alice_tv).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let tv_event =
        recv_sse_event_matching(&mut sse_stream, "layout:changed", Duration::from_secs(3)).await;
    assert_eq!(tv_event["type"], "layout:changed");
    assert_eq!(tv_event["payload"]["screen_id"], "home");
    assert_eq!(tv_event["payload"]["user_id"], "u-alice");
    assert_eq!(tv_event["payload"]["target_device"], "tv");
    assert!(tv_event["payload"]["timestamp"].as_i64().unwrap() > 0);
}
