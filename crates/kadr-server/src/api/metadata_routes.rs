use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use kadr_core::ast::ItemDetailsPayload;
use kadr_core::events::SystemEvent;
use kadr_core::models::{MediaItem, MetadataUpdate};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use serde::Deserialize;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::RequireAdmin;
use crate::events::EventBus;
use crate::metadata::{
    load_metadata_settings, update_metadata_settings, ArtworkError, ArtworkSlot,
    EnrichLibraryResult, MetadataService, MetadataServiceError, MetadataSettingsResponse,
    MetadataSettingsUpdate, TmdbError, TmdbMediaType,
};
use crate::resolver::WidgetResolver;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn err_json(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(serde_json::json!({
            "error": code,
            "message": message.into(),
        })),
    )
}

fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Default, Deserialize)]
pub struct PatchItemMetadataBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub original_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub release_year: Option<Option<i32>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub overview: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub director: Option<Option<String>>,
    #[serde(default)]
    pub writers: Option<Vec<String>>,
    #[serde(default)]
    pub actors: Option<Vec<String>>,
    #[serde(default)]
    pub genres: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub studio: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub country: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub language: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub rating: Option<Option<f32>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub poster_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub backdrop_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub series_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub season: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub episode: Option<Option<u32>>,
    #[serde(default)]
    pub reset_locks: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TmdbSearchQuery {
    pub q: Option<String>,
    pub year: Option<i32>,
    pub media_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TmdbApplyBody {
    pub tmdb_id: i64,
    pub media_type: String,
    pub download_artwork: Option<bool>,
}

pub fn metadata_routes() -> Router {
    Router::new()
        .route("/api/v1/items/{id}/metadata", patch(patch_item_metadata))
        .route("/api/v1/items/{id}/tmdb/search", get(search_item_tmdb))
        .route("/api/v1/items/{id}/tmdb/apply", post(apply_item_tmdb))
        .route(
            "/api/v1/admin/settings/metadata",
            get(get_metadata_settings).put(put_metadata_settings),
        )
        .route(
            "/api/v1/admin/libraries/{id}/tmdb/enrich",
            post(enrich_library_tmdb),
        )
}

async fn load_item_checked(
    item_id: i64,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
    lib_repo: &LibraryRepository,
) -> Result<MediaItem, ApiError> {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Item not found" })),
            ));
        }
        Err(err) => {
            tracing::error!("Failed to fetch media item {item_id}: {err}");
            return Err(err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "Failed to load media item",
            ));
        }
    };

    match lib_repo.get_by_id(&item.library_id).await {
        Ok(Some(lib)) => {
            if lib.is_private && !unlocked.is_unlocked(&item.library_id) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
                ));
            }
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!("Failed to fetch library for item {item_id}: {err}");
            return Err(err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "Failed to verify library access",
            ));
        }
    }

    Ok(item)
}

fn map_artwork_err(err: ArtworkError) -> ApiError {
    match err {
        ArtworkError::InvalidUrl(msg) => err_json(StatusCode::BAD_REQUEST, "INVALID_URL", msg),
        ArtworkError::BlockedHost(msg) => err_json(StatusCode::BAD_REQUEST, "BLOCKED_URL", msg),
        ArtworkError::TooLarge => err_json(
            StatusCode::BAD_REQUEST,
            "ARTWORK_TOO_LARGE",
            "Artwork exceeds the 15 MiB size limit",
        ),
        ArtworkError::UnsupportedFormat => err_json(
            StatusCode::BAD_REQUEST,
            "UNSUPPORTED_IMAGE",
            "Remote URL did not return a supported image (JPEG, PNG, or WebP)",
        ),
        ArtworkError::Network(msg) => err_json(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", msg),
        ArtworkError::Io(msg) => {
            err_json(StatusCode::INTERNAL_SERVER_ERROR, "ARTWORK_IO_ERROR", msg)
        }
    }
}

fn map_service_err(err: MetadataServiceError) -> ApiError {
    match err {
        MetadataServiceError::TmdbNotConfigured => err_json(
            StatusCode::BAD_REQUEST,
            "TMDB_NOT_CONFIGURED",
            "TMDb integration is disabled or API key is not configured",
        ),
        MetadataServiceError::Tmdb(TmdbError::MissingCredentials) => err_json(
            StatusCode::BAD_REQUEST,
            "TMDB_NOT_CONFIGURED",
            "TMDb API key is missing",
        ),
        MetadataServiceError::Tmdb(TmdbError::Unauthorized(_)) => err_json(
            StatusCode::BAD_GATEWAY,
            "TMDB_UNAUTHORIZED",
            "TMDb rejected the configured API key or Read Access Token",
        ),
        MetadataServiceError::Tmdb(TmdbError::NotFound) => err_json(
            StatusCode::NOT_FOUND,
            "TMDB_NOT_FOUND",
            "Requested TMDb entry was not found",
        ),
        MetadataServiceError::Tmdb(TmdbError::Upstream(msg)) => {
            err_json(StatusCode::BAD_GATEWAY, "TMDB_UPSTREAM_ERROR", msg)
        }
        MetadataServiceError::Artwork(art_err) => map_artwork_err(art_err),
        MetadataServiceError::Storage(msg) => {
            err_json(StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", msg)
        }
    }
}

async fn emit_library_updated(
    event_bus: &EventBus,
    media_repo: &MediaItemRepository,
    library_id: &str,
) {
    let count = media_repo.count_by_library(library_id).await.unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: library_id.to_string(),
        item_count: count,
        timestamp: now,
    });
}

