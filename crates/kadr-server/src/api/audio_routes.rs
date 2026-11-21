use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    Json,
};
use kadr_core::models::{AudioStreamInfo, MediaItem, TechnicalInfo};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;

fn synthesize_fallback_track(tech: &TechnicalInfo) -> Vec<AudioStreamInfo> {
    let Some(codec) = tech
        .audio_codec
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    else {
        return Vec::new();
    };
    vec![AudioStreamInfo {
        audio_index: 0,
        stream_index: 0,
        codec: codec.to_ascii_lowercase(),
        channels: tech.audio_channels,
        language: "und".to_string(),
        title: None,
        is_default: true,
        is_commentary: false,
    }]
}

struct InFlightCleanup<'a> {
    map: &'a Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    item_id: i64,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl Drop for InFlightCleanup<'_> {
    fn drop(&mut self) {
        if let Ok(mut map) = self.map.lock() {
            if let std::collections::hash_map::Entry::Occupied(entry) = map.entry(self.item_id) {
                if Arc::ptr_eq(entry.get(), &self.lock) && Arc::strong_count(&self.lock) <= 2 {
                    entry.remove();
                }
            }
        }
    }
}

/// Manages singleflight lazy `ffprobe` audio-stream discovery and SQLite persistence.
#[derive(Clone, Default)]
pub struct AudioProbeService {
    probed_items: Arc<Mutex<HashSet<i64>>>,
    in_flight: Arc<Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>>,
}

impl AudioProbeService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn in_flight_count(&self) -> usize {
        self.in_flight.lock().unwrap().len()
    }

    /// Returns persisted `audio_tracks` when present, or lazily probes supported on-disk files
    /// once via `ffprobe` (persisting discovered tracks), falling back to a synthesized Track 0.
    pub async fn list_or_probe_audio_tracks(
        &self,
        media_repo: &MediaItemRepository,
        item: &MediaItem,
    ) -> Vec<AudioStreamInfo> {
        if !item.technical.audio_tracks.is_empty() {
            return item.technical.audio_tracks.clone();
        }

        let Some(item_id) = item.id else {
            return synthesize_fallback_track(&item.technical);
        };

        if self.probed_items.lock().unwrap().contains(&item_id) {
            return synthesize_fallback_track(&item.technical);
        }

        if item.plugin_ref().is_some()
            || item.is_live()
            || !tokio::fs::try_exists(&item.file_path)
                .await
                .unwrap_or(false)
        {
            return synthesize_fallback_track(&item.technical);
        }

        let singleflight_lock = {
            let mut map = self.in_flight.lock().unwrap();
            map.entry(item_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };

        // Move `singleflight_lock` into `_cleanup` (no extra Arc clone) and declare `_cleanup`
        // before `_guard` so the mutex guard always drops before the map entry is cleaned up.
        let _cleanup = InFlightCleanup {
            map: &self.in_flight,
            item_id,
            lock: singleflight_lock,
        };
        let _guard = _cleanup.lock.lock().await;

        if let Ok(Some(fresh)) = media_repo.get_by_id(item_id).await {
            if !fresh.technical.audio_tracks.is_empty() {
                return fresh.technical.audio_tracks;
            }
        }

        if self.probed_items.lock().unwrap().contains(&item_id) {
            return synthesize_fallback_track(&item.technical);
        }

        match kadr_ingest::probe::probe_audio_streams(&item.file_path).await {
            Ok(tracks) => {
                if tracks.is_empty() {
                    self.probed_items.lock().unwrap().insert(item_id);
                } else {
                    if let Err(err) = media_repo.update_audio_tracks(item_id, &tracks).await {
                        tracing::warn!("Failed to persist audio tracks for item {item_id}: {err}");
                    }
                    return tracks;
                }
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to probe audio streams for item {item_id} ({}): {err}",
                    item.file_path.display()
                );
            }
        }

        synthesize_fallback_track(&item.technical)
    }
}

/// Handler for `GET /api/v1/items/{item_id}/audio-tracks`.
/// Lists audio tracks for a media item, enforcing private-library lock tokens and lazy probing.
pub async fn list_audio_tracks(
    _auth_user: AuthUser,
    unlocked: UnlockedLibraries,
    Path(item_id): Path<i64>,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(audio_probe): Extension<Arc<AudioProbeService>>,
) -> Result<Json<Vec<AudioStreamInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            ));
        }
        Err(err) => {
            tracing::error!("Database error fetching media item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
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
            tracing::error!("Database error fetching library for item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            ));
        }
    }

    let tracks = audio_probe
        .list_or_probe_audio_tracks(&media_repo, &item)
        .await;
    Ok(Json(tracks))
}
