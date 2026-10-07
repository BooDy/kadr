use crate::error::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct ThumbnailExtractor {
    output_dir: PathBuf,
}

impl ThumbnailExtractor {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn calculate_seek_seconds(duration_seconds: i64) -> f64 {
        if duration_seconds <= 0 {
            15.0
        } else {
            (duration_seconds as f64 * 0.1).clamp(5.0, 30.0)
        }
    }

    pub fn cache_path(&self, media_path: &Path) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(media_path.to_string_lossy().as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        self.output_dir.join(format!("{hash}.jpg"))
    }

    pub async fn extract_thumbnail(
        &self,
        media_path: &Path,
        duration_seconds: i64,
    ) -> Result<Option<PathBuf>> {
        if !tokio::fs::try_exists(media_path).await.unwrap_or(false) {
            return Ok(None);
        }

        let target_path = self.cache_path(media_path);
        if let Ok(meta) = tokio::fs::metadata(&target_path).await {
            if meta.is_file() && meta.len() > 0 {
                return Ok(Some(target_path));
            }
        }

        tokio::fs::create_dir_all(&self.output_dir).await?;

        let target_file_name = target_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("thumbnail.jpg");
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_file_name = format!(
            ".tmp.{}.{}.{}",
            std::process::id(),
            counter,
            target_file_name
        );
        let tmp_path = self.output_dir.join(tmp_file_name);

        let seek_secs = Self::calculate_seek_seconds(duration_seconds);

        let mut success = self.run_ffmpeg(media_path, &tmp_path, seek_secs).await;
        if !success && seek_secs > 0.0 {
            debug!(
                path = ?media_path,
                "Dynamic seek offset failed or yielded no output; retrying at timestamp 0.0s"
            );
            success = self.run_ffmpeg(media_path, &tmp_path, 0.0).await;
        }

        if success {
            if let Ok(meta) = tokio::fs::metadata(&tmp_path).await {
                if meta.is_file() && meta.len() > 0 {
                    if tokio::fs::rename(&tmp_path, &target_path).await.is_ok() {
                        return Ok(Some(target_path));
                    }
                    if let Ok(m) = tokio::fs::metadata(&target_path).await {
                        if m.is_file() && m.len() > 0 {
                            let _ = tokio::fs::remove_file(&tmp_path).await;
                            return Ok(Some(target_path));
                        }
                    }
                }
            }
        }

        // Clean up partial or empty file if any
        let _ = tokio::fs::remove_file(&tmp_path).await;
        Ok(None)
    }

    async fn run_ffmpeg(&self, media_path: &Path, target_path: &Path, seek_secs: f64) -> bool {
        let res = tokio::process::Command::new("ffmpeg")
            .arg("-ss")
            .arg(format!("{:.2}", seek_secs))
            .arg("-i")
            .arg(media_path)
            .arg("-frames:v")
            .arg("1")
            .arg("-q:v")
            .arg("2")
            .arg("-vf")
            .arg("scale='min(720,iw)':-1")
            .arg("-y")
            .arg(target_path)
            .output()
            .await;

        match res {
            Ok(output) if output.status.success() => tokio::fs::metadata(target_path)
                .await
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false),
            Ok(output) => {
                debug!(
                    path = ?media_path,
                    status = ?output.status,
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "ffmpeg failed to extract thumbnail frame"
                );
                false
            }
            Err(e) => {
                warn!(
                    path = ?media_path,
                    error = ?e,
                    "Failed to execute ffmpeg process for thumbnail extraction"
                );
                false
            }
        }
    }
}