/// `PATCH /api/v1/items/{id}/metadata`
#[allow(clippy::too_many_arguments)]
pub async fn patch_item_metadata(
    RequireAdmin(auth_user): RequireAdmin,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(metadata_svc): Extension<Arc<MetadataService>>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(body): Json<PatchItemMetadataBody>,
) -> Result<Json<ItemDetailsPayload>, ApiError> {
    let item = load_item_checked(item_id, &unlocked, &media_repo, &lib_repo).await?;

    if let Some(ref title) = body.title {
        if title.trim().is_empty() {
            return Err(err_json(
                StatusCode::BAD_REQUEST,
                "INVALID_TITLE",
                "Title cannot be empty",
            ));
        }
    }

    if let Some(Some(rating)) = body.rating {
        if !rating.is_finite() || !(0.0..=10.0).contains(&rating) {
            return Err(err_json(
                StatusCode::BAD_REQUEST,
                "INVALID_RATING",
                "Rating must be between 0.0 and 10.0",
            ));
        }
    }

    let poster_path = match body.poster_url {
        Some(Some(ref url)) if !url.trim().is_empty() => {
            let saved = metadata_svc
                .download_artwork(item_id, ArtworkSlot::Poster, url.trim())
                .await
                .map_err(map_artwork_err)?;
            Some(Some(saved.to_string_lossy().into_owned()))
        }
        Some(_) => Some(None),
        None => None,
    };

    let backdrop_path = match body.backdrop_url {
        Some(Some(ref url)) if !url.trim().is_empty() => {
            let saved = metadata_svc
                .download_artwork(item_id, ArtworkSlot::Backdrop, url.trim())
                .await
                .map_err(map_artwork_err)?;
            Some(Some(saved.to_string_lossy().into_owned()))
        }
        Some(_) => Some(None),
        None => None,
    };

    let patch = MetadataUpdate {
        title: body.title,
        original_title: body.original_title,
        release_year: body.release_year,
        overview: body.overview,
        director: body.director,
        writers: body.writers,
        actors: body.actors,
        genres: body.genres,
        tags: body.tags,
        studio: body.studio,
        country: body.country,
        language: body.language,
        rating: body.rating,
        poster_path,
        backdrop_path,
        series_title: body.series_title,
        season: body.season,
        episode: body.episode,
        tmdb_id: None,
        tmdb_media_type: None,
        reset_locks: body.reset_locks.unwrap_or(false),
    };

    match media_repo.update_metadata(item_id, &patch).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Item not found" })),
            ));
        }
        Err(err) => {
            tracing::error!("Failed to update metadata for item {item_id}: {err}");
            return Err(err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "Failed to persist metadata update",
            ));
        }
    }

    emit_library_updated(&event_bus, &media_repo, &item.library_id).await;

    let unlocked_ids = unlocked.to_vec();
    match resolver
        .resolve_item_details_with_unlocked(item_id, &auth_user.id, &unlocked_ids)
        .await
    {
        Ok(Some(payload)) => Ok(Json(payload)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Item not found" })),
        )),
        Err(err) => {
            tracing::error!("Failed to resolve updated item details for {item_id}: {err}");
            Err(err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RESOLVE_ERROR",
                "Failed to resolve item details",
            ))
        }
    }
}

