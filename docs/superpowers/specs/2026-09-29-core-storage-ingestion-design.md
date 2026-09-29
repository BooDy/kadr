# Design Specification: Kadr Milestone 1 — Core Architecture, Storage Engine & Ingestion Pipeline

**Document Status:** Approved  
**Date:** 2026-09-29  
**Target Milestone:** Milestone 1 (Storage Engine & Core Ingestion)  
**Author:** Pair Programming Agent & User  

---

## 1. System Overview & Scope

Kadr (كادر) is a lightweight, zero-bloat, direct-play self-hosted media server written in Rust with an embedded SQLite database. This design specification covers **Milestone 1**: the foundational workspace architecture, embedded SQLite storage engine with WAL mode, and the real-time filesystem ingestion pipeline with metadata extraction.

Subsequent milestones (HTTP 206 streaming engine, Declarative Widget AST query engine, online subtitle engine, and Web Studio) build directly upon the foundational models, storage interfaces, and ingestion events established here.

---

## 2. Workspace & Crate Architecture

The codebase is organized as a Cargo workspace with four decoupled crates:

```
kadr/
├── Cargo.toml                          # Workspace root definition
├── kadr.toml                           # Server bootstrap configuration
├── crates/
│   ├── kadr-core/                      # Pure domain models, enums, error types
│   ├── kadr-storage/                   # SQLite connection pool, migrations, repositories
│   ├── kadr-ingest/                    # Watcher, debounce queue, metadata extraction
│   └── kadr-server/                    # Server binary, configuration, lifecycle
└── docs/
    └── superpowers/specs/              # Design specifications
```

### 2.1 Crate Boundaries & Dependencies

| Crate | Responsibilities | Key Dependencies |
| :--- | :--- | :--- |
| **`kadr-core`** | Canonical domain entities (`Library`, `MediaItem`, `MediaType`, `MediaMetadata`, `TechnicalInfo`), shared validation, and core error traits. Zero I/O dependencies. | `serde`, `serde_json`, `thiserror`, `chrono` |
| **`kadr-storage`** | Embedded SQLite lifecycle, WAL configuration, embedded schema migrations, connection pooling, and typed query repositories. | `kadr-core`, `rusqlite`, `rusqlite_migration`, `deadpool-sqlite`, `tokio`, `tracing` |
| **`kadr-ingest`** | Filesystem monitoring via OS notifications, 500ms debounce queue, filename tokenizer, local `.nfo` XML parsing, pure-Rust / hybrid technical stream probing. | `kadr-core`, `kadr-storage`, `notify`, `regex`, `quick-xml`, `tokio`, `tracing` |
| **`kadr-server`** | Application binary entrypoint, layered configuration loader (`kadr.toml` + env + CLI), lifecycle management, and graceful shutdown coordination. | `kadr-core`, `kadr-storage`, `kadr-ingest`, `clap`, `config`, `tokio`, `tracing-subscriber` |

---

## 3. Data Models (`kadr-core`)

All domain entities are strictly typed and serializable.

