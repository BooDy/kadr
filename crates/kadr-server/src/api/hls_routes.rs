use axum::{
    body::Body,
    extract::{Extension, Path, Query, RawQuery},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
    Json, Router,
};
use kadr_core::transcoding::TranscodeParameters;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use serde::Deserialize;
use std::fmt::Write;
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::api::unlock_token::UnlockedLibraries;
use crate::streaming::hls::session::{HlsError, HlsSessionManager};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HlsQuery {
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub max_height: Option<u32>,
    pub start_time: Option<f64>,
    #[serde(alias = "audio_track_index")]
    pub audio_track: Option<u32>,
}

impl From<HlsQuery> for TranscodeParameters {
    fn from(q: HlsQuery) -> Self {
        Self {
            video_codec: q.video_codec.unwrap_or_else(|| "h264".to_string()),
            audio_codec: q.audio_codec.unwrap_or_else(|| "aac".to_string()),
            max_height: q.max_height,
            audio_track_index: q.audio_track.unwrap_or(0),
            start_time: q.start_time.unwrap_or(0.0),
        }
    }
}

pub fn parse_segment_seq(segment: &str) -> Option<u32> {
    let s = segment.strip_prefix("segment_")?.strip_suffix(".ts")?;
    s.parse::<u32>().ok()
}

pub async fn master_playlist(
    Path(item_id): Path<i64>,
    Query(query): Query<HlsQuery>,
    RawQuery(raw_query): RawQuery,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(session_mgr): Extension<Arc<HlsSessionManager>>,
) -> Response {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response();
        }
        Err(err) => {
            tracing::error!("Failed to fetch media item {item_id}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    };

    match lib_repo.get_by_id(&item.library_id).await {
        Ok(Some(library)) => {
            if library.is_private && !unlocked.is_unlocked(&item.library_id) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
                )
                    .into_response();
            }
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!("Failed to fetch library for HLS streaming item {item_id}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    }

    if !tokio::fs::try_exists(&item.file_path)
        .await
        .unwrap_or(false)
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media file missing from disk" })),
        )
            .into_response();
    }

    let params = TranscodeParameters::from(query);
    let session = match session_mgr
        .get_or_create_session(
            item.id.unwrap_or(item_id),
            &item.file_path,
            &item.technical,
            &params,
        )
        .await
    {
        Ok(s) => s,
        Err(HlsError::Disabled) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Transcoding is disabled" })),
            )
                .into_response();
        }
        Err(HlsError::CapacityExceeded) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": "Transcoding capacity exceeded" })),
            )
                .into_response();
        }
        Err(err) => {
            tracing::error!("Failed to create HLS transcode session: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Transcoding failed: {err}") })),
            )
                .into_response();
        }
    };

    let playlist_path = match session_mgr.wait_for_playlist(&session, 15).await {
        Ok(p) => p,
        Err(HlsError::Timeout(res)) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({ "error": format!("Timed out waiting for {res}") })),
            )
                .into_response();
        }
        Err(err) => {
            tracing::error!("Failed waiting for playlist: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Transcoding failed: {err}") })),
            )
                .into_response();
        }
    };

    let playlist_content = match tokio::fs::read_to_string(&playlist_path).await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!("Failed to read playlist file: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read playlist" })),
            )
                .into_response();
        }
    };

    let auth_params: Vec<&str> = raw_query
        .as_deref()
        .unwrap_or_default()
        .split('&')
        .filter(|p| {
            p.starts_with("token=")
                || p.starts_with("unlocked=")
                || p.starts_with("unlock_token=")
        })
        .collect();
    let query_suffix = if auth_params.is_empty() {
        String::new()
    } else {
        format!("?{}", auth_params.join("&"))
    };

    let mut rewritten = String::with_capacity(playlist_content.len() + 256);
    for line in playlist_content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            let filename = std::path::Path::new(trimmed)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(trimmed);
            let _ = writeln!(
                rewritten,
                "/api/v1/media/{}/hls/sessions/{}/{}{}",
                item_id, session.session_id, filename, query_suffix
            );
        } else {
            rewritten.push_str(line);
            rewritten.push('\n');
        }
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/vnd.apple.mpegurl"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        rewritten,
    )
        .into_response()
}

pub async fn stream_segment(
    Path((item_id, session_id, segment)): Path<(i64, String, String)>,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(session_mgr): Extension<Arc<HlsSessionManager>>,
) -> Response {
    if Uuid::parse_str(&session_id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid session ID format" })),
        )
            .into_response();
    }

    let seq = match parse_segment_seq(&segment) {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid segment filename format" })),
            )
                .into_response();
        }
    };

    let session = match session_mgr.get_session(&session_id).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Session not found" })),
            )
                .into_response();
        }
    };

    if session.media_id != item_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Session does not match media item" })),
        )
            .into_response();
    }

    session.touch();

    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response();
        }
        Err(err) => {
            tracing::error!("Failed to fetch media item {item_id}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    };

    match lib_repo.get_by_id(&item.library_id).await {
        Ok(Some(library)) => {
            if library.is_private && !unlocked.is_unlocked(&item.library_id) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
                )
                    .into_response();
            }
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!("Failed to fetch library for HLS segment streaming {item_id}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    }

    let segment_path = match session_mgr.wait_for_segment(&session, seq, 15).await {
        Ok(p) => p,
        Err(HlsError::Timeout(res)) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({ "error": format!("Timed out waiting for {res}") })),
            )
                .into_response();
        }
        Err(err) => {
            tracing::error!("Failed waiting for segment {seq}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Transcoding failed: {err}") })),
            )
                .into_response();
        }
    };

    let file = match tokio::fs::File::open(&segment_path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Segment file missing from disk" })),
            )
                .into_response();
        }
    };

    let content_length = match file.metadata().await {
        Ok(m) => Some(m.len()),
        Err(_) => None,
    };

    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = Body::from_stream(stream);

    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("video/mp2t"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("public, max-age=3600"),
    );
    if let Some(len) = content_length {
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, header::HeaderValue::from(len));
    }

    response
}

pub async fn delete_session(
    Path((item_id, session_id)): Path<(i64, String)>,
    Extension(session_mgr): Extension<Arc<HlsSessionManager>>,
) -> Response {
    if Uuid::parse_str(&session_id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid session ID format" })),
        )
            .into_response();
    }

    if let Some(session) = session_mgr.get_session(&session_id).await {
        if session.media_id != item_id {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Session does not match media item" })),
            )
                .into_response();
        }
    }

    session_mgr.terminate_session(&session_id).await;
    StatusCode::NO_CONTENT.into_response()
}

pub fn hls_routes() -> Router {
    Router::new()
        .route(
            "/api/v1/media/{item_id}/hls/master.m3u8",
            get(master_playlist),
        )
        .route(
            "/api/v1/media/{item_id}/hls/sessions/{session_id}/{segment}",
            get(stream_segment),
        )
        .route(
            "/api/v1/media/{item_id}/hls/sessions/{session_id}",
            delete(delete_session),
        )
}
