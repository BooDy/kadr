use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::get,
    Extension, Json, Router,
};
use http_body_util::BodyExt;
use kadr_core::ast::ItemDetailsPayload;
use kadr_core::events::SystemEvent;
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::create_router_with_events;
use kadr_server::api::UnlockTokenService;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::metadata::{ArtworkDownloader, MetadataService, TmdbClient};
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
use tokio::sync::RwLock;
use tower::ServiceExt;

type RecordedAuth = (Option<String>, Option<String>);

#[derive(Clone, Default)]
struct MockTmdbState {
    recorded_auth: Arc<Mutex<Vec<RecordedAuth>>>,
    recorded_languages: Arc<Mutex<Vec<String>>>,
}

/// Minimal valid JPEG magic bytes (`FF D8 FF E0 ...`) and PNG magic bytes (`89 50 4E 47 ...`).
const FAKE_JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00,
];
const FAKE_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00,
];

async fn spawn_mock_tmdb_server() -> (String, String, MockTmdbState, tokio::task::JoinHandle<()>) {
    let state = MockTmdbState::default();
    let app_state = state.clone();

    let router = Router::new()
        .route(
            "/3/search/movie",
            get(
                |State(st): State<MockTmdbState>,
                 headers: HeaderMap,
                 Query(params): Query<HashMap<String, String>>| async move {
                    let api_key_q = params.get("api_key").cloned();
                    let auth_hdr = headers
                        .get(header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(ToOwned::to_owned);
                    st.recorded_auth
                        .lock()
                        .unwrap()
                        .push((api_key_q.clone(), auth_hdr.clone()));
                    if let Some(lang) = params.get("language") {
                        st.recorded_languages.lock().unwrap().push(lang.clone());
                    }

                    if api_key_q.is_none() && auth_hdr.is_none() {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({ "status_message": "Invalid API key" })),
                        )
                            .into_response();
                    }

                    let q = params.get("query").map(String::as_str).unwrap_or("");
                    if q.to_lowercase().contains("nomatch") {
                        return Json(json!({ "results": [] })).into_response();
                    }

                    Json(json!({
                        "results": [
                            {
                                "id": 603,
                                "title": "The Matrix",
                                "original_title": "The Matrix",
                                "release_date": "1999-03-30",
                                "overview": "A computer hacker learns from mysterious rebels about the true nature of his reality.",
                                "vote_average": 8.7,
                                "poster_path": "/matrix_poster.jpg",
                                "backdrop_path": "/matrix_backdrop.png"
                            }
                        ]
                    }))
                    .into_response()
                },
            ),
        )
        .route(
            "/3/search/tv",
            get(
                |State(st): State<MockTmdbState>,
                 headers: HeaderMap,
                 Query(params): Query<HashMap<String, String>>| async move {
                    let api_key_q = params.get("api_key").cloned();
                    let auth_hdr = headers
                        .get(header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(ToOwned::to_owned);
                    st.recorded_auth
                        .lock()
                        .unwrap()
                        .push((api_key_q, auth_hdr));

                    Json(json!({
                        "results": [
                            {
                                "id": 1396,
                                "name": "Breaking Bad",
                                "original_name": "Breaking Bad",
                                "first_air_date": "2008-01-20",
                                "overview": "A chemistry teacher diagnosed with inoperable lung cancer turns to manufacturing methamphetamine.",
                                "vote_average": 8.9,
                                "poster_path": "/bb_poster.jpg",
                                "backdrop_path": "/bb_backdrop.png"
                            }
                        ]
                    }))
                    .into_response()
                },
            ),
        )
        .route(
            "/3/movie/{id}",
            get(
                |Path(id): Path<i64>,
                 State(st): State<MockTmdbState>,
                 headers: HeaderMap,
                 Query(params): Query<HashMap<String, String>>| async move {
                    let api_key_q = params.get("api_key").cloned();
                    let auth_hdr = headers
                        .get(header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(ToOwned::to_owned);
                    st.recorded_auth
                        .lock()
                        .unwrap()
                        .push((api_key_q, auth_hdr));

                    if id != 603 {
                        return (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))
                            .into_response();
                    }

                    Json(json!({
                        "id": 603,
                        "title": "The Matrix",
                        "original_title": "The Matrix",
                        "release_date": "1999-03-30",
                        "overview": "A computer hacker learns from mysterious rebels about the true nature of his reality.",
                        "original_language": "en",
                        "vote_average": 8.7,
                        "poster_path": "/matrix_poster.jpg",
                        "backdrop_path": "/matrix_backdrop.png",
                        "genres": [{ "name": "Action" }, { "name": "Science Fiction" }],
                        "production_companies": [{ "name": "Village Roadshow Pictures" }],
                        "origin_country": ["US"],
                        "credits": {
                            "cast": [
                                { "name": "Keanu Reeves", "order": 0 },
                                { "name": "Laurence Fishburne", "order": 1 },
                                { "name": "Carrie-Anne Moss", "order": 2 }
                            ],
                            "crew": [
                                { "name": "Lana Wachowski", "job": "Director", "department": "Directing" },
                                { "name": "Lilly Wachowski", "job": "Director", "department": "Directing" },
                                { "name": "Lana Wachowski", "job": "Writer", "department": "Writing" }
                            ]
                        }
                    }))
                    .into_response()
                },
            ),
        )
        .route(
            "/3/tv/{id}",
            get(|Path(id): Path<i64>| async move {
                Json(json!({
                    "id": id,
                    "name": "Breaking Bad",
                    "original_name": "Breaking Bad",
                    "first_air_date": "2008-01-20",
                    "overview": "Walter White turns to a life of crime.",
                    "original_language": "en",
                    "vote_average": 8.9,
                    "poster_path": "/bb_poster.jpg",
                    "backdrop_path": "/bb_backdrop.png",
                    "genres": [{ "name": "Drama" }, { "name": "Crime" }],
                    "production_companies": [{ "name": "High Bridge Productions" }],
                    "origin_country": ["US"],
                    "created_by": [{ "name": "Vince Gilligan" }],
                    "credits": {
                        "cast": [
                            { "name": "Bryan Cranston", "order": 0 },
                            { "name": "Aaron Paul", "order": 1 }
                        ],
                        "crew": [
                            { "name": "Peter Gould", "job": "Writer", "department": "Writing" }
                        ]
                    }
                }))
            }),
        )
        .route(
            "/t/p/{size}/{file}",
            get(|Path((_size, file)): Path<(String, String)>| async move {
                if file.ends_with(".png") {
                    ([(header::CONTENT_TYPE, "image/png")], FAKE_PNG).into_response()
                } else {
                    ([(header::CONTENT_TYPE, "image/jpeg")], FAKE_JPEG).into_response()
                }
            }),
        )
        .route(
            "/custom/poster.jpg",
            get(|| async move { ([(header::CONTENT_TYPE, "image/jpeg")], FAKE_JPEG) }),
        )
        .route(
            "/custom/not_an_image.txt",
            get(|| async move { ([(header::CONTENT_TYPE, "text/plain")], "hello world") }),
        )
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");
    let image_base_url = format!("http://{addr}/t/p");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    (base_url, image_base_url, state, handle)
}

