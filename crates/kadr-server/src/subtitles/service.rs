use std::path::{Path, PathBuf};

use kadr_core::subtitles::{srt_to_webvtt, SubtitleFormat, SubtitleSource};
use kadr_storage::repos::{MediaItemRepository, SubtitleRepository};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SubtitleServiceError {
    #[error("Subtitle track not found")]
    NotFound,
    #[error("Source subtitle file not found")]
    SourceFileNotFound,
    #[error("Storage error: {0}")]
    Storage(#[from] kadr_storage::StorageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Service managing delivery, on-the-fly WebVTT conversion, disk caching,
/// and track lifecycle for subtitles.
#[derive(Clone)]
pub struct SubtitleDeliveryService {
    cache_dir: PathBuf,
    data_dir: PathBuf,
    subtitle_repo: SubtitleRepository,
    media_repo: MediaItemRepository,
}

impl SubtitleDeliveryService {
    /// Creates a new `SubtitleDeliveryService`.
    pub fn new(
        cache_dir: PathBuf,
        subtitle_repo: SubtitleRepository,
        media_repo: MediaItemRepository,
    ) -> Self {
        let data_dir = cache_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| cache_dir.clone());
        Self::with_dirs(cache_dir, data_dir, subtitle_repo, media_repo)
    }

    /// Creates a new `SubtitleDeliveryService` with explicit cache and data directories.
    pub fn with_dirs(
        cache_dir: PathBuf,
        data_dir: PathBuf,
        subtitle_repo: SubtitleRepository,
        media_repo: MediaItemRepository,
    ) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);
        let _ = std::fs::create_dir_all(&data_dir);
        Self {
            cache_dir,
            data_dir,
            subtitle_repo,
            media_repo,
        }
    }

    /// Sets the data directory for downloaded subtitles.
    pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        self.data_dir = data_dir;
        self
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn subtitle_repo(&self) -> &SubtitleRepository {
        &self.subtitle_repo
    }

    pub fn media_repo(&self) -> &MediaItemRepository {
        &self.media_repo
    }

    /// Returns the absolute path to a ready-to-stream WebVTT file for the given subtitle ID.
    ///
    /// If already converted and cached in `<cache_dir>/<subtitle_id>.vtt`, returns immediately.
    /// Otherwise, fetches the subtitle metadata, reads the source file, converts it to WebVTT
    /// using `srt_to_webvtt`, writes to cache, and returns the cached path.
    pub async fn get_webvtt_path(&self, subtitle_id: i64) -> Result<PathBuf, SubtitleServiceError> {
        let cached_path = self.cache_dir.join(format!("{}.vtt", subtitle_id));

        // Step 1: Cache hit check
        if tokio::fs::try_exists(&cached_path).await.unwrap_or(false) {
            return Ok(cached_path);
        }

        // Step 2: Fetch track from database
        let track = self
            .subtitle_repo
            .find_by_id(subtitle_id)
            .await?
            .ok_or(SubtitleServiceError::NotFound)?;

        // Step 3: Check source file path
        let source_path_str = track
            .file_path
            .as_deref()
            .ok_or(SubtitleServiceError::SourceFileNotFound)?;

        let source_path = Path::new(source_path_str);
        if !tokio::fs::try_exists(source_path).await.unwrap_or(false) {
            return Err(SubtitleServiceError::SourceFileNotFound);
        }

        // Step 4: Read raw content with UTF-8 lossy fallback
        let raw_content = match tokio::fs::read_to_string(source_path).await {
            Ok(content) => content,
            Err(_) => {
                let raw_bytes = tokio::fs::read(source_path).await?;
                String::from_utf8_lossy(&raw_bytes).into_owned()
            }
        };

        let webvtt_content = match track.format {
            SubtitleFormat::Vtt => {
                let stripped = raw_content.strip_prefix('\u{FEFF}').unwrap_or(&raw_content);
                if stripped.trim_start().starts_with("WEBVTT") {
                    raw_content
                } else {
                    srt_to_webvtt(&raw_content)
                }
            }
            _ => srt_to_webvtt(&raw_content),
        };

        // Ensure cache directory exists before writing
        tokio::fs::create_dir_all(&self.cache_dir).await?;

        let tmp_path = self.cache_dir.join(format!("{}.vtt.tmp.{}", subtitle_id, uuid::Uuid::new_v4()));
        tokio::fs::write(&tmp_path, webvtt_content.as_bytes()).await?;
        tokio::fs::rename(&tmp_path, &cached_path).await?;

        Ok(cached_path)
    }

    /// Deletes a subtitle track by ID.
    ///
    /// Removes the track from the database, deletes the cached `.vtt` file if present,
    /// and if the track was downloaded from an online provider (`SubtitleSource::Downloaded`),
    /// deletes the source file from disk.
    pub async fn delete_track(&self, subtitle_id: i64) -> Result<bool, SubtitleServiceError> {
        let cached_path = self.cache_dir.join(format!("{}.vtt", subtitle_id));

        let track = self.subtitle_repo.find_by_id(subtitle_id).await?;
        let track = match track {
            Some(t) => t,
            None => {
                if tokio::fs::try_exists(&cached_path).await.unwrap_or(false) {
                    let _ = tokio::fs::remove_file(&cached_path).await;
                }
                return Ok(false);
            }
        };

        let deleted = self.subtitle_repo.delete(subtitle_id).await?;

        if tokio::fs::try_exists(&cached_path).await.unwrap_or(false) {
            let _ = tokio::fs::remove_file(&cached_path).await;
        }

        if track.source == SubtitleSource::Downloaded {
            if let Some(ref path_str) = track.file_path {
                let path = Path::new(path_str);
                if tokio::fs::try_exists(path).await.unwrap_or(false) {
                    let _ = tokio::fs::remove_file(path).await;
                }
            }
        }

        Ok(deleted)
    }
}
