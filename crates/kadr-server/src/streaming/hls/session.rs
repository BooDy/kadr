use crate::config::TranscodingSettings;
use kadr_core::models::TechnicalInfo;
use kadr_core::transcoding::{
    build_ffmpeg_hls_args, evaluate_transcode, TranscodeParameters, VideoAction,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error("Transcoding is disabled")]
    Disabled,
    #[error("Transcoding capacity exceeded")]
    CapacityExceeded,
    #[error("Failed to spawn ffmpeg: {0}")]
    ProcessSpawn(#[source] std::io::Error),
    #[error("Timed out waiting for {0}")]
    Timeout(String),
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Process exited prematurely: {0}")]
    ProcessExited(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct TranscodeSession {
    pub session_id: String,
    pub media_id: i64,
    pub temp_dir: PathBuf,
    pub is_full_video_transcode: bool,
    pub last_accessed_at: Arc<AtomicU64>,
    pub child_process: Arc<Mutex<Option<tokio::process::Child>>>,
    pub params: TranscodeParameters,
}

impl std::fmt::Debug for TranscodeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscodeSession")
            .field("session_id", &self.session_id)
            .field("media_id", &self.media_id)
            .field("temp_dir", &self.temp_dir)
            .field("is_full_video_transcode", &self.is_full_video_transcode)
            .field(
                "last_accessed_at",
                &self.last_accessed_at.load(Ordering::Relaxed),
            )
            .field("params", &self.params)
            .finish()
    }
}

impl TranscodeSession {
    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_accessed_at.store(now, Ordering::Relaxed);
    }
}

pub struct HlsSessionManager {
    settings: TranscodingSettings,
    sessions: Arc<RwLock<HashMap<String, Arc<TranscodeSession>>>>,
}