struct TestEnv {
    _temp: TempDir,
    data_dir: std::path::PathBuf,
    app: Router,
    metadata_svc: Arc<MetadataService>,
    media_repo: MediaItemRepository,
    lib_repo: LibraryRepository,
    event_bus: Arc<EventBus>,
    admin_token: String,
    standard_token: String,
    private_unlock_token: String,
    movie_id: i64,
    private_movie_id: i64,
    show_id: i64,
    mock_base_url: String,
    mock_state: MockTmdbState,
    _mock_handle: tokio::task::JoinHandle<()>,
}

async fn setup_env(allow_loopback_artwork: bool) -> TestEnv {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("kadr.db");

    let pool = create_pool(&db_path, 4).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let pin_hash = hash_pin("1234").unwrap();
    let admin = User {
        id: "admin-1".to_string(),
        username: "admin".to_string(),
        pin_hash: pin_hash.clone(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    let standard = User {
        id: "user-1".to_string(),
        username: "viewer".to_string(),
        pin_hash: pin_hash.clone(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    user_repo.create(&admin).await.unwrap();
    user_repo.create(&standard).await.unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public Movies".to_string(),
            path: temp.path().join("pub"),
            media_type: MediaType::Movie,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-priv".to_string(),
            name: "Private Vault".to_string(),
            path: temp.path().join("priv"),
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some(pin_hash),
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let pub_movie_path = temp.path().join("pub/The.Matrix.1999.mkv");
    let priv_movie_path = temp.path().join("priv/Secret.Movie.mkv");
    let show_path = temp.path().join("pub/Breaking.Bad");

    media_repo
        .upsert_batch(&[
            MediaItem {
                id: None,
                library_id: "lib-pub".to_string(),
                item_type: MediaType::Movie,
                title: "The Matrix".to_string(),
                original_title: None,
                release_year: Some(1999),
                added_at: 1000,
                file_path: pub_movie_path.clone(),
                file_name: "The.Matrix.1999.mkv".to_string(),
                file_size: 4096,
                technical: TechnicalInfo {
                    duration_seconds: 8160,
                    resolution: Some("1080p".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                locked_fields: Vec::new(),
            },
            MediaItem {
                id: None,
                library_id: "lib-priv".to_string(),
                item_type: MediaType::Movie,
                title: "Secret Movie".to_string(),
                original_title: None,
                release_year: Some(2022),
                added_at: 1000,
                file_path: priv_movie_path.clone(),
                file_name: "Secret.Movie.mkv".to_string(),
                file_size: 2048,
                technical: TechnicalInfo::default(),
                metadata: MediaMetadata::default(),
                locked_fields: Vec::new(),
            },
            MediaItem {
                id: None,
                library_id: "lib-pub".to_string(),
                item_type: MediaType::Show,
                title: "Breaking Bad".to_string(),
                original_title: None,
                release_year: Some(2008),
                added_at: 1000,
                file_path: show_path.clone(),
                file_name: String::new(),
                file_size: 0,
                technical: TechnicalInfo::default(),
                metadata: MediaMetadata::default(),
                locked_fields: Vec::new(),
            },
        ])
        .await
        .unwrap();

    let movie_id = media_repo
        .find_by_path(&pub_movie_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();
    let private_movie_id = media_repo
        .find_by_path(&priv_movie_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();
    let show_id = media_repo
        .find_by_path(&show_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();

    let secret = "test-jwt-secret-metadata-tmdb-0123456789";
    let jwt_svc = JwtService::new(secret, 3600);
    let admin_token = jwt_svc.generate_token(&admin).unwrap();
    let standard_token = jwt_svc.generate_token(&standard).unwrap();
    let unlock_svc = UnlockTokenService::new(secret.as_bytes(), 3600);
    let (private_unlock_token, _) = unlock_svc.generate_token("lib-priv").unwrap();

    let limiter = RateLimiter::new(20, Duration::from_secs(60), Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        data_dir.join("subs"),
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        db_path.clone(),
        session_registry.clone(),
        event_bus.clone(),
    ));

    let (mock_base_url, mock_image_url, mock_state, mock_handle) = spawn_mock_tmdb_server().await;

    let mut cfg = AppConfig::default();
    cfg.server.data_dir = data_dir.clone();
    let shared_cfg = Arc::new(RwLock::new(cfg));

    let metadata_svc = Arc::new(
        MetadataService::new(media_repo.clone(), shared_cfg)
            .with_tmdb_client(TmdbClient::with_urls(&mock_base_url, &mock_image_url))
            .with_artwork_downloader(ArtworkDownloader::new(
                data_dir.join("artwork"),
                allow_loopback_artwork,
            )),
    );

    let app = create_router_with_events(
        user_repo,
        playback_repo,
        media_repo.clone(),
        lib_repo.clone(),
        jwt_svc,
        limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry_collector,
    )
    .layer(Extension(metadata_svc.clone()));

    TestEnv {
        _temp: temp,
        data_dir,
        app,
        metadata_svc,
        media_repo,
        lib_repo,
        event_bus,
        admin_token,
        standard_token,
        private_unlock_token,
        movie_id,
        private_movie_id,
        show_id,
        mock_base_url,
        mock_state,
        _mock_handle: mock_handle,
    }
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    unlocked: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    if let Some(unl) = unlocked {
        builder = builder.header("X-Kadr-Unlocked", unl);
    }
    let req_body = if let Some(b) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
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
    let json_val = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
    (status, json_val)
}

#[tokio::test]
async fn metadata_and_tmdb_routes_enforce_admin_rbac_and_private_library_locks() {
    let env = setup_env(true).await;

    // 1. Unauthenticated -> 401
    let (status, _) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", env.movie_id),
        None,
        None,
        Some(json!({ "title": "Hack" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 2. Standard (non-admin) user -> 403 Forbidden across all mutation & TMDb endpoints
    for (method, uri, body) in [
        (
            "PATCH",
            format!("/api/v1/items/{}/metadata", env.movie_id),
            Some(json!({ "title": "Hack" })),
        ),
        ("GET", "/api/v1/admin/settings/metadata".to_string(), None),
        (
            "PUT",
            "/api/v1/admin/settings/metadata".to_string(),
            Some(json!({ "enabled": true })),
        ),
        (
            "GET",
            format!("/api/v1/items/{}/tmdb/search", env.movie_id),
            None,
        ),
        (
            "POST",
            format!("/api/v1/items/{}/tmdb/apply", env.movie_id),
            Some(json!({ "tmdb_id": 603, "media_type": "movie" })),
        ),
        (
            "POST",
            "/api/v1/admin/libraries/lib-pub/tmdb/enrich".to_string(),
            None,
        ),
    ] {
        let (st, _) = send_json(
            &env.app,
            method,
            &uri,
            Some(&env.standard_token),
            None,
            body,
        )
        .await;
        assert_eq!(
            st,
            StatusCode::FORBIDDEN,
            "Expected 403 for standard user on {method} {uri}"
        );
    }

    // 3. Admin accessing locked private library without unlock token -> 403 LIBRARY_LOCKED
    let (st_patch, err_patch) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", env.private_movie_id),
        Some(&env.admin_token),
        None,
        Some(json!({ "title": "Edited Secret" })),
    )
    .await;
    assert_eq!(st_patch, StatusCode::FORBIDDEN);
    assert_eq!(err_patch["error"], "LIBRARY_LOCKED");

    let (st_enrich, err_enrich) = send_json(
        &env.app,
        "POST",
        "/api/v1/admin/libraries/lib-priv/tmdb/enrich",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_enrich, StatusCode::FORBIDDEN);
    assert_eq!(err_enrich["error"], "LIBRARY_LOCKED");

    // 4. Admin WITH valid unlock token succeeds!
    let (st_ok, body_ok) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", env.private_movie_id),
        Some(&env.admin_token),
        Some(&env.private_unlock_token),
        Some(json!({ "title": "Unlocked Edited Secret" })),
    )
    .await;
    assert_eq!(st_ok, StatusCode::OK);
    assert_eq!(body_ok["card"]["title"], "Unlocked Edited Secret");
}

#[tokio::test]
async fn patch_metadata_downloads_offline_artwork_emits_event_and_guards_ssrf() {
    let env = setup_env(true).await;
    let mut rx = env.event_bus.subscribe();

    let poster_url = format!("{}/custom/poster.jpg", env.mock_base_url);
    let (status, body) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", env.movie_id),
        Some(&env.admin_token),
        None,
        Some(json!({
            "title": "The Matrix Reloaded Title",
            "director": "The Wachowskis",
            "genres": ["Sci-Fi", "Action"],
            "rating": 8.8,
            "poster_url": poster_url,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let payload: ItemDetailsPayload = serde_json::from_value(body).unwrap();
    assert_eq!(payload.card.title, "The Matrix Reloaded Title");
    assert_eq!(payload.director.as_deref(), Some("The Wachowskis"));
    assert_eq!(payload.genres, vec!["Sci-Fi", "Action"]);
    assert!(payload.locked_fields.contains(&"title".to_string()));
    assert!(payload.locked_fields.contains(&"poster_path".to_string()));

    // Verify local cached artwork file exists on disk under `<data_dir>/artwork/`
    let cached_file = env
        .data_dir
        .join("artwork")
        .join(format!("item_{}_poster.jpg", env.movie_id));
    assert!(
        cached_file.exists(),
        "Poster should be cached locally on disk"
    );

    // Verify `/api/v1/artwork/{id}/poster` serves the locally cached poster offline
    let art_resp = env
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/artwork/{}/poster", env.movie_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(art_resp.status(), StatusCode::OK);
    assert_eq!(
        art_resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg"
    );
    let art_bytes = art_resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&art_bytes[..], FAKE_JPEG);

    // Verify SystemEvent::LibraryUpdated was emitted
    let ev = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        ev,
        SystemEvent::LibraryUpdated { ref library_id, .. } if library_id == "lib-pub"
    ));

    // Non-image content is rejected
    let bad_img_url = format!("{}/custom/not_an_image.txt", env.mock_base_url);
    let (bad_st, _) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", env.movie_id),
        Some(&env.admin_token),
        None,
        Some(json!({ "poster_url": bad_img_url })),
    )
    .await;
    assert_eq!(bad_st, StatusCode::BAD_REQUEST);

    // SSRF guard: when loopback override is false, http:// and https://127.0.0.1 are blocked!
    let strict_env = setup_env(false).await;
    let (ssrf_http_st, _) = send_json(
        &strict_env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", strict_env.movie_id),
        Some(&strict_env.admin_token),
        None,
        Some(json!({ "poster_url": "http://127.0.0.1:9999/poster.jpg" })),
    )
    .await;
    assert_eq!(ssrf_http_st, StatusCode::BAD_REQUEST);

    let (ssrf_priv_st, ssrf_priv_body) = send_json(
        &strict_env.app,
        "PATCH",
        &format!("/api/v1/items/{}/metadata", strict_env.movie_id),
        Some(&strict_env.admin_token),
        None,
        Some(json!({ "poster_url": "https://127.0.0.1/poster.jpg" })),
    )
    .await;
    assert_eq!(ssrf_priv_st, StatusCode::BAD_REQUEST);
    assert_eq!(ssrf_priv_body["error"], "BLOCKED_URL");
}

#[tokio::test]
async fn tmdb_settings_mask_key_and_support_v3_query_key_and_v4_bearer_jwt() {
    let env = setup_env(true).await;

    // 1. Default settings
    let (st0, s0) = send_json(
        &env.app,
        "GET",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(st0, StatusCode::OK);
    assert_eq!(s0["enabled"], false);
    assert_eq!(s0["language"], "en-US");
    assert_eq!(s0["api_key_configured"], false);
    assert!(s0["api_key_hint"].is_null());

    // 2. Configure v3 32-hex API key -> GET never exposes raw key!
    let v3_key = "0123456789abcdef0123456789abcdef";
    let (st1, s1) = send_json(
        &env.app,
        "PUT",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        Some(json!({
            "enabled": true,
            "auto_fetch_on_scan": true,
            "language": "ar-EG",
            "api_key": format!("  {v3_key}  "),
        })),
    )
    .await;
    assert_eq!(st1, StatusCode::OK);
    assert_eq!(s1["enabled"], true);
    assert_eq!(s1["auto_fetch_on_scan"], true);
    assert_eq!(s1["language"], "ar-EG");
    assert_eq!(s1["api_key_configured"], true);
    assert_eq!(s1["api_key_hint"], "••••cdef");
    assert!(!s1.to_string().contains(v3_key));

    let (_, get_check) = send_json(
        &env.app,
        "GET",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert!(!get_check.to_string().contains(v3_key));

    // Search with v3 key -> mock server receives `?api_key=...` and NO Authorization header
    let (search_st, search_body) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/items/{}/tmdb/search", env.movie_id),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(search_st, StatusCode::OK);
    assert_eq!(search_body["results"][0]["tmdb_id"], 603);
    {
        let recorded = env.mock_state.recorded_auth.lock().unwrap();
        let last = recorded.last().unwrap();
        assert_eq!(last.0.as_deref(), Some(v3_key));
        assert!(last.1.is_none());
        let langs = env.mock_state.recorded_languages.lock().unwrap();
        assert_eq!(langs.last().map(String::as_str), Some("ar-EG"));
    }

    // 3. Configure v4 JWT Read Access Token -> mock server receives `Authorization: Bearer ...` and NO `api_key` query param
    let v4_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJ0bWRiLXY0LXRlc3QifQ.signature1234";
    let (st2, s2) = send_json(
        &env.app,
        "PUT",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        Some(json!({ "api_key": v4_jwt })),
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(s2["api_key_hint"], "••••1234");
    assert!(!s2.to_string().contains(v4_jwt));

    let (search2_st, _) = send_json(
        &env.app,
        "GET",
        &format!(
            "/api/v1/items/{}/tmdb/search?q=Matrix&year=1999",
            env.movie_id
        ),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(search2_st, StatusCode::OK);
    {
        let recorded = env.mock_state.recorded_auth.lock().unwrap();
        let last = recorded.last().unwrap();
        assert!(last.0.is_none());
        assert_eq!(last.1.as_deref(), Some(format!("Bearer {v4_jwt}").as_str()));
    }

    // 4. Empty string clears key
    let (_, cleared) = send_json(
        &env.app,
        "PUT",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        Some(json!({ "api_key": "   " })),
    )
    .await;
    assert_eq!(cleared["api_key_configured"], false);
    assert!(cleared["api_key_hint"].is_null());
}

#[tokio::test]
async fn tmdb_apply_and_library_batch_enrich_persist_artwork_and_survive_rescan() {
    let env = setup_env(true).await;

    // Enable TMDb
    send_json(
        &env.app,
        "PUT",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        Some(json!({
            "enabled": true,
            "language": "en-US",
            "api_key": "abcdef0123456789abcdef0123456789",
        })),
    )
    .await;

    // 1. Apply TMDb directly on the Movie item
    let (apply_st, apply_body) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/items/{}/tmdb/apply", env.movie_id),
        Some(&env.admin_token),
        None,
        Some(json!({
            "tmdb_id": 603,
            "media_type": "movie",
            "download_artwork": true
        })),
    )
    .await;
    assert_eq!(apply_st, StatusCode::OK);
    let details: ItemDetailsPayload = serde_json::from_value(apply_body).unwrap();
    assert_eq!(details.card.title, "The Matrix");
    assert_eq!(details.card.release_year, Some(1999));
    assert_eq!(details.tmdb_id, Some(603));
    assert_eq!(
        details.director.as_deref(),
        Some("Lana Wachowski, Lilly Wachowski")
    );
    assert_eq!(
        details.actors,
        vec!["Keanu Reeves", "Laurence Fishburne", "Carrie-Anne Moss"]
    );
    assert_eq!(details.studio.as_deref(), Some("Village Roadshow Pictures"));
    assert_eq!(details.country.as_deref(), Some("US"));
    assert!(details.locked_fields.contains(&"title".to_string()));
    assert!(details.locked_fields.contains(&"poster_path".to_string()));
    assert!(details.locked_fields.contains(&"backdrop_path".to_string()));

    // Verify both poster (jpg) and backdrop (png) were downloaded locally
    assert!(env
        .data_dir
        .join("artwork")
        .join(format!("item_{}_poster.jpg", env.movie_id))
        .exists());
    assert!(env
        .data_dir
        .join("artwork")
        .join(format!("item_{}_backdrop.png", env.movie_id))
        .exists());

    // 2. Seed an unenriched movie with no TMDb match + an episode in `lib-pub`, then batch enrich!
    let nomatch_path = env.data_dir.join("nomatch.mkv");
    let ep_path = env.data_dir.join("S01E01.mkv");
    env.media_repo
        .upsert_batch(&[
            MediaItem {
                id: None,
                library_id: "lib-pub".to_string(),
                item_type: MediaType::Movie,
                title: "NoMatchObscureFilm".to_string(),
                release_year: Some(2024),
                added_at: 1001,
                file_path: nomatch_path,
                file_name: "nomatch.mkv".to_string(),
                file_size: 100,
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "lib-pub".to_string(),
                item_type: MediaType::Episode,
                title: "Pilot".to_string(),
                added_at: 1002,
                file_path: ep_path,
                file_name: "S01E01.mkv".to_string(),
                file_size: 100,
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    // `lib-pub` now has 4 items:
    // - movie_id (already has tmdb_id=603 & locked_fields -> skipped)
    // - show_id ("Breaking Bad", unenriched -> matched!)
    // - NoMatchObscureFilm (0 results -> skipped)
    // - Episode "Pilot" (episode -> skipped)
    let (enrich_st, enrich_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/admin/libraries/lib-pub/tmdb/enrich",
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(enrich_st, StatusCode::OK);
    assert_eq!(enrich_body["scanned"], 4);
    assert_eq!(enrich_body["matched"], 1);
    assert_eq!(enrich_body["skipped"], 3);
    assert_eq!(enrich_body["errors"], 0);

    let enriched_show = env
        .media_repo
        .get_by_id(env.show_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        enriched_show.metadata.director.as_deref(),
        Some("Vince Gilligan")
    );
    assert_eq!(
        enriched_show
            .metadata
            .extra
            .get("tmdb_id")
            .and_then(|v| v.as_i64()),
        Some(1396)
    );

    // Rescanning `Breaking Bad` preserves TMDb enriched metadata!
    let mut wiped_show = enriched_show.clone();
    wiped_show.title = "breaking.bad.raw".to_string();
    wiped_show.metadata = MediaMetadata::default();
    wiped_show.locked_fields = Vec::new();
    env.media_repo.upsert_batch(&[wiped_show]).await.unwrap();

    let after_rescan = env
        .media_repo
        .get_by_id(env.show_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_rescan.title, "Breaking Bad");
    assert_eq!(
        after_rescan.metadata.director.as_deref(),
        Some("Vince Gilligan")
    );
    assert_eq!(
        after_rescan
            .metadata
            .extra
            .get("tmdb_id")
            .and_then(|v| v.as_i64()),
        Some(1396)
    );
    let _ = &env.lib_repo;
}

#[tokio::test]
async fn auto_fetch_on_scan_hook_enriches_ingested_items_only_when_opted_in() {
    let env = setup_env(true).await;
    let raw_movie = env
        .media_repo
        .get_by_id(env.movie_id)
        .await
        .unwrap()
        .unwrap();

    // 1. Default (disabled): enrich_ingested_batch is a complete no-op (zero network calls)
    env.metadata_svc
        .enrich_ingested_batch(std::slice::from_ref(&raw_movie), &env.event_bus)
        .await;
    assert!(env.mock_state.recorded_auth.lock().unwrap().is_empty());

    // 2. Enabled with auto_fetch_on_scan = false: still a no-op on scan
    send_json(
        &env.app,
        "PUT",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        Some(json!({
            "enabled": true,
            "auto_fetch_on_scan": false,
            "api_key": "abcdef0123456789abcdef0123456789",
        })),
    )
    .await;
    env.metadata_svc
        .enrich_ingested_batch(std::slice::from_ref(&raw_movie), &env.event_bus)
        .await;
    assert!(env.mock_state.recorded_auth.lock().unwrap().is_empty());

    // 3. Opt-in auto_fetch_on_scan = true: scan batch enriches item, caches artwork, and emits LibraryUpdated
    send_json(
        &env.app,
        "PUT",
        "/api/v1/admin/settings/metadata",
        Some(&env.admin_token),
        None,
        Some(json!({
            "enabled": true,
            "auto_fetch_on_scan": true,
        })),
    )
    .await;

    let mut rx = env.event_bus.subscribe();
    env.metadata_svc
        .enrich_ingested_batch(std::slice::from_ref(&raw_movie), &env.event_bus)
        .await;

    let enriched = env
        .media_repo
        .get_by_id(env.movie_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        enriched
            .metadata
            .extra
            .get("tmdb_id")
            .and_then(|v| v.as_i64()),
        Some(603)
    );
    assert!(enriched.locked_fields.contains(&"title".to_string()));
    assert!(env
        .data_dir
        .join("artwork")
        .join(format!("item_{}_poster.jpg", env.movie_id))
        .exists());

    let ev = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        ev,
        SystemEvent::LibraryUpdated { ref library_id, .. } if library_id == "lib-pub"
    ));
}
