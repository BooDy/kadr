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

use crate::auth::jwt::AuthUser;
use crate::streaming::range::{parse_range_header, RangeResult};
use kadr_storage::repos::MediaItemRepository;

pub fn resolve_mime(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

pub async fn stream_media_item(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    headers: HeaderMap,
    Extension(media_repo): Extension<MediaItemRepository>,
) -> Response {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response();
        }
    };

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

    let container = item.technical.container.unwrap_or_else(|| {
        item.file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string()
    });
    let mime = resolve_mime(&container);

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
        assert_eq!(resolve_mime("mkv"), "video/x-matroska");
        assert_eq!(resolve_mime("webm"), "video/webm");
        assert_eq!(resolve_mime("avi"), "video/x-msvideo");
        assert_eq!(resolve_mime("mov"), "video/quicktime");
        assert_eq!(resolve_mime("mp3"), "audio/mpeg");
        assert_eq!(resolve_mime("flac"), "audio/flac");
        assert_eq!(resolve_mime("unknown"), "application/octet-stream");
    }
}
