use std::collections::{HashMap, HashSet};
use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use kadr_core::events::SystemEvent;
use kadr_core::models::{Library, MediaItem, MediaType, UserRole, WatchState};
use kadr_core::playlists::{
    AddFolderItemsSpec, AddPlaylistItemsRequest, AddPlaylistItemsResponse, AddShowItemsSpec,
    CreatePlaylistRequest, Playlist, PlaylistDetail, PlaylistItemView, PlaylistSummary,
    PlaylistsListResponse, ReorderPlaylistItemsRequest, UpdatePlaylistRequest,
};
use kadr_storage::error::StorageError;
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, PlaylistRepository,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::error;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::events::EventBus;
use crate::resolver::to_card_view_model;

type ApiErr = (StatusCode, Json<Value>);

fn api_err(status: StatusCode, msg: &str) -> ApiErr {
    (status, Json(json!({ "error": msg })))
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

#[derive(Debug, Default, Deserialize)]
pub struct ListPlaylistsQuery {
    pub contains_item_id: Option<i64>,
}

pub fn playlist_routes() -> Router {
    Router::new()
        .route(
            "/api/v1/playlists",
            get(list_playlists).post(create_playlist),
        )
        .route(
            "/api/v1/playlists/{id}",
            get(get_playlist)
                .patch(update_playlist)
                .delete(delete_playlist),
        )
        .route("/api/v1/playlists/{id}/items", post(add_playlist_items))
        .route(
            "/api/v1/playlists/{id}/items/order",
            put(reorder_playlist_items),
        )
        .route(
            "/api/v1/playlists/{id}/items/{media_item_id}",
            delete(remove_playlist_item),
        )
}

fn validate_name(raw: &str) -> Result<String, ApiErr> {
    let trimmed = raw.trim();
    let count = trimmed.chars().count();
    if !(1..=120).contains(&count) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "Playlist name must be between 1 and 120 characters",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_description(raw: Option<&str>) -> Result<Option<String>, ApiErr> {
    let Some(s) = raw else {
        return Ok(None);
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 500 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "Playlist description must not exceed 500 characters",
        ));
    }
    Ok(Some(trimmed.to_string()))
}

async fn load_readable_playlist(
    playlist_id: &str,
    auth_user: &AuthUser,
    playlist_repo: &PlaylistRepository,
) -> Result<(Playlist, String), ApiErr> {
    let raw = playlist_repo
        .get_playlist_raw(playlist_id)
        .await
        .map_err(|e| internal_err(e, "Failed to load playlist"))?;

    let Some((pl, owner_username)) = raw else {
        return Err(api_err(StatusCode::NOT_FOUND, "Playlist not found"));
    };

    if pl.owner_user_id != auth_user.id && !pl.is_public {
        return Err(api_err(StatusCode::NOT_FOUND, "Playlist not found"));
    }

    Ok((pl, owner_username))
}

async fn load_editable_playlist(
    playlist_id: &str,
    auth_user: &AuthUser,
    playlist_repo: &PlaylistRepository,
) -> Result<(Playlist, String), ApiErr> {
    let raw = playlist_repo
        .get_playlist_raw(playlist_id)
        .await
        .map_err(|e| internal_err(e, "Failed to load playlist"))?;

    let Some((pl, owner_username)) = raw else {
        return Err(api_err(StatusCode::NOT_FOUND, "Playlist not found"));
    };

    if pl.owner_user_id != auth_user.id && !pl.is_public {
        return Err(api_err(StatusCode::NOT_FOUND, "Playlist not found"));
    }

    if pl.owner_user_id != auth_user.id && auth_user.role != UserRole::Admin {
        return Err(api_err(StatusCode::FORBIDDEN, "Forbidden"));
    }

    Ok((pl, owner_username))
}

async fn load_summary_or_500(
    playlist_id: &str,
    auth_user: &AuthUser,
    unlocked: &UnlockedLibraries,
    playlist_repo: &PlaylistRepository,
) -> Result<PlaylistSummary, ApiErr> {
    let is_admin = auth_user.role == UserRole::Admin;
    let unlocked_ids = unlocked.to_vec();
    playlist_repo
        .get_playlist_summary_for_user(playlist_id, &auth_user.id, is_admin, &unlocked_ids, None)
        .await
        .map_err(|e| internal_err(e, "Failed to load playlist summary"))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Playlist not found"))
}

