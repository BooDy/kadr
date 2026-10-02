use crate::auth::jwt::AuthUser;
use crate::events::EventBus;
use crate::playback::session::SessionRegistry;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use kadr_core::events::SystemEvent;
use kadr_core::models::{PlaybackSession, WatchState};
use kadr_storage::repos::{MediaItemRepository, PlaybackRepository};
use serde::{Deserialize, Serialize};
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

    let resume_pos = match playback_repo
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

    sessions.insert(session.clone()).await;

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
