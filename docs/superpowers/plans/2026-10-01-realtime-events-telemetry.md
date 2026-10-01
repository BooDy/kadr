# Milestone 5A: Real-Time Event Bus (SSE) & System Telemetry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a real-time Server-Sent Events (SSE) event bus and system telemetry subsystem for Kadr to broadcast domain events (`library:updated`, `layout:changed`, `subtitle:downloaded`, `session:synced`, `system:telemetry`) and expose live resource metrics (RSS memory, active streams, SQLite WAL/DB file sizes).

**Architecture:** Strongly-typed domain event models in `kadr-core`, a bounded in-memory `EventBus` (`tokio::sync::broadcast`) in `kadr-server`, public `/api/v1/events` SSE route with 15-second keep-alive heartbeats, a background `TelemetryCollector` querying `/proc/self/statm` and SQLite file sizes without blocking, and event publishing hooks across ingest, playback scrobble, and subtitle download workflows.

**Tech Stack:** Rust (edition 2021), Tokio (async runtime, broadcast channel), Axum 0.8 (SSE response and routers), tokio-stream (BroadcastStream), serde/serde_json.

## Global Constraints

- RSS memory budget must remain <= 30 MB under idle and streaming workloads.
- Zero whole-file in-memory buffering.
- Zero external C/native runtime dependencies baseline (compilable against musl).
- SQLite operations must maintain WAL mode, PRAGMA synchronous = NORMAL, PRAGMA foreign_keys = ON, and PRAGMA busy_timeout = 5000.
- SSE endpoint `/api/v1/events` must support public / unauthenticated access with `text/event-stream` and 15-second keep-alive heartbeats.
- Telemetry REST endpoint `/api/v1/system/telemetry` requires `RequireAdmin` role.

---

### Task 1: Domain Event Models & Telemetry Schema (`kadr-core`)

**Files:**
- Create: `crates/kadr-core/src/events.rs`
- Modify: `crates/kadr-core/src/lib.rs`
- Test: `crates/kadr-core/tests/events_test.rs`

**Interfaces:**
- Consumes: None
- Produces:
  - `TelemetrySnapshot`: struct with `active_sessions_count: usize`, `rss_memory_bytes: u64`, `db_size_bytes: u64`, `wal_size_bytes: u64`, `timestamp: i64`
  - `SystemEvent`: enum with variants:
    - `LibraryUpdated { library_id: String, item_count: usize, timestamp: i64 }`
    - `LayoutChanged { screen_id: String, timestamp: i64 }`
    - `SubtitleDownloaded { item_id: i64, subtitle_id: i64, language: String, timestamp: i64 }`
    - `SessionSynced { session_id: String, item_id: i64, user_id: String, position_seconds: i64, timestamp: i64 }`
    - `SystemTelemetry(TelemetrySnapshot)`
  - `SystemEvent::event_type(&self) -> &'static str`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-core/tests/events_test.rs` testing:
- Serialization and deserialization of all `SystemEvent` variants with expected JSON tag `type` and `payload`.
- `event_type()` helper returning `"library:updated"`, `"layout:changed"`, `"subtitle:downloaded"`, `"session:synced"`, `"system:telemetry"`.
- Serialization of `TelemetrySnapshot`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-core --test events_test`
Expected: Compilation failure due to missing `events` module.

- [ ] **Step 3: Implement domain models**
Implement `crates/kadr-core/src/events.rs` and re-export in `crates/kadr-core/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-core --test events_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(core): implement domain event models and telemetry schema`

---

### Task 2: In-Memory Bounded EventBus (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/events/bus.rs`
- Create: `crates/kadr-server/src/events/mod.rs`
- Modify: `crates/kadr-server/src/lib.rs`
- Test: `crates/kadr-server/tests/event_bus_test.rs`

**Interfaces:**
- Consumes: `SystemEvent` from `kadr-core::events`
- Produces:
  - `EventBus`: struct with methods:
    - `pub fn new(capacity: usize) -> Self`
    - `pub fn default_bus() -> Self` (capacity 512)
    - `pub fn publish(&self, event: SystemEvent) -> usize` (returns receiver count)
    - `pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SystemEvent>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/event_bus_test.rs` testing:
- Creation of `EventBus` with custom capacity.
- Publishing events to multiple subscribers simultaneously.
- Publishing with zero subscribers returns 0 without error.
- Bounded ring buffer lag handling (`RecvError::Lagged`).

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test event_bus_test`
Expected: Compilation failure due to missing `events::EventBus`.

- [ ] **Step 3: Implement EventBus**
Implement `crates/kadr-server/src/events/bus.rs`, expose in `src/events/mod.rs`, and export in `crates/kadr-server/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test event_bus_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement in-memory bounded EventBus using Tokio broadcast`

---

### Task 3: System Telemetry Collector (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/telemetry/collector.rs`
- Create: `crates/kadr-server/src/telemetry/mod.rs`
- Modify: `crates/kadr-server/src/lib.rs`
- Test: `crates/kadr-server/tests/telemetry_test.rs`

**Interfaces:**
- Consumes:
  - `TelemetrySnapshot`, `SystemEvent` from `kadr-core::events`
  - `SessionRegistry` from `crate::playback::session`
  - `EventBus` from `crate::events`
- Produces:
  - `TelemetryCollector`: struct with:
    - `pub fn new(db_path: PathBuf, session_registry: Arc<SessionRegistry>, event_bus: Arc<EventBus>) -> Self`
    - `pub async fn collect_snapshot(&self) -> TelemetrySnapshot`
    - `pub fn spawn_periodic_broadcaster(self: Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/telemetry_test.rs` testing:
- `collect_snapshot()` reporting non-zero RSS memory on Linux.
- Accurately reporting active session counts from `SessionRegistry`.
- Accurately measuring database file size and WAL file size on disk.
- Periodic broadcasting emits `SystemEvent::SystemTelemetry` over the `EventBus`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test telemetry_test`
Expected: Compilation failure due to missing `telemetry` module.

- [ ] **Step 3: Implement TelemetryCollector**
Implement `crates/kadr-server/src/telemetry/collector.rs` with zero-dependency pure-Rust Linux `/proc/self/statm` parsing (reading resident page count multiplied by page size from `sysconf(_SC_PAGESIZE)` or standard 4096), non-blocking file size metadata query, and periodic Tokio loop.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test telemetry_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement zero-dependency TelemetryCollector with periodic EventBus broadcast`

---

### Task 4: Public SSE Event Streaming & Admin Telemetry REST Endpoints (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/events_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Test: `crates/kadr-server/tests/events_routes_test.rs`

**Interfaces:**
- Consumes:
  - `EventBus` from `crate::events`
  - `TelemetryCollector` from `crate::telemetry`
  - `RequireAdmin` from `crate::auth::jwt`
- Produces:
  - `GET /api/v1/events` (public SSE stream, keep-alive 15s)
  - `GET /api/v1/system/telemetry` (requires admin auth, returns `TelemetrySnapshot`)
  - Integration in router constructors in `crates/kadr-server/src/api/mod.rs`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/events_routes_test.rs` testing:
- Public unauthenticated connection to `/api/v1/events` returns `Content-Type: text/event-stream`.
- Publishing event on `EventBus` delivers SSE frame with `event:` and `data:`.
- `GET /api/v1/system/telemetry` returns 401 without auth and 403 for standard user.
- `GET /api/v1/system/telemetry` returns 200 with valid `TelemetrySnapshot` JSON when called by Admin user.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test events_routes_test`
Expected: Compilation failure due to missing routes.

- [ ] **Step 3: Implement events and telemetry routes**
Implement `crates/kadr-server/src/api/events_routes.rs`, wire into `crates/kadr-server/src/api/mod.rs`, preserving backward compatibility for router constructors.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test events_routes_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement public SSE stream and admin telemetry REST endpoints`

---

### Task 5: Subsystem Event Emission Integration (`kadr-ingest` & `kadr-server`)

**Files:**
- Modify: `crates/kadr-ingest/src/watcher/worker.rs`
- Modify: `crates/kadr-server/src/api/playback.rs`
- Modify: `crates/kadr-server/src/api/subtitle_routes.rs`
- Test: `crates/kadr-server/tests/subsystem_events_test.rs`

**Interfaces:**
- Consumes:
  - `EventBus` from `crate::events`
  - `SystemEvent` from `kadr-core::events`
- Produces:
  - `IngestWorker::with_event_bus(mut self, event_bus: Arc<dyn Fn(SystemEvent) + Send + Sync>)` or `Arc<EventBus>`
  - Event emission in `playback::progress_heartbeat`: `SystemEvent::SessionSynced`
  - Event emission in `subtitle_routes::download_subtitle`: `SystemEvent::SubtitleDownloaded`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/subsystem_events_test.rs` testing:
- When a playback heartbeat is posted, `SystemEvent::SessionSynced` is published on `EventBus`.
- When a subtitle is downloaded, `SystemEvent::SubtitleDownloaded` is published on `EventBus`.
- When `IngestWorker` commits a batch, `SystemEvent::LibraryUpdated` is published on `EventBus`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test subsystem_events_test`
Expected: Compilation failure or missing event assertions.

- [ ] **Step 3: Wire event publication into subsystems**
Add event emission hooks into `worker.rs`, `playback.rs`, and `subtitle_routes.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test subsystem_events_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server,ingest): integrate event bus publishing into ingest, playback, and subtitle workflows`

---

### Task 6: Server Bootstrap Assembly & End-to-End Milestone 5A Integration Test

**Files:**
- Modify: `crates/kadr-server/src/main.rs`
- Modify: `crates/kadr-server/Cargo.toml`
- Create: `tests/e2e_events_telemetry_test.rs`

**Interfaces:**
- Consumes: All Milestone 5A components
- Produces:
  - Fully assembled server in `main.rs` with `EventBus` and `TelemetryCollector` periodic broadcast
  - End-to-end integration test exercising the full event lifecycle and SSE delivery
  - `[[test]]` entry in `Cargo.toml` targeting `../../tests/e2e_events_telemetry_test.rs`

- [ ] **Step 1: Write the end-to-end integration test**
Create `tests/e2e_events_telemetry_test.rs` testing:
1. Initialize test database, media library, and router.
2. Connect to `GET /api/v1/events` as a client stream.
3. Authenticate admin user, query `GET /api/v1/system/telemetry` -> verify valid telemetry JSON.
4. Record playback heartbeat -> verify `session:synced` SSE event received on stream.
5. Ingest media -> verify `library:updated` SSE event received on stream.
6. Verify periodic `system:telemetry` SSE event received on stream within interval.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test e2e_events_telemetry_test`
Expected: Compilation / routing failure.

- [ ] **Step 3: Update `main.rs` and `Cargo.toml`**
Wire `EventBus` and `TelemetryCollector` into `main.rs` and update `crates/kadr-server/Cargo.toml` with `[[test]]`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test e2e_events_telemetry_test`
Expected: PASS

- [ ] **Step 5: Run full workspace verification**
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: All tests pass, 0 clippy warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): assemble real-time EventBus and telemetry into main router and add Milestone 5A E2E test`
