use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use kadr_core::ast::{ScreenId, ScreenLayout};
use serde::{Deserialize, Serialize};

pub use crate::api::item_routes::get_item_details;
use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::{AuthUser, RequireAdmin};
use crate::config::AppConfig;
use crate::layout::LayoutRegistry;
use crate::resolver::WidgetResolver;

/// Summary representation of a registered screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSummary {
    pub id: String,
    pub title: String,
}

/// Query parameters for fetching screen layouts.
#[derive(Debug, Deserialize)]
pub struct ScreenQuery {
    #[serde(default)]
    pub unhydrated: bool,
}

/// Handler for `GET /api/v1/screens`.
/// Returns a list of all registered screens with their IDs and titles.
pub async fn list_screens(
    _auth_user: AuthUser,
    Extension(registry): Extension<LayoutRegistry>,
) -> Json<Vec<ScreenSummary>> {
    let screens = registry
        .list_screens()
        .into_iter()
        .map(|(id, title)| ScreenSummary {
            id: id.to_string(),
            title,
        })
        .collect();
    Json(screens)
}

/// Validates that a screen ID is not empty, does not start with '.', and contains only ASCII alphanumeric, '-', or '_'.
pub fn validate_screen_id(id: &str) -> Result<(), &'static str> {
    if id.is_empty() {
        return Err("Screen ID cannot be empty");
    }
    if id.starts_with('.') {
        return Err("Screen ID cannot start with '.'");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Screen ID must contain only ASCII alphanumeric, '-', or '_' characters");
    }
    Ok(())
}

/// Handler for `GET /api/v1/screens/{screen_id}`.
/// Returns the hydrated (or raw if `unhydrated=true`) AST layout for the requested screen.
pub async fn get_screen(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(screen_id): Path<String>,
    Query(query): Query<ScreenQuery>,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
) -> Result<Json<ScreenLayout>, (StatusCode, Json<serde_json::Value>)> {
    if let Err(err_msg) = validate_screen_id(&screen_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let parsed_id: ScreenId = screen_id.parse().unwrap();
    let layout = match registry.get_screen(&parsed_id) {
        Some(l) => l,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Screen not found" })),
            ));
        }
    };

    if query.unhydrated {
        Ok(Json(layout))
    } else {
        let unlocked_ids = unlocked.to_vec();
        let hydrated = resolver
            .resolve_screen_with_unlocked(layout, &auth_user.id, &unlocked_ids)
            .await;
        Ok(Json(hydrated))
    }
}

/// Request body for creating a custom screen layout.
#[derive(Debug, Deserialize)]
pub struct CreateScreenRequest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Handler for `PUT /api/v1/screens/{screen_id}`.
/// Saves custom or updated screen AST layout to disk and registry. Requires admin role.
pub async fn save_screen_handler(
    _admin: RequireAdmin,
    Path(screen_id): Path<String>,
    Extension(mut registry): Extension<LayoutRegistry>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
    Json(mut screen): Json<ScreenLayout>,
) -> Result<Json<ScreenLayout>, (StatusCode, Json<serde_json::Value>)> {
    if let Err(err_msg) = validate_screen_id(&screen_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let parsed_id: ScreenId = screen_id.parse().unwrap();
    screen.id = parsed_id;
    let screens_dir = config.read().await.server.data_dir.join("screens");
    registry
        .save_screen(screen.clone(), &screens_dir)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(screen))
}

/// Handler for `POST /api/v1/screens`.
/// Creates a new screen AST layout and persists it. Requires admin role.
pub async fn create_screen_handler(
    _admin: RequireAdmin,
    Extension(mut registry): Extension<LayoutRegistry>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
    Json(payload): Json<CreateScreenRequest>,
) -> Result<(StatusCode, Json<ScreenLayout>), (StatusCode, Json<serde_json::Value>)> {
    if let Err(err_msg) = validate_screen_id(&payload.id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let parsed_id: ScreenId = payload.id.parse().unwrap();
    if registry.get_screen(&parsed_id).is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": format!("Screen '{}' already exists", payload.id) })),
        ));
    }
    let screen = ScreenLayout::new(parsed_id, payload.title, vec![]);
    let screens_dir = config.read().await.server.data_dir.join("screens");
    registry
        .save_screen(screen.clone(), &screens_dir)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;
    Ok((StatusCode::CREATED, Json(screen)))
}

/// Handler for `DELETE /api/v1/screens/{screen_id}`.
/// Resets built-in screen to factory defaults or deletes custom screen from disk and registry. Requires admin role.
pub async fn delete_screen_handler(
    _admin: RequireAdmin,
    Path(screen_id): Path<String>,
    Extension(mut registry): Extension<LayoutRegistry>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Err(err_msg) = validate_screen_id(&screen_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        ));
    }

    let parsed_id: ScreenId = screen_id.parse().unwrap();
    let screens_dir = config.read().await.server.data_dir.join("screens");
    match parsed_id {
        ScreenId::Home | ScreenId::Movies | ScreenId::Shows => {
            registry
                .reset_screen(&parsed_id, &screens_dir)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": e.to_string() })),
                    )
                })?;
            Ok(Json(serde_json::json!({ "message": "Screen reset to defaults" })))
        }
        ScreenId::Custom(_) => {
            if registry.get_screen(&parsed_id).is_none() {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Screen not found" })),
                ));
            }
            registry
                .delete_custom_screen(&parsed_id, &screens_dir)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": e.to_string() })),
                    )
                })?;
            Ok(Json(serde_json::json!({ "message": "Screen deleted" })))
        }
    }
}