### 3.1 Types & Structures

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Movie,
    Show,
    Season,
    Episode,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub media_type: MediaType,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechnicalInfo {
    pub duration_seconds: i64,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u8>,
    pub container: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub director: Option<String>,
    pub writers: Vec<String>,
    pub actors: Vec<String>,
    pub overview: Option<String>,
    pub country: Option<String>,
    pub language: Option<String>,
    pub tags: Vec<String>,
    pub studio: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub release_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: Option<i64>,
    pub library_id: String,
    pub item_type: MediaType,
    pub title: String,
    pub original_title: Option<String>,
    pub release_year: Option<i32>,
    pub added_at: i64,
    pub file_path: PathBuf,
    pub file_name: String,
    pub file_size: u64,
    pub technical: TechnicalInfo,
    pub metadata: MediaMetadata,
}
```

---

## 4. SQLite Storage Engine (`kadr-storage`)

### 4.1 Connection Management & PRAGMAs
To guarantee $\le 30\text{ MB}$ RSS memory usage and concurrent low-latency reads:
* **Pool:** Managed via `deadpool-sqlite` with a configurable pool of read connections (default 4) and a serialized write queue.
* **Per-Connection PRAGMAs:**
  ```sql
  PRAGMA journal_mode = WAL;
  PRAGMA synchronous = NORMAL;
  PRAGMA busy_timeout = 5000;
  PRAGMA foreign_keys = ON;
  PRAGMA cache_size = -8000;    -- ~8 MB page cache
  PRAGMA temp_store = MEMORY;
  ```

### 4.2 Embedded Schema Migrations
Managed via `rusqlite_migration` compiled into the library binary.

#### Migration `001_initial_schema.sql`
```sql
CREATE TABLE libraries (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT UNIQUE NOT NULL,
    media_type TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    pin_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'standard',
    created_at INTEGER NOT NULL
);

CREATE TABLE media_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    item_type TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    release_year INTEGER,
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    file_path TEXT UNIQUE NOT NULL,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    resolution TEXT,
    video_codec TEXT,
    audio_codec TEXT,
    audio_channels INTEGER,
    container TEXT,
    metadata JSON NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_media_library_type ON media_items(library_id, item_type);
CREATE INDEX idx_media_added ON media_items(added_at DESC);
CREATE INDEX idx_media_path ON media_items(file_path);
CREATE INDEX idx_media_title_year ON media_items(title, release_year);
```

### 4.3 Repositories
* **`LibraryRepository`:**
  * `create(library: &Library) -> Result<()>`
  * `get_all() -> Result<Vec<Library>>`
  * `get_by_id(id: &str) -> Result<Option<Library>>`
  * `delete(id: &str) -> Result<bool>`
* **`MediaItemRepository`:**
  * `upsert_batch(items: &[MediaItem]) -> Result<usize>`: Wraps in a single `BEGIN IMMEDIATE` transaction to ingest batches without lock contention.
  * `delete_by_path(path: &Path) -> Result<bool>`
  * `list_by_library(library_id: &str, limit: usize, offset: usize) -> Result<Vec<MediaItem>>`
  * `get_by_id(id: i64) -> Result<Option<MediaItem>>`
  * `get_by_path(path: &Path) -> Result<Option<MediaItem>>`
  * `count_by_library(library_id: &str) -> Result<usize>`

---

## 5. Ingestion Engine & Metadata Pipeline (`kadr-ingest`)

### 5.1 Architecture Pipeline
```
[ Filesystem Event ] -> [ Debounce Queue ] -> [ Extraction Pipeline ] -> [ Batch Worker ] -> [ Storage Pool ]
  (notify crate)        (500ms settling)       1. FilenameParser          (bounded MPSC)      (SQLite WAL)
                                               2. SidecarScanner (.nfo)
                                               3. TechnicalProbe (Hybrid)
