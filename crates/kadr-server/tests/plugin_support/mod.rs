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
