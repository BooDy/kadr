use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use kadr_core::events::SystemEvent;
use kadr_core::favorites::{FavoriteItemView, FavoriteStatusResponse, FavoritesListResponse};
use kadr_core::models::{MediaItem, MediaType, WatchState};
use kadr_storage::repos::{
    FavoritesRepository, LibraryRepository, MediaItemRepository, PlaybackRepository,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::error;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::events::EventBus;
use crate::resolver::to_card_view_model_with_favorite;

type ApiErr = (StatusCode, Json<Value>);

fn api_err(status: StatusCode, msg: &str) -> ApiErr {
    (status, Json(json!({ "error": msg })))
}

fn library_locked_err(library_id: &str) -> ApiErr {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "LIBRARY_LOCKED",
            "code": "LIBRARY_LOCKED",
            "library_id": library_id,
        })),
    )
}

fn internal_err(e: impl std::fmt::Display, context: &str) -> ApiErr {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
        _ => serde_json::from_value::<MediaType>(Value::String(s.to_string()))
            .map_err(|_| format!("Invalid item_type '{s}'"))?,
    };

    Ok(Some(parsed))
}

#[derive(Debug, Default, Deserialize)]
pub struct ListFavoritesQuery {
    pub item_type: Option<String>,
    pub library_id: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub fn favorite_routes() -> Router {
    Router::new()
        .route("/api/v1/favorites", get(list_favorites))
        .route(
            "/api/v1/favorites/{item_id}",
            put(add_favorite).delete(remove_favorite),
        )
}

async fn verify_item_accessible(
    item_id: i64,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
    lib_repo: &LibraryRepository,
) -> Result<MediaItem, ApiErr> {
    let item = media_repo
        .get_by_id(item_id)
        .await
        .map_err(|e| internal_err(e, "Failed to load media item"))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Media item not found"))?;

    let lib = lib_repo
        .get_by_id(&item.library_id)
        .await
        .map_err(|e| internal_err(e, "Failed to load library"))?;

    match lib {
        Some(lib) => {
            if lib.is_private && !unlocked.is_unlocked(&lib.id) {
                return Err(library_locked_err(&lib.id));
            }
        }
        None => {
            return Err(library_locked_err(&item.library_id));
        }
    }

    Ok(item)
}

pub async fn list_favorites(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Query(query): Query<ListFavoritesQuery>,
    Extension(favorites_repo): Extension<FavoritesRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> Result<Json<FavoritesListResponse>, ApiErr> {
    let item_type = parse_media_type_filter(query.item_type.as_deref())
        .map_err(|msg| api_err(StatusCode::BAD_REQUEST, &msg))?;

    let limit = query.limit.unwrap_or(60).clamp(1, 200);
    let offset = query.offset.unwrap_or(0);
    let library_id = query
        .library_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let sort = query
        .sort
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let unlocked_ids = unlocked.to_vec();

    let (fav_pairs, total_count, locked_item_count) = favorites_repo
        .list_favorites(
            &auth_user.id,
            library_id,
            item_type,
            limit,
            offset,
            sort,
            &unlocked_ids,
            None,
        )
        .await
        .map_err(|e| internal_err(e, "Failed to list favorites"))?;

    let visible_ids: Vec<i64> = fav_pairs.iter().filter_map(|(_, item)| item.id).collect();
    let playback_map = if visible_ids.is_empty() {
        HashMap::new()
    } else {
        playback_repo
            .get_states_for_items(&auth_user.id, &visible_ids)
            .await
            .unwrap_or_default()
    };

    let items = fav_pairs
        .into_iter()
        .map(|(favorited_at, item)| {
            let state = item.id.and_then(|id| playback_map.get(&id));
            let resume_position_seconds = state.and_then(|s| {
                if s.watch_state == WatchState::InProgress && s.playback_position_seconds > 0 {
                    Some(s.playback_position_seconds)
                } else {
                    None
                }
            });
            FavoriteItemView {
                favorited_at,
                duration_seconds: item.technical.duration_seconds.max(0),
                resume_position_seconds,
                card: to_card_view_model_with_favorite(&item, state, true),
            }
        })
        .collect();

    Ok(Json(FavoritesListResponse {
        items,
        total_count,
        locked_item_count,
    }))
}

pub async fn add_favorite(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(favorites_repo): Extension<FavoritesRepository>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<Json<FavoriteStatusResponse>, ApiErr> {
    verify_item_accessible(item_id, &unlocked, &media_repo, &lib_repo).await?;

    let now = now_epoch();
    favorites_repo
        .add_favorite(&auth_user.id, item_id, now)
        .await
        .map_err(|e| internal_err(e, "Failed to add favorite"))?;

    event_bus.publish(SystemEvent::FavoriteUpdated {
        user_id: auth_user.id,
        item_id,
        is_favorite: true,
        timestamp: now,
    });

    Ok(Json(FavoriteStatusResponse {
        item_id,
        is_favorite: true,
    }))
}

pub async fn remove_favorite(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(favorites_repo): Extension<FavoritesRepository>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<Json<FavoriteStatusResponse>, ApiErr> {
    verify_item_accessible(item_id, &unlocked, &media_repo, &lib_repo).await?;

    let now = now_epoch();
    favorites_repo
        .remove_favorite(&auth_user.id, item_id)
        .await
        .map_err(|e| internal_err(e, "Failed to remove favorite"))?;

    event_bus.publish(SystemEvent::FavoriteUpdated {
        user_id: auth_user.id,
        item_id,
        is_favorite: false,
        timestamp: now,
    });

    Ok(Json(FavoriteStatusResponse {
        item_id,
        is_favorite: false,
    }))
}
