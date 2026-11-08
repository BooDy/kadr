#![allow(dead_code)]

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// In-process fake plugin bound to 127.0.0.1 on a random port.
#[derive(Clone)]
pub struct MockPlugin {
    pub base_url: String,
    pub manifest: Arc<Mutex<Value>>,
    pub descriptor: Arc<Mutex<Value>>,
    pub expected_secret: Arc<Mutex<Option<String>>>,
    pub healthy: Arc<AtomicBool>,
    pub resolve_calls: Arc<AtomicUsize>,
    pub last_resolve_body: Arc<Mutex<Option<Value>>>,
    pub resolve_delay_ms: Arc<AtomicUsize>,
    /// 0 = respond normally; otherwise respond with this HTTP status and an empty body.
    pub resolve_status: Arc<AtomicUsize>,
}

impl MockPlugin {
    pub fn set_descriptor(&self, v: Value) {
        *self.descriptor.lock().unwrap() = v;
    }
    pub fn set_secret(&self, s: &str) {
        *self.expected_secret.lock().unwrap() = Some(s.to_string());
    }
    pub fn set_manifest(&self, v: Value) {
        *self.manifest.lock().unwrap() = v;
    }
    pub fn media_url(&self) -> String {
        format!("{}/media", self.base_url)
    }
    pub fn calls(&self) -> usize {
        self.resolve_calls.load(Ordering::SeqCst)
    }
}

fn authorized(m: &MockPlugin, headers: &HeaderMap) -> bool {
    match &*m.expected_secret.lock().unwrap() {
        None => true,
        Some(secret) => {
            headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok())
                == Some(format!("Bearer {secret}").as_str())
        }
    }
}

async fn manifest(State(m): State<MockPlugin>) -> Json<Value> {
    Json(m.manifest.lock().unwrap().clone())
}