async fn hydrate_playlist_detail(
    playlist_id: &str,
    auth_user: &AuthUser,
    unlocked: &UnlockedLibraries,
    playlist_repo: &PlaylistRepository,
    playback_repo: &PlaybackRepository,
) -> Result<PlaylistDetail, ApiErr> {
    let summary = load_summary_or_500(playlist_id, auth_user, unlocked, playlist_repo).await?;
    let unlocked_ids = unlocked.to_vec();
    let (visible_pairs, _locked_count) = playlist_repo
        .get_playlist_items_with_privacy(playlist_id, &unlocked_ids)
        .await
        .map_err(|e| internal_err(e, "Failed to load playlist items"))?;

    let visible_ids: Vec<i64> = visible_pairs
        .iter()
        .filter_map(|(_, item)| item.id)
        .collect();

    let playback_map = if visible_ids.is_empty() {
        HashMap::new()
    } else {
        playback_repo
            .get_states_for_items(&auth_user.id, &visible_ids)
            .await
            .unwrap_or_default()
    };

    let items = visible_pairs
        .into_iter()
        .enumerate()
        .map(|(position, (added_at, item))| {
            let state = item.id.and_then(|id| playback_map.get(&id));
            let resume_position_seconds = state.and_then(|s| {
                if s.watch_state == WatchState::InProgress && s.playback_position_seconds > 0 {
                    Some(s.playback_position_seconds)
                } else {
                    None
                }
            });
            let card = to_card_view_model(&item, state);
            PlaylistItemView {
                position,
                added_at,
                duration_seconds: item.technical.duration_seconds.max(0),
                resume_position_seconds,
                card,
            }
        })
        .collect();

    Ok(PlaylistDetail { summary, items })
}