```

### 5.2 Filesystem Monitoring & Debounce
1. **Initial Startup Scan:** Walks each library's root directory recursively, reconciling existing files against `media_items.file_path`. Removed files are deleted; new or modified files are queued.
2. **OS Watcher:** Uses `notify::RecommendedWatcher` with recursive watches on all registered library roots.
3. **Debounce Queue:**
   * Files being written emit bursts of `Modify` events.
   * Events are debounced by file path with a sliding 500ms timer.
   * Before handoff, the debouncer verifies stability: ensures file size is unchanged across 200ms and file read access can be acquired.

### 5.3 Metadata Extraction Pipeline
Files pass sequentially through three extractors:

1. **`FilenameParser`:**
   * Regex tokenizer for standard scene and P2P conventions.
   * Extracts clean title (normalizes separators `.`, `_`, `-`), release year, source (`BluRay`, `WEB-DL`), resolution (`2160p`, `4k`, `1080p`, `720p`), codec (`hevc`, `x264`, `av1`), and release group.
2. **`SidecarScanner`:**
   * Looks for matching `.nfo` files (`{base_name}.nfo` or `movie.nfo`).
   * Parses XML tags with `quick-xml`: `<title>`, `<originaltitle>`, `<year>`, `<plot>`, `<director>`, `<actor>`, `<genre>`, `<studio>`.
   * Discovers adjacent artwork: `{base_name}-poster.jpg`, `poster.jpg`, `cover.png`, `backdrop.jpg`.
   * Sidecar `.nfo` values take precedence over filename regex guesses.
3. **`TechnicalProbe` (Hybrid Strategy):**
   * **Pure Rust Baseline:** Inspects container headers (Matroska/EBML header for `.mkv`, moov/mvhd atoms for `.mp4`) to verify container validity and resolution without external binaries.
   * **Optional `ffprobe`:** If `ffprobe` is detected in system `$PATH` and enabled in configuration, runs an asynchronous probe command (`ffprobe -v quiet -print_format json -show_format -show_streams`) to populate exact `duration_seconds`, codec strings, and audio channel count (`stereo`, `5.1`, `7.1`).
   * If `ffprobe` is absent, duration and codecs fall back to container heuristics and filename parsing.

### 5.4 Batch Ingest Worker
* A background Tokio task receives extracted `MediaItem` instances over a bounded channel.
* Batches items (up to 50 items or every 100ms) into a single write transaction via `MediaItemRepository::upsert_batch`.
* Deletion events trigger `delete_by_path` immediately.

---

## 6. Server Bootstrap & Configuration (`kadr-server`)

### 6.1 Configuration (`kadr.toml`)
```toml
[server]
host = "0.0.0.0"
port = 8096
data_dir = "./data"

[storage]
database_path = "./data/kadr.db"
max_readers = 4

[scanner]
debounce_millis = 500
use_ffprobe = true

[[libraries]]
id = "movies"
name = "Movies"
path = "./test_media/movies"
media_type = "Movie"
```

### 6.2 Application Lifecycle
1. Initialize structured logging (`tracing-subscriber` with `RUST_LOG` support).
2. Load configuration (`kadr.toml` + `KADR_*` environment variables).
3. Ensure directories exist (`data_dir`, database parent directory).
4. Initialize `deadpool-sqlite` pool and run migrations to latest version.
5. Synchronize bootstrap libraries into `libraries` table.
6. Initialize `IngestWorker` and start `FsWatcher` for all libraries.
7. Listen for `tokio::signal::ctrl_c()`. On signal, flush pending batches, close watchers, and terminate gracefully.

---

## 7. Error Handling & Resilience
* **Crate-Specific Errors:** Structured via `thiserror`:
  * `kadr_core::Error`: Validation and domain entity errors.
  * `kadr_storage::StorageError`: SQLite connection, migration, and query errors.
  * `kadr_ingest::IngestError`: Filesystem I/O, regex, XML parsing, and process execution errors.
* **Fault Isolation:** Corrupted files, non-media files, or malformed `.nfo` sidecars log structured warnings (`tracing::warn!`) and do not interrupt scanner execution or crash the server.

---

## 8. Verification & Testing Strategy
* **Unit Tests:**
  * `kadr-core`: Model serialization and deserialization roundtrips.
  * `kadr-storage`: In-memory SQLite tests for migrations, `LibraryRepository`, and batch `MediaItemRepository` upserts.
  * `kadr-ingest`:
    * Filename regex parser against diverse scene strings (standard releases, complex multi-tag releases, minimal filenames).
    * XML parser against Kodi/Jellyfin `.nfo` fixtures.
    * Technical probe baseline checks on test media files.
* **Integration Tests:**
  * Spins up a temporary directory with simulated media files, writes an `.nfo` sidecar, triggers the ingestion pipeline, and verifies proper persistence in SQLite.