async fn health(State(m): State<MockPlugin>, headers: HeaderMap) -> StatusCode {
    if !authorized(&m, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if m.healthy.load(Ordering::SeqCst) {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn resolve(
    State(m): State<MockPlugin>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    m.resolve_calls.fetch_add(1, Ordering::SeqCst);
    *m.last_resolve_body.lock().unwrap() = Some(body);
    if !authorized(&m, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let delay = m.resolve_delay_ms.load(Ordering::SeqCst);
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
    }
    let status = m.resolve_status.load(Ordering::SeqCst);
    if status != 0 {
        return StatusCode::from_u16(status as u16).unwrap().into_response();
    }
    Json(m.descriptor.lock().unwrap().clone()).into_response()
}

async fn media(headers: HeaderMap) -> Response {
    const BODY: &[u8] = b"0123456789";
    if let Some(range) = headers
        .get(header::RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|r| r.strip_prefix("bytes="))
    {
        if let Some((a, b)) = range.split_once('-') {
            let start: usize = a.parse().unwrap_or(0);
            let end: usize = b.parse().unwrap_or(BODY.len() - 1).min(BODY.len() - 1);
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "video/mp4")
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/10"))
                .body(Body::from(BODY[start..=end].to_vec()))
                .unwrap();
        }
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::from(BODY.to_vec()))
        .unwrap()
}

pub fn manifest_json(id: &str, permissions: &[&str]) -> Value {
    json!({
        "id": id,
        "name": format!("Mock {id}"),
        "version": "0.1.0",
        "api_version": 1,
        "permissions": permissions,
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    })
}

pub async fn spawn_mock(id: &str, permissions: &[&str]) -> MockPlugin {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mock = MockPlugin {
        base_url: format!("http://{addr}"),
        manifest: Arc::new(Mutex::new(manifest_json(id, permissions))),
        descriptor: Arc::new(Mutex::new(
            json!({ "mode": "direct", "url": "https://cdn.example.com/video.mp4" }),
        )),
        expected_secret: Arc::new(Mutex::new(None)),
        healthy: Arc::new(AtomicBool::new(true)),
        resolve_calls: Arc::new(AtomicUsize::new(0)),
        last_resolve_body: Arc::new(Mutex::new(None)),
        resolve_delay_ms: Arc::new(AtomicUsize::new(0)),
        resolve_status: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/manifest.json", get(manifest))
        .route("/health", get(health))
        .route("/playback/resolve", post(resolve))
        .route("/media", get(media))
        .with_state(mock.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    mock
}

use axum::http::Request;
use http_body_util::BodyExt;
use kadr_core::models::{User, UserRole};
use kadr_server::api::create_router_with_ingest;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::{AppConfig, PluginSettings};
use kadr_server::events::EventBus;
use kadr_server::identity::ServerIdentity;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, PluginRepository,
    SubtitleRepository, UserRepository,
};
use kadr_storage::{create_in_memory_pool, initialize_database};
use std::time::Duration;
use tower::ServiceExt;

pub struct Harness {
    pub app: Router,
    pub media_repo: MediaItemRepository,
    pub lib_repo: LibraryRepository,
    pub plugin_repo: PluginRepository,
    pub playback_repo: PlaybackRepository,
    pub admin_token: String,
    pub user_token: String,
    pub jwt_secret: String,
    pub events: Arc<EventBus>,
}

pub fn enabled_settings() -> PluginSettings {
    PluginSettings {
        enabled: true,
        resolve_timeout_secs: 2,
        allow_private_proxy: true,
    }
}

pub async fn build(settings: PluginSettings) -> Harness {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let plugin_repo = PluginRepository::new(pool.clone());

    let jwt_secret = "test-secret-with-sufficient-length-for-hmac-sha256".to_string();
    let jwt_svc = JwtService::new(&jwt_secret, 3600);
    let mk = |role: UserRole, name: &str| User {
        id: name.to_string(),
        username: name.to_string(),
        pin_hash: String::new(),
        role,
        created_at: 0,
    };
    let admin_token = jwt_svc.generate_token(&mk(UserRole::Admin, "admin")).unwrap();
    let user_token = jwt_svc.generate_token(&mk(UserRole::Standard, "viewer")).unwrap();

    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        std::env::temp_dir().join(format!("kadr-plugin-test-{}", uuid::Uuid::new_v4())),
        SubtitleRepository::new(pool.clone()),
        media_repo.clone(),
    ));
    let telemetry = Arc::new(TelemetryCollector::new(
        std::path::PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let (ingest_tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut config = AppConfig::default();
    config.plugins = settings;

    let playback_repo_handle = playback_repo.clone();
    let app = create_router_with_ingest(
        user_repo,
        playback_repo,
        media_repo.clone(),
        lib_repo.clone(),
        jwt_svc,
        RateLimiter::new(50, Duration::from_secs(60), Duration::from_secs(60)),
        session_registry,
        LayoutRegistry::new(),
        widget_resolver,
        subtitle_service,
        Arc::new(OpenSubtitlesClient::new(None, None)),
        event_bus.clone(),
        telemetry,
        ingest_tx,
        Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None)),
        Arc::new(tokio::sync::RwLock::new(config)),
        Arc::new(ServerIdentity {
            id: "00000000-0000-0000-0000-000000000000".to_string(),
        }),
    );

    Harness {
        app,
        media_repo,
        lib_repo,
        plugin_repo,
        playback_repo: playback_repo_handle,
        admin_token,
        user_token,
        jwt_secret,
        events: event_bus,
    }
}

pub async fn call_raw(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = match body {
        Some(v) => builder
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, bytes)
}

/// JSON convenience wrapper: returns `Value::Null` for empty or non-JSON bodies.
pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, _, bytes) = call_raw(app, method, uri, token, &[], body).await;
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Registers the mock through the admin API, approves it with all requested permissions,
/// configures the mock's expected bearer secret, and returns `(plugin_token, secret)`.
pub async fn register_and_approve(h: &Harness, mock: &MockPlugin) -> (String, String) {
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    let secret = body["secret"].as_str().unwrap().to_string();
    let id = body["plugin"]["id"].as_str().unwrap().to_string();
    mock.set_secret(&secret);

    let (status, body) = call(
        &h.app,
        "POST",
        &format!("/api/v1/admin/plugins/{id}/approve"),
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "approve failed: {body}");
    (body["token"].as_str().unwrap().to_string(), secret)
}

