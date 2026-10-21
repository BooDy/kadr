use crate::config::TranscodingSettings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};

pub struct TranscodeSession {
    pub session_id: String,
    pub media_id: i64,
    pub temp_dir: PathBuf,
    pub is_full_video_transcode: bool,
    pub last_accessed_at: Arc<AtomicU64>,
    pub child_process: Arc<Mutex<Option<tokio::process::Child>>>,
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
        sessions.values().filter(|s| s.is_full_video_transcode).count()
    }
}
