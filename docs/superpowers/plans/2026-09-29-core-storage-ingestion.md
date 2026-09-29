# Kadr Milestone 1: Core Architecture, Storage Engine & Ingestion Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundational Rust workspace, embedded SQLite storage engine with WAL mode, and real-time media ingestion pipeline with metadata extraction for Kadr.

**Architecture:** A decoupled Cargo workspace comprising `kadr-core` (pure domain models), `kadr-storage` (embedded SQLite connection pool, migrations, and typed repositories), `kadr-ingest` (OS-level `notify` watcher, debouncing queue, filename/sidecar/technical metadata extraction pipeline, and batch worker), and `kadr-server` (layered configuration, lifecycle coordinator, and graceful shutdown).

**Tech Stack:** Rust (edition 2021), `tokio`, `rusqlite` + `rusqlite_migration` + `deadpool-sqlite`, `notify`, `regex`, `quick-xml`, `serde` + `serde_json`, `thiserror`, `tracing` + `tracing-subscriber`, `config` + `clap`.

## Global Constraints

- RSS memory usage must remain $\le 30\text{ MB}$ under idle and standard ingestion workloads.
- Zero-external runtime dependencies baseline (compilable against musl); `ffprobe` is an optional progressive enhancement, not a required dependency.
- All database operations in SQLite must run with WAL mode, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.
- File writes must settle with a 500ms sliding debounce window before triggering ingestion.

---

### Task 1: Workspace Scaffolding & `kadr-core` Domain Models

**Files:**
- Create: `Cargo.toml`
- Create: `crates/kadr-core/Cargo.toml`
- Create: `crates/kadr-core/src/lib.rs`
- Create: `crates/kadr-core/src/error.rs`
- Create: `crates/kadr-core/src/models.rs`
- Test: `crates/kadr-core/tests/models_test.rs`

**Interfaces:**
- Consumes: None (root domain crate)
- Produces: `MediaType`, `Library`, `TechnicalInfo`, `MediaMetadata`, `MediaItem`, `CoreError`

- [ ] **Step 1: Write the failing domain serialization test**

```rust
// crates/kadr-core/tests/models_test.rs
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use std::path::PathBuf;

#[test]
fn test_media_item_serialization_roundtrip() {
    let item = MediaItem {
        id: Some(42),
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Cairo Station".to_string(),
        original_title: Some("Bab El Hadid".to_string()),
        release_year: Some(1958),
        added_at: 1700000000,
        file_path: PathBuf::from("/media/movies/Cairo Station (1958).mkv"),
        file_name: "Cairo Station (1958).mkv".to_string(),
        file_size: 4_500_000_000,
        technical: TechnicalInfo {
            duration_seconds: 4620,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
        },
        metadata: MediaMetadata {
            director: Some("Youssef Chahine".to_string()),
            writers: vec!["Abdel Hay Adib".to_string()],
            actors: vec!["Farid Shawqi".to_string(), "Hind Rostom".to_string()],
            overview: Some("A crippled newspaper vendor becomes obsessed with a lemonade seller.".to_string()),
            country: Some("Egypt".to_string()),
            language: Some("ara".to_string()),
            tags: vec!["classic".to_string(), "drama".to_string()],
            studio: Some("Studio Misr".to_string()),
            poster_path: Some("/media/movies/poster.jpg".to_string()),
            backdrop_path: None,
            release_group: Some("Ghareeb".to_string()),
        },
    };

    let serialized = serde_json::to_string(&item).expect("serialization failed");
    let deserialized: MediaItem = serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(item, deserialized);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-core`
Expected: FAIL with compilation error (manifest or crate not found)

- [ ] **Step 3: Create workspace root and implement `kadr-core`**