impl HlsSessionManager {
    pub fn new(settings: TranscodingSettings) -> Self {
        Self {
            settings,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn settings(&self) -> &TranscodingSettings {
        &self.settings
    }

    pub async fn active_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub async fn active_full_transcode_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.is_full_video_transcode)
            .count()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<Arc<TranscodeSession>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    pub async fn get_or_create_session(
        &self,
        media_id: i64,
        media_path: &Path,
        tech: &TechnicalInfo,
        params: &TranscodeParameters,
    ) -> Result<Arc<TranscodeSession>, HlsError> {
        if !self.settings.enabled {
            return Err(HlsError::Disabled);
        }

        let hwaccel = if self.settings.hwaccel == "none" {
            None
        } else {
            Some(self.settings.hwaccel.as_str())
        };

        let decision = evaluate_transcode(tech, params, hwaccel);
        let is_full_video_transcode = matches!(decision.video, VideoAction::Transcode { .. });

        let mut sessions = self.sessions.write().await;

        // Session deduplication: reuse an existing session for the same media item and params
        for session in sessions.values() {
            if session.media_id == media_id && session.params == *params {
                session.touch();
                return Ok(session.clone());
            }
        }

        // Concurrency gating for full video transcode jobs
        if is_full_video_transcode {
            let active_full = sessions
                .values()
                .filter(|s| s.is_full_video_transcode)
                .count();
            if active_full >= self.settings.max_concurrent_transcodes {
                return Err(HlsError::CapacityExceeded);
            }
        }

        let session_id = uuid::Uuid::new_v4().to_string();
        let session_dir = self.settings.temp_dir.join(&session_id);
        tokio::fs::create_dir_all(&session_dir).await?;

        let args = build_ffmpeg_hls_args(media_path, &session_dir, &decision, params);

        let spawn_res = tokio::process::Command::new(&self.settings.ffmpeg_path)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let child = match spawn_res {
            Ok(c) => c,
            Err(err) => {
                let _ = tokio::fs::remove_dir_all(&session_dir).await;
                return Err(HlsError::ProcessSpawn(err));
            }
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = Arc::new(TranscodeSession {
            session_id: session_id.clone(),
            media_id,
            temp_dir: session_dir,
            is_full_video_transcode,
            last_accessed_at: Arc::new(AtomicU64::new(now)),
            child_process: Arc::new(Mutex::new(Some(child))),
            params: params.clone(),
        });

        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    pub async fn wait_for_playlist(
        &self,
        session: &TranscodeSession,
        timeout_secs: u64,
    ) -> Result<PathBuf, HlsError> {
        let playlist_path = session.temp_dir.join("playlist.m3u8");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

        loop {
            if tokio::fs::try_exists(&playlist_path).await.unwrap_or(false) {
                if let Ok(meta) = tokio::fs::metadata(&playlist_path).await {
                    if meta.len() > 0 {
                        session.touch();
                        return Ok(playlist_path);
                    }
                }
            }

            // Check if ffmpeg child process exited unexpectedly
            {
                let mut child_guard = session.child_process.lock().await;
                if let Some(child) = child_guard.as_mut() {
                    if let Ok(Some(status)) = child.try_wait() {
                        if tokio::fs::try_exists(&playlist_path).await.unwrap_or(false) {
                            if let Ok(meta) = tokio::fs::metadata(&playlist_path).await {
                                if meta.len() > 0 {
                                    session.touch();
                                    return Ok(playlist_path);
                                }
                            }
                        }
                        return Err(HlsError::ProcessExited(format!(
                            "ffmpeg exited with status {}",
                            status
                        )));
                    }
                }
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(HlsError::Timeout("playlist.m3u8".to_string()));
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn wait_for_segment(
        &self,
        session: &TranscodeSession,
        seq: u32,
        timeout_secs: u64,
    ) -> Result<PathBuf, HlsError> {
        let segment_name = format!("segment_{:04}.ts", seq);
        let segment_path = session.temp_dir.join(&segment_name);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

        loop {
            if tokio::fs::try_exists(&segment_path).await.unwrap_or(false) {
                if let Ok(meta) = tokio::fs::metadata(&segment_path).await {
                    if meta.len() > 0 {
                        session.touch();
                        return Ok(segment_path);
                    }
                }
            }

            // Check if ffmpeg child process exited unexpectedly
            {
                let mut child_guard = session.child_process.lock().await;
                if let Some(child) = child_guard.as_mut() {
                    if let Ok(Some(status)) = child.try_wait() {
                        if tokio::fs::try_exists(&segment_path).await.unwrap_or(false) {
                            if let Ok(meta) = tokio::fs::metadata(&segment_path).await {
                                if meta.len() > 0 {
                                    session.touch();
                                    return Ok(segment_path);
                                }
                            }
                        }
                        return Err(HlsError::ProcessExited(format!(
                            "ffmpeg exited with status {}",
                            status
                        )));
                    }
                }
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(HlsError::Timeout(segment_name));
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub async fn terminate_session(&self, session_id: &str) -> bool {
        let session = self.sessions.write().await.remove(session_id);
        if let Some(session) = session {
            let mut child_guard = session.child_process.lock().await;
            if let Some(mut child) = child_guard.take() {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            let _ = tokio::fs::remove_dir_all(&session.temp_dir).await;
            true
        } else {
            false
        }
    }

    pub async fn sweep_expired_sessions(&self) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let expired_sessions: Vec<Arc<TranscodeSession>> = {
            let mut sessions = self.sessions.write().await;
            let mut expired_keys = Vec::new();
            for (id, session) in sessions.iter() {
                let last_accessed = session.last_accessed_at.load(Ordering::Relaxed);
                if now.saturating_sub(last_accessed) > self.settings.inactivity_timeout_secs {
                    expired_keys.push(id.clone());
                }
            }
            let mut removed = Vec::new();
            for key in expired_keys {
                if let Some(session) = sessions.remove(&key) {
                    removed.push(session);
                }
            }
            removed
        };

        let count = expired_sessions.len();
        for session in expired_sessions {
            let mut child_guard = session.child_process.lock().await;
            if let Some(mut child) = child_guard.take() {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            let _ = tokio::fs::remove_dir_all(&session.temp_dir).await;
        }

        count
    }

    pub fn start_reaper(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            interval.tick().await;
            loop {
                interval.tick().await;
                let count = self.sweep_expired_sessions().await;
                if count > 0 {
                    tracing::info!(
                        reaped_sessions = count,
                        "HlsSessionManager swept expired sessions"
                    );
                }
            }
        })
    }

    pub async fn register_test_session(
        &self,
        session_id: &str,
        media_id: i64,
        temp_dir: PathBuf,
        is_full_video_transcode: bool,
    ) -> Arc<TranscodeSession> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = Arc::new(TranscodeSession {
            session_id: session_id.to_string(),
            media_id,
            temp_dir,
            is_full_video_transcode,
            last_accessed_at: Arc::new(AtomicU64::new(now)),
            child_process: Arc::new(Mutex::new(None)),
            params: TranscodeParameters::default(),
        });

        self.sessions
            .write()
            .await
            .insert(session_id.to_string(), session.clone());
        session
    }
}
