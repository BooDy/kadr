use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub active_sessions_count: usize,
    pub rss_memory_bytes: u64,
    pub db_size_bytes: u64,
    pub wal_size_bytes: u64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum SystemEvent {
    #[serde(rename = "library:updated")]
    LibraryUpdated {
        library_id: String,
        item_count: usize,
        timestamp: i64,
    },
    #[serde(rename = "layout:changed")]
    LayoutChanged { screen_id: String, timestamp: i64 },
    #[serde(rename = "subtitle:downloaded")]
    SubtitleDownloaded {
        item_id: i64,
        subtitle_id: i64,
        language: String,
        timestamp: i64,
    },
    #[serde(rename = "session:synced")]
    SessionSynced {
        session_id: String,
        item_id: i64,
        user_id: String,
        position_seconds: i64,
        timestamp: i64,
    },
    #[serde(rename = "system:telemetry")]
    SystemTelemetry(TelemetrySnapshot),
}

impl SystemEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::LibraryUpdated { .. } => "library:updated",
            Self::LayoutChanged { .. } => "layout:changed",
            Self::SubtitleDownloaded { .. } => "subtitle:downloaded",
            Self::SessionSynced { .. } => "session:synced",
            Self::SystemTelemetry(_) => "system:telemetry",
        }
    }
}