Create `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "crates/kadr-core",
    "crates/kadr-storage",
    "crates/kadr-ingest",
    "crates/kadr-server",
]

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
tokio = { version = "1.43", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

Create `crates/kadr-core/Cargo.toml`:
```toml
[package]
name = "kadr-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
```

Create `crates/kadr-core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid media item: {0}")]
    ValidationError(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
```

Create `crates/kadr-core/src/models.rs`:
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

Create `crates/kadr-core/src/lib.rs`:
```rust
pub mod error;
pub mod models;

pub use error::CoreError;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-core`
Expected: PASS (`test_media_item_serialization_roundtrip ... ok`)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/kadr-core
git commit -m "feat(core): setup workspace and implement core domain models"
```

---

### Task 2: SQLite Connection Pool & Embedded Migrations (`kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/Cargo.toml`
- Create: `crates/kadr-storage/src/lib.rs`
- Create: `crates/kadr-storage/src/error.rs`
- Create: `crates/kadr-storage/src/pool.rs`
- Create: `crates/kadr-storage/src/migrations.rs`
- Create: `crates/kadr-storage/src/migrations/001_initial_schema.sql`
- Test: `crates/kadr-storage/tests/migration_test.rs`

**Interfaces:**
- Consumes: `kadr-core::models::*`
- Produces: `StoragePool`, `StorageError`, `run_migrations`

- [ ] **Step 1: Write failing test for pool initialization and migration execution**

```rust
// crates/kadr-storage/tests/migration_test.rs
use kadr_storage::pool::create_in_memory_pool;
use kadr_storage::migrations::run_migrations;

#[tokio::test]
async fn test_in_memory_pool_migration_and_pragmas() {
    let pool = create_in_memory_pool().expect("failed to create pool");
    let conn = pool.get().await.expect("failed to get connection");

    conn.interact(|c| {
        run_migrations(c).expect("failed to run migrations");

        // Verify WAL and foreign keys
        let fk_enabled: i32 = c.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();
        assert_eq!(fk_enabled, 1);

        // Verify tables exist
        let tables: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"libraries".to_string()));
        assert!(tables.contains(&"media_items".to_string()));
        assert!(tables.contains(&"users".to_string()));
    }).await.expect("interact failed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-storage`
Expected: FAIL with crate/module not found

- [ ] **Step 3: Implement `kadr-storage` pool and migrations**

Create `crates/kadr-storage/Cargo.toml`:
```toml
[package]
name = "kadr-storage"
version = "0.1.0"
edition = "2021"

[dependencies]
kadr-core = { path = "../kadr-core" }
rusqlite = { version = "0.32", features = ["bundled", "chrono"] }
rusqlite_migration = "1.3"
deadpool-sqlite = "0.10"
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tokio = { version = "1.43", features = ["full", "macros"] }
```

Create `crates/kadr-storage/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] deadpool_sqlite::PoolError),
    #[error("Interact error: {0}")]
    Interact(#[from] deadpool_sqlite::InteractError),
    #[error("Migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("Entity not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;
```

Create `crates/kadr-storage/src/migrations/001_initial_schema.sql`:
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

Create `crates/kadr-storage/src/migrations.rs`:
```rust
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use crate::error::Result;

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/001_initial_schema.sql")),
    ])
}

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    Ok(())
}
```

Create `crates/kadr-storage/src/pool.rs`:
```rust
use std::path::Path;
use deadpool_sqlite::{Config, Pool, Runtime};
use rusqlite::Connection;
use crate::error::Result;
use crate::migrations::run_migrations;

fn apply_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;
         PRAGMA cache_size = -8000;
         PRAGMA temp_store = MEMORY;",
    )
}

pub fn create_pool<P: AsRef<Path>>(path: P, max_size: usize) -> Result<Pool> {
    let cfg = Config::new(path.as_ref());
    let pool = cfg.builder(Runtime::Tokio1)
        .max_size(max_size)
        .post_create(|conn, _| {
            Box::pin(async move {
                conn.interact(|c| apply_pragmas(c)).await??;
                Ok(())
            })
        })
        .build()?;
    Ok(pool)
}

pub fn create_in_memory_pool() -> Result<Pool> {
    create_pool(":memory:", 1)
}

pub async fn initialize_database(pool: &Pool) -> Result<()> {
    let conn = pool.get().await?;
    conn.interact(|c| run_migrations(c)).await??;
    Ok(())
}
```

Create `crates/kadr-storage/src/lib.rs`:
```rust
pub mod error;
pub mod migrations;
pub mod pool;

pub use error::{Result, StorageError};
pub use pool::{create_in_memory_pool, create_pool, initialize_database};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-storage`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-storage
git commit -m "feat(storage): setup sqlite connection pool and embedded migrations"
```

---

### Task 3: Storage Repositories (`LibraryRepository` & `MediaItemRepository`)

**Files:**
- Create: `crates/kadr-storage/src/repos/library_repo.rs`
- Create: `crates/kadr-storage/src/repos/media_item_repo.rs`
- Create: `crates/kadr-storage/src/repos/mod.rs`
- Modify: `crates/kadr-storage/src/lib.rs`
- Test: `crates/kadr-storage/tests/repositories_test.rs`

**Interfaces:**
- Consumes: `deadpool_sqlite::Pool`, `kadr_core::models::*`
- Produces: `LibraryRepository`, `MediaItemRepository`

- [ ] **Step 1: Write failing repository integration test**

```rust
// crates/kadr-storage/tests/repositories_test.rs
use std::path::PathBuf;
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

#[tokio::test]
async fn test_library_and_media_item_repositories() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // 1. Create Library
    let lib = Library {
        id: "movies".to_string(),
        name: "Feature Films".to_string(),
        path: PathBuf::from("/media/movies"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    };
    lib_repo.create(&lib).await.unwrap();

    let retrieved_lib = lib_repo.get_by_id("movies").await.unwrap().expect("library not found");
    assert_eq!(retrieved_lib.name, "Feature Films");

    // 2. Insert Batch of Media Items
    let item1 = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "The Nightingale's Prayer".to_string(),
        original_title: Some("Doaa al-Karawan".to_string()),
        release_year: Some(1959),
        added_at: 1700000100,
        file_path: PathBuf::from("/media/movies/The.Nightingales.Prayer.1959.1080p.mkv"),
        file_name: "The.Nightingales.Prayer.1959.1080p.mkv".to_string(),
        file_size: 3_200_000_000,
        technical: TechnicalInfo {
            duration_seconds: 6540,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
        },
        metadata: MediaMetadata {
            director: Some("Henry Barakat".to_string()),
            writers: vec!["Taha Hussein".to_string()],
            actors: vec!["Faten Hamama".to_string(), "Ahmed Mazhar".to_string()],
            overview: Some("A young woman seeks revenge for her sister's honor killing.".to_string()),
            country: Some("Egypt".to_string()),
            language: Some("ara".to_string()),
            tags: vec!["drama".to_string()],
            studio: None,
            poster_path: None,
            backdrop_path: None,
            release_group: None,
        },
    };

    let count = media_repo.upsert_batch(&[item1.clone()]).await.unwrap();
    assert_eq!(count, 1);

    // 3. Query items
    let items = media_repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "The Nightingale's Prayer");
    assert_eq!(items[0].technical.resolution.as_deref(), Some("1080p"));

    // 4. Delete item
    let deleted = media_repo.delete_by_path(&item1.file_path).await.unwrap();
    assert!(deleted);
    let after_delete = media_repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(after_delete.len(), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-storage --test repositories_test`
Expected: FAIL with missing module `repos`

- [ ] **Step 3: Implement `LibraryRepository` and `MediaItemRepository`**

Create `crates/kadr-storage/src/repos/library_repo.rs`:
```rust
use std::path::PathBuf;
use deadpool_sqlite::Pool;
use rusqlite::params;
use kadr_core::models::{Library, MediaType};
use crate::error::{Result, StorageError};

#[derive(Clone)]
pub struct LibraryRepository {
    pool: Pool,
}

impl LibraryRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, lib: &Library) -> Result<()> {
        let lib = lib.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO libraries (id, name, path, media_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    path = excluded.path,
                    media_type = excluded.media_type",
                params![
                    lib.id,
                    lib.name,
                    lib.path.to_str().unwrap_or_default(),
                    serde_json::to_string(&lib.media_type).unwrap_or_default().trim_matches('"'),
                    lib.created_at,
                ],
            )?;
            Ok(())
        }).await?
    }

    pub async fn get_all(&self) -> Result<Vec<Library>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut stmt = c.prepare("SELECT id, name, path, media_type, created_at FROM libraries ORDER BY name ASC")?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path: String = row.get(2)?;
                let media_type_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let media_type: MediaType = serde_json::from_str(&format!("\"{}\"", media_type_str))
                    .unwrap_or(MediaType::Unknown);

                Ok(Library {
                    id,
                    name,
                    path: PathBuf::from(path),
                    media_type,
                    created_at,
                })
            })?;

            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        }).await?
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<Library>> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT id, name, path, media_type, created_at FROM libraries WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path: String = row.get(2)?;
                let media_type_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let media_type: MediaType = serde_json::from_str(&format!("\"{}\"", media_type_str))
                    .unwrap_or(MediaType::Unknown);

                Ok(Some(Library {
                    id,
                    name,
                    path: PathBuf::from(path),
                    media_type,
                    created_at,
                }))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute("DELETE FROM libraries WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        }).await?
    }
}
```

Create `crates/kadr-storage/src/repos/media_item_repo.rs`:
```rust
use std::path::{Path, PathBuf};
use deadpool_sqlite::Pool;
use rusqlite::params;
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use crate::error::{Result, StorageError};

#[derive(Clone)]
pub struct MediaItemRepository {
    pool: Pool,
}

impl MediaItemRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn upsert_batch(&self, items: &[MediaItem]) -> Result<usize> {
        let items = items.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let mut count = 0;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO media_items (
                        library_id, item_type, title, original_title, release_year,
                        duration_seconds, added_at, file_path, file_name, file_size,
                        resolution, video_codec, audio_codec, audio_channels, container, metadata
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                     ON CONFLICT(file_path) DO UPDATE SET
                        library_id = excluded.library_id,
                        item_type = excluded.item_type,
                        title = excluded.title,
                        original_title = excluded.original_title,
                        release_year = excluded.release_year,
                        duration_seconds = excluded.duration_seconds,
                        file_name = excluded.file_name,
                        file_size = excluded.file_size,
                        resolution = excluded.resolution,
                        video_codec = excluded.video_codec,
                        audio_codec = excluded.audio_codec,
                        audio_channels = excluded.audio_channels,
                        container = excluded.container,
                        metadata = excluded.metadata",
                )?;

