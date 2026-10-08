use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{Extension, Path, Query};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use kadr_core::ast::{CardViewModel, QueryMacro, ScreenId, WidgetNode};
use kadr_core::events::SystemEvent;
use kadr_core::models::{Library, MediaItem, MediaType};
use kadr_ingest::watcher::{scan_directory_recursive, IngestMessage, IngestPipeline};
use kadr_storage::error::StorageError;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;
use tracing::{error, info};

use crate::api::auth_routes::ClientIp;
use crate::api::unlock_token::{UnlockTokenService, UnlockedLibraries};
use crate::auth::jwt::{AuthUser, RequireAdmin};
use crate::auth::pin::{hash_pin, validate_pin, verify_pin};
use crate::auth::rate_limiter::{RateLimitStatus, RateLimiter};
use crate::config::AppConfig;
use crate::events::EventBus;
use crate::layout::{default_library_layout, LayoutRegistry};
use crate::resolver::to_card_view_model;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreadcrumbItem {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderImageEntry {
    pub name: String,
    pub path: String,
    pub url: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryFolderResponse {
    pub library_id: String,
    pub library_name: String,
    pub current_path: String,
    pub parent_path: Option<String>,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub directories: Vec<FolderEntry>,
    pub items: Vec<CardViewModel>,
    #[serde(default)]
    pub images: Vec<FolderImageEntry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BrowseFolderQuery {
    pub path: Option<String>,
}

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
pub struct UpdateLibraryRequest {
    pub name: String,
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
    pub unlock_token: String,
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
                if let Err(e) = tx_clone.send(IngestMessage::Upsert(item, subs)).await {
                    error!(error = %e, "Failed to send IngestMessage to worker channel");
                }
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
                        token: token.clone(),
                        unlock_token: token,
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

/// Handler for `PATCH /api/v1/libraries/{id}`.
///
/// Updates the library name, synchronizes associated screen layout title,
/// cleans up old screen overrides, and broadcasts a LibraryUpdated event. Requires admin role.
pub async fn update_library(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(layout_registry): Extension<Arc<LayoutRegistry>>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
    Json(payload): Json<UpdateLibraryRequest>,
) -> Result<Json<Library>, StatusCode> {
    let trimmed_name = payload.name.trim();
    if trimmed_name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 1. Fetch library before update so we know its original name
    let old_lib = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to get library before update");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let old_name = old_lib.name;

    // 2. Update library name in storage repository
    let updated = match lib_repo.update_name(&id, trimmed_name).await {
        Ok(lib) => lib,
        Err(StorageError::NotFound(_)) => return Err(StatusCode::NOT_FOUND),
        Err(StorageError::InvalidInput(_)) => return Err(StatusCode::BAD_REQUEST),
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to update library name");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // 3. Synchronize associated screen layout in registry and on disk
    let screens_dir = config.read().await.server.data_dir.join("screens");
    let canonical_id: ScreenId = id.parse().unwrap();

    // Look for an existing screen matching this library either by ID or old name
    let existing_screen_opt = layout_registry.find_screen_for_library(&id, &old_name);

    if let Some(mut screen) = existing_screen_opt {
        let old_screen_id = screen.id.clone();
        match old_screen_id {
            ScreenId::Home => {}
            ScreenId::Movies | ScreenId::Shows => {
                // Built-in screens remain with their built-in ID, but title and widget titles update
                screen.title = trimmed_name.to_string();
                for widget in &mut screen.widgets {
                    if let WidgetNode::Grid { title, .. } = widget {
                        if title.starts_with("All ") || title == &format!("All {old_name}") {
                            *title = format!("All {trimmed_name}");
                        }
                    }
                }
                let _ = layout_registry.save_screen(screen, &screens_dir);
            }
            ScreenId::Custom(_) => {
                // If the old screen had an ID different from the canonical library ID (e.g. named after old_name),
                // remove it from layout registry and delete its file from screens_dir
                if old_screen_id != canonical_id {
                    let _ = layout_registry.delete_custom_screen(&old_screen_id, &screens_dir);
                }

                // Also delete any {old_name}.json or {old_name}.toml if it exists on disk
                let old_name_file = screens_dir.join(format!("{old_name}.json"));
                if old_name_file.exists() {
                    let _ = std::fs::remove_file(&old_name_file);
                }
                let old_name_toml = screens_dir.join(format!("{old_name}.toml"));
                if old_name_toml.exists() {
                    let _ = std::fs::remove_file(&old_name_toml);
                }
                let old_name_id: ScreenId = old_name.parse().unwrap();
                if old_name_id != canonical_id && old_name_id != old_screen_id {
                    let _ = layout_registry.delete_custom_screen(&old_name_id, &screens_dir);
                }

                screen.id = canonical_id;
                screen.title = trimmed_name.to_string();
                for widget in &mut screen.widgets {
                    if let WidgetNode::Grid { title, binding, .. } = widget {
                        if title.starts_with("All ") || title == &format!("All {old_name}") {
                            *title = format!("All {trimmed_name}");
                        }
                        if let QueryMacro::LibraryItems { library_id } = &mut binding.macro_type {
                            *library_id = id.clone();
                        }
                    }
                }
                let _ = layout_registry.save_screen(screen, &screens_dir);
            }
        }
    } else {
        // No screen was found matching the library; clean up any old_name files and create default layout
        let old_name_file = screens_dir.join(format!("{old_name}.json"));
        if old_name_file.exists() {
            let _ = std::fs::remove_file(&old_name_file);
        }
        let old_name_toml = screens_dir.join(format!("{old_name}.toml"));
        if old_name_toml.exists() {
            let _ = std::fs::remove_file(&old_name_toml);
        }
        let old_name_id: ScreenId = old_name.parse().unwrap();
        if old_name_id != canonical_id {
            let _ = layout_registry.delete_custom_screen(&old_name_id, &screens_dir);
        }

        let default_layout = default_library_layout(&id, trimmed_name);
        let _ = layout_registry.save_screen(default_layout, &screens_dir);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: id,
        item_count: 0,
        timestamp: now,
    });

    Ok(Json(updated))
}

/// Handler for `DELETE /api/v1/libraries/{id}`.
///
/// Removes a library registration, cleans up associated custom screen layouts,
/// and broadcasts a LibraryUpdated event. Requires admin role.
pub async fn delete_library(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(event_bus): Extension<Arc<EventBus>>,
    Extension(layout_registry): Extension<Arc<LayoutRegistry>>,
    Extension(config): Extension<Arc<tokio::sync::RwLock<AppConfig>>>,
) -> Result<StatusCode, StatusCode> {
    let old_lib = lib_repo.get_by_id(&id).await.unwrap_or(None);
    match lib_repo.delete(&id).await {
        Ok(true) => {
            let screens_dir = config.read().await.server.data_dir.join("screens");
            let canonical_id: ScreenId = id.parse().unwrap();
            let _ = layout_registry.delete_custom_screen(&canonical_id, &screens_dir);
            if let Some(lib) = old_lib {
                let name_id: ScreenId = lib.name.parse().unwrap();
                if name_id != canonical_id {
                    let _ = layout_registry.delete_custom_screen(&name_id, &screens_dir);
                }
                let name_file = screens_dir.join(format!("{}.json", lib.name));
                if name_file.exists() {
                    let _ = std::fs::remove_file(&name_file);
                }
                let name_toml = screens_dir.join(format!("{}.toml", lib.name));
                if name_toml.exists() {
                    let _ = std::fs::remove_file(&name_toml);
                }
            }

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
                if let Err(e) = tx_clone.send(IngestMessage::Upsert(item, subs)).await {
                    error!(error = %e, "Failed to send IngestMessage to worker channel");
                }
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
                if let Err(e) = tx_clone.send(IngestMessage::Upsert(item, subs)).await {
                    error!(error = %e, "Failed to send IngestMessage to worker channel");
                }
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

fn is_video_file(path: &std::path::Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mkv" | "mp4" | "webm" | "avi" | "mov" | "m4v"
    )
}

fn is_image_file(path: &std::path::Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "bmp"
    )
}

/// Handler for `GET /api/v1/libraries/{id}/folders`.
///
/// Returns sandboxed directory contents, subdirectories with item counts,
/// and media items enriched with database metadata and playback progress.
pub async fn browse_library_folders(
    auth_user: Option<AuthUser>,
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Query(query): Query<BrowseFolderQuery>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(playback_repo): Extension<PlaybackRepository>,
) -> Result<Json<LibraryFolderResponse>, (StatusCode, Json<serde_json::Value>)> {
    let library = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Library not found" })),
            ));
        }
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to load library");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            ));
        }
    };

    if library.is_private && !unlocked.is_unlocked(&library.id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
        ));
    }

    let raw_roots = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    let mut canonical_roots = Vec::new();
    for root in &raw_roots {
        if let Ok(canon) = std::fs::canonicalize(root) {
            canonical_roots.push(canon);
        }
    }
    canonical_roots.sort();
    canonical_roots.dedup();

    if canonical_roots.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Library path not found on disk" })),
        ));
    }

    let requested_path = query.path.unwrap_or_default();
    let trimmed = requested_path.trim();

    if trimmed.contains('\0') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid path containing null bytes" })),
        ));
    }

    if trimmed.contains("..") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path traversal not allowed" })),
        ));
    }

    let rel_path_check = std::path::Path::new(trimmed);
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || rel_path_check.is_absolute()
        || rel_path_check.has_root()
        || trimmed.contains(':')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Absolute path not allowed" })),
        ));
    }

    for comp in rel_path_check.components() {
        match comp {
            std::path::Component::ParentDir => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Path traversal not allowed" })),
                ));
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Absolute path not allowed" })),
                ));
            }
            _ => {}
        }
    }

    let clean_path = trimmed.trim_matches(|c| c == '/' || c == '\\').to_string();

    let mut matched_target_dirs: Vec<PathBuf> = Vec::new();
    if clean_path.is_empty() {
        matched_target_dirs.extend(canonical_roots.iter().cloned());
    } else {
        let mut any_candidate_exists = false;
        for root in &canonical_roots {
            let candidate = root.join(&clean_path);
            if candidate.exists() {
                any_candidate_exists = true;
                let canon = match std::fs::canonicalize(&candidate) {
                    Ok(c) => c,
                    Err(e) => {
                        error!(error = %e, path = ?candidate, "Failed to canonicalize path");
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({ "error": "Invalid path" })),
                        ));
                    }
                };
                if !canon.starts_with(root) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "Path outside library root" })),
                    ));
                }
                if !canon.is_dir() {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "Path is not a directory" })),
                    ));
                }
                matched_target_dirs.push(canon);
            }
        }

        if !any_candidate_exists || matched_target_dirs.is_empty() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Path not found" })),
            ));
        }
    }

    let current_path = clean_path.clone();

    let parent_path = if clean_path.is_empty() {
        None
    } else if let Some((parent, _)) = clean_path.rsplit_once('/') {
        Some(parent.to_string())
    } else {
        Some(String::new())
    };

    let breadcrumbs = if clean_path.is_empty() {
        Vec::new()
    } else {
        let mut crumbs = Vec::new();
        let mut accum = String::new();
        for seg in clean_path.split('/') {
            if seg.is_empty() {
                continue;
            }
            if accum.is_empty() {
                accum.push_str(seg);
            } else {
                accum.push('/');
                accum.push_str(seg);
            }
            crumbs.push(BreadcrumbItem {
                name: seg.to_string(),
                path: accum.clone(),
            });
        }
        crumbs
    };

    let mut dir_map: std::collections::BTreeMap<String, FolderEntry> =
        std::collections::BTreeMap::new();
    let mut video_paths = Vec::new();
    let mut discovered_images: Vec<(String, PathBuf)> = Vec::new();

    for target_dir in matched_target_dirs {
        let mut reader = match tokio::fs::read_dir(&target_dir).await {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, dir = ?target_dir, "Failed to read directory");
                continue;
            }
        };

        while let Ok(Some(entry)) = reader.next_entry().await {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.') {
                continue;
            }

            let entry_path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            let is_dir = if file_type.is_dir() {
                true
            } else if file_type.is_symlink() {
                entry_path.is_dir()
            } else {
                false
            };

            if is_dir {
                let mut sub_count = 0;
                if let Ok(mut sub_reader) = tokio::fs::read_dir(&entry_path).await {
                    while let Ok(Some(sub_entry)) = sub_reader.next_entry().await {
                        let sub_name = sub_entry.file_name().to_string_lossy().into_owned();
                        if !sub_name.starts_with('.') {
                            sub_count += 1;
                        }
                    }
                }

                let rel_dir_path = if current_path.is_empty() {
                    file_name.clone()
                } else {
                    format!("{}/{}", current_path, file_name)
                };

                dir_map
                    .entry(file_name.clone())
                    .and_modify(|e| e.item_count += sub_count)
                    .or_insert(FolderEntry {
                        name: file_name,
                        path: rel_dir_path,
                        item_count: sub_count,
                    });
            } else if is_video_file(&entry_path) {
                video_paths.push(entry_path);
            } else if is_image_file(&entry_path) {
                discovered_images.push((file_name, entry_path));
            }
        }
    }

    video_paths.sort();
    video_paths.dedup();

    let mut lookup_paths: Vec<&std::path::Path> = Vec::new();
    for p in &video_paths {
        lookup_paths.push(p.as_path());
    }
    let canon_paths: Vec<PathBuf> = video_paths
        .iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect();
    for p in &canon_paths {
        lookup_paths.push(p.as_path());
    }

    let media_map = match media_repo.find_by_paths(&lookup_paths).await {
        Ok(m) => m,
        Err(e) => {
            error!(error = %e, "Failed to query media items for folder browsing");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            ));
        }
    };

    let mut media_items = Vec::new();
    let mut item_ids = Vec::new();

    for video_path in &video_paths {
        let item_opt = media_map.get(video_path).or_else(|| {
            std::fs::canonicalize(video_path)
                .ok()
                .and_then(|c| media_map.get(&c))
        });

        if let Some(item) = item_opt {
            if let Some(id) = item.id {
                item_ids.push(id);
            }
            media_items.push((video_path.clone(), Some(item.clone())));
        } else {
            media_items.push((video_path.clone(), None));
        }
    }

    item_ids.sort_unstable();
    item_ids.dedup();

    let playback_map = if let Some(ref user) = auth_user {
        if !item_ids.is_empty() {
            playback_repo
                .get_states_for_items(&user.id, &item_ids)
                .await
                .unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        }
    } else {
        std::collections::HashMap::new()
    };

    let mut items = Vec::new();
    for (video_path, item_opt) in media_items {
        if let Some(item) = item_opt {
            let playback = item.id.and_then(|id| playback_map.get(&id));
            items.push(to_card_view_model(&item, playback));
        } else {
            let file_name = video_path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Unknown".to_string());
            let stem = video_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| file_name.clone());
            let synthetic = MediaItem {
                id: None,
                library_id: library.id.clone(),
                item_type: library.media_type,
                title: stem,
                original_title: None,
                release_year: None,
                added_at: 0,
                file_path: video_path,
                file_name: file_name.clone(),
                file_size: 0,
                technical: Default::default(),
                metadata: Default::default(),
            };
            let rel_path = if current_path.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", current_path, file_name)
            };
            let mut card = to_card_view_model(&synthetic, None);
            card.poster_url = Some(format!(
                "/api/v1/libraries/{}/thumbnail?path={}",
                library.id,
                urlencoding::encode(&rel_path)
            ));
            items.push(card);
        }
    }

    let mut image_entries = Vec::new();
    for (img_name, img_path) in discovered_images {
        let rel_path = if current_path.is_empty() {
            img_name.clone()
        } else {
            format!("{}/{}", current_path, img_name)
        };
        let size_bytes = tokio::fs::metadata(&img_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let url = format!(
            "/api/v1/libraries/{}/image?path={}",
            library.id,
            urlencoding::encode(&rel_path)
        );
        image_entries.push(FolderImageEntry {
            name: img_name,
            path: rel_path,
            url,
            size_bytes,
        });
    }
    image_entries.sort_by_key(|a| a.name.to_lowercase());
    image_entries.dedup_by(|a, b| a.name.to_lowercase() == b.name.to_lowercase());

    let mut directories: Vec<FolderEntry> = dir_map.into_values().collect();
    directories.sort_by_key(|a| a.name.to_lowercase());
    items.sort_by_key(|a| a.title.to_lowercase());

    Ok(Json(LibraryFolderResponse {
        library_id: library.id,
        library_name: library.name,
        current_path,
        parent_path,
        breadcrumbs,
        directories,
        items,
        images: image_entries,
    }))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThumbnailQuery {
    pub path: String,
}

/// Handler for `GET /api/v1/libraries/{id}/thumbnail?path=...`.
///
/// On-demand thumbnail extraction route for unindexed video files in library folders.
pub async fn get_library_thumbnail(
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Query(query): Query<ThumbnailQuery>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(pipeline): Extension<Arc<IngestPipeline>>,
) -> Response {
    let library = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Library not found" })),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to load library");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    };

    if library.is_private && !unlocked.is_unlocked(&library.id) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
        )
            .into_response();
    }

    let trimmed = query.path.trim();
    if trimmed.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path cannot be empty" })),
        )
            .into_response();
    }

    if trimmed.contains('\0') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid path containing null bytes" })),
        )
            .into_response();
    }

    if trimmed.contains("..") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path traversal not allowed" })),
        )
            .into_response();
    }

    let rel_path_check = std::path::Path::new(trimmed);
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || rel_path_check.is_absolute()
        || rel_path_check.has_root()
        || trimmed.contains(':')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Absolute path not allowed" })),
        )
            .into_response();
    }

    for comp in rel_path_check.components() {
        match comp {
            std::path::Component::ParentDir => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Path traversal not allowed" })),
                )
                    .into_response();
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Absolute path not allowed" })),
                )
                    .into_response();
            }
            _ => {}
        }
    }

    let clean_path = trimmed.trim_matches(|c| c == '/' || c == '\\');
    if clean_path.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid path" })),
        )
            .into_response();
    }

    let raw_roots = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    let mut canonical_roots = Vec::new();
    for root in &raw_roots {
        if let Ok(canon) = std::fs::canonicalize(root) {
            canonical_roots.push(canon);
        }
    }
    canonical_roots.sort();
    canonical_roots.dedup();

    if canonical_roots.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Library path not found on disk" })),
        )
            .into_response();
    }

    let mut matched_target_file: Option<PathBuf> = None;
    for root in &canonical_roots {
        let candidate = root.join(clean_path);
        if candidate.exists() {
            if let Ok(canon) = std::fs::canonicalize(&candidate) {
                if !canon.starts_with(root) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "Path outside library root" })),
                    )
                        .into_response();
                }
                if canon.is_file() {
                    matched_target_file = Some(canon);
                    break;
                }
            }
        }
    }

    let Some(canonical_file_path) = matched_target_file else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "File not found" })),
        )
            .into_response();
    };

    let Some(extractor) = pipeline.thumbnail_extractor() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Thumbnail extractor not available" })),
        )
            .into_response();
    };

    let thumb_path = match extractor.extract_thumbnail(&canonical_file_path, 0).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Thumbnail not found" })),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, path = ?canonical_file_path, "Failed to extract thumbnail");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to extract thumbnail" })),
            )
                .into_response();
        }
    };

    let file = match tokio::fs::File::open(&thumb_path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Thumbnail not found" })),
            )
                .into_response();
        }
    };

    let total_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(err) => {
            error!(
                "Failed to read metadata for thumbnail {}: {err}",
                thumb_path.display()
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read thumbnail file metadata" })),
            )
                .into_response();
        }
    };

    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "image/jpeg")
        .header(axum::http::header::CACHE_CONTROL, "public, max-age=86400")
        .header(axum::http::header::ACCEPT_RANGES, "bytes")
        .header(axum::http::header::CONTENT_LENGTH, total_size.to_string())
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn resolve_image_mime(path: &std::path::Path) -> &'static str {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return "application/octet-stream";
    };
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageQuery {
    pub path: String,
}

