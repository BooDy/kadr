use axum::{
    body::Body,
    extract::{Extension, Path},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use kadr_storage::repos::MediaItemRepository;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

/// Resolves the MIME content type based on the file extension.
/// Supported extensions:
/// - `.jpg` / `.jpeg` -> `image/jpeg`
/// - `.png` -> `image/png`
/// - `.webp` -> `image/webp`
/// - `.avif` -> `image/avif`
/// - default / fallback -> `image/jpeg`
pub fn resolve_artwork_mime(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "avif" => "image/avif",
        _ => "image/jpeg",
    }
}

enum ArtworkType {
    Poster,
    Backdrop,
}

async fn stream_artwork(
    item_id: i64,
    artwork_type: ArtworkType,
    media_repo: MediaItemRepository,
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

    let path_opt = match artwork_type {
        ArtworkType::Poster => item.metadata.poster_path,
        ArtworkType::Backdrop => item.metadata.backdrop_path,
    };

    let Some(artwork_path) = path_opt.filter(|s| !s.trim().is_empty()) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Artwork not found" })),
        )
            .into_response();
    };

    let file = match File::open(&artwork_path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Artwork not found" })),
            )
                .into_response();
        }
    };

    let total_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(err) => {
            tracing::error!("Failed to read metadata for artwork {artwork_path}: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read artwork file metadata" })),
            )
                .into_response();
        }
    };

    let mime = resolve_artwork_mime(&artwork_path);
    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, total_size.to_string())
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Handler for `GET /api/v1/artwork/{item_id}/poster`.
pub async fn get_poster(
    Path(item_id): Path<i64>,
    Extension(media_repo): Extension<MediaItemRepository>,
) -> Response {
    stream_artwork(item_id, ArtworkType::Poster, media_repo).await
}

/// Handler for `GET /api/v1/artwork/{item_id}/backdrop`.
pub async fn get_backdrop(
    Path(item_id): Path<i64>,
    Extension(media_repo): Extension<MediaItemRepository>,
) -> Response {
    stream_artwork(item_id, ArtworkType::Backdrop, media_repo).await
}
