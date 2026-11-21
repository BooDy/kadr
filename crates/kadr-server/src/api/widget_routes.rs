use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    Json,
};
use kadr_core::ast::{CardViewModel, ScreenId, WidgetQueryBinding};
use kadr_core::filter_ast::CatalogFacets;
use serde::{Deserialize, Serialize};

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::{AuthUser, RequireAdmin};
use crate::layout::{default_library_layout, LayoutRegistry};
use crate::resolver::WidgetResolver;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

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
                        if let Some(canonical) =
                            registry.find_screen_for_library(&lib.id, &lib.name)
                        {
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
        resolver
            .resolve_library_id_for_screen_with_registry(sid, &registry)
            .await
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

/// Query parameters for fetching distinct catalog facet values.
#[derive(Debug, Deserialize)]
pub struct CatalogFacetsQuery {
    pub library_id: Option<String>,
}

/// Handler for `GET /api/v1/catalog/facets`.
/// Returns distinct metadata and technical facets across visible libraries.
pub async fn get_catalog_facets(
    _auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Query(query): Query<CatalogFacetsQuery>,
    Extension(media_repo): Extension<MediaItemRepository>,
) -> Result<Json<CatalogFacets>, (StatusCode, Json<serde_json::Value>)> {
    let unlocked_ids = unlocked.to_vec();
    let library_id = query
        .library_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let facets = media_repo
        .get_catalog_facets(library_id, &unlocked_ids)
        .await
        .map_err(|err| {
            tracing::error!("Failed to load catalog facets: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to load catalog facets" })),
            )
        })?;
    Ok(Json(facets))
}

/// Request payload for stateless live widget preview evaluation.
#[derive(Debug, Deserialize)]
pub struct WidgetPreviewRequest {
    pub binding: WidgetQueryBinding,
    pub screen_id: Option<String>,
    pub limit: Option<u32>,
}

/// Response payload for stateless live widget preview evaluation.
#[derive(Debug, Serialize, Deserialize)]
pub struct WidgetPreviewResponse {
    pub items: Vec<CardViewModel>,
    pub total_count: u64,
    pub execution_ms: u64,
}

/// Handler for `POST /api/v1/widgets/preview`.
/// Evaluates a widget query binding (including custom_query filter ASTs) without persisting layout state.
pub async fn preview_widget(
    RequireAdmin(auth_user): RequireAdmin,
    unlocked: UnlockedLibraries,
    Extension(registry): Extension<LayoutRegistry>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
    Json(payload): Json<WidgetPreviewRequest>,
) -> Result<Json<WidgetPreviewResponse>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(tree) = payload
        .binding
        .filters
        .as_ref()
        .and_then(|f| f.rule_tree.as_ref())
    {
        if let Err(err) = tree.validate() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            ));
        }
    }

    let mut binding = payload.binding;
    binding.limit = payload.limit.unwrap_or(10).clamp(1, 24);

    let default_library_id = match payload
        .screen_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(sid_str) => {
            let sid: ScreenId = sid_str.parse().unwrap();
            resolver
                .resolve_library_id_for_screen_with_registry(&sid, &registry)
                .await
        }
        None => None,
    };

    let unlocked_ids = unlocked.to_vec();
    let started = std::time::Instant::now();
    let (items, _next_cursor, total_opt) = resolver
        .resolve_widget_data_with_library(
            &binding,
            &auth_user.id,
            0,
            &unlocked_ids,
            default_library_id.as_deref(),
        )
        .await
        .map_err(|err| {
            tracing::error!("Failed to preview widget query: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to preview widget query" })),
            )
        })?;
    let execution_ms = started.elapsed().as_millis() as u64;
    let total_count = total_opt.unwrap_or(items.len() as u64);

    Ok(Json(WidgetPreviewResponse {
        items,
        total_count,
        execution_ms,
    }))
}
