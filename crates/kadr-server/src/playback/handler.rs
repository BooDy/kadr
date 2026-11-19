use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::events::EventBus;
use crate::playback::session::SessionRegistry;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use kadr_core::events::SystemEvent;
use kadr_core::models::{PlaybackSession, WatchState};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct CreateSessionRequest {
    pub media_item_id: i64,
}

#[derive(Serialize, Debug)]
pub struct SessionResponse {
    pub session_id: String,
    pub media_item_id: i64,
    pub duration_seconds: i64,
    pub resume_position_seconds: i64,
}

#[derive(Deserialize, Debug)]
pub struct ProgressHeartbeatRequest {
    pub position_seconds: i64,
}

pub fn evaluate_scrobble(position: i64, duration: i64) -> WatchState {
    if duration <= 0 {
        return WatchState::Unwatched;
    }
    let pct = position as f64 / duration as f64;
    if pct >= 0.90 {
        WatchState::Completed
    } else if position > 60 || pct > 0.02 {
        WatchState::InProgress
    } else {
        WatchState::Unwatched
    }
}

pub async fn create_session(
    auth_user: AuthUser,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    Extension(sessions): Extension<Arc<SessionRegistry>>,
    Json(payload): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let item = match media_repo.get_by_id(payload.media_item_id).await {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response();
        }
    };

    let resume_pos = if item.is_live() {
        0
    } else {
        match playback_repo
            .get_state(&auth_user.id, payload.media_item_id)
            .await
        {
            Ok(Some(s)) => {
                if s.watch_state == WatchState::Completed {
                    0
                } else {
                    s.playback_position_seconds
                }
            }
            _ => 0,
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session = PlaybackSession {
        session_id: Uuid::new_v4().to_string(),
        user_id: auth_user.id,
        media_item_id: payload.media_item_id,
        duration_seconds: item.technical.duration_seconds,
        current_position_seconds: resume_pos,
        started_at: now,
        last_heartbeat_at: now,
    };

    sessions.insert_live(session.clone(), item.is_live()).await;

    (
        StatusCode::CREATED,
        Json(SessionResponse {
            session_id: session.session_id,
            media_item_id: session.media_item_id,
            duration_seconds: session.duration_seconds,
            resume_position_seconds: resume_pos,
        }),
    )
        .into_response()
}

pub async fn progress_heartbeat(
    auth_user: AuthUser,
    Path(session_id): Path<String>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    Extension(sessions): Extension<Arc<SessionRegistry>>,
    event_bus: Option<Extension<Arc<EventBus>>>,
    Json(payload): Json<ProgressHeartbeatRequest>,
) -> impl IntoResponse {
    let session = match sessions.get(&session_id).await {
        Some(s) if s.user_id == auth_user.id => s,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Playback session not found or unauthorized" })),
            )
                .into_response();
        }
    };

    // Live items: skip scrobbling and never persist watch state.
    if sessions.is_live(&session_id).await {
        sessions
            .update_progress(&session_id, payload.position_seconds)
            .await;
        return StatusCode::OK.into_response();
    }

    let watch_state = evaluate_scrobble(payload.position_seconds, session.duration_seconds);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if let Err(e) = playback_repo
        .upsert_progress(
            &auth_user.id,
            session.media_item_id,
            payload.position_seconds,
            watch_state,
            now,
        )
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    sessions
        .update_progress(&session_id, payload.position_seconds)
        .await;

    if let Some(Extension(event_bus)) = event_bus {
        event_bus.publish(SystemEvent::SessionSynced {
            session_id: session_id.clone(),
            item_id: session.media_item_id,
            user_id: auth_user.id.clone(),
            position_seconds: payload.position_seconds,
            timestamp: now,
        });
    }

    StatusCode::OK.into_response()
}

