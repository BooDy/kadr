use std::path::{Path, PathBuf};

use kadr_core::subtitles::{srt_to_webvtt_stream, SubtitleFormat, SubtitleSource};
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
    /// using `srt_to_webvtt_stream`, writes to cache, and returns the cached path.
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

        // Ensure cache directory exists before writing
        tokio::fs::create_dir_all(&self.cache_dir).await?;

        let tmp_path = self.cache_dir.join(format!("{}.vtt.tmp.{}", subtitle_id, uuid::Uuid::new_v4()));
        let source_path_buf = source_path.to_path_buf();
        let tmp_path_clone = tmp_path.clone();
        let format = track.format;

        let spawn_res = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;

            let src_file = std::fs::File::open(&source_path_buf)?;
            let mut reader = std::io::BufReader::new(src_file);

            let dst_file = std::fs::File::create(&tmp_path_clone)?;
            let mut writer = std::io::BufWriter::new(dst_file);

            if format == SubtitleFormat::Vtt {
                let is_webvtt = {
                    use std::io::BufRead;
                    let buf = reader.fill_buf()?;
                    let trimmed = buf.strip_prefix(b"\xef\xbb\xbf").unwrap_or(buf);
                    let trimmed_leading = trimmed
                        .iter()
                        .position(|&b| !b.is_ascii_whitespace())
                        .map(|idx| &trimmed[idx..])
                        .unwrap_or(trimmed);
                    trimmed_leading.starts_with(b"WEBVTT")
                };

                if is_webvtt {
                    std::io::copy(&mut reader, &mut writer)?;
                    writer.flush()?;
                    return Ok(());
                }
            }

            srt_to_webvtt_stream(&mut reader, &mut writer)?;
            writer.flush()?;
            Ok(())
        })
        .await;

        match spawn_res {
            Ok(Ok(())) => {
                tokio::fs::rename(&tmp_path, &cached_path).await?;
                Ok(cached_path)
            }
            Ok(Err(io_err)) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                Err(SubtitleServiceError::Io(io_err))
            }
            Err(join_err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                Err(SubtitleServiceError::Io(std::io::Error::other(join_err)))
            }
        }
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