                for item in &items {
                    let type_str = serde_json::to_string(&item.item_type)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let meta_json = serde_json::to_string(&item.metadata)
                        .unwrap_or_else(|_| "{}".to_string());

                    stmt.execute(params![
                        item.library_id,
                        type_str,
                        item.title,
                        item.original_title,
                        item.release_year,
                        item.technical.duration_seconds,
                        item.added_at,
                        item.file_path.to_str().unwrap_or_default(),
                        item.file_name,
                        item.file_size as i64,
                        item.technical.resolution,
                        item.technical.video_codec,
                        item.technical.audio_codec,
                        item.technical.audio_channels,
                        item.technical.container,
                        meta_json,
                    ])?;
                    count += 1;
                }
            }
            tx.commit()?;
            Ok(count)
        }).await?
    }

    pub async fn delete_by_path<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let path_str = path.as_ref().to_str().unwrap_or_default().to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute("DELETE FROM media_items WHERE file_path = ?1", params![path_str])?;
            Ok(rows > 0)
        }).await?
    }

    pub async fn list_by_library(&self, library_id: &str, limit: usize, offset: usize) -> Result<Vec<MediaItem>> {
        let lib_id = library_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, library_id, item_type, title, original_title, release_year,
                        duration_seconds, added_at, file_path, file_name, file_size,
                        resolution, video_codec, audio_codec, audio_channels, container, metadata
                 FROM media_items
                 WHERE library_id = ?1
                 ORDER BY title ASC
                 LIMIT ?2 OFFSET ?3"
            )?;

            let rows = stmt.query_map(params![lib_id, limit as i64, offset as i64], |row| {
                let id: i64 = row.get(0)?;
                let library_id: String = row.get(1)?;
                let item_type_str: String = row.get(2)?;
                let title: String = row.get(3)?;
                let original_title: Option<String> = row.get(4)?;
                let release_year: Option<i32> = row.get(5)?;
                let duration_seconds: i64 = row.get(6)?;
                let added_at: i64 = row.get(7)?;
                let file_path: String = row.get(8)?;
                let file_name: String = row.get(9)?;
                let file_size: i64 = row.get(10)?;
                let resolution: Option<String> = row.get(11)?;
                let video_codec: Option<String> = row.get(12)?;
                let audio_codec: Option<String> = row.get(13)?;
                let audio_channels: Option<u8> = row.get(14)?;
                let container: Option<String> = row.get(15)?;
                let metadata_str: String = row.get(16)?;

                let item_type: MediaType = serde_json::from_str(&format!("\"{}\"", item_type_str))
                    .unwrap_or(MediaType::Unknown);
                let metadata: MediaMetadata = serde_json::from_str(&metadata_str)
                    .unwrap_or_default();

                Ok(MediaItem {
                    id: Some(id),
                    library_id,
                    item_type,
                    title,
                    original_title,
                    release_year,
                    added_at,
                    file_path: PathBuf::from(file_path),
                    file_name,
                    file_size: file_size as u64,
                    technical: TechnicalInfo {
                        duration_seconds,
                        resolution,
                        video_codec,
                        audio_codec,
                        audio_channels,
                        container,
                    },
                    metadata,
                })
            })?;

            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        }).await?
    }

    pub async fn count_by_library(&self, library_id: &str) -> Result<usize> {
        let lib_id = library_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let count: i64 = c.query_row(
                "SELECT COUNT(*) FROM media_items WHERE library_id = ?1",
                params![lib_id],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        }).await?
    }
}
```

Create `crates/kadr-storage/src/repos/mod.rs`:
```rust
pub mod library_repo;
pub mod media_item_repo;

pub use library_repo::LibraryRepository;
pub use media_item_repo::MediaItemRepository;
```

Update `crates/kadr-storage/src/lib.rs` to expose `repos`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-storage`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-storage
git commit -m "feat(storage): implement library and media item repositories"
```

---

### Task 4: Filename Parser & Scene Release Tokenizer (`kadr-ingest`)

**Files:**
- Create: `crates/kadr-ingest/Cargo.toml`
- Create: `crates/kadr-ingest/src/lib.rs`
- Create: `crates/kadr-ingest/src/error.rs`
- Create: `crates/kadr-ingest/src/parser/filename.rs`
- Create: `crates/kadr-ingest/src/parser/mod.rs`
- Test: `crates/kadr-ingest/tests/filename_parser_test.rs`

**Interfaces:**
- Consumes: `kadr_core::models::*`
- Produces: `FilenameParser`, `ParsedFilename`

- [ ] **Step 1: Write failing filename parser test with various release patterns**

```rust
// crates/kadr-ingest/tests/filename_parser_test.rs
use kadr_ingest::parser::FilenameParser;

#[test]
fn test_standard_scene_release() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Bab.El-Hadid.1958.Restored.1080p.BluRay.x264-Ghareeb.mkv").unwrap();

    assert_eq!(parsed.title, "Bab El-Hadid");
    assert_eq!(parsed.year, Some(1958));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.video_codec.as_deref(), Some("x264"));
    assert_eq!(parsed.container, "mkv");
    assert_eq!(parsed.release_group.as_deref(), Some("Ghareeb"));
}

#[test]
fn test_release_with_parenthesized_year() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("The Nightingale's Prayer (1959) [1080p].mp4").unwrap();

    assert_eq!(parsed.title, "The Nightingale's Prayer");
    assert_eq!(parsed.year, Some(1959));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.container, "mp4");
}

#[test]
fn test_simple_movie_file() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Cairo 30.mkv").unwrap();

    assert_eq!(parsed.title, "Cairo 30");
    assert_eq!(parsed.year, None);
    assert_eq!(parsed.container, "mkv");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-ingest`
Expected: FAIL (crate/module not found)

- [ ] **Step 3: Implement `FilenameParser`**

Create `crates/kadr-ingest/Cargo.toml`:
```toml
[package]
name = "kadr-ingest"
version = "0.1.0"
edition = "2021"

[dependencies]
kadr-core = { path = "../kadr-core" }
kadr-storage = { path = "../kadr-storage" }
notify = "8.0"
regex = "1.11"
quick-xml = "0.37"
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tokio = { version = "1.43", features = ["full", "macros"] }
tempfile = "3.17"
```

Create `crates/kadr-ingest/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IngestError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Storage error: {0}")]
    Storage(#[from] kadr_storage::StorageError),
    #[error("Parse error: {0}")]
    ParseError(String),
}

pub type Result<T> = std::result::Result<T, IngestError>;
```

Create `crates/kadr-ingest/src/parser/filename.rs`:
```rust
use regex::Regex;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFilename {
    pub title: String,
    pub year: Option<i32>,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub source: Option<String>,
    pub release_group: Option<String>,
    pub container: String,
}

pub struct FilenameParser {
    scene_regex: Regex,
    paren_year_regex: Regex,
}

