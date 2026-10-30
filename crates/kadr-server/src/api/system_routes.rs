use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::auth::jwt::RequireAdmin;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsDirectoryEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsShortcut {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsBrowseResponse {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub directories: Vec<FsDirectoryEntry>,
    pub shortcuts: Vec<FsShortcut>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FsBrowseQuery {
    pub path: Option<String>,
}

/// Handler for `GET /api/v1/system/fs`.
///
/// Browses server filesystem directories for administrative library path selection.
/// Requires admin role.
pub async fn browse_filesystem(
    _admin: RequireAdmin,
    Query(params): Query<FsBrowseQuery>,
) -> Result<Json<FsBrowseResponse>, (StatusCode, Json<serde_json::Value>)> {
    let target_path = match params.path.as_deref() {
        Some(p) if !p.trim().is_empty() => p.trim(),
        _ => "/",
    };

    let canonical = match std::fs::canonicalize(target_path) {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, kind = ?e.kind(), path = %target_path, "Failed to resolve directory path");
            let detail = match e.kind() {
                std::io::ErrorKind::PermissionDenied => format!(
                    "Permission denied accessing '{}' ({}). Ensure the 'kadr' service user has read/execute permissions on this directory (and check ProtectHome in kadr.service if browsing /home).",
                    target_path, e
                ),
                std::io::ErrorKind::NotFound => format!(
                    "Path not found: '{}' ({})",
                    target_path, e
                ),
                _ => format!(
                    "Path not found or invalid '{}': {}",
                    target_path, e
                ),
            };
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": detail,
                    "path": target_path,
                    "error_kind": format!("{:?}", e.kind()),
                })),
            ));
        }
    };

    if !canonical.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Path is not a directory: '{}'", canonical.display()),
                "path": canonical.to_string_lossy(),
            })),
        ));
    }

    let current_path = canonical.to_string_lossy().into_owned();
    let parent_path = if current_path == "/" {
        None
    } else {
        canonical.parent().map(|p| p.to_string_lossy().into_owned())
    };

    let mut directories = Vec::new();
    match tokio::fs::read_dir(&canonical).await {
        Ok(mut reader) => {
            while let Ok(Some(entry)) = reader.next_entry().await {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                if file_name.starts_with('.') {
                    continue;
                }
                let is_dir = match entry.file_type().await {
                    Ok(ft) => {
                        if ft.is_dir() {
                            true
                        } else if ft.is_symlink() {
                            entry.path().is_dir()
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                };
                if is_dir {
                    directories.push(FsDirectoryEntry {
                        name: file_name,
                        path: entry.path().to_string_lossy().into_owned(),
                    });
                }
            }
        }
        Err(e) => {
            error!(error = %e, kind = ?e.kind(), path = %current_path, "Failed to read directory");
            let detail = match e.kind() {
                std::io::ErrorKind::PermissionDenied => format!(
                    "Permission denied reading directory '{}' ({}). Ensure the 'kadr' service user has read/execute permissions (and check ProtectHome in kadr.service if browsing /home).",
                    current_path, e
                ),
                _ => format!("Failed to read directory '{}': {}", current_path, e),
            };
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": detail,
                    "path": current_path,
                    "error_kind": format!("{:?}", e.kind()),
                })),
            ));
        }
    }

    directories.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });

    let shortcut_candidates = [
        ("/", "Root (/)"),
        ("/media", "Media"),
        ("/mnt", "Mounts"),
        ("/home", "Home"),
        ("/var", "Var"),
    ];

    let mut shortcuts = Vec::new();
    for (sc_path, sc_name) in shortcut_candidates {
        if std::path::Path::new(sc_path).is_dir() {
            shortcuts.push(FsShortcut {
                name: sc_name.to_string(),
                path: sc_path.to_string(),
            });
        }
    }

    Ok(Json(FsBrowseResponse {
        current_path,
        parent_path,
        directories,
        shortcuts,
    }))
}
