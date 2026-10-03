use std::sync::Arc;

use axum::extract::Extension;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::auth::jwt::RequireAdmin;
use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfigResponse {
    pub host: String,
    pub port: u16,
    pub data_dir: String,
    pub web_dir: Option<String>,
    pub database_path: String,
    pub max_readers: usize,
    pub debounce_millis: u64,
    pub use_ffprobe: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfigPayload {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub debounce_millis: Option<u64>,
    pub use_ffprobe: Option<bool>,
}

/// Handler for `GET /api/v1/system/config`.
///
/// Returns current server configuration. Requires admin role.
pub async fn get_config(
    _admin: RequireAdmin,
    Extension(config): Extension<Arc<RwLock<AppConfig>>>,
) -> (StatusCode, Json<SystemConfigResponse>) {
    let cfg = config.read().await;
    let resp = SystemConfigResponse {
        host: cfg.server.host.clone(),
        port: cfg.server.port,
        data_dir: cfg.server.data_dir.to_string_lossy().into_owned(),
        web_dir: cfg
            .server
            .web_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        database_path: cfg.storage.database_path.to_string_lossy().into_owned(),
        max_readers: cfg.storage.max_readers,
        debounce_millis: cfg.scanner.debounce_millis,
        use_ffprobe: cfg.scanner.use_ffprobe,
    };
    (StatusCode::OK, Json(resp))
}

/// Handler for `PUT /api/v1/system/config`.
///
/// Updates server runtime configuration. Requires admin role.
pub async fn update_config(
    _admin: RequireAdmin,
    Extension(config): Extension<Arc<RwLock<AppConfig>>>,
    Json(payload): Json<UpdateConfigPayload>,
) -> (StatusCode, Json<SystemConfigResponse>) {
    let mut cfg = config.write().await;
    if let Some(host) = payload.host {
        cfg.server.host = host;
    }
    if let Some(port) = payload.port {
        cfg.server.port = port;
    }
    if let Some(debounce) = payload.debounce_millis {
        cfg.scanner.debounce_millis = debounce;
    }
    if let Some(ffprobe) = payload.use_ffprobe {
        cfg.scanner.use_ffprobe = ffprobe;
    }

    let resp = SystemConfigResponse {
        host: cfg.server.host.clone(),
        port: cfg.server.port,
        data_dir: cfg.server.data_dir.to_string_lossy().into_owned(),
        web_dir: cfg
            .server
            .web_dir
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        database_path: cfg.storage.database_path.to_string_lossy().into_owned(),
        max_readers: cfg.storage.max_readers,
        debounce_millis: cfg.scanner.debounce_millis,
        use_ffprobe: cfg.scanner.use_ffprobe,
    };
    (StatusCode::OK, Json(resp))
}