impl Default for FilenameParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FilenameParser {
    pub fn new() -> Self {
        // Match: Title.Year.Quality.Source.Codec-Group.ext
        let scene_regex = Regex::new(
            r"(?i)^(?P<title>.+?)[._ ](?P<year>(?:19|20)\d{2})[._ ].*?(?:(?P<res>2160p|4k|1080p|720p|480p))?.*?(?:(?P<source>bluray|web-dl|webrip|hdtv))?.*?(?:(?P<codec>x264|x265|hevc|av1|h\.?264|h\.?265))?.*?(?:-(?P<group>[a-zA-Z0-9_]+))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        // Match: Title (Year) [Tags].ext
        let paren_year_regex = Regex::new(
            r"(?i)^(?P<title>.+?)\s*\((?P<year>(?:19|20)\d{2})\).*?(?:\[?(?P<res>2160p|4k|1080p|720p|480p)\]?)?.*?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        Self {
            scene_regex,
            paren_year_regex,
        }
    }

    pub fn parse(&self, filename: &str) -> Option<ParsedFilename> {
        let ext = filename.split('.').last()?.to_lowercase();
        if !["mkv", "mp4", "webm", "avi"].contains(&ext.as_str()) {
            return None;
        }

        if let Some(caps) = self.scene_regex.captures(filename) {
            let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
            let clean_title = raw_title.replace('.', " ").replace('_', " ").trim().to_string();
            let year = caps.name("year").and_then(|m| m.as_str().parse::<i32>().ok());
            let resolution = caps.name("res").map(|m| m.as_str().to_lowercase());
            let source = caps.name("source").map(|m| m.as_str().to_string());
            let video_codec = caps.name("codec").map(|m| m.as_str().to_lowercase());
            let release_group = caps.name("group").map(|m| m.as_str().to_string());

            return Some(ParsedFilename {
                title: clean_title,
                year,
                resolution,
                video_codec,
                source,
                release_group,
                container: ext,
            });
        }

        if let Some(caps) = self.paren_year_regex.captures(filename) {
            let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
            let clean_title = raw_title.trim().to_string();
            let year = caps.name("year").and_then(|m| m.as_str().parse::<i32>().ok());
            let resolution = caps.name("res").map(|m| m.as_str().to_lowercase());

            return Some(ParsedFilename {
                title: clean_title,
                year,
                resolution,
                video_codec: None,
                source: None,
                release_group: None,
                container: ext,
            });
        }

        // Fallback: Strip extension and replace dots/underscores
        let stem = match filename.rfind('.') {
            Some(idx) => &filename[..idx],
            None => filename,
        };
        let clean_title = stem.replace('.', " ").replace('_', " ").trim().to_string();

        Some(ParsedFilename {
            title: clean_title,
            year: None,
            resolution: None,
            video_codec: None,
            source: None,
            release_group: None,
            container: ext,
        })
    }
}
```

Create `crates/kadr-ingest/src/parser/mod.rs`:
```rust
pub mod filename;
pub use filename::{FilenameParser, ParsedFilename};
```

Create `crates/kadr-ingest/src/lib.rs`:
```rust
pub mod error;
pub mod parser;

pub use error::{IngestError, Result};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-ingest`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest
git commit -m "feat(ingest): implement filename parser and release tokenizer"
```

---

### Task 5: Sidecar Metadata & Artwork Scanner (`SidecarScanner`)

**Files:**
- Create: `crates/kadr-ingest/src/sidecars/nfo.rs`
- Create: `crates/kadr-ingest/src/sidecars/artwork.rs`
- Create: `crates/kadr-ingest/src/sidecars/mod.rs`
- Modify: `crates/kadr-ingest/src/lib.rs`
- Test: `crates/kadr-ingest/tests/sidecar_test.rs`

**Interfaces:**
- Consumes: File paths, `kadr_core::models::MediaMetadata`
- Produces: `SidecarScanner`, `NfoData`, `ArtworkPaths`

- [ ] **Step 1: Write failing test for .nfo parsing and artwork discovery**

```rust
// crates/kadr-ingest/tests/sidecar_test.rs
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;
use kadr_ingest::sidecars::SidecarScanner;

#[test]
fn test_parse_nfo_file() {
    let dir = tempdir().unwrap();
    let nfo_path = dir.path().join("movie.nfo");
    let xml_content = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<movie>
    <title>The Land</title>
    <originaltitle>Al-Ard</originaltitle>
    <year>1969</year>
    <plot>Peasants in an Egyptian village struggle against feudal oppression.</plot>
    <director>Youssef Chahine</director>
    <studio>General Company for Arab Film Production</studio>
    <actor>
        <name>Mahmoud El-Meliguy</name>
    </actor>
    <actor>
        <name>Ezzat El Alaili</name>
    </actor>
    <genre>Drama</genre>
    <genre>Historical</genre>
</movie>"#;

    let mut file = File::create(&nfo_path).unwrap();
    file.write_all(xml_content.as_bytes()).unwrap();

    let scanner = SidecarScanner::new();
    let nfo = scanner.read_nfo(&nfo_path).unwrap().expect("nfo should parse");

    assert_eq!(nfo.title.as_deref(), Some("The Land"));
    assert_eq!(nfo.original_title.as_deref(), Some("Al-Ard"));
    assert_eq!(nfo.year, Some(1969));
    assert_eq!(nfo.director.as_deref(), Some("Youssef Chahine"));
    assert_eq!(nfo.actors, vec!["Mahmoud El-Meliguy", "Ezzat El Alaili"]);
    assert_eq!(nfo.tags, vec!["Drama", "Historical"]);
}

#[test]
fn test_discover_artwork() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("film.mkv");
    let poster_path = dir.path().join("poster.jpg");
    let backdrop_path = dir.path().join("backdrop.jpg");

    File::create(&video_path).unwrap();
    File::create(&poster_path).unwrap();
    File::create(&backdrop_path).unwrap();

    let scanner = SidecarScanner::new();
    let artwork = scanner.find_artwork(&video_path);

    assert_eq!(artwork.poster, Some(poster_path));
    assert_eq!(artwork.backdrop, Some(backdrop_path));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-ingest --test sidecar_test`
Expected: FAIL (missing `sidecars` module)

- [ ] **Step 3: Implement `NfoParser` and `ArtworkFinder`**

Create `crates/kadr-ingest/src/sidecars/nfo.rs`:
```rust
use std::fs;
use std::path::Path;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct NfoData {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub director: Option<String>,
    pub studio: Option<String>,
    pub actors: Vec<String>,
    pub tags: Vec<String>,
}

pub fn parse_nfo<P: AsRef<Path>>(path: P) -> Result<Option<NfoData>> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    let mut nfo = NfoData::default();
    let mut current_tag = String::new();
    let mut in_actor = false;

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "actor" {
                    in_actor = true;
                }
                current_tag = name;
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "actor" {
                    in_actor = false;
                }
                current_tag.clear();
            }
            Event::Text(e) => {
                let text = e.unescape()?.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                match (current_tag.as_str(), in_actor) {
                    ("title", false) => nfo.title = Some(text),
                    ("originaltitle", false) => nfo.original_title = Some(text),
                    ("year", false) => nfo.year = text.parse::<i32>().ok(),
                    ("plot", false) | ("overview", false) => nfo.overview = Some(text),
                    ("director", false) => nfo.director = Some(text),
                    ("studio", false) => nfo.studio = Some(text),
                    ("name", true) => nfo.actors.push(text),
                    ("genre", false) | ("tag", false) => nfo.tags.push(text),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(Some(nfo))
}
```

Create `crates/kadr-ingest/src/sidecars/artwork.rs`:
```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtworkPaths {
    pub poster: Option<PathBuf>,
    pub backdrop: Option<PathBuf>,
}

pub fn find_artwork<P: AsRef<Path>>(media_file: P) -> ArtworkPaths {
    let media_path = media_file.as_ref();
    let parent = match media_path.parent() {
        Some(p) => p,
        None => return ArtworkPaths::default(),
    };

    let stem = media_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    let poster_candidates = [
        parent.join(format!("{}-poster.jpg", stem)),
        parent.join(format!("{}-poster.png", stem)),
        parent.join("poster.jpg"),
        parent.join("poster.png"),
        parent.join("cover.jpg"),
        parent.join("cover.png"),
        parent.join("folder.jpg"),
    ];

    let backdrop_candidates = [
        parent.join(format!("{}-backdrop.jpg", stem)),
        parent.join(format!("{}-fanart.jpg", stem)),
        parent.join("backdrop.jpg"),
        parent.join("backdrop.png"),
        parent.join("fanart.jpg"),
        parent.join("background.jpg"),
    ];

    let poster = poster_candidates.into_iter().find(|p| p.is_file());
    let backdrop = backdrop_candidates.into_iter().find(|p| p.is_file());

    ArtworkPaths { poster, backdrop }
}
```

Create `crates/kadr-ingest/src/sidecars/mod.rs`:
```rust
pub mod artwork;
pub mod nfo;

use std::path::{Path, PathBuf};
pub use artwork::ArtworkPaths;
pub use nfo::NfoData;

#[derive(Default)]
pub struct SidecarScanner;

impl SidecarScanner {
    pub fn new() -> Self {
        Self
    }

    pub fn read_nfo<P: AsRef<Path>>(&self, path: P) -> crate::error::Result<Option<NfoData>> {
        nfo::parse_nfo(path)
    }

    pub fn find_nfo_for_media<P: AsRef<Path>>(&self, media_path: P) -> crate::error::Result<Option<NfoData>> {
        let p = media_path.as_ref();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent = match p.parent() {
            Some(dir) => dir,
            None => return Ok(None),
        };

        let candidate_named = parent.join(format!("{}.nfo", stem));
        if candidate_named.is_file() {
            return self.read_nfo(&candidate_named);
        }

        let candidate_movie = parent.join("movie.nfo");
        if candidate_movie.is_file() {
            return self.read_nfo(&candidate_movie);
        }

        Ok(None)
    }

    pub fn find_artwork<P: AsRef<Path>>(&self, media_path: P) -> ArtworkPaths {
        artwork::find_artwork(media_path)
    }
}
```

Update `crates/kadr-ingest/src/lib.rs` to export `sidecars`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-ingest`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest
git commit -m "feat(ingest): implement sidecar nfo parser and artwork discovery"
```

---

### Task 6: Technical Media Prober (Pure-Rust + Optional ffprobe)

**Files:**
- Create: `crates/kadr-ingest/src/probe/mod.rs`
- Create: `crates/kadr-ingest/src/probe/pure_rust.rs`
- Create: `crates/kadr-ingest/src/probe/ffprobe.rs`
- Modify: `crates/kadr-ingest/src/lib.rs`
- Test: `crates/kadr-ingest/tests/probe_test.rs`

**Interfaces:**
- Consumes: Media file path, `kadr_core::models::TechnicalInfo`
- Produces: `TechnicalProber`

- [ ] **Step 1: Write failing probe unit test**

```rust
// crates/kadr-ingest/tests/probe_test.rs
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;
use kadr_ingest::probe::TechnicalProber;

#[tokio::test]
async fn test_pure_rust_container_detection() {
    let dir = tempdir().unwrap();
    let mkv_path = dir.path().join("sample.mkv");
    // Write Matroska EBML header magic: 0x1A, 0x45, 0xDF, 0xA3
    let mut file = File::create(&mkv_path).unwrap();
    file.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00]).unwrap();

    let prober = TechnicalProber::new(false); // disable ffprobe for pure rust test
    let info = prober.probe(&mkv_path).await.unwrap();

    assert_eq!(info.container.as_deref(), Some("mkv"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-ingest --test probe_test`
Expected: FAIL (missing `probe` module)

- [ ] **Step 3: Implement Pure-Rust and FFprobe Probing**

Create `crates/kadr-ingest/src/probe/pure_rust.rs`:
```rust
use std::fs::File;
use std::io::Read;
use std::path::Path;
use kadr_core::models::TechnicalInfo;
use crate::error::Result;

pub fn inspect_container<P: AsRef<Path>>(path: P) -> Result<TechnicalInfo> {
    let mut file = match File::open(path.as_ref()) {
        Ok(f) => f,
        Err(_) => return Ok(TechnicalInfo::default()),
    };

    let mut header = [0u8; 16];
    let bytes_read = file.read(&mut header).unwrap_or(0);
    let mut info = TechnicalInfo::default();

    if bytes_read >= 4 && &header[0..4] == &[0x1A, 0x45, 0xDF, 0xA3] {
        info.container = Some("mkv".to_string());
    } else if bytes_read >= 8 && &header[4..8] == b"ftyp" {
        info.container = Some("mp4".to_string());
    } else {
        // Fallback to extension
        if let Some(ext) = path.as_ref().extension().and_then(|s| s.to_str()) {
            info.container = Some(ext.to_lowercase());
        }
    }

    Ok(info)
}
```

Create `crates/kadr-ingest/src/probe/ffprobe.rs`:
```rust
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use serde_json::Value;
use kadr_core::models::TechnicalInfo;
use crate::error::Result;

pub async fn run_ffprobe<P: AsRef<Path>>(path: P) -> Result<Option<TechnicalInfo>> {
    let output = match Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            path.as_ref().to_str().unwrap_or_default(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(out) if out.status.success() => out,
        _ => return Ok(None),
    };

    let json: Value = match serde_json::from_slice(&output.stdout) {
        Ok(j) => j,
        Err(_) => return Ok(None),
    };

    let mut info = TechnicalInfo::default();

    if let Some(duration_str) = json["format"]["duration"].as_str() {
        if let Ok(secs) = duration_str.parse::<f64>() {
            info.duration_seconds = secs.round() as i64;
        }
    }

    if let Some(streams) = json["streams"].as_array() {
        for stream in streams {
            let codec_type = stream["codec_type"].as_str().unwrap_or("");
            let codec_name = stream["codec_name"].as_str().map(|s| s.to_string());

            if codec_type == "video" && info.video_codec.is_none() {
                info.video_codec = codec_name;
                let width = stream["width"].as_i64().unwrap_or(0);
                let height = stream["height"].as_i64().unwrap_or(0);
                if width >= 3800 || height >= 2000 {
                    info.resolution = Some("4k".to_string());
                } else if width >= 1900 || height >= 1000 {
                    info.resolution = Some("1080p".to_string());
                } else if width >= 1200 || height >= 700 {
                    info.resolution = Some("720p".to_string());
                } else if height > 0 {
                    info.resolution = Some("480p".to_string());
                }
            } else if codec_type == "audio" && info.audio_codec.is_none() {
                info.audio_codec = codec_name;
                info.audio_channels = stream["channels"].as_u64().map(|c| c as u8);
            }
        }
    }

    Ok(Some(info))
}
```

Create `crates/kadr-ingest/src/probe/mod.rs`:
```rust
pub mod ffprobe;
pub mod pure_rust;

use std::path::Path;
use kadr_core::models::TechnicalInfo;
use crate::error::Result;

pub struct TechnicalProber {
    enable_ffprobe: bool,
}

impl TechnicalProber {
    pub fn new(enable_ffprobe: bool) -> Self {
        Self { enable_ffprobe }
    }

    pub async fn probe<P: AsRef<Path>>(&self, path: P) -> Result<TechnicalInfo> {
        let mut base_info = pure_rust::inspect_container(&path)?;

        if self.enable_ffprobe {
            if let Ok(Some(probed)) = ffprobe::run_ffprobe(&path).await {
                if probed.duration_seconds > 0 {
                    base_info.duration_seconds = probed.duration_seconds;
                }
                if probed.resolution.is_some() {
                    base_info.resolution = probed.resolution;
                }
                if probed.video_codec.is_some() {
                    base_info.video_codec = probed.video_codec;
                }
                if probed.audio_codec.is_some() {
                    base_info.audio_codec = probed.audio_codec;
                }
                if probed.audio_channels.is_some() {
                    base_info.audio_channels = probed.audio_channels;
                }
            }
        }

        Ok(base_info)
    }
}
```

Update `crates/kadr-ingest/src/lib.rs` to expose `probe`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-ingest`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest
git commit -m "feat(ingest): implement technical stream prober with pure-rust and ffprobe support"
```

---

### Task 7: Debounce Engine, Ingestion Pipeline & Worker (`kadr-ingest`)

**Files:**
- Create: `crates/kadr-ingest/src/watcher/debouncer.rs`
- Create: `crates/kadr-ingest/src/watcher/fs_watcher.rs`
- Create: `crates/kadr-ingest/src/watcher/pipeline.rs`
- Create: `crates/kadr-ingest/src/watcher/worker.rs`
- Create: `crates/kadr-ingest/src/watcher/mod.rs`
- Modify: `crates/kadr-ingest/src/lib.rs`
- Test: `crates/kadr-ingest/tests/pipeline_test.rs`

**Interfaces:**
- Consumes: `kadr_storage::repos::MediaItemRepository`, `kadr_core::models::Library`
- Produces: `IngestPipeline`, `IngestWorker`, `start_library_watcher`

- [ ] **Step 1: Write failing pipeline test**

```rust
// crates/kadr-ingest/tests/pipeline_test.rs
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;
use kadr_core::models::{Library, MediaType};
use kadr_ingest::watcher::pipeline::IngestPipeline;

#[tokio::test]
async fn test_pipeline_processes_file_and_extracts_all_metadata() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("Cairo.Station.1958.1080p.BluRay.x264-Ghareeb.mkv");
    let nfo_path = dir.path().join("Cairo.Station.1958.1080p.BluRay.x264-Ghareeb.nfo");

    // Write dummy video file with MKV magic
    let mut vfile = File::create(&video_path).unwrap();
    vfile.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00]).unwrap();

    // Write nfo
    let mut nfile = File::create(&nfo_path).unwrap();
    nfile.write_all(r#"<movie><director>Youssef Chahine</director></movie>"#.as_bytes()).unwrap();

    let library = Library {
        id: "classics".to_string(),
        name: "Classics".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    };

    let pipeline = IngestPipeline::new(false);
    let item = pipeline.process_file(&library, &video_path).await.unwrap().expect("should process");

    assert_eq!(item.title, "Cairo Station");
    assert_eq!(item.release_year, Some(1958));
    assert_eq!(item.technical.resolution.as_deref(), Some("1080p"));
    assert_eq!(item.metadata.director.as_deref(), Some("Youssef Chahine"));
    assert_eq!(item.metadata.release_group.as_deref(), Some("Ghareeb"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-ingest --test pipeline_test`
Expected: FAIL (missing `watcher` module)

- [ ] **Step 3: Implement Pipeline, Debouncer, and IngestWorker**

Create `crates/kadr-ingest/src/watcher/pipeline.rs`:
```rust
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use crate::error::Result;
use crate::parser::FilenameParser;
use crate::probe::TechnicalProber;
use crate::sidecars::SidecarScanner;

pub struct IngestPipeline {
    filename_parser: FilenameParser,
    sidecar_scanner: SidecarScanner,
    prober: TechnicalProber,
}

impl IngestPipeline {
    pub fn new(enable_ffprobe: bool) -> Self {
        Self {
            filename_parser: FilenameParser::new(),
            sidecar_scanner: SidecarScanner::new(),
            prober: TechnicalProber::new(enable_ffprobe),
        }
    }

    pub async fn process_file<P: AsRef<Path>>(&self, library: &Library, path: P) -> Result<Option<MediaItem>> {
        let path = path.as_ref();
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => return Ok(None),
        };

        let parsed = match self.filename_parser.parse(filename) {
            Some(p) => p,
            None => return Ok(None),
        };

        let metadata_fs = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };
        let file_size = metadata_fs.len();
        let added_at = metadata_fs.modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut technical = self.prober.probe(path).await?;
        if technical.resolution.is_none() {
            technical.resolution = parsed.resolution;
        }
        if technical.video_codec.is_none() {
            technical.video_codec = parsed.video_codec;
        }
        if technical.container.is_none() {
            technical.container = Some(parsed.container);
        }

        let artwork = self.sidecar_scanner.find_artwork(path);
        let nfo = self.sidecar_scanner.find_nfo_for_media(path)?;

        let mut final_title = parsed.title;
        let mut final_year = parsed.year;
        let mut original_title = None;
        let mut meta = MediaMetadata::default();

        if let Some(nfo_data) = nfo {
            if let Some(t) = nfo_data.title {
                final_title = t;
            }
            if let Some(y) = nfo_data.year {
                final_year = Some(y);
            }
            original_title = nfo_data.original_title;
            meta.overview = nfo_data.overview;
            meta.director = nfo_data.director;
            meta.studio = nfo_data.studio;
            meta.actors = nfo_data.actors;
            meta.tags = nfo_data.tags;
        }

        meta.release_group = parsed.release_group;
        meta.poster_path = artwork.poster.and_then(|p| p.to_str().map(String::from));
        meta.backdrop_path = artwork.backdrop.and_then(|p| p.to_str().map(String::from));

        Ok(Some(MediaItem {
            id: None,
            library_id: library.id.clone(),
            item_type: library.media_type,
            title: final_title,
            original_title,
            release_year: final_year,
            added_at,
            file_path: path.to_path_buf(),
            file_name: filename.to_string(),
            file_size,
            technical,
            metadata: meta,
        }))
    }
}
```

Create `crates/kadr-ingest/src/watcher/worker.rs`:
```rust
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info};
use kadr_core::models::MediaItem;
use kadr_storage::repos::MediaItemRepository;

pub enum IngestMessage {
    Upsert(MediaItem),
    Delete(PathBuf),
}

pub struct IngestWorker {
    rx: mpsc::Receiver<IngestMessage>,
    repo: MediaItemRepository,
}

impl IngestWorker {
    pub fn new(rx: mpsc::Receiver<IngestMessage>, repo: MediaItemRepository) -> Self {
        Self { rx, repo }
    }

    pub async fn run(mut self) {
        let mut batch = Vec::with_capacity(50);

        loop {
            tokio::select! {
                maybe_msg = self.rx.recv() => {
                    match maybe_msg {
                        Some(IngestMessage::Upsert(item)) => {
                            batch.push(item);
                            if batch.len() >= 50 {
                                self.flush_batch(&mut batch).await;
                            }
                        }
                        Some(IngestMessage::Delete(path)) => {
                            if let Err(e) = self.repo.delete_by_path(&path).await {
                                error!(path = ?path, error = ?e, "Failed to delete removed media file");
                            }
                        }
                        None => {
                            // Channel closed, flush remaining items and exit
                            if !batch.is_empty() {
                                self.flush_batch(&mut batch).await;
                            }
                            break;
                        }
                    }
                }
                _ = sleep(Duration::from_millis(100)), if !batch.is_empty() => {
                    self.flush_batch(&mut batch).await;
                }
            }
        }
    }

    async fn flush_batch(&self, batch: &mut Vec<MediaItem>) {
        if batch.is_empty() {
            return;
        }
        match self.repo.upsert_batch(batch).await {
            Ok(count) => {
                info!(count = count, "Successfully ingested media batch");
            }
            Err(e) => {
                error!(error = ?e, "Error flushing media batch to database");
            }
        }
        batch.clear();
    }
}
```

Create `crates/kadr-ingest/src/watcher/debouncer.rs`:
```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct DebounceQueue {
    quiet_duration: Duration,
    pending: HashMap<PathBuf, Instant>,
}

impl DebounceQueue {
    pub fn new(quiet_duration: Duration) -> Self {
        Self {
            quiet_duration,
            pending: HashMap::new(),
        }
    }

    pub fn record_event(&mut self, path: PathBuf) {
        self.pending.insert(path, Instant::now());
    }

    pub fn remove(&mut self, path: &PathBuf) {
        self.pending.remove(path);
    }

    pub fn extract_settled(&mut self) -> Vec<PathBuf> {
        let now = Instant::now();
        let mut settled = Vec::new();

        self.pending.retain(|path, last_seen| {
            if now.duration_since(*last_seen) >= self.quiet_duration {
                settled.push(path.clone());
                false
            } else {
                true
            }
        });

        settled
    }
}
```

Create `crates/kadr-ingest/src/watcher/fs_watcher.rs`:
```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use kadr_core::models::Library;
use crate::watcher::debouncer::DebounceQueue;
use crate::watcher::pipeline::IngestPipeline;
use crate::watcher::worker::IngestMessage;

pub fn scan_directory_recursive<P: AsRef<Path>>(dir: P) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(scan_directory_recursive(path));
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ["mkv", "mp4", "webm", "avi"].contains(&ext.to_lowercase().as_str()) {
                        files.push(path);
                    }
                }
            }
        }
    }
    files
}

pub async fn start_library_watcher(
    library: Library,
    pipeline: Arc<IngestPipeline>,
    tx: mpsc::Sender<IngestMessage>,
    debounce_duration: Duration,
) -> notify::Result<RecommendedWatcher> {
    let (notify_tx, mut notify_rx) = mpsc::channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = notify_tx.blocking_send(event);
            }
        },
        Config::default(),
    )?;

    watcher.watch(&library.path, RecursiveMode::Recursive)?;

    // Initial reconciliation scan
    let lib_clone = library.clone();
    let pipe_clone = pipeline.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let initial_files = scan_directory_recursive(&lib_clone.path);
        info!(library = %lib_clone.name, count = initial_files.len(), "Running startup library scan");
        for file in initial_files {
            if let Ok(Some(item)) = pipe_clone.process_file(&lib_clone, &file).await {
                let _ = tx_clone.send(IngestMessage::Upsert(item)).await;
            }
        }
    });

    // Reactive watcher task with debouncing
    tokio::spawn(async move {
        let mut debouncer = DebounceQueue::new(debounce_duration);
        let mut interval = tokio::time::interval(Duration::from_millis(200));

        loop {
            tokio::select! {
                Some(event) = notify_rx.recv() => {
                    for path in event.paths {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => {
                                debouncer.record_event(path);
                            }
                            EventKind::Remove(_) => {
                                debouncer.remove(&path);
                                let _ = tx.send(IngestMessage::Delete(path)).await;
                            }
                            _ => {}
                        }
                    }
                }
                _ = interval.tick() => {
                    let settled = debouncer.extract_settled();
                    for path in settled {
                        if path.is_file() {
                            match pipeline.process_file(&library, &path).await {
                                Ok(Some(item)) => {
                                    let _ = tx.send(IngestMessage::Upsert(item)).await;
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    warn!(path = ?path, error = ?e, "Failed to ingest media file");
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(watcher)
}
```

Create `crates/kadr-ingest/src/watcher/mod.rs`:
```rust
pub mod debouncer;
pub mod fs_watcher;
pub mod pipeline;
pub mod worker;

pub use debouncer::DebounceQueue;
pub use fs_watcher::{scan_directory_recursive, start_library_watcher};
pub use pipeline::IngestPipeline;
pub use worker::{IngestMessage, IngestWorker};
```

Update `crates/kadr-ingest/src/lib.rs` to expose `watcher`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-ingest`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest
git commit -m "feat(ingest): implement debounced filesystem watcher and batch ingestion worker"
```

---

### Task 8: Server Configuration, Bootstrap & Lifecycle (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/Cargo.toml`
- Create: `crates/kadr-server/src/config.rs`
- Create: `crates/kadr-server/src/main.rs`
- Create: `kadr.toml`
- Test: `crates/kadr-server/tests/config_test.rs`

**Interfaces:**
- Consumes: `kadr-core`, `kadr-storage`, `kadr-ingest`
- Produces: `kadr` server executable

- [ ] **Step 1: Write failing configuration load test**

```rust
// crates/kadr-server/tests/config_test.rs
use kadr_server::config::AppConfig;

#[test]
fn test_config_parsing_from_str() {
    let toml_str = r#"
        [server]
        host = "127.0.0.1"
        port = 8096
        data_dir = "./test_data"

        [storage]
        database_path = "./test_data/kadr.db"
        max_readers = 4

        [scanner]
        debounce_millis = 500
        use_ffprobe = false

        [[libraries]]
        id = "movies"
        name = "Movies"
        path = "./test_media"
        media_type = "Movie"
    "#;

    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.server.port, 8096);
    assert_eq!(config.storage.max_readers, 4);
    assert_eq!(config.libraries.len(), 1);
    assert_eq!(config.libraries[0].id, "movies");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server`
Expected: FAIL (crate not found)

- [ ] **Step 3: Implement `kadr-server` configuration and bootstrap**

Create `crates/kadr-server/Cargo.toml`:
```toml
[package]
name = "kadr-server"
version = "0.1.0"
edition = "2021"

[dependencies]
kadr-core = { path = "../kadr-core" }
kadr-storage = { path = "../kadr-storage" }
kadr-ingest = { path = "../kadr-ingest" }
clap = { version = "4.5", features = ["derive"] }
toml = "0.8"
serde = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

[dev-dependencies]
tokio = { version = "1.43", features = ["full", "macros"] }
```

Create `crates/kadr-server/src/config.rs`:
```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use kadr_core::models::MediaType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    #[serde(default = "default_db_path")]
    pub database_path: PathBuf,
    #[serde(default = "default_max_readers")]
    pub max_readers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerSettings {
    #[serde(default = "default_debounce")]
    pub debounce_millis: u64,
    #[serde(default = "default_ffprobe")]
    pub use_ffprobe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConfig {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub media_type: MediaType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub storage: StorageSettings,
    #[serde(default)]
    pub scanner: ScannerSettings,
    #[serde(default)]
    pub libraries: Vec<LibraryConfig>,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 8096 }
fn default_data_dir() -> PathBuf { PathBuf::from("./data") }
fn default_db_path() -> PathBuf { PathBuf::from("./data/kadr.db") }
fn default_max_readers() -> usize { 4 }
fn default_debounce() -> u64 { 500 }
fn default_ffprobe() -> bool { true }

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            data_dir: default_data_dir(),
        }
    }
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            database_path: default_db_path(),
            max_readers: default_max_readers(),
        }
    }
}

impl Default for ScannerSettings {
    fn default() -> Self {
        Self {
            debounce_millis: default_debounce(),
            use_ffprobe: default_ffprobe(),
        }
    }
}

impl AppConfig {
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
```

Create `crates/kadr-server/src/main.rs`:
```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod config;
use config::AppConfig;
use kadr_core::models::Library;
use kadr_ingest::watcher::{start_library_watcher, IngestPipeline, IngestWorker};
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

#[derive(Parser, Debug)]
#[command(name = "kadr", about = "Kadr Media Server")]
struct Args {
    #[arg(short, long, default_value = "kadr.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kadr=info,kadr_server=info,kadr_ingest=info,kadr_storage=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    info!(config = ?args.config, "Starting Kadr media server");

    let config = if args.config.exists() {
        AppConfig::load_from_file(&args.config)?
    } else {
        info!("Config file not found, using defaults");
        AppConfig {
            server: Default::default(),
            storage: Default::default(),
            scanner: Default::default(),
            libraries: vec![],
        }
    };

    if let Some(parent) = config.storage.database_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    info!(db = ?config.storage.database_path, "Initializing SQLite storage");
    let pool = create_pool(&config.storage.database_path, config.storage.max_readers)?;
    initialize_database(&pool).await?;

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // Sync bootstrap libraries
    for lib_cfg in &config.libraries {
        let lib = Library {
            id: lib_cfg.id.clone(),
            name: lib_cfg.name.clone(),
            path: lib_cfg.path.clone(),
            media_type: lib_cfg.media_type,
            created_at: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() as i64,
        };
        lib_repo.create(&lib).await?;
    }

    let active_libraries = lib_repo.get_all().await?;
    info!(count = active_libraries.len(), "Loaded registered libraries");

    let (ingest_tx, ingest_rx) = tokio::sync::mpsc::channel(200);
    let worker = IngestWorker::new(ingest_rx, media_repo);
    let worker_handle = tokio::spawn(worker.run());

    let pipeline = Arc::new(IngestPipeline::new(config.scanner.use_ffprobe));
    let mut watchers = Vec::new();

    for lib in active_libraries {
        if lib.path.exists() {
            info!(library = %lib.name, path = ?lib.path, "Starting filesystem watcher");
            let watcher = start_library_watcher(
                lib,
                pipeline.clone(),
                ingest_tx.clone(),
                Duration::from_millis(config.scanner.debounce_millis),
            ).await?;
            watchers.push(watcher);
        } else {
            error!(library = %lib.name, path = ?lib.path, "Library path does not exist, skipping watcher");
        }
    }

    info!("Kadr Milestone 1 Core & Storage ready. Press Ctrl+C to terminate.");
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received. Stopping watchers and flushing storage...");

    drop(watchers);
    drop(ingest_tx);
    let _ = worker_handle.await;

    info!("Kadr server terminated cleanly.");
    Ok(())
}
```

Create root default `kadr.toml`:
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p kadr-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server kadr.toml
git commit -m "feat(server): implement configuration loading, bootstrap, and lifecycle runner"
```

---

### Task 9: End-to-End System Ingestion Integration Test

**Files:**
- Create: `tests/e2e_ingest_test.rs`

**Interfaces:**
- Consumes: Full workspace (`kadr-core`, `kadr-storage`, `kadr-ingest`)
- Produces: Verified end-to-end integration test

- [ ] **Step 1: Write comprehensive end-to-end ingestion test**

```rust
// tests/e2e_ingest_test.rs
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;
use kadr_core::models::{Library, MediaType};
use kadr_ingest::watcher::{start_library_watcher, IngestPipeline, IngestWorker};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

#[tokio::test]
async fn test_end_to_end_library_scan_and_reactive_ingest() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    let dir = tempdir().unwrap();
    let library = Library {
        id: "classics".to_string(),
        name: "Classic Cinema".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    };
    lib_repo.create(&library).await.unwrap();