/// Handler for `GET /api/v1/libraries/{id}/image?path=...`.
///
/// Streams raw image files from within library folders with security sandboxing and caching.
pub async fn get_library_image(
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Query(query): Query<ImageQuery>,
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Response {
    let library = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Library not found" })),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to load library");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    };

    if library.is_private && !unlocked.is_unlocked(&library.id) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
        )
            .into_response();
    }

    let trimmed = query.path.trim();
    if trimmed.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path cannot be empty" })),
        )
            .into_response();
    }

    if trimmed.contains('\0') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid path containing null bytes" })),
        )
            .into_response();
    }

    if trimmed.contains("..") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path traversal not allowed" })),
        )
            .into_response();
    }

    let rel_check = std::path::Path::new(trimmed);
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || rel_check.is_absolute()
        || rel_check.has_root()
        || trimmed.contains(':')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Absolute path not allowed" })),
        )
            .into_response();
    }

    for comp in rel_check.components() {
        match comp {
            std::path::Component::ParentDir => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Path traversal not allowed" })),
                )
                    .into_response();
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Absolute path not allowed" })),
                )
                    .into_response();
            }
            _ => {}
        }
    }

    let clean_path = trimmed.trim_matches(|c| c == '/' || c == '\\');
    let raw_roots = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    let mut canonical_roots = Vec::new();
    for root in &raw_roots {
        if let Ok(canon) = std::fs::canonicalize(root) {
            canonical_roots.push(canon);
        }
    }
    canonical_roots.sort();
    canonical_roots.dedup();

    if canonical_roots.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Library path not found on disk" })),
        )
            .into_response();
    }

    let mut found_path: Option<PathBuf> = None;
    for root in &canonical_roots {
        let candidate = root.join(clean_path);
        if candidate.exists() {
            if let Ok(canon_file) = std::fs::canonicalize(&candidate) {
                if !canon_file.starts_with(root) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": "Path outside library root" })),
                    )
                        .into_response();
                }
                if canon_file.is_file() {
                    found_path = Some(canon_file);
                    break;
                }
            }
        }
    }

    let Some(target_file) = found_path else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Image file not found" })),
        )
            .into_response();
    };

    if !is_image_file(&target_file) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Requested file is not an image" })),
        )
            .into_response();
    }

    let meta = match tokio::fs::metadata(&target_file).await {
        Ok(m) => m,
        Err(e) => {
            error!(error = %e, path = ?target_file, "Failed to read image metadata");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read image file" })),
            )
                .into_response();
        }
    };

    let file = match tokio::fs::File::open(&target_file).await {
        Ok(f) => f,
        Err(e) => {
            error!(error = %e, path = ?target_file, "Failed to open image file");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to open image file" })),
            )
                .into_response();
        }
    };

    let mime = resolve_image_mime(&target_file);
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, meta.len().to_string())
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(body)
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR).into_response())
}
