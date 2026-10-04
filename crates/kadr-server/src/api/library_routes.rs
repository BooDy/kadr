use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::Json;
use kadr_core::events::SystemEvent;
use kadr_core::models::{Library, MediaType};
use kadr_ingest::watcher::{scan_directory_recursive, IngestMessage, IngestPipeline};
use kadr_storage::repos::LibraryRepository;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::auth::jwt::RequireAdmin;
use crate::events::EventBus;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub path: PathBuf,
    pub media_type: MediaType,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResultResponse {
    pub library_id: String,
    pub files_scanned: usize,
    pub queued: bool,
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

    let id = uuid::Uuid::new_v4().to_string();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let library = Library {
        id,
        name: payload.name.trim().to_string(),
        path: payload.path,
        media_type: payload.media_type,
        created_at: now,
        ..Default::default()
    };

    if let Err(e) = lib_repo.insert(&library).await {
        error!(error = %e, "Failed to create library in database");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    event_bus.publish(SystemEvent::LibraryUpdated {
        library_id: library.id.clone(),
        item_count: 0,
        timestamp: now,
    });

    // If path exists on disk, trigger background scanning
    if library.path.exists() {
        let lib_clone = library.clone();
        let pipe_clone = pipeline.clone();
        let tx_clone = ingest_tx.clone();
        tokio::spawn(async move {
            let files = scan_directory_recursive(&lib_clone.path);
            info!(
                library = %lib_clone.name,
                count = files.len(),
                "Scanning newly registered library files"
            );
            for file in files {
                if let Ok(Some((item, subs))) = pipe_clone.process_file(&lib_clone, &file).await {
                    let _ = tx_clone.send(IngestMessage::Upsert(item, subs)).await;
                }
            }
        });
    }

    Ok((StatusCode::CREATED, Json(library)))
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

    let files = scan_directory_recursive(&library.path);
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
