use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use kadr_core::subtitles::{OnlineSubtitleMatch, SubtitleFormat, SubtitleSource, SubtitleTrack};
use kadr_storage::repos::MediaItemRepository;
use serde::{Deserialize, Serialize};

use crate::auth::jwt::AuthUser;
use crate::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService, SubtitleServiceError};

/// Subtitle track representation returned to API consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrackResponse {
    pub id: i64,
    pub media_item_id: i64,
    pub source: SubtitleSource,
    pub language: String,
    pub title: Option<String>,
    pub format: SubtitleFormat,
    pub is_default: bool,
    pub is_forced: bool,
    pub stream_url: String,
}

impl SubtitleTrackResponse {
    pub fn from_track(track: SubtitleTrack) -> Self {
        let stream_url = format!("/api/v1/subtitles/{}/stream.vtt", track.id);
        Self {
            id: track.id,
            media_item_id: track.media_item_id,
            source: track.source,
            language: track.language,
            title: track.title,
            format: track.format,
            is_default: track.is_default,
            is_forced: track.is_forced,
            stream_url,
        }
    }
}

/// Query parameters for online subtitle search.
#[derive(Debug, Default, Deserialize)]
pub struct SubtitleSearchQuery {
    pub languages: Option<String>,
}

/// Response payload for online subtitle search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineSubtitleSearchResponse {
    pub configured: bool,
    pub matches: Vec<OnlineSubtitleMatch>,
}

/// Request body for downloading an online subtitle track.
#[derive(Debug, Deserialize)]
pub struct DownloadSubtitleRequest {
    pub file_id: String,
    pub language: String,
    pub title: Option<String>,
    #[serde(default)]
    pub is_forced: bool,
}

/// Handler for `GET /api/v1/items/{item_id}/subtitles`.
/// Lists all subtitle tracks for a media item.
pub async fn list_subtitles(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(subtitle_service): Extension<Arc<SubtitleDeliveryService>>,
) -> Result<Json<Vec<SubtitleTrackResponse>>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Verify media item exists
    let item_exists = media_repo
        .find_by_id(item_id)
        .await
        .map_err(|e| {
            tracing::error!("Database error fetching media item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
        })?;

    if item_exists.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item not found" })),
        ));
    }

    // 2. Fetch subtitle tracks for media item
    let tracks = subtitle_service
        .subtitle_repo()
        .find_by_media_item(item_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch subtitles for item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to fetch subtitles" })),
            )
        })?;

    let responses: Vec<SubtitleTrackResponse> = tracks
        .into_iter()
        .map(SubtitleTrackResponse::from_track)
        .collect();

    Ok(Json(responses))
}

/// Handler for `GET /api/v1/subtitles/{subtitle_id}/stream.vtt`.
/// Public WebVTT streaming endpoint with disk caching and bounded memory streaming.
pub async fn stream_webvtt(
    Path(subtitle_id): Path<i64>,
    Extension(subtitle_service): Extension<Arc<SubtitleDeliveryService>>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // 1. Obtain WebVTT file path (converts on-the-fly and caches if needed)
    let vtt_path = match subtitle_service.get_webvtt_path(subtitle_id).await {
        Ok(path) => path,
        Err(SubtitleServiceError::NotFound) | Err(SubtitleServiceError::SourceFileNotFound) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Subtitle not found" })),
            ));
        }
        Err(err) => {
            tracing::error!("Failed to get WebVTT path for subtitle {subtitle_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to stream subtitle" })),
            ));
        }
    };

    // 2. Open file for bounded streaming (zero whole-file in-memory buffering)
    let file = tokio::fs::File::open(&vtt_path).await.map_err(|e| {
        tracing::error!("Failed to open WebVTT file {}: {e}", vtt_path.display());
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Subtitle not found" })),
        )
    })?;

    let stream = tokio_util::io::ReaderStream::with_capacity(file, 64 * 1024);
    let body = axum::body::Body::from_stream(stream);

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/vtt; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    response.headers_mut().insert(
        header::ACCEPT_RANGES,
        HeaderValue::from_static("bytes"),
    );

    Ok(response)
}

