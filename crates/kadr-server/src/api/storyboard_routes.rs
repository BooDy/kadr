use axum::{
    body::Body,
    extract::{Extension, Path, Query},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kadr_core::models::{MediaItem, MediaType};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use serde::Deserialize;
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::storyboard::{
    clamp_tile_width, snap_frame_time, FrameStatus, SheetStatus, StoryboardService,
    StoryboardStatus,
};

#[derive(Debug, Default, Deserialize)]
pub struct FrameQuery {
    pub t: Option<f64>,
    pub w: Option<u32>,
}

fn has_video_stream(item: &MediaItem) -> bool {
    if item.plugin_ref().is_some() {
        return false;
    }
    if item.technical.duration_seconds <= 0 {
        return false;
    }
    if matches!(
        item.item_type,
        MediaType::Music
            | MediaType::Audiobook
            | MediaType::Show
            | MediaType::Season
            | MediaType::LiveChannel
    ) {
        return false;
    }
    if let Some(ref container) = item.technical.container {
        if matches!(
            container.to_ascii_lowercase().as_str(),
            "mp3" | "flac" | "aac" | "ogg" | "wav" | "m4a" | "opus"
        ) && item.technical.video_codec.is_none()
        {
            return false;
        }
    }
    if let Some(ext) = item.file_path.extension().and_then(|e| e.to_str()) {
        if matches!(
            ext.to_ascii_lowercase().as_str(),
            "mp3" | "flac" | "aac" | "ogg" | "wav" | "m4a" | "opus"
        ) && item.technical.video_codec.is_none()
        {
            return false;
        }
    }
    true
}

async fn load_and_authorize_item(
    item_id: i64,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
    lib_repo: &LibraryRepository,
    storyboard_svc: &StoryboardService,
) -> Result<MediaItem, Response> {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response());
        }
        Err(err) => {
            tracing::error!("Failed to fetch media item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response());
        }
    };

    match lib_repo.get_by_id(&item.library_id).await {
        Ok(Some(library)) => {
            if library.is_private && !unlocked.is_unlocked(&item.library_id) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
                )
                    .into_response());
            }
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!("Failed to fetch library for media item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response());
        }
    }

    if !storyboard_svc.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response());
    }

    if !has_video_stream(&item) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response());
    }

    match tokio::fs::metadata(&item.file_path).await {
        Ok(meta) if meta.is_file() => Ok(item),
        _ => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media file missing from disk" })),
        )
            .into_response()),
    }
}

fn generating_response() -> Response {
    (
        StatusCode::ACCEPTED,
        [(header::RETRY_AFTER, "5")],
        Json(serde_json::json!({ "status": "generating" })),
    )
        .into_response()
}

async fn serve_cached_jpeg(path: &std::path::Path, etag: &str, headers: &HeaderMap) -> Response {
    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if if_none_match
            .split(',')
            .any(|candidate| candidate.trim() == etag || candidate.trim() == "*")
        {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, etag)
                .header(header::CACHE_CONTROL, "public, max-age=604800")
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::NOT_MODIFIED.into_response());
        }
    }

    let file = match File::open(path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Storyboard sheet not found" })),
            )
                .into_response();
        }
    };

    let content_len = file.metadata().await.ok().map(|m| m.len());
    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = Body::from_stream(stream);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "public, max-age=604800")
        .header(header::ETAG, etag);

    if let Some(len) = content_len {
        builder = builder.header(header::CONTENT_LENGTH, len.to_string());
    }

    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub async fn get_storyboard_manifest(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(storyboard_svc): Extension<Arc<StoryboardService>>,
) -> Response {
    let item =
        match load_and_authorize_item(item_id, &unlocked, &media_repo, &lib_repo, &storyboard_svc)
            .await
        {
            Ok(i) => i,
            Err(resp) => return resp,
        };

    match storyboard_svc
        .get_or_enqueue_manifest(item_id, &item.file_path, item.technical.duration_seconds)
        .await
    {
        Ok(StoryboardStatus::Ready(manifest)) => (StatusCode::OK, Json(manifest)).into_response(),
        Ok(StoryboardStatus::Generating) => generating_response(),
        Ok(StoryboardStatus::NoVideoStream) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response(),
        Ok(StoryboardStatus::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Storyboard manifest error for item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read storyboard" })),
            )
                .into_response()
        }
    }
}

pub async fn get_storyboard_sheet(
    _auth_user: AuthUser,
    Path((item_id, sheet_name)): Path<(i64, String)>,
    headers: HeaderMap,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(storyboard_svc): Extension<Arc<StoryboardService>>,
) -> Response {
    let Some(index_str) = sheet_name.strip_suffix(".jpg") else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Storyboard sheet not found" })),
        )
            .into_response();
    };
    let Ok(index) = index_str.parse::<u32>() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Storyboard sheet not found" })),
        )
            .into_response();
    };

    let item =
        match load_and_authorize_item(item_id, &unlocked, &media_repo, &lib_repo, &storyboard_svc)
            .await
        {
            Ok(i) => i,
            Err(resp) => return resp,
        };

    match storyboard_svc
        .get_or_enqueue_sheet(
            item_id,
            index,
            &item.file_path,
            item.technical.duration_seconds,
        )
        .await
    {
        Ok(SheetStatus::Ready { path, etag }) => serve_cached_jpeg(&path, &etag, &headers).await,
        Ok(SheetStatus::Generating) => generating_response(),
        Ok(SheetStatus::OutOfRange) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Storyboard sheet index out of range" })),
        )
            .into_response(),
        Ok(SheetStatus::NoVideoStream) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response(),
        Ok(SheetStatus::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Storyboard sheet error for item {item_id} index {index}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read storyboard sheet" })),
            )
                .into_response()
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn get_single_frame(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Query(query): Query<FrameQuery>,
    headers: HeaderMap,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(storyboard_svc): Extension<Arc<StoryboardService>>,
) -> Response {
    let item =
        match load_and_authorize_item(item_id, &unlocked, &media_repo, &lib_repo, &storyboard_svc)
            .await
        {
            Ok(i) => i,
            Err(resp) => return resp,
        };

    let raw_t = query.t.unwrap_or(0.0);
    let Some(snapped_t) = snap_frame_time(raw_t, item.technical.duration_seconds) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid frame timestamp" })),
        )
            .into_response();
    };
    let clamped_w = clamp_tile_width(query.w.unwrap_or(320));

    match storyboard_svc
        .extract_or_get_frame(
            &item.file_path,
            item.technical.duration_seconds,
            snapped_t,
            clamped_w,
        )
        .await
    {
        Ok(FrameStatus::Ready { path, etag }) => serve_cached_jpeg(&path, &etag, &headers).await,
        Ok(FrameStatus::NoVideoStream) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response(),
        Ok(FrameStatus::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Frame extraction error for item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to extract frame" })),
            )
                .into_response()
        }
    }
}

pub fn storyboard_routes() -> Router {
    Router::new()
        .route(
            "/api/v1/media/{item_id}/storyboard",
            get(get_storyboard_manifest),
        )
        .route(
            "/api/v1/media/{item_id}/storyboard/{sheet}",
            get(get_storyboard_sheet),
        )
        .route("/api/v1/media/{item_id}/frame", get(get_single_frame))
}
