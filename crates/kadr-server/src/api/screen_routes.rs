use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use kadr_core::ast::{ScreenId, ScreenLayout};
use serde::{Deserialize, Serialize};

use crate::auth::jwt::AuthUser;
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

/// Handler for `GET /api/v1/screens/{screen_id}`.
/// Returns the hydrated (or raw if `unhydrated=true`) AST layout for the requested screen.
pub async fn get_screen(
    auth_user: AuthUser,
    Path(screen_id): Path<String>,
    Query(query): Query<ScreenQuery>,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
) -> Result<Json<ScreenLayout>, (StatusCode, Json<serde_json::Value>)> {
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
        let hydrated = resolver.resolve_screen(layout, &auth_user.id).await;
        Ok(Json(hydrated))
    }
}
