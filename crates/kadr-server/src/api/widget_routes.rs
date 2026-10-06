use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use kadr_core::ast::{CardViewModel, ScreenId};
use serde::{Deserialize, Serialize};

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::layout::{default_library_layout, LayoutRegistry};
use crate::resolver::WidgetResolver;
use kadr_storage::repos::LibraryRepository;

fn default_widget_limit() -> u32 {
    20
}

/// Query parameters for fetching paginated widget data.
#[derive(Debug, Deserialize)]
pub struct WidgetDataQuery {
    pub screen_id: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_widget_limit")]
    pub limit: u32,
    pub sort: Option<String>,
}

/// Paginated response payload for widget data queries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetDataResponse {
    pub widget_id: String,
    pub items: Vec<CardViewModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_count: Option<u64>,
}

/// Handler for `GET /api/v1/widgets/{widget_id}/data`.
/// Searches the widget binding in the layout registry and resolves paginated card items.
pub async fn get_widget_data(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(widget_id): Path<String>,
    Query(query): Query<WidgetDataQuery>,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Result<Json<WidgetDataResponse>, (StatusCode, Json<serde_json::Value>)> {
    let (widget, target_screen_id) = if let Some(ref screen_id_str) = query.screen_id {
        let sid: ScreenId = screen_id_str.parse().unwrap();
        let screen = match registry.get_screen(&sid) {
            Some(s) => {
                if let ScreenId::Custom(ref id_str) = sid {
                    if let Ok(Some(lib)) = lib_repo.get_by_id(id_str).await {
                        if let Some(canonical) = registry.find_screen_for_library(&lib.id, &lib.name) {
                            canonical
                        } else {
                            s
                        }
                    } else {
                        s
                    }
                } else {
                    s
                }
            }
            None => {
                if let Ok(Some(lib)) = lib_repo.get_by_id(screen_id_str).await {
                    if let Some(canonical) = registry.find_screen_for_library(&lib.id, &lib.name) {
                        canonical
                    } else {
                        let def = default_library_layout(&lib.id, &lib.name);
                        registry.register_screen(def.clone());
                        def
                    }
                } else {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({ "error": "Widget not found" })),
                    ));
                }
            }
        };
        let w = screen.widgets.into_iter().find(|w| w.id() == widget_id);
        (w, Some(sid))
    } else {
        let mut found = None;
        let mut found_sid = None;
        for (sid, _) in registry.list_screens() {
            if let Some(screen) = registry.get_screen(&sid) {
                if let Some(w) = screen.widgets.into_iter().find(|w| w.id() == widget_id) {
                    found = Some(w);
                    found_sid = Some(sid);
                    break;
                }
            }
        }
        (found, found_sid)
    };

    let widget = widget.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Widget not found" })),
        )
    })?;

    let binding = widget.binding().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Widget not found" })),
        )
    })?;

    let mut query_binding = binding.clone();
    query_binding.limit = query.limit;
    if let Some(ref sort) = query.sort {
        query_binding.sort = Some(sort.clone());
    }

    let default_library_id = if let Some(ref sid) = target_screen_id {
        resolver.resolve_library_id_for_screen(sid).await
    } else {
        None
    };

    let unlocked_ids = unlocked.to_vec();
    let (items, next_cursor, total_count) = resolver
        .resolve_widget_data_with_library(
            &query_binding,
            &auth_user.id,
            query.offset,
            &unlocked_ids,
            default_library_id.as_deref(),
        )
        .await
        .map_err(|err| {
            tracing::error!("Failed to resolve widget data: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to resolve widget data" })),
            )
        })?;

    let next_cursor_url = next_cursor.map(|cursor| {
        let mut url = format!(
            "/api/v1/widgets/{widget_id}/data?offset={cursor}&limit={}",
            query.limit
        );
        if let Some(ref sid) = query.screen_id {
            url.push_str(&format!("&screen_id={sid}"));
        }
        if let Some(ref sort) = query.sort {
            url.push_str(&format!("&sort={sort}"));
        }
        url
    });

    Ok(Json(WidgetDataResponse {
        widget_id,
        items,
        next_cursor: next_cursor_url,
        total_count,
    }))
}