pub async fn get_playback_state(
    auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> impl IntoResponse {
    match playback_repo.get_state(&auth_user.id, item_id).await {
        Ok(Some(s)) => Json(s).into_response(),
        Ok(None) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "user_id": auth_user.id,
                "media_item_id": item_id,
                "playback_position_seconds": 0,
                "watch_state": "unwatched",
                "last_watched_at": 0,
                "play_count": 0
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn list_continue_watching(
    auth_user: AuthUser,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> impl IntoResponse {
    match playback_repo
        .list_user_states(&auth_user.id, Some(WatchState::InProgress), 50)
        .await
    {
        Ok(list) => Json(list).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn close_session(
    auth_user: AuthUser,
    Path(session_id): Path<String>,
    Extension(sessions): Extension<Arc<SessionRegistry>>,
) -> impl IntoResponse {
    if let Some(session) = sessions.get(&session_id).await {
        if session.user_id == auth_user.id {
            sessions.remove(&session_id).await;
            return StatusCode::NO_CONTENT.into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

#[derive(Deserialize, Debug)]
pub struct SetWatchStateRequest {
    pub watched: bool,
}

#[derive(Deserialize, Debug)]
pub struct SetBatchWatchStateRequest {
    pub item_ids: Vec<i64>,
    pub watched: bool,
}

async fn check_library_unlocked(
    lib_repo: &LibraryRepository,
    unlocked: &UnlockedLibraries,
    library_id: &str,
) -> Option<Response> {
    match lib_repo.get_by_id(library_id).await {
        Ok(Some(lib)) if lib.is_private && !unlocked.is_unlocked(library_id) => Some(
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
            )
                .into_response(),
        ),
        Ok(_) => None,
        Err(e) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        ),
    }
}

fn publish_home_layout_changed(event_bus: Option<&Extension<Arc<EventBus>>>, timestamp: i64) {
    if let Some(Extension(bus)) = event_bus {
        bus.publish(SystemEvent::LayoutChanged {
            screen_id: "home".to_string(),
            timestamp,
        });
    }
}

/// Manually sets the watch state (`completed` or `unwatched`) for a single media item.
#[allow(clippy::too_many_arguments)]
pub async fn set_playback_state(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    event_bus: Option<Extension<Arc<EventBus>>>,
    Json(payload): Json<SetWatchStateRequest>,
) -> impl IntoResponse {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    if item.is_live() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Live items do not support watch state" })),
        )
            .into_response();
    }

    if let Some(resp) = check_library_unlocked(&lib_repo, &unlocked, &item.library_id).await {
        return resp;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if let Err(e) = playback_repo
        .set_watch_states_batch(
            &auth_user.id,
            &[(item_id, item.technical.duration_seconds)],
            payload.watched,
            now,
        )
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    publish_home_layout_changed(event_bus.as_ref(), now);

    match playback_repo.get_state(&auth_user.id, item_id).await {
        Ok(Some(s)) => (StatusCode::OK, Json(s)).into_response(),
        Ok(None) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Manually sets the watch state (`completed` or `unwatched`) for up to 500 media items in a single batch.
pub async fn set_playback_states_batch(
    auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
    event_bus: Option<Extension<Arc<EventBus>>>,
    Json(payload): Json<SetBatchWatchStateRequest>,
) -> impl IntoResponse {
    let mut seen = HashSet::new();
    let unique_ids: Vec<i64> = payload
        .item_ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .collect();

    if unique_ids.is_empty() || unique_ids.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "item_ids must contain 1..=500 items" })),
        )
            .into_response();
    }

    let items_map = match media_repo.get_by_ids(&unique_ids).await {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let mut checked_libs = HashSet::new();
    for item in items_map.values() {
        if checked_libs.insert(item.library_id.clone()) {
            if let Some(resp) =
                check_library_unlocked(&lib_repo, &unlocked, &item.library_id).await
            {
                return resp;
            }
        }
    }

    let entries: Vec<(i64, i64)> = unique_ids
        .into_iter()
        .filter_map(|id| items_map.get(&id).map(|it| (id, it)))
        .filter(|(_, it)| !it.is_live())
        .map(|(id, it)| (id, it.technical.duration_seconds))
        .collect();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    match playback_repo
        .set_watch_states_batch(&auth_user.id, &entries, payload.watched, now)
        .await
    {
        Ok(updated) => {
            if updated > 0 {
                publish_home_layout_changed(event_bus.as_ref(), now);
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "updated": updated,
                    "watched": payload.watched,
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_scrobble_thresholds() {
        let duration = 5000;
        // Under 60s and <= 2% (100s) -> Unwatched
        assert_eq!(evaluate_scrobble(30, duration), WatchState::Unwatched);
        assert_eq!(evaluate_scrobble(60, duration), WatchState::Unwatched);

        // > 60s -> InProgress
        assert_eq!(evaluate_scrobble(61, duration), WatchState::InProgress);

        // > 2% of short duration (e.g. 100s duration, 3s position = 3% > 2%)
        assert_eq!(evaluate_scrobble(3, 100), WatchState::InProgress);

        // >= 90% (4500s of 5000s) -> Completed
        assert_eq!(evaluate_scrobble(4499, duration), WatchState::InProgress);
        assert_eq!(evaluate_scrobble(4500, duration), WatchState::Completed);
        assert_eq!(evaluate_scrobble(5000, duration), WatchState::Completed);
        assert_eq!(evaluate_scrobble(5500, duration), WatchState::Completed);

        // Duration <= 0 -> Unwatched
        assert_eq!(evaluate_scrobble(100, 0), WatchState::Unwatched);
        assert_eq!(evaluate_scrobble(100, -10), WatchState::Unwatched);
    }
}
