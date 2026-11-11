use crate::config::StoryboardSettings;
use crate::storyboard::math::{
    build_manifest, compute_storyboard_plan, parse_jpeg_dimensions, rewrite_manifest_urls,
    StoryboardManifest, StoryboardPlan, STORYBOARD_ROWS,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, warn};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryboardStatus {
    Ready(StoryboardManifest),
    Generating,
    NoVideoStream,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetStatus {
    Ready { path: PathBuf, etag: String },
    Generating,
    OutOfRange,
    NoVideoStream,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameStatus {
    Ready { path: PathBuf, etag: String },
    NoVideoStream,
    Unavailable,
}

#[derive(Clone)]
pub struct StoryboardService {
    inner: Arc<StoryboardInner>,
}

struct StoryboardInner {
    cache_root: PathBuf,
    settings: StoryboardSettings,
    ffmpeg_path: String,
    nice_available: bool,
    ffmpeg_available: bool,
    storyboard_semaphore: Arc<Semaphore>,
    frame_semaphore: Arc<Semaphore>,
    in_flight: Mutex<HashSet<String>>,
    storyboard_generations: AtomicU64,
    frame_generations: AtomicU64,
}

impl StoryboardService {
    pub fn new(cache_root: PathBuf, settings: StoryboardSettings, ffmpeg_path: String) -> Self {
        let ffmpeg_available = probe_binary(&ffmpeg_path, "-version");
        Self::with_availability_override(cache_root, settings, ffmpeg_path, ffmpeg_available)
    }

    pub fn with_availability_override(
        cache_root: PathBuf,
        settings: StoryboardSettings,
        ffmpeg_path: String,
        ffmpeg_available: bool,
    ) -> Self {
        let nice_available = cfg!(unix) && probe_binary("nice", "--version");
        let permits = settings.max_concurrent.max(1);
        Self {
            inner: Arc::new(StoryboardInner {
                cache_root,
                settings,
                ffmpeg_path,
                nice_available,
                ffmpeg_available,
                storyboard_semaphore: Arc::new(Semaphore::new(permits)),
                frame_semaphore: Arc::new(Semaphore::new(2)),
                in_flight: Mutex::new(HashSet::new()),
                storyboard_generations: AtomicU64::new(0),
                frame_generations: AtomicU64::new(0),
            }),
        }
    }

    pub fn is_available(&self) -> bool {
        self.inner.settings.enabled && self.inner.ffmpeg_available
    }

    pub fn settings(&self) -> &StoryboardSettings {
        &self.inner.settings
    }

    pub fn storyboard_generation_count(&self) -> u64 {
        self.inner.storyboard_generations.load(Ordering::SeqCst)
    }

    pub fn frame_generation_count(&self) -> u64 {
        self.inner.frame_generations.load(Ordering::SeqCst)
    }

    pub fn path_hash(file_path: &Path) -> String {
        let canonical =
            std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
        let mut hasher = Sha256::new();
        hasher.update(canonical.to_string_lossy().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    async fn file_fingerprint(file_path: &Path) -> std::io::Result<(PathBuf, u64, u32, u64)> {
        let canonical = tokio::fs::canonicalize(file_path)
            .await
            .unwrap_or_else(|_| file_path.to_path_buf());
        let meta = tokio::fs::metadata(&canonical).await?;
        if !meta.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not a regular file",
            ));
        }
        let (mtime_secs, mtime_nanos) = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| (d.as_secs(), d.subsec_nanos()))
            .unwrap_or((0, 0));
        Ok((canonical, mtime_secs, mtime_nanos, meta.len()))
    }

    fn compute_source_hash(
        canonical_path: &Path,
        mtime_secs: u64,
        mtime_nanos: u32,
        file_size: u64,
        tile_width: u32,
        interval_ms: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(canonical_path.to_string_lossy().as_bytes());
        hasher.update(b"|");
        hasher.update(mtime_secs.to_le_bytes());
        hasher.update(mtime_nanos.to_le_bytes());
        hasher.update(file_size.to_le_bytes());
        hasher.update(tile_width.to_le_bytes());
        hasher.update(interval_ms.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn compute_frame_hash(
        canonical_path: &Path,
        mtime_secs: u64,
        mtime_nanos: u32,
        file_size: u64,
        snapped_t: u64,
        clamped_w: u32,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(canonical_path.to_string_lossy().as_bytes());
        hasher.update(b"|frame|");
        hasher.update(mtime_secs.to_le_bytes());
        hasher.update(mtime_nanos.to_le_bytes());
        hasher.update(file_size.to_le_bytes());
        hasher.update(snapped_t.to_le_bytes());
        hasher.update(clamped_w.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn item_dir(&self, path_hash: &str) -> PathBuf {
        self.inner.cache_root.join("items").join(path_hash)
    }

    fn frames_dir(&self, path_hash: &str) -> PathBuf {
        self.inner.cache_root.join("frames").join(path_hash)
    }

    pub async fn purge_for_path(&self, file_path: &Path) -> std::io::Result<()> {
        let mut hashes = HashSet::new();
        let mut raw_hasher = Sha256::new();
        raw_hasher.update(file_path.to_string_lossy().as_bytes());
        hashes.insert(format!("{:x}", raw_hasher.finalize()));
        hashes.insert(Self::path_hash(file_path));

        for h in hashes {
            let item_dir = self.item_dir(&h);
            if tokio::fs::try_exists(&item_dir).await.unwrap_or(false) {
                let _ = tokio::fs::remove_dir_all(&item_dir).await;
            }
            let frames_dir = self.frames_dir(&h);
            if tokio::fs::try_exists(&frames_dir).await.unwrap_or(false) {
                let _ = tokio::fs::remove_dir_all(&frames_dir).await;
            }
        }
        Ok(())
    }

    pub async fn get_or_enqueue_manifest(
        &self,
        item_id: i64,
        file_path: &Path,
        duration_seconds: i64,
    ) -> std::io::Result<StoryboardStatus> {
        if !self.is_available() {
            return Ok(StoryboardStatus::Unavailable);
        }
        let Some(plan) = compute_storyboard_plan(
            duration_seconds,
            self.inner.settings.interval_secs,
            self.inner.settings.tile_width,
        ) else {
            return Ok(StoryboardStatus::NoVideoStream);
        };

        let (canonical, mtime_secs, mtime_nanos, file_size) =
            Self::file_fingerprint(file_path).await?;
        let path_hash = Self::path_hash(&canonical);
        let source_hash = Self::compute_source_hash(
            &canonical,
            mtime_secs,
            mtime_nanos,
            file_size,
            plan.tile_width,
            plan.interval_ms,
        );

        let source_dir = self.item_dir(&path_hash).join(&source_hash);
        let no_video_marker = source_dir.join(".novideo");
        if tokio::fs::try_exists(&no_video_marker).await.unwrap_or(false) {
            return Ok(StoryboardStatus::NoVideoStream);
        }

        let manifest_path = source_dir.join("manifest.json");
        if let Ok(bytes) = tokio::fs::read(&manifest_path).await {
            if let Ok(manifest) = serde_json::from_slice::<StoryboardManifest>(&bytes) {
                return Ok(StoryboardStatus::Ready(rewrite_manifest_urls(
                    manifest, item_id,
                )));
            }
        }

        self.spawn_generation_if_needed(canonical, path_hash, source_hash, plan)
            .await;
        Ok(StoryboardStatus::Generating)
    }

    pub async fn get_or_enqueue_sheet(
        &self,
        _item_id: i64,
        index: u32,
        file_path: &Path,
        duration_seconds: i64,
    ) -> std::io::Result<SheetStatus> {
        if !self.is_available() {
            return Ok(SheetStatus::Unavailable);
        }
        let Some(plan) = compute_storyboard_plan(
            duration_seconds,
            self.inner.settings.interval_secs,
            self.inner.settings.tile_width,
        ) else {
            return Ok(SheetStatus::NoVideoStream);
        };

        let (canonical, mtime_secs, mtime_nanos, file_size) =
            Self::file_fingerprint(file_path).await?;
        let path_hash = Self::path_hash(&canonical);
        let source_hash = Self::compute_source_hash(
            &canonical,
            mtime_secs,
            mtime_nanos,
            file_size,
            plan.tile_width,
            plan.interval_ms,
        );

        let source_dir = self.item_dir(&path_hash).join(&source_hash);
        let no_video_marker = source_dir.join(".novideo");
        if tokio::fs::try_exists(&no_video_marker).await.unwrap_or(false) {
            return Ok(SheetStatus::NoVideoStream);
        }

        let manifest_path = source_dir.join("manifest.json");
        if let Ok(bytes) = tokio::fs::read(&manifest_path).await {
            if let Ok(manifest) = serde_json::from_slice::<StoryboardManifest>(&bytes) {
                if (index as usize) >= manifest.sheets.len() {
                    return Ok(SheetStatus::OutOfRange);
                }
                let sheet_path = source_dir.join(format!("{index}.jpg"));
                if tokio::fs::try_exists(&sheet_path).await.unwrap_or(false) {
                    let etag = format!("\"{source_hash}-{index}\"");
                    return Ok(SheetStatus::Ready {
                        path: sheet_path,
                        etag,
                    });
                }
                return Ok(SheetStatus::OutOfRange);
            }
        }

        if index >= plan.sheet_count {
            return Ok(SheetStatus::OutOfRange);
        }

        self.spawn_generation_if_needed(canonical, path_hash, source_hash, plan)
            .await;
        Ok(SheetStatus::Generating)
    }

    pub async fn extract_or_get_frame(
        &self,
        file_path: &Path,
        duration_seconds: i64,
        snapped_t: u64,
        clamped_w: u32,
    ) -> std::io::Result<FrameStatus> {
        if !self.is_available() {
            return Ok(FrameStatus::Unavailable);
        }
        if duration_seconds <= 0 {
            return Ok(FrameStatus::NoVideoStream);
        }

        let (canonical, mtime_secs, mtime_nanos, file_size) =
            Self::file_fingerprint(file_path).await?;
        let path_hash = Self::path_hash(&canonical);
        let frame_hash = Self::compute_frame_hash(
            &canonical,
            mtime_secs,
            mtime_nanos,
            file_size,
            snapped_t,
            clamped_w,
        );

        let frames_dir = self.frames_dir(&path_hash);
        let target_path = frames_dir.join(format!("{frame_hash}.jpg"));
        let etag = format!("\"frame-{frame_hash}\"");

        if let Ok(meta) = tokio::fs::metadata(&target_path).await {
            if meta.is_file() && meta.len() > 0 {
                return Ok(FrameStatus::Ready {
                    path: target_path,
                    etag,
                });
            }
        }

        let _permit = self
            .inner
            .frame_semaphore
            .acquire()
            .await
            .map_err(|_| std::io::Error::other("frame semaphore closed"))?;

        if let Ok(meta) = tokio::fs::metadata(&target_path).await {
            if meta.is_file() && meta.len() > 0 {
                return Ok(FrameStatus::Ready {
                    path: target_path,
                    etag,
                });
            }
        }

        tokio::fs::create_dir_all(&frames_dir).await?;
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = frames_dir.join(format!(
            ".tmp.{}.{}.{}.jpg",
            std::process::id(),
            counter,
            frame_hash
        ));

        self.inner.frame_generations.fetch_add(1, Ordering::SeqCst);
        let mut ok = self
            .run_frame_ffmpeg(&canonical, &tmp_path, snapped_t, clamped_w)
            .await;
        if !ok && snapped_t > 0 {
            ok = self
                .run_frame_ffmpeg(&canonical, &tmp_path, 0, clamped_w)
                .await;
        }

        if ok
            && (tokio::fs::rename(&tmp_path, &target_path).await.is_ok()
                || tokio::fs::try_exists(&target_path).await.unwrap_or(false))
        {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Ok(FrameStatus::Ready {
                path: target_path,
                etag,
            });
        }
        let _ = tokio::fs::remove_file(&tmp_path).await;
        Ok(FrameStatus::NoVideoStream)
    }

    async fn spawn_generation_if_needed(
        &self,
        canonical_path: PathBuf,
        path_hash: String,
        source_hash: String,
        plan: StoryboardPlan,
    ) {
        {
            let mut guard = self.inner.in_flight.lock().await;
            if !guard.insert(source_hash.clone()) {
                return;
            }
        }

        let svc = self.clone();
        tokio::spawn(async move {
            let _permit = svc.inner.storyboard_semaphore.acquire().await;
            if let Err(e) = svc
                .generate_storyboard_job(&canonical_path, &path_hash, &source_hash, &plan)
                .await
            {
                warn!(
                    path = ?canonical_path,
                    error = %e,
                    "Storyboard generation job failed"
                );
            }
            let mut guard = svc.inner.in_flight.lock().await;
            guard.remove(&source_hash);
        });
    }

    async fn generate_storyboard_job(
        &self,
        canonical_path: &Path,
        path_hash: &str,
        source_hash: &str,
        plan: &StoryboardPlan,
    ) -> std::io::Result<()> {
        let item_dir = self.item_dir(path_hash);
        let final_dir = item_dir.join(source_hash);
        if tokio::fs::try_exists(final_dir.join("manifest.json"))
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }

        tokio::fs::create_dir_all(&item_dir).await?;
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_dir = item_dir.join(format!(
            ".tmp.{}.{}.{}",
            std::process::id(),
            counter,
            source_hash
        ));
        tokio::fs::create_dir_all(&tmp_dir).await?;

        self.inner
            .storyboard_generations
            .fetch_add(1, Ordering::SeqCst);

        let mut produced = self
            .run_storyboard_ffmpeg(canonical_path, &tmp_dir, plan, true)
            .await;
        if produced == 0 {
            debug!(
                path = ?canonical_path,
                "Keyframe-only pass produced 0 sheets; retrying without -skip_frame nokey"
            );
            produced = self
                .run_storyboard_ffmpeg(canonical_path, &tmp_dir, plan, false)
                .await;
        }

        if produced == 0 {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            tokio::fs::create_dir_all(&final_dir).await?;
            tokio::fs::write(final_dir.join(".novideo"), b"").await?;
            return Ok(());
        }

        let first_sheet_tmp = tmp_dir.join("sheet_001.jpg");
        let first_bytes = tokio::fs::read(&first_sheet_tmp).await?;
        let (_, sheet_height) = parse_jpeg_dimensions(&first_bytes).unwrap_or((
            plan.tile_width * plan.columns,
            ((plan.tile_width * 9 / 16) & !1) * STORYBOARD_ROWS,
        ));
        let tile_height = ((sheet_height / STORYBOARD_ROWS).max(2)) & !1;

        for idx in 0..produced {
            let src = tmp_dir.join(format!("sheet_{:03}.jpg", idx + 1));
            let dst = tmp_dir.join(format!("{idx}.jpg"));
            tokio::fs::rename(&src, &dst).await?;
        }

        let manifest = build_manifest(0, plan, tile_height, Some(produced));
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        tokio::fs::write(tmp_dir.join("manifest.json"), manifest_bytes).await?;

        if tokio::fs::try_exists(&final_dir).await.unwrap_or(false) {
            let _ = tokio::fs::remove_dir_all(&final_dir).await;
        }
        if let Err(e) = tokio::fs::rename(&tmp_dir, &final_dir).await {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            return Err(e);
        }

        // Prune older sibling source_hash directories for this file path
        if let Ok(mut entries) = tokio::fs::read_dir(&item_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str != source_hash && !name_str.starts_with(".tmp.") {
                    let _ = tokio::fs::remove_dir_all(entry.path()).await;
                }
            }
        }

        Ok(())
    }

    async fn run_storyboard_ffmpeg(
        &self,
        media_path: &Path,
        out_dir: &Path,
        plan: &StoryboardPlan,
        keyframe_only: bool,
    ) -> u32 {
        let mut cmd = if self.inner.nice_available {
            let mut c = tokio::process::Command::new("nice");
            c.args(["-n", "10", &self.inner.ffmpeg_path]);
            c
        } else {
            tokio::process::Command::new(&self.inner.ffmpeg_path)
        };

        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        if keyframe_only {
            cmd.args(["-skip_frame", "nokey"]);
        }
        let vf = format!(
            "fps=1/{},scale={}:-2,tile={}x{}",
            plan.interval_secs, plan.tile_width, plan.columns, plan.rows
        );
        let pattern = out_dir.join("sheet_%03d.jpg");

        cmd.arg("-i")
            .arg(media_path)
            .args(["-an", "-sn", "-vf", &vf, "-frames:v"])
            .arg(plan.sheet_count.to_string())
            .args(["-q:v", "6"])
            .arg(&pattern)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let _ = cmd.output().await;

        let mut count = 0u32;
        for idx in 1..=plan.sheet_count {
            let candidate = out_dir.join(format!("sheet_{idx:03}.jpg"));
            if let Ok(meta) = tokio::fs::metadata(&candidate).await {
                if meta.is_file() && meta.len() > 0 {
                    count += 1;
                    continue;
                }
            }
            break;
        }
        count
    }

    async fn run_frame_ffmpeg(
        &self,
        media_path: &Path,
        target_path: &Path,
        snapped_t: u64,
        clamped_w: u32,
    ) -> bool {
        let vf = format!("scale={clamped_w}:-2");
        let res = tokio::process::Command::new(&self.inner.ffmpeg_path)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-ss"])
            .arg(snapped_t.to_string())
            .arg("-i")
            .arg(media_path)
            .args(["-frames:v", "1", "-vf", &vf, "-q:v", "5"])
            .arg(target_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await;

        match res {
            Ok(out) if out.status.success() => tokio::fs::metadata(target_path)
                .await
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false),
            _ => false,
        }
    }
}

fn probe_binary(bin: &str, arg: &str) -> bool {
    std::process::Command::new(bin)
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
