use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kadr_core::events::{SystemEvent, TelemetrySnapshot};
use tokio::task::JoinHandle;

use crate::events::EventBus;
use crate::playback::session::SessionRegistry;

/// Pure-Rust, zero-dependency telemetry collector for system health and metrics.
#[derive(Debug)]
pub struct TelemetryCollector {
    db_path: PathBuf,
    session_registry: Arc<SessionRegistry>,
    event_bus: Arc<EventBus>,
}

impl TelemetryCollector {
    /// Creates a new `TelemetryCollector`.
    pub fn new(
        db_path: PathBuf,
        session_registry: Arc<SessionRegistry>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            db_path,
            session_registry,
            event_bus,
        }
    }

    /// Collects a point-in-time `TelemetrySnapshot`.
    pub async fn collect_snapshot(&self) -> TelemetrySnapshot {
        let active_sessions_count = self.session_registry.len().await;
        let rss_memory_bytes = read_rss_memory_bytes().await;

        let db_size_bytes = tokio::fs::metadata(&self.db_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let mut wal_path_os = self.db_path.as_os_str().to_os_string();
        wal_path_os.push("-wal");
        let wal_path = PathBuf::from(wal_path_os);

        let wal_size_bytes = tokio::fs::metadata(&wal_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        TelemetrySnapshot {
            active_sessions_count,
            rss_memory_bytes,
            db_size_bytes,
            wal_size_bytes,
            timestamp,
        }
    }

    /// Spawns a background task broadcasting `TelemetrySnapshot` periodically on `EventBus`.
    pub fn spawn_periodic_broadcaster(
        self: Arc<Self>,
        interval: Duration,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let snapshot = self.collect_snapshot().await;
                self.event_bus.publish(SystemEvent::SystemTelemetry(snapshot));
            }
        })
    }
}

#[cfg(target_os = "linux")]
async fn read_rss_memory_bytes() -> u64 {
    tokio::fs::read_to_string("/proc/self/statm")
        .await
        .ok()
        .and_then(|content| {
            content
                .split_whitespace()
                .nth(1)
                .and_then(|val| val.parse::<u64>().ok())
        })
        .map(|pages| pages.saturating_mul(4096))
        .unwrap_or(0)
}

#[cfg(not(target_os = "linux"))]
async fn read_rss_memory_bytes() -> u64 {
    0
}
