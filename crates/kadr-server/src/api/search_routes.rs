use axum::{
    extract::{Extension, Query},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use kadr_core::models::MediaType;
use kadr_core::search::{clamp_search_limit, SearchQuery, SearchResponse, DEFAULT_SEARCH_LIMIT};
use kadr_storage::repos::SearchRepository;
use serde::Deserialize;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;

#[derive(Debug, Default, Deserialize)]
pub struct SearchRequestParams {
    pub q: Option<String>,
    pub item_type: Option<String>,
    pub library_id: Option<String>,
    pub limit: Option<usize>,
}

fn parse_media_type_filter(raw: Option<&str>) -> Result<Option<MediaType>, String> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    let lower = s.to_ascii_lowercase();
    let parsed = match lower.as_str() {
        "all" => return Ok(None),
        "movie" | "movies" => MediaType::Movie,
        "show" | "shows" | "series" | "tv" => MediaType::Show,
        "season" | "seasons" => MediaType::Season,
        "episode" | "episodes" => MediaType::Episode,
        "anime" => MediaType::Anime,
        "music" => MediaType::Music,
        "home_videos" | "homevideos" | "home_video" => MediaType::HomeVideos,
        "audiobook" | "audiobooks" => MediaType::Audiobook,
        "live_channel" | "live_channels" | "livechannel" => MediaType::LiveChannel,
        "unknown" => MediaType::Unknown,
        _ => serde_json::from_value::<MediaType>(serde_json::Value::String(s.to_string()))
            .map_err(|_| format!("Invalid item_type '{s}'"))?,
    };

    Ok(Some(parsed))
}

/// Handler for `GET /api/v1/search?q=...&item_type=...&library_id=...&limit=...`.
/// Enforces JWT authentication and PIN-locked private-library isolation via `UnlockedLibraries`.
pub async fn search_media(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Query(params): Query<SearchRequestParams>,
    Extension(search_repo): Extension<SearchRepository>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<serde_json::Value>)> {
    let item_type = parse_media_type_filter(params.item_type.as_deref()).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
    })?;

    let raw_q = params.q.unwrap_or_default();
    if raw_q.trim().is_empty() {
        return Ok(Json(SearchResponse::empty("")));
    }

    let limit = clamp_search_limit(params.limit.unwrap_or(DEFAULT_SEARCH_LIMIT));
    let library_id = params
        .library_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let query = SearchQuery {
        q: raw_q,
        item_type,
        library_id,
        limit,
    };

    let unlocked_ids = unlocked.to_vec();
    match search_repo
        .search(&query, &auth_user.id, &unlocked_ids)
        .await
    {
        Ok(res) => Ok(Json(res)),
        Err(err) => {
            tracing::error!("Global FTS5 search failed: {err}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Search query failed" })),
            ))
        }
    }
}

pub fn search_routes() -> Router {
    Router::new().route("/api/v1/search", get(search_media))
}
