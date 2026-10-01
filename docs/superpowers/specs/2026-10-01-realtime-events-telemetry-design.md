# Milestone 5A Design Spec: Real-Time Event Bus (SSE) & System Telemetry

**Date:** 2026-10-01  
**Status:** Approved  
**Author:** Antigravity Team  

---

## 1. Overview & Objectives

Milestone 5A implements the real-time event distribution and system telemetry subsystem for Kadr. It provides:
1. A strongly-typed domain event model in `kadr-core`.
2. A high-performance, bounded in-memory `EventBus` (`tokio::sync::broadcast`) in `kadr-server`.
3. A public Server-Sent Events (SSE) streaming endpoint (`GET /api/v1/events`) with 15-second keep-alive heartbeats, enabling zero-configuration live updates for web, TV, and desktop clients.
4. A periodic telemetry collector gathering process memory (RSS), SQLite database and WAL file sizes, and active playback session count, broadcasting updates every 5 seconds.
5. An authenticated administrative REST endpoint (`GET /api/v1/system/telemetry`) returning instantaneous runtime metrics snapshots.
6. Event emission across key subsystems: library ingestion, playback heartbeat progress, layout modifications, and subtitle downloads.

---

## 2. Global Constraints & Non-Functional Requirements

- **Memory Budget:** Process RSS memory must remain $\le 30\text{ MB}$ under idle and standard streaming workloads. The `EventBus` ring buffer is bounded to 512 events to prevent unbounded queueing for slow consumers.
- **Portability:** Zero external C/native runtime dependencies. Linux process memory metrics read directly from `/proc/self/statm` without C libraries, with safe fallbacks on non-Linux operating systems.
- **Network Protocol:** Complies with W3C Server-Sent Events standard (`text/event-stream`, `Cache-Control: no-cache`, `Connection: keep-alive`).
- **Database Guarantees:** SQLite telemetry operates via non-blocking asynchronous filesystem metadata queries without holding transaction locks on the database file.

---

## 3. Architecture & Data Models

### 3.1 Domain Models (`crates/kadr-core/src/events.rs`)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetrySnapshot {
    pub active_sessions_count: usize,
    pub rss_memory_bytes: u64,
    pub db_size_bytes: u64,
    pub wal_size_bytes: u64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum SystemEvent {
    #[serde(rename = "library:updated")]
    LibraryUpdated {
        library_id: String,
        item_count: usize,
        timestamp: i64,
    },
    #[serde(rename = "layout:changed")]
    LayoutChanged {
        screen_id: String,
        timestamp: i64,
    },
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
```

### 3.2 In-Memory EventBus (`crates/kadr-server/src/events/bus.rs`)

```rust
use tokio::sync::broadcast;
use kadr_core::events::SystemEvent;

pub struct EventBus {
    sender: broadcast::Sender<SystemEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: SystemEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemEvent> {
        self.sender.subscribe()
    }
}
```

### 3.3 Telemetry Collector (`crates/kadr-server/src/telemetry/collector.rs`)

Collects runtime metrics periodically:
- `active_sessions_count`: Reads `session_registry.active_count()`.
- `rss_memory_bytes`: On Linux, parses `/proc/self/statm` (second token `resident_pages * page_size`). On other OS, defaults to 0 or estimates.
- `db_size_bytes`: `tokio::fs::metadata(&db_path).await.map(|m| m.len()).unwrap_or(0)`.
- `wal_size_bytes`: `tokio::fs::metadata(&wal_path).await.map(|m| m.len()).unwrap_or(0)`.
- Emits `SystemEvent::SystemTelemetry` over `EventBus` every 5 seconds.

---

## 4. REST and SSE Endpoints (`crates/kadr-server/src/api/events_routes.rs`)

### 4.1 `GET /api/v1/events` (Server-Sent Events)
- **Auth:** Public / Unauthenticated access (so browser `EventSource` and TV clients can connect directly without non-standard headers).
- **Headers:**
  - `Content-Type: text/event-stream`
  - `Cache-Control: no-cache`
  - `Connection: keep-alive`
- **Stream:**
  - Wraps `BroadcastStream::new(event_bus.subscribe())`.
  - Filters out lagged/dropped receiver errors cleanly without closing the stream.
  - Serializes events with `.event(event.event_type()).json_data(&event)`.
  - Sets keep-alive heartbeat interval to 15 seconds.

### 4.2 `GET /api/v1/system/telemetry`
- **Auth:** Requires `RequireAdmin` extractor.
- **Returns:** `200 OK` with JSON `TelemetrySnapshot`.

---

## 5. Subsystem Event Integrations

1. **Ingestion Pipeline (`kadr-ingest`)**:
   `IngestWorker::flush_batch` publishes `SystemEvent::LibraryUpdated` whenever media items and subtitles are committed.
2. **Playback Scrobble (`kadr-server`)**:
   `progress_heartbeat` handler publishes `SystemEvent::SessionSynced` when a heartbeat updates playback position.
3. **Subtitle Downloader (`kadr-server`)**:
   `download_subtitle` route publishes `SystemEvent::SubtitleDownloaded` upon successful fetch and disk write.

---

## 6. Testing Strategy

1. **Unit Tests (`crates/kadr-core/tests/events_test.rs`)**:
   - Serialization and deserialization roundtrips for all `SystemEvent` variants.
   - Verification of `event_type()` helper values.
2. **Bus & Telemetry Tests (`crates/kadr-server/tests/event_bus_test.rs`, `telemetry_test.rs`)**:
   - `EventBus` multi-subscriber broadcast, lag handling, and subscriber count.
   - `TelemetryCollector` accuracy of session counts and file sizes.
3. **API & Route Tests (`crates/kadr-server/tests/events_routes_test.rs`)**:
   - Connect to `/api/v1/events`, publish events via `EventBus`, verify SSE frames received.
   - Admin-only protection on `/api/v1/system/telemetry`.
4. **End-to-End Test (`tests/e2e_events_telemetry_test.rs`)**:
   - Launch server, subscribe to `/api/v1/events`, trigger ingest, scrobble, and subtitle download, verify corresponding SSE notifications arrive in real time.