/// Handler for `GET /api/v1/subtitles/{item_id}/search`.
/// Searches OpenSubtitles for subtitle matches for the specified media item.
pub async fn search_online_subtitles(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Query(query): Query<SubtitleSearchQuery>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(opensubtitles_client): Extension<Arc<OpenSubtitlesClient>>,
) -> Result<Json<OnlineSubtitleSearchResponse>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Verify media item exists
    let item = media_repo
        .find_by_id(item_id)
        .await
        .map_err(|e| {
            tracing::error!("Database error fetching media item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
        })?;

    let item = match item {
        Some(item) => item,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            ));
        }
    };

    // 2. Check if OpenSubtitles client is configured
    if !opensubtitles_client.is_configured() {
        return Ok(Json(OnlineSubtitleSearchResponse {
            configured: false,
            matches: Vec::new(),
        }));
    }

    // 3. Parse language filter
    let languages: Vec<String> = query
        .languages
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // 4. Perform search
    let year = item.release_year.map(|y| y as u32);
    let matches = opensubtitles_client
        .search(&item.title, year, &languages)
        .await
        .map_err(|e| {
            tracing::error!("OpenSubtitles search failed for item {item_id}: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("OpenSubtitles search failed: {e}") })),
            )
        })?;

    Ok(Json(OnlineSubtitleSearchResponse {
        configured: true,
        matches,
    }))
}

/// Handler for `POST /api/v1/subtitles/{item_id}/download`.
/// Downloads an online subtitle file, writes it to disk, and indexes it in the database.
pub async fn download_subtitle(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(subtitle_service): Extension<Arc<SubtitleDeliveryService>>,
    Extension(opensubtitles_client): Extension<Arc<OpenSubtitlesClient>>,
    Json(body): Json<DownloadSubtitleRequest>,
) -> Result<(StatusCode, Json<SubtitleTrackResponse>), (StatusCode, Json<serde_json::Value>)> {
    // 1. Verify media item exists
    let item_exists = media_repo
        .find_by_id(item_id)
        .await
        .map_err(|e| {
            tracing::error!("Database error fetching media item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
        })?;

    if item_exists.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item not found" })),
        ));
    }

    // 2. Check if OpenSubtitles client is configured
    if !opensubtitles_client.is_configured() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "OpenSubtitles integration is not configured" })),
        ));
    }

    // 3. Download subtitle file from OpenSubtitles
    let (bytes, _file_name) = opensubtitles_client
        .download(&body.file_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to download subtitle {} from OpenSubtitles: {e}", body.file_id);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("Failed to download subtitle: {e}") })),
            )
        })?;

    // 4. Save to target path: <data_dir>/subtitles/{item_id}/{file_id}.srt
    let item_subtitles_dir = subtitle_service
        .data_dir()
        .join("subtitles")
        .join(item_id.to_string());

    tokio::fs::create_dir_all(&item_subtitles_dir).await.map_err(|e| {
        tracing::error!("Failed to create subtitle save directory {}: {e}", item_subtitles_dir.display());
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to create directory on disk" })),
        )
    })?;

    let target_file_path = item_subtitles_dir.join(format!("{}.srt", body.file_id));
    tokio::fs::write(&target_file_path, &bytes).await.map_err(|e| {
        tracing::error!("Failed to write downloaded subtitle to {}: {e}", target_file_path.display());
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to save file to disk" })),
        )
    })?;

    let file_path_str = target_file_path.to_string_lossy().into_owned();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // 5. Insert subtitle record into database
    let track = SubtitleTrack {
        id: 0,
        media_item_id: item_id,
        source: SubtitleSource::Downloaded,
        language: body.language,
        title: body.title,
        format: SubtitleFormat::Srt,
        file_path: Some(file_path_str),
        stream_index: None,
        is_default: false,
        is_forced: body.is_forced,
        created_at: now,
    };

    let inserted_id = subtitle_service
        .subtitle_repo()
        .create(&track)
        .await
        .map_err(|e| {
            tracing::error!("Failed to save subtitle record to database: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to save subtitle record" })),
            )
        })?;

    let mut created_track = track;
    created_track.id = inserted_id;

    Ok((
        StatusCode::CREATED,
        Json(SubtitleTrackResponse::from_track(created_track)),
    ))
}

/// Handler for `DELETE /api/v1/subtitles/{subtitle_id}`.
/// Deletes a subtitle track record and cleans up cached WebVTT and downloaded source files.
pub async fn delete_subtitle(
    _auth_user: AuthUser,
    Path(subtitle_id): Path<i64>,
    Extension(subtitle_service): Extension<Arc<SubtitleDeliveryService>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let deleted = subtitle_service
        .delete_track(subtitle_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete subtitle track {subtitle_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to delete subtitle track" })),
            )
        })?;

    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Subtitle track not found" })),
        ));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}
