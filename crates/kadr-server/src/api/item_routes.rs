use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use kadr_core::ast::ItemDetailsPayload;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::resolver::WidgetResolver;

/// Handler for `GET /api/v1/items/{item_id}` and `GET /api/v1/items/{item_id}/details`.
/// Returns comprehensive metadata, playback progress, technical specifications, and child episodes.
pub async fn get_item_details(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Result<Json<ItemDetailsPayload>, (StatusCode, Json<serde_json::Value>)> {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Item not found" })),
            ));
        }
        Err(err) => {
            tracing::error!("Failed to fetch media item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to resolve item details" })),
            ));
        }
    };

    if let Ok(Some(lib)) = lib_repo.get_by_id(&item.library_id).await {
        if lib.is_private && !unlocked.is_unlocked(&item.library_id) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
            ));
        }
    }

    let unlocked_ids = unlocked.to_vec();
    match resolver
        .resolve_item_details_with_unlocked(item_id, &auth_user.id, &unlocked_ids)
        .await
    {
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