/// `GET /api/v1/admin/settings/metadata`
pub async fn get_metadata_settings(
    _admin: RequireAdmin,
    Extension(metadata_svc): Extension<Arc<MetadataService>>,
) -> Json<MetadataSettingsResponse> {
    let settings = load_metadata_settings(metadata_svc.settings_repo()).await;
    Json(settings.to_response())
}

/// `PUT /api/v1/admin/settings/metadata`
pub async fn put_metadata_settings(
    _admin: RequireAdmin,
    Extension(metadata_svc): Extension<Arc<MetadataService>>,
    Json(update): Json<MetadataSettingsUpdate>,
) -> Result<Json<MetadataSettingsResponse>, ApiError> {
    let updated = update_metadata_settings(metadata_svc.settings_repo(), update)
        .await
        .map_err(|msg| err_json(StatusCode::BAD_REQUEST, "INVALID_SETTINGS", msg))?;
    Ok(Json(updated.to_response()))
}

/// `GET /api/v1/items/{id}/tmdb/search`
pub async fn search_item_tmdb(
    _admin: RequireAdmin,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Query(query): Query<TmdbSearchQuery>,
    Extension(metadata_svc): Extension<Arc<MetadataService>>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let item = load_item_checked(item_id, &unlocked, &media_repo, &lib_repo).await?;

    let tmdb_type = match query.media_type.as_deref() {
        Some(raw) if !raw.trim().is_empty() => {
            TmdbMediaType::parse_param(raw).ok_or_else(|| {
                err_json(
                    StatusCode::BAD_REQUEST,
                    "INVALID_MEDIA_TYPE",
                    "media_type must be 'movie' or 'tv'",
                )
            })?
        }
        _ => TmdbMediaType::from_media_type(item.item_type),
    };

    let q_str = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(item.title.as_str());

    let results = metadata_svc
        .search_tmdb(tmdb_type, q_str, query.year)
        .await
        .map_err(map_service_err)?;

    Ok(Json(serde_json::json!({ "results": results })))
}

/// `POST /api/v1/items/{id}/tmdb/apply`
#[allow(clippy::too_many_arguments)]
pub async fn apply_item_tmdb(
    RequireAdmin(auth_user): RequireAdmin,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(metadata_svc): Extension<Arc<MetadataService>>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(resolver): Extension<Arc<WidgetResolver>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(body): Json<TmdbApplyBody>,
) -> Result<Json<ItemDetailsPayload>, ApiError> {
    let item = load_item_checked(item_id, &unlocked, &media_repo, &lib_repo).await?;

    let tmdb_type = TmdbMediaType::parse_param(&body.media_type).ok_or_else(|| {
        err_json(
            StatusCode::BAD_REQUEST,
            "INVALID_MEDIA_TYPE",
            "media_type must be 'movie' or 'tv'",
        )
    })?;

    let download_artwork = body.download_artwork.unwrap_or(true);
    match metadata_svc
        .apply_tmdb_to_item(item_id, body.tmdb_id, tmdb_type, download_artwork)
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Item not found" })),
            ));
        }
        Err(err) => return Err(map_service_err(err)),
    }

    emit_library_updated(&event_bus, &media_repo, &item.library_id).await;

    let unlocked_ids = unlocked.to_vec();
    match resolver
        .resolve_item_details_with_unlocked(item_id, &auth_user.id, &unlocked_ids)
        .await
    {
        Ok(Some(payload)) => Ok(Json(payload)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Item not found" })),
        )),
        Err(err) => {
            tracing::error!("Failed to resolve item details after TMDb apply {item_id}: {err}");
            Err(err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RESOLVE_ERROR",
                "Failed to resolve item details",
            ))
        }
    }
}

/// `POST /api/v1/admin/libraries/{id}/tmdb/enrich`
pub async fn enrich_library_tmdb(
    _admin: RequireAdmin,
    unlocked: UnlockedLibraries,
    Path(library_id): Path<String>,
    Extension(metadata_svc): Extension<Arc<MetadataService>>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<Json<EnrichLibraryResult>, ApiError> {
    match lib_repo.get_by_id(&library_id).await {
        Ok(Some(lib)) => {
            if lib.is_private && !unlocked.is_unlocked(&library_id) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
                ));
            }
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Library not found" })),
            ));
        }
        Err(err) => {
            tracing::error!("Failed to load library {library_id}: {err}");
            return Err(err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "Failed to load library",
            ));
        }
    }

    let result = metadata_svc
        .enrich_library(&library_id)
        .await
        .map_err(map_service_err)?;

    if result.matched > 0 {
        emit_library_updated(&event_bus, &media_repo, &library_id).await;
    }

    Ok(Json(result))
}
