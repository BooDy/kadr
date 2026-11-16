use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kadr_core::subtitles::{srt_to_webvtt_stream, SubtitleFormat, SubtitleSource, SubtitleTrack};
use kadr_storage::repos::{MediaItemRepository, SubtitleRepository};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SubtitleServiceError {
    #[error("Subtitle track not found")]
    NotFound,
    #[error("Source subtitle file not found")]
    SourceFileNotFound,
    #[error("Subtitle extraction failed: {0}")]
    ExtractionFailed(String),
    #[error("Storage error: {0}")]
    Storage(#[from] kadr_storage::StorageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Service managing delivery, on-the-fly WebVTT conversion, disk caching,
/// lazy discovery, and track lifecycle for subtitles.
#[derive(Clone)]
pub struct SubtitleDeliveryService {
    cache_dir: PathBuf,
    data_dir: PathBuf,
    subtitle_repo: SubtitleRepository,
    media_repo: MediaItemRepository,
    probed_embedded_items: Arc<Mutex<HashSet<i64>>>,
    in_flight_extractions: Arc<Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>>,
    in_flight_probes: Arc<Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>>,
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
            probed_embedded_items: Arc::new(Mutex::new(HashSet::new())),
            in_flight_extractions: Arc::new(Mutex::new(HashMap::new())),
            in_flight_probes: Arc::new(Mutex::new(HashMap::new())),
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

    /// Returns the number of currently active in-flight subtitle extractions.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight_extractions.lock().unwrap().len()
    }

    /// Returns the number of currently active in-flight subtitle probes.
    pub fn in_flight_probe_count(&self) -> usize {
        self.in_flight_probes.lock().unwrap().len()
    }

    fn cleanup_in_flight_probe(&self, item_id: i64, lock: &Arc<tokio::sync::Mutex<()>>) {
        let mut map = self.in_flight_probes.lock().unwrap();
        if let std::collections::hash_map::Entry::Occupied(entry) = map.entry(item_id) {
            if Arc::ptr_eq(entry.get(), lock) && Arc::strong_count(lock) <= 2 {
                entry.remove();
            }
        }
    }

    fn cleanup_in_flight_extraction(&self, subtitle_id: i64, lock: &Arc<tokio::sync::Mutex<()>>) {
        let mut map = self.in_flight_extractions.lock().unwrap();
        if let std::collections::hash_map::Entry::Occupied(entry) = map.entry(subtitle_id) {
            if Arc::ptr_eq(entry.get(), lock) && Arc::strong_count(lock) <= 2 {
                entry.remove();
            }
        }
    }

    /// Lists subtitle tracks for a media item, lazily probing embedded tracks from supported
    /// video containers on disk if they haven't been probed yet.
    /// Uses singleflight synchronization to prevent parallel `ffprobe` runs and duplicate DB rows.
    pub async fn list_or_probe_subtitles(
        &self,
        item: &kadr_core::models::MediaItem,
    ) -> Result<Vec<SubtitleTrack>, SubtitleServiceError> {
        let item_id = item.id.ok_or(SubtitleServiceError::NotFound)?;
        let tracks = self.subtitle_repo.find_by_media_item(item_id).await?;

        // Fast path check before acquiring singleflight probe lock
        let already_probed = self
            .probed_embedded_items
            .lock()
            .unwrap()
            .contains(&item_id);

        if already_probed || tracks.iter().any(|t| t.source == SubtitleSource::Embedded) {
            return Ok(tracks);
        }

        let ext = item
            .file_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_supported_container = matches!(
            ext.as_str(),
            "mkv" | "mp4" | "m4v" | "webm" | "mov" | "avi" | "ts"
        );

        if !is_supported_container
            || !tokio::fs::try_exists(&item.file_path)
                .await
                .unwrap_or(false)
        {
            return Ok(tracks);
        }

        // Singleflight probe lock per media item ID
        let singleflight_lock = {
            let mut map = self.in_flight_probes.lock().unwrap();
            map.entry(item_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let guard = singleflight_lock.lock().await;

        // Re-check under the lock in case another task finished probing while we waited
        let already_probed = self
            .probed_embedded_items
            .lock()
            .unwrap()
            .contains(&item_id);
        let mut tracks = self.subtitle_repo.find_by_media_item(item_id).await?;
        if already_probed || tracks.iter().any(|t| t.source == SubtitleSource::Embedded) {
            drop(guard);
            self.cleanup_in_flight_probe(item_id, &singleflight_lock);
            return Ok(tracks);
        }

        match kadr_ingest::probe::probe_embedded_subtitles(&item.file_path).await {
            Ok(discovered) => {
                let existing_indices: HashSet<u32> = tracks
                    .iter()
                    .filter(|t| t.source == SubtitleSource::Embedded)
                    .filter_map(|t| t.stream_index)
                    .collect();

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                let new_tracks: Vec<SubtitleTrack> = discovered
                    .into_iter()
                    .filter(|d| match d.stream_index {
                        Some(idx) => !existing_indices.contains(&idx),
                        None => false,
                    })
                    .map(|d| d.into_subtitle_track(item_id, now))
                    .collect();

                if !new_tracks.is_empty() {
                    self.subtitle_repo.batch_insert(&new_tracks).await?;
                    tracks = self.subtitle_repo.find_by_media_item(item_id).await?;
                }
                self.probed_embedded_items.lock().unwrap().insert(item_id);
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to probe embedded subtitles for item {item_id} ({}): {err}",
                    item.file_path.display()
                );
            }
        }

        drop(guard);
        self.cleanup_in_flight_probe(item_id, &singleflight_lock);

        Ok(tracks)
    }

    /// Centralized helper to execute `ffmpeg` writing to a temporary file, verifying valid WebVTT
    /// output, ensuring `kill_on_drop(true)` on timeouts, and renaming atomically to `cached_path`.
    async fn execute_ffmpeg_to_webvtt<I, S>(
        &self,
        args: I,
        subtitle_id: i64,
        cached_path: &Path,
    ) -> Result<PathBuf, SubtitleServiceError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        tokio::fs::create_dir_all(&self.cache_dir).await?;
        let tmp_path = self
            .cache_dir
            .join(format!("{}.vtt.tmp.{}", subtitle_id, uuid::Uuid::new_v4()));

        let mut cmd = tokio::process::Command::new("ffmpeg");
        cmd.args(args)
            .arg(&tmp_path)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());

        let output_res = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await;

        let output = match output_res {
            Ok(Ok(output)) => output,
            Ok(Err(io_err)) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(SubtitleServiceError::ExtractionFailed(format!(
                    "Failed to execute ffmpeg: {io_err}"
                )));
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(SubtitleServiceError::ExtractionFailed(
                    "ffmpeg extraction timed out after 60s".to_string(),
                ));
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(SubtitleServiceError::ExtractionFailed(format!(
                "ffmpeg exited with status {}: {}",
                output.status, stderr
            )));
        }

        // Verify non-empty output and valid WebVTT header
        let is_valid = match tokio::fs::read(&tmp_path).await {
            Ok(bytes) if !bytes.is_empty() => {
                let trimmed = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
                let trimmed_leading = trimmed
                    .iter()
                    .position(|&b| !b.is_ascii_whitespace())
                    .map(|idx| &trimmed[idx..])
                    .unwrap_or(trimmed);
                trimmed_leading.starts_with(b"WEBVTT")
            }
            _ => false,
        };

        if !is_valid {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(SubtitleServiceError::ExtractionFailed(
                "ffmpeg produced invalid or empty WebVTT file".to_string(),
            ));
        }

        if let Err(e) = tokio::fs::rename(&tmp_path, cached_path).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(SubtitleServiceError::Io(e));
        }

        Ok(cached_path.to_path_buf())
    }

    /// Internal locked implementation of `get_webvtt_path`.
    async fn get_webvtt_path_locked(
        &self,
        subtitle_id: i64,
        cached_path: &Path,
    ) -> Result<PathBuf, SubtitleServiceError> {
        // Re-check cache under the lock in case another task just finished it
        if tokio::fs::try_exists(cached_path).await.unwrap_or(false) {
            return Ok(cached_path.to_path_buf());
        }

        let track = self
            .subtitle_repo
            .find_by_id(subtitle_id)
            .await?
            .ok_or(SubtitleServiceError::NotFound)?;

        tokio::fs::create_dir_all(&self.cache_dir).await?;

        // Case 1: Embedded subtitle stream extracted from media container via ffmpeg
        if track.source == SubtitleSource::Embedded {
            let stream_index = track
                .stream_index
                .ok_or(SubtitleServiceError::SourceFileNotFound)?;

            let media_item = self
                .media_repo
                .find_by_id(track.media_item_id)
                .await?
                .ok_or(SubtitleServiceError::SourceFileNotFound)?;

            if !tokio::fs::try_exists(&media_item.file_path)
                .await
                .unwrap_or(false)
            {
                return Err(SubtitleServiceError::SourceFileNotFound);
            }

            let stream_spec = format!("0:{}", stream_index);
            let args: &[&std::ffi::OsStr] = &[
                std::ffi::OsStr::new("-nostdin"),
                std::ffi::OsStr::new("-y"),
                std::ffi::OsStr::new("-v"),
                std::ffi::OsStr::new("error"),
                std::ffi::OsStr::new("-i"),
                media_item.file_path.as_os_str(),
                std::ffi::OsStr::new("-map"),
                std::ffi::OsStr::new(&stream_spec),
                std::ffi::OsStr::new("-c:s"),
                std::ffi::OsStr::new("webvtt"),
                std::ffi::OsStr::new("-f"),
                std::ffi::OsStr::new("webvtt"),
            ];

            return self
                .execute_ffmpeg_to_webvtt(args, subtitle_id, cached_path)
                .await;
        }

        // Case 2: Sidecar or Downloaded subtitle file
        let source_path_str = track
            .file_path
            .as_deref()
            .ok_or(SubtitleServiceError::SourceFileNotFound)?;

        let source_path = Path::new(source_path_str);
        if !tokio::fs::try_exists(source_path).await.unwrap_or(false) {
            return Err(SubtitleServiceError::SourceFileNotFound);
        }

        // Subcase 2a: ASS subtitle converted to WebVTT via ffmpeg
        if track.format == SubtitleFormat::Ass {
            let args: &[&std::ffi::OsStr] = &[
                std::ffi::OsStr::new("-nostdin"),
                std::ffi::OsStr::new("-y"),
                std::ffi::OsStr::new("-v"),
                std::ffi::OsStr::new("error"),
                std::ffi::OsStr::new("-i"),
                source_path.as_os_str(),
                std::ffi::OsStr::new("-c:s"),
                std::ffi::OsStr::new("webvtt"),
                std::ffi::OsStr::new("-f"),
                std::ffi::OsStr::new("webvtt"),
            ];

            return self
                .execute_ffmpeg_to_webvtt(args, subtitle_id, cached_path)
                .await;
        }

        // Subcase 2b: VTT sidecar copied directly, or SRT converted via srt_to_webvtt_stream
        let tmp_path = self
            .cache_dir
            .join(format!("{}.vtt.tmp.{}", subtitle_id, uuid::Uuid::new_v4()));
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
                if let Err(e) = tokio::fs::rename(&tmp_path, cached_path).await {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return Err(SubtitleServiceError::Io(e));
                }
                Ok(cached_path.to_path_buf())
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

    /// Returns the absolute path to a ready-to-stream WebVTT file for the given subtitle ID.
    ///
    /// If already converted and cached in `<cache_dir>/<subtitle_id>.vtt`, returns immediately.
    /// Otherwise, extracts embedded tracks via `ffmpeg`, converts ASS sidecars via `ffmpeg`,
    /// or converts SRT to WebVTT, writes to cache, and returns the cached path.
    pub async fn get_webvtt_path(&self, subtitle_id: i64) -> Result<PathBuf, SubtitleServiceError> {
        let cached_path = self.cache_dir.join(format!("{}.vtt", subtitle_id));

        // Fast-path cache hit check before acquiring singleflight lock
        if tokio::fs::try_exists(&cached_path).await.unwrap_or(false) {
            return Ok(cached_path);
        }

        // Singleflight lock: ensure only one task extracts/converts a given subtitle_id at a time
        let singleflight_lock = {
            let mut map = self.in_flight_extractions.lock().unwrap();
            map.entry(subtitle_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let guard = singleflight_lock.lock().await;

        let result = self.get_webvtt_path_locked(subtitle_id, &cached_path).await;

        drop(guard);

        // Clean up in-flight entry when no other tasks are waiting to prevent unbounded memory growth
        self.cleanup_in_flight_extraction(subtitle_id, &singleflight_lock);

        result
    }

    /// Deletes a subtitle track by ID.
    ///
    /// Removes the track from the database, deletes the cached `.vtt` file if present,
    /// purges any active in-flight singleflight lock entry, and if the track was downloaded
    /// from an online provider (`SubtitleSource::Downloaded`), deletes the source file from disk.
    pub async fn delete_track(&self, subtitle_id: i64) -> Result<bool, SubtitleServiceError> {
        let cached_path = self.cache_dir.join(format!("{}.vtt", subtitle_id));

        self.in_flight_extractions
            .lock()
            .unwrap()
            .remove(&subtitle_id);

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