pub async fn list_playlists(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Query(query): Query<ListPlaylistsQuery>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
) -> Result<Json<PlaylistsListResponse>, ApiErr> {
    let is_admin = auth_user.role == UserRole::Admin;
    let unlocked_ids = unlocked.to_vec();
    let playlists = playlist_repo
        .list_playlists_for_user(
            &auth_user.id,
            is_admin,
            &unlocked_ids,
            query.contains_item_id,
        )
        .await
        .map_err(|e| internal_err(e, "Failed to list playlists"))?;

    Ok(Json(PlaylistsListResponse { playlists }))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_playlist(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<CreatePlaylistRequest>,
) -> Result<(StatusCode, Json<PlaylistDetail>), ApiErr> {
    let name = validate_name(&payload.name)?;
    let description = validate_description(payload.description.as_deref())?;

    // Validate and expand initial items BEFORE creating the playlist row so failed checks leave no orphan
    let initial_ids = if let Some(ref items_req) = payload.items {
        Some(expand_items_request(items_req, &unlocked, &media_repo, &lib_repo).await?)
    } else {
        None
    };

    let id = uuid::Uuid::new_v4().to_string();
    playlist_repo
        .create_playlist(
            &id,
            &auth_user.id,
            &name,
            description.as_deref(),
            payload.is_public,
        )
        .await
        .map_err(|e| internal_err(e, "Failed to create playlist"))?;

    if let Some(ref ids) = initial_ids {
        playlist_repo
            .add_items(&id, ids)
            .await
            .map_err(|e| internal_err(e, "Failed to seed initial playlist items"))?;
    }

    let detail =
        hydrate_playlist_detail(&id, &auth_user, &unlocked, &playlist_repo, &playback_repo).await?;

    event_bus.publish(SystemEvent::PlaylistUpdated {
        playlist_id: id,
        user_id: auth_user.id,
        action: "created".to_string(),
        timestamp: detail.summary.updated_at,
    });

    Ok((StatusCode::CREATED, Json(detail)))
}

pub async fn get_playlist(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> Result<Json<PlaylistDetail>, ApiErr> {
    load_readable_playlist(&id, &auth_user, &playlist_repo).await?;
    let detail =
        hydrate_playlist_detail(&id, &auth_user, &unlocked, &playlist_repo, &playback_repo).await?;
    Ok(Json(detail))
}

pub async fn update_playlist(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<UpdatePlaylistRequest>,
) -> Result<Json<PlaylistSummary>, ApiErr> {
    load_editable_playlist(&id, &auth_user, &playlist_repo).await?;

    let validated_name = match payload.name {
        Some(ref raw) => Some(validate_name(raw)?),
        None => None,
    };
    let validated_desc = match payload.description {
        Some(ref raw) => Some(validate_description(Some(raw))?),
        None => None,
    };

    playlist_repo
        .update_playlist(
            &id,
            validated_name.as_deref(),
            validated_desc.as_ref().map(|opt| opt.as_deref()),
            payload.is_public,
        )
        .await
        .map_err(|e| internal_err(e, "Failed to update playlist"))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Playlist not found"))?;

    let summary = load_summary_or_500(&id, &auth_user, &unlocked, &playlist_repo).await?;

    event_bus.publish(SystemEvent::PlaylistUpdated {
        playlist_id: id,
        user_id: auth_user.id,
        action: "updated".to_string(),
        timestamp: summary.updated_at,
    });

    Ok(Json(summary))
}

pub async fn delete_playlist(
    auth_user: AuthUser,
    Path(id): Path<String>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<StatusCode, ApiErr> {
    load_editable_playlist(&id, &auth_user, &playlist_repo).await?;

    let deleted = playlist_repo
        .delete_playlist(&id)
        .await
        .map_err(|e| internal_err(e, "Failed to delete playlist"))?;

    if !deleted {
        return Err(api_err(StatusCode::NOT_FOUND, "Playlist not found"));
    }

    event_bus.publish(SystemEvent::PlaylistUpdated {
        playlist_id: id,
        user_id: auth_user.id,
        action: "deleted".to_string(),
        timestamp: now_epoch(),
    });

    Ok(StatusCode::NO_CONTENT)
}

#[allow(clippy::too_many_arguments)]
pub async fn add_playlist_items(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<AddPlaylistItemsRequest>,
) -> Result<Json<AddPlaylistItemsResponse>, ApiErr> {
    load_editable_playlist(&id, &auth_user, &playlist_repo).await?;

    let candidate_ids = expand_items_request(&payload, &unlocked, &media_repo, &lib_repo).await?;

    let (added_count, skipped_duplicate_count) = playlist_repo
        .add_items(&id, &candidate_ids)
        .await
        .map_err(|e| match e {
            StorageError::NotFound(_) => api_err(StatusCode::NOT_FOUND, "Playlist not found"),
            other => internal_err(other, "Failed to add items to playlist"),
        })?;

    let summary = load_summary_or_500(&id, &auth_user, &unlocked, &playlist_repo).await?;

    event_bus.publish(SystemEvent::PlaylistUpdated {
        playlist_id: id,
        user_id: auth_user.id,
        action: "items_changed".to_string(),
        timestamp: summary.updated_at,
    });

    Ok(Json(AddPlaylistItemsResponse {
        added_count,
        skipped_duplicate_count,
        summary,
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn remove_playlist_item(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path((id, media_item_id)): Path<(String, i64)>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<Json<PlaylistSummary>, ApiErr> {
    load_editable_playlist(&id, &auth_user, &playlist_repo).await?;

    let item_opt = media_repo
        .get_by_id(media_item_id)
        .await
        .map_err(|e| internal_err(e, "Failed to query media item"))?;

    if let Some(item) = item_opt {
        let lib = lib_repo
            .get_by_id(&item.library_id)
            .await
            .map_err(|e| internal_err(e, "Failed to query library"))?;
        match lib {
            Some(l) => {
                if l.is_private && !unlocked.is_unlocked(&l.id) {
                    return Err(api_err(StatusCode::FORBIDDEN, "LIBRARY_LOCKED"));
                }
            }
            None => {
                return Err(api_err(StatusCode::FORBIDDEN, "LIBRARY_LOCKED"));
            }
        }
    }

    let removed = playlist_repo
        .remove_item(&id, media_item_id)
        .await
        .map_err(|e| internal_err(e, "Failed to remove item from playlist"))?;

    if !removed {
        return Err(api_err(StatusCode::NOT_FOUND, "Item not found in playlist"));
    }

    let summary = load_summary_or_500(&id, &auth_user, &unlocked, &playlist_repo).await?;

    event_bus.publish(SystemEvent::PlaylistUpdated {
        playlist_id: id,
        user_id: auth_user.id,
        action: "items_changed".to_string(),
        timestamp: summary.updated_at,
    });

    Ok(Json(summary))
}

pub async fn reorder_playlist_items(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Extension(playlist_repo): Extension<PlaylistRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<ReorderPlaylistItemsRequest>,
) -> Result<Json<PlaylistSummary>, ApiErr> {
    load_editable_playlist(&id, &auth_user, &playlist_repo).await?;

    let unlocked_ids = unlocked.to_vec();
    playlist_repo
        .reorder_visible_items(&id, &payload.item_ids, &unlocked_ids)
        .await
        .map_err(|e| match e {
            StorageError::InvalidInput(msg) => api_err(StatusCode::BAD_REQUEST, &msg),
            StorageError::NotFound(_) => api_err(StatusCode::NOT_FOUND, "Playlist not found"),
            other => internal_err(other, "Failed to reorder playlist items"),
        })?;

    let summary = load_summary_or_500(&id, &auth_user, &unlocked, &playlist_repo).await?;

    event_bus.publish(SystemEvent::PlaylistUpdated {
        playlist_id: id,
        user_id: auth_user.id,
        action: "items_changed".to_string(),
        timestamp: summary.updated_at,
    });

    Ok(Json(summary))
}

fn ensure_library_unlocked(
    library_id: &str,
    libs_map: &HashMap<String, Library>,
    unlocked: &UnlockedLibraries,
) -> Result<(), ApiErr> {
    let Some(lib) = libs_map.get(library_id) else {
        return Err(api_err(StatusCode::FORBIDDEN, "LIBRARY_LOCKED"));
    };
    if lib.is_private && !unlocked.is_unlocked(&lib.id) {
        return Err(api_err(StatusCode::FORBIDDEN, "LIBRARY_LOCKED"));
    }
    Ok(())
}

fn is_episode_lib_visible(
    library_id: &str,
    libs_map: &HashMap<String, Library>,
    unlocked: &UnlockedLibraries,
) -> bool {
    libs_map
        .get(library_id)
        .is_some_and(|l| !l.is_private || unlocked.is_unlocked(&l.id))
}

fn sort_episodes(episodes: &mut [MediaItem]) {
    episodes.sort_by(|a, b| {
        (
            a.metadata.season.unwrap_or(u32::MAX),
            a.metadata.episode.unwrap_or(u32::MAX),
            a.title.as_str(),
            a.id.unwrap_or(i64::MAX),
        )
            .cmp(&(
                b.metadata.season.unwrap_or(u32::MAX),
                b.metadata.episode.unwrap_or(u32::MAX),
                b.title.as_str(),
                b.id.unwrap_or(i64::MAX),
            ))
    });
}

async fn expand_show_episodes(
    show_item: &MediaItem,
    season_filter: Option<u32>,
    libs_map: &HashMap<String, Library>,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
) -> Result<Vec<i64>, ApiErr> {
    let raw_episodes = media_repo
        .find_episodes_by_series(&show_item.title)
        .await
        .map_err(|e| internal_err(e, "Failed to query episodes for show"))?;

    let mut visible_eps: Vec<MediaItem> = raw_episodes
        .into_iter()
        .filter(|ep| {
            ep.item_type != MediaType::Show
                && ep.item_type != MediaType::Season
                && is_episode_lib_visible(&ep.library_id, libs_map, unlocked)
        })
        .collect();

    if visible_eps
        .iter()
        .any(|ep| ep.library_id == show_item.library_id)
    {
        visible_eps.retain(|ep| ep.library_id == show_item.library_id);
    }

    if let Some(season_num) = season_filter {
        visible_eps.retain(|ep| ep.metadata.season == Some(season_num));
    }

    sort_episodes(&mut visible_eps);

    Ok(visible_eps.into_iter().filter_map(|ep| ep.id).collect())
}

async fn expand_items_request(
    req: &AddPlaylistItemsRequest,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
    lib_repo: &LibraryRepository,
) -> Result<Vec<i64>, ApiErr> {
    if req.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "No playable media items to add",
        ));
    }

    let all_libs = lib_repo
        .get_all()
        .await
        .map_err(|e| internal_err(e, "Failed to load libraries"))?;
    let libs_map: HashMap<String, Library> =
        all_libs.into_iter().map(|l| (l.id.clone(), l)).collect();

    let mut candidates = Vec::new();
    let mut saw_empty_show_in_ids = false;

    // 1. Expand item_ids in order
    if let Some(ref ids) = req.item_ids {
        for &item_id in ids {
            let item = media_repo
                .get_by_id(item_id)
                .await
                .map_err(|e| internal_err(e, "Failed to query media item"))?
                .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Media item not found"))?;

            ensure_library_unlocked(&item.library_id, &libs_map, unlocked)?;

            match item.item_type {
                MediaType::Season => {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "Non-playable media item type",
                    ));
                }
                MediaType::Show => {
                    let ep_ids =
                        expand_show_episodes(&item, None, &libs_map, unlocked, media_repo).await?;
                    if ep_ids.is_empty() {
                        saw_empty_show_in_ids = true;
                    } else {
                        candidates.extend(ep_ids);
                    }
                }
                _ => {
                    if let Some(id) = item.id {
                        candidates.push(id);
                    }
                }
            }
        }
    }

    // 2. Expand show spec
    if let Some(AddShowItemsSpec {
        show_id,
        season_number,
    }) = req.show
    {
        let show_item = media_repo
            .get_by_id(show_id)
            .await
            .map_err(|e| internal_err(e, "Failed to query show item"))?
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Media item not found"))?;

        ensure_library_unlocked(&show_item.library_id, &libs_map, unlocked)?;

        if show_item.item_type != MediaType::Show {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "Specified item is not a show",
            ));
        }

        let ep_ids =
            expand_show_episodes(&show_item, season_number, &libs_map, unlocked, media_repo)
                .await?;
        if ep_ids.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "No playable episodes found",
            ));
        }
        candidates.extend(ep_ids);
    }

    // 3. Expand folder spec
    if let Some(ref folder_spec) = req.folder {
        let folder_ids =
            expand_folder_items(folder_spec, &libs_map, unlocked, media_repo).await?;
        candidates.extend(folder_ids);
    }

    // Deduplicate preserving first-seen order
    let mut seen = HashSet::with_capacity(candidates.len());
    candidates.retain(|id| seen.insert(*id));

    if candidates.is_empty() {
        let msg = if saw_empty_show_in_ids {
            "Show has no playable episodes"
        } else {
            "No playable media items to add"
        };
        return Err(api_err(StatusCode::BAD_REQUEST, msg));
    }

    Ok(candidates)
}

