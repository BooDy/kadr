use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use kadr_core::ast::ItemDetailsPayload;

use crate::auth::jwt::AuthUser;
use crate::resolver::WidgetResolver;

/// Handler for `GET /api/v1/items/{item_id}/details`.
/// Returns comprehensive metadata, playback progress, technical specifications, and child episodes.
pub async fn get_item_details(
    auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
) -> Result<Json<ItemDetailsPayload>, (StatusCode, Json<serde_json::Value>)> {
    match resolver.resolve_item_details(item_id, &auth_user.id).await {
        Ok(Some(payload)) => Ok(Json(payload)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Item not found" })),
        )),
        Err(err) => {
            tracing::error!("Failed to resolve item details for item {item_id}: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to resolve item details" })),
            ))
        }
    }
}
