use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Extension, Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use kadr_core::events::SystemEvent;
use kadr_core::models::{Library, MediaType};
use kadr_ingest::watcher::{scan_directory_recursive, IngestMessage, IngestPipeline};
use kadr_storage::error::StorageError;
use kadr_storage::repos::LibraryRepository;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::api::auth_routes::ClientIp;
use crate::api::unlock_token::UnlockTokenService;
use crate::auth::jwt::RequireAdmin;
use crate::auth::pin::{hash_pin, validate_pin, verify_pin};
use crate::auth::rate_limiter::{RateLimitStatus, RateLimiter};
use crate::events::EventBus;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub path: Option<PathBuf>,
    pub paths: Option<Vec<PathBuf>>,
    pub media_type: MediaType,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub pin: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddPathRequest {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RemovePathQuery {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemovePathPayload {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResultResponse {
    pub library_id: String,
    pub files_scanned: usize,
    pub queued: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnlockLibraryRequest {
    pub pin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnlockLibraryResponse {
    pub library_id: String,
    pub token: String,
    pub expires_at: i64,
}

/// Handler for `GET /api/v1/libraries`.
///
/// Returns all registered media libraries.
pub async fn list_libraries(
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Result<Json<Vec<Library>>, StatusCode> {
    match lib_repo.get_all().await {
        Ok(libs) => Ok(Json(libs)),
        Err(e) => {
            error!(error = %e, "Failed to list media libraries");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Handler for `POST /api/v1/libraries`.
///
/// Creates a new library in storage and triggers an initial scan if the path exists.
/// Requires admin role.
pub async fn create_library(
    _admin: RequireAdmin,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(pipeline): Extension<Arc<IngestPipeline>>,
    Extension(ingest_tx): Extension<mpsc::Sender<IngestMessage>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, Json<Library>), StatusCode> {
    if payload.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (raw_paths, validate_existence) = match payload.paths {
        Some(paths) if !paths.is_empty() => (paths, true),
        _ => (payload.path.into_iter().collect(), false),
    };

    if raw_paths.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let resolved_paths: Vec<PathBuf> = raw_paths
        .into_iter()
        .map(|p| match std::fs::canonicalize(&p) {
            Ok(canon) => canon,
            Err(_) => p,
        })
        .collect();

    for p in &resolved_paths {
        if !p.is_absolute() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if validate_existence {
        for p in &resolved_paths {
            if !p.is_dir() {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        for p in &resolved_paths {
            if p.exists() && !p.is_dir() {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    let pin_hash = if payload.is_private {
        match payload.pin.as_deref() {
            Some(pin) if validate_pin(pin).is_ok() => match hash_pin(pin) {
                Ok(h) => Some(h),
                Err(e) => {
                    error!(error = %e, "Failed to hash library PIN");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            },
            _ => return Err(StatusCode::BAD_REQUEST),
        }
    } else {
        None
    };

    let id = uuid::Uuid::new_v4().to_string();
    let library = match lib_repo
        .create_with_paths(
            &id,
            payload.name.trim(),
            &resolved_paths,
            payload.media_type,
            payload.is_private,
            pin_hash.as_deref(),
        )
        .await
    {
        Ok(lib) => lib,
        Err(e) => {
            error!(error = %e, "Failed to create library in database");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: library.id.clone(),
        item_count: 0,
        timestamp: library.created_at,
    });

    // If paths exist on disk, trigger background scanning
    let lib_clone = library.clone();
    let pipe_clone = pipeline.clone();
    let tx_clone = ingest_tx.clone();
    tokio::spawn(async move {
        let mut all_files = Vec::new();
        for p in &lib_clone.paths {
            if p.exists() {
                all_files.extend(scan_directory_recursive(p));
            }
        }
        info!(
            library = %lib_clone.name,
            count = all_files.len(),
            "Scanning newly registered library files"
        );
        for file in all_files {
            if let Ok(Some((item, subs))) = pipe_clone.process_file(&lib_clone, &file).await {
                let _ = tx_clone.send(IngestMessage::Upsert(item, subs)).await;
            }
        }
    });

    Ok((StatusCode::CREATED, Json(library)))
}

/// Handler for `POST /api/v1/libraries/{id}/unlock`.
///
/// Validates the 4-digit PIN against the library's pin_hash with rate limiting,
/// and on success returns a signed HMAC unlock token.
pub async fn unlock_library(
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(limiter): Extension<RateLimiter>,
    Extension(token_svc): Extension<UnlockTokenService>,
    ClientIp(client_ip): ClientIp,
    Json(payload): Json<UnlockLibraryRequest>,
) -> Response {
    if let RateLimitStatus::LockedOut { retry_after_secs } = limiter.check_attempt(&client_ip).await
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", retry_after_secs.to_string())],
            Json(serde_json::json!({
                "error": "Too many failed attempts. Try again later.",
                "retry_after_seconds": retry_after_secs
            })),
        )
            .into_response();
    }

    let library = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => {
            limiter.record_failure(&client_ip).await;
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Library not found" })),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, library_id = %id, "Database error retrieving library during unlock");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
                .into_response();
        }
    };

    let pin_hash = match library.pin_hash.as_deref() {
        Some(h) if library.is_private => h,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Library is not private or has no PIN configured"
                })),
            )
                .into_response();
        }
    };

    match verify_pin(&payload.pin, pin_hash) {
        Ok(true) => {
            limiter.record_success(&client_ip).await;
            match token_svc.generate_token(&library.id) {
                Ok((token, expires_at)) => (
                    StatusCode::OK,
                    Json(UnlockLibraryResponse {
                        library_id: library.id,
                        token,
                        expires_at,
                    }),
                )
                    .into_response(),
                Err(e) => {
                    error!(error = %e, library_id = %library.id, "Failed to generate unlock token");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": "Internal server error" })),
                    )
                        .into_response()
                }
            }
        }
        _ => {
            limiter.record_failure(&client_ip).await;
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid PIN" })),
            )
                .into_response()
        }
    }
}

/// Handler for `DELETE /api/v1/libraries/{id}`.
///
/// Removes a library registration. Requires admin role.
pub async fn delete_library(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<StatusCode, StatusCode> {
    match lib_repo.delete(&id).await {
        Ok(true) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            event_bus.publish(SystemEvent::LibraryUpdated {
                library_id: id,
                item_count: 0,
                timestamp: now,
            });
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to delete library");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Handler for `POST /api/v1/libraries/{id}/scan`.
///
/// Triggers an on-demand directory scan for the specified library.
/// Requires admin role.
pub async fn scan_library(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(pipeline): Extension<Arc<IngestPipeline>>,
    Extension(ingest_tx): Extension<mpsc::Sender<IngestMessage>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
) -> Result<(StatusCode, Json<ScanResultResponse>), StatusCode> {
    let library = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to lookup library");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let paths = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    let mut files = Vec::new();
    for p in &paths {
        if p.exists() {
            files.extend(scan_directory_recursive(p));
        }
    }
    let files_scanned = files.len();

    let lib_clone = library.clone();
    let pipe_clone = pipeline.clone();
    let tx_clone = ingest_tx.clone();
    tokio::spawn(async move {
        info!(
            library = %lib_clone.name,
            count = files.len(),
            "Starting manual re-scan of library"
        );
        for file in files {
            if let Ok(Some((item, subs))) = pipe_clone.process_file(&lib_clone, &file).await {
                let _ = tx_clone.send(IngestMessage::Upsert(item, subs)).await;
            }
        }
    });

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: id.clone(),
        item_count: files_scanned,
        timestamp: now,
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ScanResultResponse {
            library_id: id,
            files_scanned,
            queued: true,
        }),
    ))
}

/// Handler for `POST /api/v1/libraries/{id}/paths`.
///
/// Adds an additional storage path to a library and initiates scanning.
/// Requires admin role.
pub async fn add_library_path(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(pipeline): Extension<Arc<IngestPipeline>>,
    Extension(ingest_tx): Extension<mpsc::Sender<IngestMessage>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Json(payload): Json<AddPathRequest>,
) -> Result<(StatusCode, Json<Library>), StatusCode> {
    let path = match std::fs::canonicalize(&payload.path) {
        Ok(canon) => canon,
        Err(_) => payload.path,
    };

    if !path.is_absolute() || !path.is_dir() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let existing_lib = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to get library");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Err(e) = lib_repo.add_path(&id, &path).await {
        error!(error = %e, library_id = %id, "Failed to add path to library");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let updated_lib = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        _ => existing_lib,
    };

    let lib_clone = updated_lib.clone();
    let added_path = path;
    let pipe_clone = pipeline.clone();
    let tx_clone = ingest_tx.clone();
    tokio::spawn(async move {
        let files = scan_directory_recursive(&added_path);
        info!(
            library = %lib_clone.name,
            path = ?added_path,
            count = files.len(),
            "Scanning newly added library path"
        );
        for file in files {
            if let Ok(Some((item, subs))) = pipe_clone.process_file(&lib_clone, &file).await {
                let _ = tx_clone.send(IngestMessage::Upsert(item, subs)).await;
            }
        }
    });

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: id,
        item_count: 0,
        timestamp: now,
    });

    Ok((StatusCode::OK, Json(updated_lib)))
}

/// Handler for `DELETE /api/v1/libraries/{id}/paths`.
///
/// Removes a storage path from a library. Returns 400 Bad Request if trying to remove the last path.
/// Accepts `?path=...` query param or JSON `{ "path": "..." }` body.
/// Requires admin role.
pub async fn remove_library_path(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Query(query): Query<RemovePathQuery>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    body: axum::body::Bytes,
) -> Result<StatusCode, StatusCode> {
    let raw_path = if let Some(p) = query.path {
        p
    } else if let Ok(payload) = serde_json::from_slice::<RemovePathPayload>(&body) {
        payload.path
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let path = match std::fs::canonicalize(&raw_path) {
        Ok(canon) => canon,
        Err(_) => raw_path,
    };

    match lib_repo.remove_path(&id, &path).await {
        Ok(()) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            event_bus.publish(SystemEvent::LibraryUpdated {
                library_id: id,
                item_count: 0,
                timestamp: now,
            });
            Ok(StatusCode::OK)
        }
        Err(StorageError::NotFound(_)) => Err(StatusCode::NOT_FOUND),
        Err(StorageError::InvalidInput(_)) => Err(StatusCode::BAD_REQUEST),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to remove path from library");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
