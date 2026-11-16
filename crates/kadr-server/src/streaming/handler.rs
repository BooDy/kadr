use axum::{
    body::Body,
    extract::{Extension, Path},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use std::io::SeekFrom;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::plugins::PluginService;
use crate::streaming::range::{parse_range_header, RangeResult};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

pub fn resolve_mime(ext: &str) -> &'static str {
    let video_mime = kadr_core::video_mime_for_extension(ext);
    if video_mime != "application/octet-stream" {
        return video_mime;
    }
    match ext
        .strip_prefix('.')
        .unwrap_or(ext)
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

pub async fn stream_media_item(
    auth_user: Option<AuthUser>,
    Path(item_id): Path<i64>,
    headers: HeaderMap,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    plugins: Option<Extension<PluginService>>,
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
            tracing::error!("Failed to fetch library for streaming item {item_id}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    }

    if item.plugin_ref().is_some() {
        let Some(Extension(plugins)) = plugins else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "PLUGIN_UNAVAILABLE", "message": "Plugins are not available" })),
            )
                .into_response();
        };
        let user_id = auth_user
            .as_ref()
            .map(|u| u.id.clone())
            .unwrap_or_else(|| "anonymous".to_string());
        return crate::plugins::playback::stream_plugin_item(&plugins, &item, &user_id, &headers)
            .await;
    }

    let file = match File::open(&item.file_path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media file missing from disk" })),
            )
                .into_response();
        }
    };

    let total_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read file metadata" })),
            )
                .into_response();
        }
    };

    let mut mime = item
        .technical
        .container
        .as_deref()
        .map(resolve_mime)
        .unwrap_or("application/octet-stream");
    if mime == "application/octet-stream" {
        if let Some(ext) = item.file_path.extension().and_then(|e| e.to_str()) {
            mime = resolve_mime(ext);
        }
    }

    let range_result = headers
        .get(header::RANGE)
        .and_then(|h| h.to_str().ok())
        .map(|h| parse_range_header(h, total_size))
        .unwrap_or(RangeResult::Ignore);

    match range_result {
        RangeResult::Satisfiable { start, end } => {
            let length = end - start + 1;
            let mut file = file;
            if file.seek(SeekFrom::Start(start)).await.is_err() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Seek error" })),
                )
                    .into_response();
            }

            let stream = ReaderStream::with_capacity(file.take(length), 64 * 1024);
            let body = Body::from_stream(stream);

            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, length.to_string())
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, total_size),
                )
                .body(body)
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        RangeResult::Unsatisfiable => Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes */{}", total_size))
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        RangeResult::Ignore => {
            let stream = ReaderStream::with_capacity(file, 64 * 1024);
            let body = Body::from_stream(stream);

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, total_size.to_string())
                .body(body)
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_mime_types() {
        assert_eq!(resolve_mime("mp4"), "video/mp4");
        assert_eq!(resolve_mime("MP4"), "video/mp4");
        assert_eq!(resolve_mime("m4v"), "video/mp4");
        assert_eq!(resolve_mime("M4V"), "video/mp4");
        assert_eq!(resolve_mime("mkv"), "video/x-matroska");
        assert_eq!(resolve_mime("webm"), "video/webm");
        assert_eq!(resolve_mime("avi"), "video/x-msvideo");
        assert_eq!(resolve_mime("mov"), "video/quicktime");
        assert_eq!(resolve_mime("MOV"), "video/quicktime");
        assert_eq!(resolve_mime("ts"), "video/mp2t");
        assert_eq!(resolve_mime("m2ts"), "video/mp2t");
        assert_eq!(resolve_mime("mts"), "video/mp2t");
        assert_eq!(resolve_mime("3gp"), "video/3gpp");
        assert_eq!(resolve_mime("3g2"), "video/3gpp2");
        assert_eq!(resolve_mime("mp3"), "audio/mpeg");
        assert_eq!(resolve_mime("flac"), "audio/flac");
        assert_eq!(resolve_mime("unknown"), "application/octet-stream");
    }
}