fn is_playable_media_file(path: &FsPath) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mkv"
            | "mp4"
            | "webm"
            | "avi"
            | "mov"
            | "m4v"
            | "ts"
            | "m2ts"
            | "mpg"
            | "mpeg"
            | "wmv"
            | "flv"
            | "mp3"
            | "flac"
            | "m4a"
            | "aac"
            | "ogg"
            | "opus"
            | "wav"
            | "m4b"
    )
}

fn validate_relative_folder_path(raw: &str) -> Result<String, ApiErr> {
    let trimmed = raw.trim();
    if trimmed.contains('\0') || trimmed.contains("..") {
        return Err(api_err(StatusCode::BAD_REQUEST, "Invalid folder path"));
    }

    let rel_check = FsPath::new(trimmed);
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || rel_check.is_absolute()
        || rel_check.has_root()
        || trimmed.contains(':')
    {
        return Err(api_err(StatusCode::BAD_REQUEST, "Invalid folder path"));
    }

    for comp in rel_check.components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(api_err(StatusCode::BAD_REQUEST, "Invalid folder path"));
            }
            _ => {}
        }
    }

    Ok(trimmed.trim_matches(|c| c == '/' || c == '\\').to_string())
}

fn collect_media_files_in_dir(
    base_dir: &FsPath,
    current_dir: &FsPath,
    library_root: &FsPath,
    recursive: bool,
    visited_dirs: &mut HashSet<PathBuf>,
    out: &mut Vec<(String, PathBuf)>,
) {
    let Ok(canon_current) = std::fs::canonicalize(current_dir) else {
        return;
    };
    if !canon_current.starts_with(library_root) || !visited_dirs.insert(canon_current) {
        return;
    }

    let Ok(entries) = std::fs::read_dir(current_dir) else {
        return;
    };

    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        let is_dir = ft.is_dir() || (ft.is_symlink() && path.is_dir());
        let is_file = ft.is_file() || (ft.is_symlink() && path.is_file());

        if is_dir {
            if recursive {
                subdirs.push(path);
            }
        } else if is_file && is_playable_media_file(&path) {
            if let Ok(canon_file) = std::fs::canonicalize(&path) {
                if !canon_file.starts_with(library_root) {
                    continue;
                }
            }
            let rel = path
                .strip_prefix(base_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }

    if recursive {
        subdirs.sort();
        for sub in subdirs {
            collect_media_files_in_dir(base_dir, &sub, library_root, true, visited_dirs, out);
        }
    }
}

async fn expand_folder_items(
    spec: &AddFolderItemsSpec,
    libs_map: &HashMap<String, Library>,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
) -> Result<Vec<i64>, ApiErr> {
    let Some(library) = libs_map.get(&spec.library_id) else {
        return Err(api_err(StatusCode::NOT_FOUND, "Library not found"));
    };

    if library.is_private && !unlocked.is_unlocked(&library.id) {
        return Err(api_err(StatusCode::FORBIDDEN, "LIBRARY_LOCKED"));
    }

    let clean_path = validate_relative_folder_path(&spec.path)?;

    let raw_roots = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    let mut canonical_roots = Vec::new();
    for root in &raw_roots {
        if let Ok(canon) = std::fs::canonicalize(root) {
            canonical_roots.push(canon);
        }
    }
    canonical_roots.sort();
    canonical_roots.dedup();

    if canonical_roots.is_empty() {
        return Err(api_err(StatusCode::NOT_FOUND, "Folder not found"));
    }

    let mut matched_targets: Vec<(PathBuf, PathBuf)> = Vec::new(); // (library_root, target_dir)
    if clean_path.is_empty() {
        for root in &canonical_roots {
            matched_targets.push((root.clone(), root.clone()));
        }
    } else {
        let mut any_exists = false;
        for root in &canonical_roots {
            let candidate = root.join(&clean_path);
            if candidate.exists() {
                any_exists = true;
                let canon = std::fs::canonicalize(&candidate)
                    .map_err(|_| api_err(StatusCode::BAD_REQUEST, "Invalid folder path"))?;
                if !canon.starts_with(root) || !canon.is_dir() {
                    return Err(api_err(StatusCode::BAD_REQUEST, "Invalid folder path"));
                }
                matched_targets.push((root.clone(), canon));
            }
        }
        if !any_exists || matched_targets.is_empty() {
            return Err(api_err(StatusCode::NOT_FOUND, "Folder not found"));
        }
    }

    let recursive = spec.recursive;
    let discovered = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        let mut visited = HashSet::new();
        for (lib_root, target_dir) in &matched_targets {
            collect_media_files_in_dir(
                target_dir,
                target_dir,
                lib_root,
                recursive,
                &mut visited,
                &mut files,
            );
        }
        files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        files.dedup_by(|a, b| a.1 == b.1);
        files
    })
    .await
    .map_err(|e| internal_err(e, "Folder walk task failed"))?;

    if discovered.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "No playable media items in folder",
        ));
    }

    let canon_paths: Vec<PathBuf> = discovered
        .iter()
        .filter_map(|(_, p)| std::fs::canonicalize(p).ok())
        .collect();

    let mut lookup_refs: Vec<&FsPath> = Vec::with_capacity(discovered.len() + canon_paths.len());
    for (_, p) in &discovered {
        lookup_refs.push(p.as_path());
    }
    for p in &canon_paths {
        lookup_refs.push(p.as_path());
    }

    let media_map = media_repo
        .find_by_paths(&lookup_refs)
        .await
        .map_err(|e| internal_err(e, "Failed to resolve media items for folder"))?;

    let mut resolved_ids = Vec::new();
    for (_, file_path) in &discovered {
        let item_opt = media_map.get(file_path).or_else(|| {
            std::fs::canonicalize(file_path)
                .ok()
                .and_then(|c| media_map.get(&c))
        });
        if let Some(item) = item_opt {
            if item.library_id == spec.library_id
                && item.item_type != MediaType::Show
                && item.item_type != MediaType::Season
            {
                if let Some(id) = item.id {
                    resolved_ids.push(id);
                }
            }
        }
    }

    if resolved_ids.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "No playable media items in folder",
        ));
    }

    Ok(resolved_ids)
}