    // 1. Pre-create a file before watcher starts
    let movie1 = dir.path().join("The.Flirtation.of.Girls.1949.1080p.BluRay.x264-Scene.mkv");
    let mut f1 = File::create(&movie1).unwrap();
    f1.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00]).unwrap();

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let worker = IngestWorker::new(rx, media_repo.clone());
    let _worker_handle = tokio::spawn(worker.run());

    let pipeline = Arc::new(IngestPipeline::new(false));
    let _watcher = start_library_watcher(
        library.clone(),
        pipeline.clone(),
        tx.clone(),
        Duration::from_millis(100),
    ).await.unwrap();

    // Wait for initial scan to flush to DB
    sleep(Duration::from_millis(500)).await;

    let items = media_repo.list_by_library("classics", 10, 0).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "The Flirtation of Girls");
    assert_eq!(items[0].release_year, Some(1949));
    assert_eq!(items[0].technical.resolution.as_deref(), Some("1080p"));

    // 2. Reactively add a new movie with an .nfo sidecar
    let movie2 = dir.path().join("Terror.and.Kebab.1992.mkv");
    let nfo2 = dir.path().join("Terror.and.Kebab.1992.nfo");

    let mut f2 = File::create(&movie2).unwrap();
    f2.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00]).unwrap();

    let mut n2 = File::create(&nfo2).unwrap();
    n2.write_all(r#"<movie><director>Sherif Arafa</director><actor><name>Adel Emam</name></actor></movie>"#.as_bytes()).unwrap();

    // Wait for debouncer (100ms) + batch flush (100ms)
    sleep(Duration::from_millis(600)).await;

    let all_items = media_repo.list_by_library("classics", 10, 0).await.unwrap();
    assert_eq!(all_items.len(), 2);

    let second_item = all_items.iter().find(|i| i.title == "Terror and Kebab").expect("item not found");
    assert_eq!(second_item.release_year, Some(1992));
    assert_eq!(second_item.metadata.director.as_deref(), Some("Sherif Arafa"));
    assert_eq!(second_item.metadata.actors, vec!["Adel Emam"]);
}
```

- [ ] **Step 2: Run end-to-end integration test**

Run: `cargo test --test e2e_ingest_test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/e2e_ingest_test.rs
git commit -m "test: add end-to-end library ingestion and reactive watcher test"
```
