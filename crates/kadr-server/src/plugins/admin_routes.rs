use crate::auth::jwt::RequireAdmin;
use crate::config::PluginSettings;
use crate::plugins::service::{PluginService, ServiceError};
use crate::plugins::settings::PluginSettingsUpdate;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use kadr_core::plugin::{PluginPermission, PluginRecord};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct RegisterBody {
    pub base_url: String,
}

#[derive(Deserialize, Default)]
pub struct ApproveBody {
    pub permissions: Option<Vec<PluginPermission>>,
}

#[derive(Deserialize)]
pub struct RemoveQuery {
    #[serde(default)]
    pub purge: bool,
}

pub fn admin_routes() -> Router {
    Router::new()
        .route(
            "/api/v1/admin/plugins/settings",
            get(get_plugin_settings).put(put_plugin_settings),
        )
        .route(
            "/api/v1/admin/plugins",
            get(list_plugins).post(register_plugin),
        )
        .route("/api/v1/admin/plugins/{id}/approve", post(approve_plugin))
        .route("/api/v1/admin/plugins/{id}/connection", get(plugin_connection))
        .route("/api/v1/admin/plugins/{id}/token", post(issue_plugin_token))
        .route("/api/v1/admin/plugins/{id}/enable", post(enable_plugin))
        .route("/api/v1/admin/plugins/{id}/disable", post(disable_plugin))
        .route("/api/v1/admin/plugins/{id}", delete(remove_plugin))
}

async fn list_plugins(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<Vec<PluginRecord>>, ServiceError> {
    Ok(Json(svc.list().await?))
}

async fn register_plugin(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<RegisterBody>,
) -> Result<(StatusCode, Json<Value>), ServiceError> {
    let (plugin, secret) = svc.register(&body.base_url).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "plugin": plugin, "secret": secret })),
    ))
}

async fn approve_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
    body: Option<Json<ApproveBody>>,
) -> Result<Json<Value>, ServiceError> {
    let requested = body.and_then(|Json(b)| b.permissions);
    let (plugin, token) = svc.approve(&id, requested).await?;
    Ok(Json(json!({ "plugin": plugin, "token": token })))
}

async fn plugin_connection(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<Value>, ServiceError> {
    let (record, secret) = svc.connection(&id).await?;
    Ok(Json(json!({ "base_url": record.base_url, "secret": secret })))
}

async fn issue_plugin_token(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<Value>, ServiceError> {
    let token = svc.issue_token(&id).await?;
    Ok(Json(json!({ "token": token })))
}

async fn enable_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<PluginRecord>, ServiceError> {
    Ok(Json(svc.set_enabled(&id, true).await?))
}

async fn disable_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<PluginRecord>, ServiceError> {
    Ok(Json(svc.set_enabled(&id, false).await?))
}

async fn remove_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Query(q): Query<RemoveQuery>,
    Extension(svc): Extension<PluginService>,
) -> Result<StatusCode, ServiceError> {
    svc.remove(&id, q.purge).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_plugin_settings(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
) -> Json<PluginSettings> {
    Json(svc.get_settings().await)
}

async fn put_plugin_settings(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
    Json(update): Json<PluginSettingsUpdate>,
) -> Result<Json<PluginSettings>, ServiceError> {
    Ok(Json(svc.update_settings(update).await?))
}
