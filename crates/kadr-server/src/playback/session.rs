use kadr_core::models::PlaybackSession;
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub session: PlaybackSession,
    pub last_heartbeat_instant: Instant,
}

#[derive(Default, Debug)]
pub struct SessionRegistry {
    sessions: RwLock<HashMap<String, ActiveSession>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, session: PlaybackSession) {
        let mut write = self.sessions.write().await;
        write.insert(
            session.session_id.clone(),
            ActiveSession {
                session,
                last_heartbeat_instant: Instant::now(),
            },
        );
    }

    pub async fn get(&self, session_id: &str) -> Option<PlaybackSession> {
        let read = self.sessions.read().await;
        read.get(session_id).map(|s| s.session.clone())
    }

    pub async fn update_progress(
        &self,
        session_id: &str,
        position: i64,
    ) -> Option<PlaybackSession> {
        let mut write = self.sessions.write().await;
        if let Some(active) = write.get_mut(session_id) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            active.session.current_position_seconds = position;
            active.session.last_heartbeat_at = now;
            active.last_heartbeat_instant = Instant::now();
            Some(active.session.clone())
        } else {
            None
        }
    }

    pub async fn remove(&self, session_id: &str) -> Option<PlaybackSession> {
        let mut write = self.sessions.write().await;
        write.remove(session_id).map(|s| s.session)
    }

    pub async fn prune_stale(&self, max_idle: std::time::Duration) -> usize {
        let now = Instant::now();
        let mut write = self.sessions.write().await;
        let initial_len = write.len();
        write.retain(|_, s| now.duration_since(s.last_heartbeat_instant) < max_idle);
        initial_len - write.len()
    }

    pub async fn len(&self) -> usize {
        let read = self.sessions.read().await;
        read.len()
    }

    pub async fn is_empty(&self) -> bool {
        let read = self.sessions.read().await;
        read.is_empty()
    }
}
