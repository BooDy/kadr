# Plugin Framework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Kadr host out-of-process HTTP plugins that publish virtual (file-less) media items and resolve playback for them (`direct`, `proxy`, `transcode`).

**Architecture:** A new `plugins/` module in `kadr-server` owns registration, scoped auth, an HTTP client with a circuit breaker, publishing routes and playback resolution. Plugin items reuse the existing `media_items` table via a synthetic `plugin://<plugin_id>/<remote_id>` path, so no existing struct literal or upsert changes. Playback hooks are added next to the existing file-based code paths, and the `Path`-based HLS functions stay as thin wrappers.

**Tech Stack:** Rust 2021, axum 0.8, rusqlite via deadpool_sqlite, reqwest 0.12 (rustls), jsonwebtoken 9, hmac and sha2 and base64, tokio. Spec: `docs/superpowers/specs/2026-11-07-plugin-framework-design.md`.

## Global Constraints

- Plugins are separate processes reached over HTTP. The server never starts or supervises them.
- `[plugins]` in `kadr.toml`: `enabled` (default `false`), `resolve_timeout_secs` (default `5`), `allow_private_proxy` (default `false`). Individual plugins are NOT listed in `kadr.toml`.
- Plugin API version: `api_version = 1`. Permissions: `items:publish`, `playback:resolve`.
- Error codes: `503 PLUGIN_UNAVAILABLE`, `502 PLUGIN_BAD_RESPONSE`, `504 PLUGIN_TIMEOUT`.
- Server-to-plugin secret: `base64url_nopad(HMAC-SHA256(server_secret, "kadr-plugin-secret-v1:" + plugin_id))`. It is never stored.
- Do NOT add fields to `MediaItem` or `Library` (about 47 exhaustive struct literals exist). Plugin items are identified by `file_path` = `plugin://<plugin_id>/<remote_id>`. `live` and `seekable` live in `metadata.extra`.
- Existing behavior for file items must not change. Run `cargo test --workspace` after every task.
- Descriptor URLs must be `http` or `https`. `proxy` and `transcode` URLs must not resolve to loopback or private ranges unless `allow_private_proxy = true`.
- Release builds target static musl: add no native-library dependencies. Only add crates already in `Cargo.lock` (`reqwest::Url` re-exports `url`).
- Commit after every task with a conventional-commit message.

## File Structure

| File | Responsibility |
|---|---|
| `crates/kadr-core/src/plugin.rs` (new) | Plugin domain types, `plugin://` path helpers, `MediaItem` accessors, playback descriptor and source types |
| `crates/kadr-core/src/models.rs` | Add `MediaType::LiveChannel` |
| `crates/kadr-core/src/transcoding/builder.rs` | `build_ffmpeg_hls_args_for_source`; old function becomes a wrapper |
| `crates/kadr-core/src/events.rs` | `SystemEvent::PluginError` |
| `crates/kadr-storage/src/migrations/007_plugins.sql` (new) | `plugins` table and `libraries.plugin_id` |
| `crates/kadr-storage/src/repos/plugin_repo.rs` (new) | `PluginRepository` |
| `crates/kadr-storage/src/repos/library_repo.rs` | Virtual-library methods |
| `crates/kadr-server/src/config.rs` | `PluginSettings` |
| `crates/kadr-server/src/streaming/hls/session.rs` | `get_or_create_session_for_source` |
| `crates/kadr-server/src/plugins/mod.rs` (new) | Module wiring and `plugin_routes()` |
| `crates/kadr-server/src/plugins/auth.rs` (new) | Plugin tokens, derived secret, `PluginAuth` extractor |
| `crates/kadr-server/src/plugins/url_guard.rs` (new) | Descriptor and SSRF validation |
| `crates/kadr-server/src/plugins/client.rs` (new) | `PluginClient`, `CircuitBreaker`, `PluginError` |
| `crates/kadr-server/src/plugins/service.rs` (new) | `PluginService` (lifecycle, authorization, resolve and cache) |
| `crates/kadr-server/src/plugins/admin_routes.rs` (new) | Admin registration and approval API |
| `crates/kadr-server/src/plugins/publish_routes.rs` (new) | Plugin-to-server publishing API |
| `crates/kadr-server/src/plugins/playback.rs` (new) | Stream, proxy and HLS integration helpers |
| `crates/kadr-server/src/plugins/health.rs` (new) | Health monitor |
| `crates/kadr-server/src/api/mod.rs`, `streaming/handler.rs`, `api/hls_routes.rs`, `playback/handler.rs`, `resolver/card.rs`, `main.rs`, `lib.rs` | Wiring and playback branches |
| `crates/kadr-server/tests/plugin_support/mod.rs` (new) | Shared test harness and mock plugin |
| `examples/plugin-hello/` (new, outside the workspace) | Example plugin |
| `docs/PLUGIN_GUIDE.md` (new), `CHANGELOG.md` | Docs |

---

### Task 1: Storage — migration, `PluginRepository`, virtual-library methods

**Files:**
- Create: `crates/kadr-storage/src/migrations/007_plugins.sql`
- Create: `crates/kadr-storage/src/repos/plugin_repo.rs`
- Create: `crates/kadr-core/src/plugin.rs` (domain types only in this task; helpers come in Task 2)
- Modify: `crates/kadr-core/src/lib.rs`, `crates/kadr-storage/src/migrations.rs`, `crates/kadr-storage/src/repos/mod.rs`, `crates/kadr-storage/src/lib.rs`, `crates/kadr-storage/src/repos/library_repo.rs`
- Test: `crates/kadr-storage/tests/plugin_repo_test.rs`

**Interfaces:**
- Produces (kadr-core): `PluginPermission`, `PluginStatus`, `PluginCapabilities`, `PluginManifest`, `PluginRecord` (all in `kadr_core::plugin`, re-exported at crate root).
- Produces (kadr-storage):
  - `PluginRepository::{new(Pool), insert(&PluginRecord), get(&str) -> Result<Option<PluginRecord>>, list() -> Result<Vec<PluginRecord>>, set_status(&str, PluginStatus) -> Result<bool>, set_granted(&str, &[PluginPermission]) -> Result<bool>, delete(&str) -> Result<bool>}`
  - `LibraryRepository::{create_virtual(plugin_id: &str, id: &str, name: &str, media_type: MediaType) -> Result<Library>, plugin_id_of(&str) -> Result<Option<String>>, delete_by_plugin(&str) -> Result<usize>}`

- [ ] **Step 1: Create the core domain types**

Create `crates/kadr-core/src/plugin.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const SUPPORTED_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginPermission {
    #[serde(rename = "items:publish")]
    ItemsPublish,
    #[serde(rename = "playback:resolve")]
    PlaybackResolve,
}

impl PluginPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ItemsPublish => "items:publish",
            Self::PlaybackResolve => "playback:resolve",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Pending,
    Enabled,
    Disabled,
    Unhealthy,
}

impl PluginStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Unhealthy => "unhealthy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "enabled" => Some(Self::Enabled),
            "disabled" => Some(Self::Disabled),
            "unhealthy" => Some(Self::Unhealthy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub virtual_library: bool,
    #[serde(default)]
    pub playback_resolver: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
}

impl PluginManifest {
    /// Validates id format (`[a-z0-9-]{1,32}`), name, and API version.
    pub fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 32
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err("manifest id must match [a-z0-9-]{1,32}".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("manifest name must not be empty".to_string());
        }
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(format!(
                "unsupported api_version {} (supported: {})",
                self.api_version, SUPPORTED_API_VERSION
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub manifest: PluginManifest,
    pub granted_permissions: Vec<PluginPermission>,
    pub status: PluginStatus,
    pub created_at: i64,
}

impl PluginRecord {
    pub fn has_permission(&self, perm: PluginPermission) -> bool {
        self.granted_permissions.contains(&perm)
    }
}
```

In `crates/kadr-core/src/lib.rs` add `pub mod plugin;` (after `pub mod models;`) and `pub use plugin::*;` (after `pub use models::*;`).

- [ ] **Step 2: Write the failing storage tests**

Create `crates/kadr-storage/tests/plugin_repo_test.rs`:

```rust
use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::{
    PluginCapabilities, PluginManifest, PluginPermission, PluginRecord, PluginStatus,
};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PluginRepository};
use kadr_storage::{create_in_memory_pool, initialize_database};

fn record(id: &str) -> PluginRecord {
    PluginRecord {
        id: id.to_string(),
        name: format!("Plugin {id}"),
        base_url: "http://127.0.0.1:9000".to_string(),
        manifest: PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {id}"),
            version: "0.1.0".to_string(),
            api_version: 1,
            permissions: vec![PluginPermission::ItemsPublish],
            capabilities: PluginCapabilities::default(),
        },
        granted_permissions: vec![],
        status: PluginStatus::Pending,
        created_at: 1_700_000_000,
    }
}

async fn setup() -> (PluginRepository, LibraryRepository, MediaItemRepository) {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    (
        PluginRepository::new(pool.clone()),
        LibraryRepository::new(pool.clone()),
        MediaItemRepository::new(pool),
    )
}

#[tokio::test]
async fn plugin_insert_get_list_roundtrip() {
    let (repo, _, _) = setup().await;
    repo.insert(&record("iptv")).await.unwrap();
    repo.insert(&record("hello")).await.unwrap();

    let got = repo.get("iptv").await.unwrap().unwrap();
    assert_eq!(got, record("iptv"));
    assert!(repo.get("missing").await.unwrap().is_none());

    let ids: Vec<String> = repo.list().await.unwrap().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, vec!["hello".to_string(), "iptv".to_string()]);
}

#[tokio::test]
async fn plugin_status_and_permissions_update_and_delete() {
    let (repo, _, _) = setup().await;
    repo.insert(&record("iptv")).await.unwrap();

    assert!(repo.set_status("iptv", PluginStatus::Enabled).await.unwrap());
    assert!(repo
        .set_granted("iptv", &[PluginPermission::ItemsPublish])
        .await
        .unwrap());
    let got = repo.get("iptv").await.unwrap().unwrap();
    assert_eq!(got.status, PluginStatus::Enabled);
    assert_eq!(got.granted_permissions, vec![PluginPermission::ItemsPublish]);

    assert!(!repo.set_status("nope", PluginStatus::Enabled).await.unwrap());
    assert!(repo.delete("iptv").await.unwrap());
    assert!(!repo.delete("iptv").await.unwrap());
}

#[tokio::test]
async fn virtual_library_lifecycle_and_purge() {
    let (_, libs, media) = setup().await;

    let lib = libs
        .create_virtual("iptv", "lib-1", "Live TV", MediaType::LiveChannel)
        .await
        .unwrap();
    assert_eq!(lib.path.to_string_lossy(), "plugin://iptv/library/lib-1");
    assert_eq!(libs.plugin_id_of("lib-1").await.unwrap().as_deref(), Some("iptv"));

    // Regular libraries have no plugin owner.
    libs.create("real", "Real", "/tmp/real", MediaType::Movie, false, None)
        .await
        .unwrap();
    assert_eq!(libs.plugin_id_of("real").await.unwrap(), None);
    assert_eq!(libs.plugin_id_of("absent").await.unwrap(), None);

    // The virtual library is returned by get_all and get_by_id.
    assert!(libs.get_by_id("lib-1").await.unwrap().is_some());
    assert!(libs.get_all().await.unwrap().iter().any(|l| l.id == "lib-1"));

    let item = MediaItem {
        library_id: "lib-1".to_string(),
        item_type: MediaType::LiveChannel,
        title: "Channel 1".to_string(),
        file_path: "plugin://iptv/ch1".into(),
        file_name: "ch1".to_string(),
        added_at: 1,
        ..Default::default()
    };
    media.upsert_batch(&[item]).await.unwrap();
    assert_eq!(media.count_by_library("lib-1").await.unwrap(), 1);

    let removed = libs.delete_by_plugin("iptv").await.unwrap();
    assert_eq!(removed, 1);
    assert!(libs.get_by_id("lib-1").await.unwrap().is_none());
    assert_eq!(media.count_by_library("lib-1").await.unwrap(), 0);
    // Regular library untouched.
    assert!(libs.get_by_id("real").await.unwrap().is_some());
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p kadr-storage --test plugin_repo_test`
Expected: FAIL to compile (`PluginRepository` and `create_virtual` do not exist).

- [ ] **Step 4: Add the migration**

Create `crates/kadr-storage/src/migrations/007_plugins.sql`:

```sql
CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    manifest TEXT NOT NULL,
    granted_permissions TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL
);

ALTER TABLE libraries ADD COLUMN plugin_id TEXT DEFAULT NULL;
CREATE INDEX IF NOT EXISTS idx_libraries_plugin ON libraries(plugin_id);
```

In `crates/kadr-storage/src/migrations.rs` append to the `vec![...]`:

```rust
        M::up(include_str!("migrations/007_plugins.sql")),
```

- [ ] **Step 5: Implement `PluginRepository`**

Create `crates/kadr-storage/src/repos/plugin_repo.rs`:

```rust
use crate::error::Result;
use deadpool_sqlite::Pool;
use kadr_core::plugin::{PluginManifest, PluginPermission, PluginRecord, PluginStatus};
use rusqlite::params;

#[derive(Clone)]
pub struct PluginRepository {
    pool: Pool,
}

fn conversion_err<E: std::error::Error + Send + Sync + 'static>(
    idx: usize,
    e: E,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<PluginRecord> {
    let manifest_s: String = row.get(3)?;
    let perms_s: String = row.get(4)?;
    let status_s: String = row.get(5)?;
    let manifest: PluginManifest =
        serde_json::from_str(&manifest_s).map_err(|e| conversion_err(3, e))?;
    let granted_permissions: Vec<PluginPermission> =
        serde_json::from_str(&perms_s).map_err(|e| conversion_err(4, e))?;
    let status = PluginStatus::parse(&status_s).unwrap_or(PluginStatus::Disabled);
    Ok(PluginRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        manifest,
        granted_permissions,
        status,
        created_at: row.get(6)?,
    })
}

const COLUMNS: &str = "id, name, base_url, manifest, granted_permissions, status, created_at";

impl PluginRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, rec: &PluginRecord) -> Result<()> {
        let rec = rec.clone();
        let manifest = serde_json::to_string(&rec.manifest)?;
        let perms = serde_json::to_string(&rec.granted_permissions)?;
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO plugins (id, name, base_url, manifest, granted_permissions, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    rec.id,
                    rec.name,
                    rec.base_url,
                    manifest,
                    perms,
                    rec.status.as_str(),
                    rec.created_at
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get(&self, id: &str) -> Result<Option<PluginRecord>> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!("SELECT {COLUMNS} FROM plugins WHERE id = ?1");
            let mut stmt = c.prepare(&sql)?;
            let mut rows = stmt.query(params![id])?;
            match rows.next()? {
                Some(row) => Ok(Some(map_row(row)?)),
                None => Ok(None),
            }
        })
        .await?
    }

    pub async fn list(&self) -> Result<Vec<PluginRecord>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!("SELECT {COLUMNS} FROM plugins ORDER BY id ASC");
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map([], map_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await?
    }

    pub async fn set_status(&self, id: &str, status: PluginStatus) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute(
                "UPDATE plugins SET status = ?1 WHERE id = ?2",
                params![status.as_str(), id],
            )?;
            Ok(n > 0)
        })
        .await?
    }

    pub async fn set_granted(&self, id: &str, granted: &[PluginPermission]) -> Result<bool> {
        let id = id.to_string();
        let perms = serde_json::to_string(granted)?;
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute(
                "UPDATE plugins SET granted_permissions = ?1 WHERE id = ?2",
                params![perms, id],
            )?;
            Ok(n > 0)
        })
        .await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute("DELETE FROM plugins WHERE id = ?1", params![id])?;
            Ok(n > 0)
        })
        .await?
    }
}
```

In `crates/kadr-storage/src/repos/mod.rs` add `pub mod plugin_repo;` and `pub use plugin_repo::PluginRepository;`. In `crates/kadr-storage/src/lib.rs` add `PluginRepository` to the `pub use repos::{...}` list.

- [ ] **Step 6: Add the virtual-library methods**

In `crates/kadr-storage/src/repos/library_repo.rs`, add inside `impl LibraryRepository` (before the final closing brace):

```rust
    /// Creates a library owned by a plugin. It has no real directory: its `path` is the
    /// synthetic `plugin://<plugin_id>/library/<id>` and no `library_paths` row is written.
    pub async fn create_virtual(
        &self,
        plugin_id: &str,
        id: &str,
        name: &str,
        media_type: MediaType,
    ) -> Result<Library> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let path = PathBuf::from(format!("plugin://{plugin_id}/library/{id}"));
        let lib = Library {
            id: id.to_string(),
            name: name.to_string(),
            path: path.clone(),
            paths: vec![path],
            media_type,
            is_private: false,
            pin_hash: None,
            created_at: now,
        };
        let row = lib.clone();
        let plugin_id = plugin_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO libraries (id, name, path, media_type, is_private, pin_hash, created_at, plugin_id)
                 VALUES (?1, ?2, ?3, ?4, 0, NULL, ?5, ?6)",
                params![
                    row.id,
                    row.name,
                    row.path.to_string_lossy().into_owned(),
                    serde_json::to_string(&row.media_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                    row.created_at,
                    plugin_id,
                ],
            )?;
            Ok(())
        })
        .await??;
        Ok(lib)
    }

    /// Returns the owning plugin id for a virtual library, or `None` for regular/unknown libraries.
    pub async fn plugin_id_of(&self, library_id: &str) -> Result<Option<String>> {
        let library_id = library_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT plugin_id FROM libraries WHERE id = ?1")?;
            let mut rows = stmt.query(params![library_id])?;
            match rows.next()? {
                Some(row) => Ok(row.get::<_, Option<String>>(0)?),
                None => Ok(None),
            }
        })
        .await?
    }

    /// Deletes every library owned by the plugin (items cascade). Returns the number removed.
    pub async fn delete_by_plugin(&self, plugin_id: &str) -> Result<usize> {
        let plugin_id = plugin_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "DELETE FROM library_paths WHERE library_id IN
                 (SELECT id FROM libraries WHERE plugin_id = ?1)",
                params![plugin_id],
            )?;
            let n = tx.execute("DELETE FROM libraries WHERE plugin_id = ?1", params![plugin_id])?;
            tx.commit()?;
            Ok(n)
        })
        .await?
    }
```

- [ ] **Step 7: Add `MediaType::LiveChannel` (needed by the test)**

In `crates/kadr-core/src/models.rs`, in `enum MediaType`, add before `#[default] Unknown`:

```rust
    #[serde(alias = "LiveChannel", alias = "LIVE_CHANNEL")]
    LiveChannel,
```

In `crates/kadr-server/src/resolver/card.rs`, in the match that maps `MediaType` to strings (around line 69-77), add the arm:

```rust
        MediaType::LiveChannel => "live_channel",
```

- [ ] **Step 8: Run the tests**

Run: `cargo test -p kadr-storage --test plugin_repo_test && cargo test -p kadr-storage && cargo build --workspace`
Expected: all PASS and the workspace compiles. If a migration test asserts a fixed schema version (for example `migration_test.rs`), update the expected count from 6 to 7.

- [ ] **Step 9: Commit**

```bash
git add crates/kadr-core crates/kadr-storage crates/kadr-server/src/resolver/card.rs
git commit -m "feat(storage): add plugins table, plugin repository and virtual libraries"
```

---

### Task 2: Core — `plugin://` helpers, `MediaItem` accessors, playback types

**Files:**
- Modify: `crates/kadr-core/src/plugin.rs`
- Test: `crates/kadr-core/tests/plugin_types_test.rs`

**Interfaces:**
- Consumes: Task 1 types, `MediaItem`.
- Produces:
  - `PLUGIN_SCHEME: &str = "plugin://"`
  - `PluginItemRef { plugin_id: String, remote_id: String }`
  - `plugin_item_path(plugin_id: &str, remote_id: &str) -> PathBuf`
  - `parse_plugin_path(&Path) -> Option<PluginItemRef>`
  - `is_plugin_path(&Path) -> bool`
  - `validate_remote_id(&str) -> Result<(), String>`
  - `MediaItem::{plugin_ref() -> Option<PluginItemRef>, is_live() -> bool, is_seekable() -> bool}`
  - `PlaybackMode { Direct, Proxy, Transcode }`
  - `PlaybackDescriptor { mode, url, headers: BTreeMap<String,String>, expires_in_secs: Option<u64>, content_type: Option<String> }`
  - `PlaybackSource::{File(PathBuf), Url { url: String, headers: BTreeMap<String, String> }}`
  - `ClientCapabilities { codecs: Vec<String>, containers: Vec<String>, max_height: Option<u32> }`
  - `ResolveRequest { remote_id: String, user_id: String, client: ClientCapabilities }`

- [ ] **Step 1: Write the failing tests**

Create `crates/kadr-core/tests/plugin_types_test.rs`:

```rust
use kadr_core::models::{MediaItem, MediaMetadata};
use kadr_core::plugin::*;
use std::path::{Path, PathBuf};

#[test]
fn plugin_path_roundtrip_and_remote_id_may_contain_slashes() {
    let p = plugin_item_path("iptv", "group/ch 1");
    assert_eq!(p, PathBuf::from("plugin://iptv/group/ch 1"));
    let r = parse_plugin_path(&p).unwrap();
    assert_eq!(r.plugin_id, "iptv");
    assert_eq!(r.remote_id, "group/ch 1");
}

#[test]
fn parse_rejects_non_plugin_and_malformed_paths() {
    assert!(parse_plugin_path(Path::new("/media/movie.mkv")).is_none());
    assert!(parse_plugin_path(Path::new("plugin://iptv")).is_none());
    assert!(parse_plugin_path(Path::new("plugin://iptv/")).is_none());
    assert!(parse_plugin_path(Path::new("plugin:///abc")).is_none());
    assert!(is_plugin_path(Path::new("plugin://iptv/library/x")));
    assert!(!is_plugin_path(Path::new("/plugin://x")));
}

#[test]
fn remote_id_validation() {
    assert!(validate_remote_id("abc-123").is_ok());
    assert!(validate_remote_id("").is_err());
    assert!(validate_remote_id("bad\nid").is_err());
    assert!(validate_remote_id(&"x".repeat(257)).is_err());
}

#[test]
fn media_item_accessors() {
    let mut item = MediaItem::default();
    assert!(item.plugin_ref().is_none());
    assert!(!item.is_live());
    assert!(item.is_seekable());

    item.file_path = plugin_item_path("iptv", "ch1");
    let mut meta = MediaMetadata::default();
    meta.extra.insert("live".into(), serde_json::json!(true));
    item.metadata = meta;
    assert_eq!(item.plugin_ref().unwrap().remote_id, "ch1");
    assert!(item.is_live());
    assert!(!item.is_seekable(), "live items are never seekable");

    let mut vod = MediaItem::default();
    vod.metadata.extra.insert("seekable".into(), serde_json::json!(false));
    assert!(!vod.is_seekable());
}

#[test]
fn manifest_validation() {
    let ok = PluginManifest {
        id: "iptv".into(),
        name: "IPTV".into(),
        version: "0.1.0".into(),
        api_version: 1,
        permissions: vec![],
        capabilities: Default::default(),
    };
    assert!(ok.validate().is_ok());
    let mut bad = ok.clone();
    bad.id = "Has Space".into();
    assert!(bad.validate().is_err());
    let mut bad = ok.clone();
    bad.api_version = 2;
    assert!(bad.validate().is_err());
    let mut bad = ok;
    bad.name = "  ".into();
    assert!(bad.validate().is_err());
}

#[test]
fn descriptor_and_manifest_json_shapes() {
    let m: PluginManifest = serde_json::from_str(
        r#"{"id":"iptv","name":"IPTV","version":"0.1.0","api_version":1,
            "permissions":["items:publish","playback:resolve"],
            "capabilities":{"virtual_library":true,"playback_resolver":true}}"#,
    )
    .unwrap();
    assert_eq!(
        m.permissions,
        vec![PluginPermission::ItemsPublish, PluginPermission::PlaybackResolve]
    );

    let d: PlaybackDescriptor =
        serde_json::from_str(r#"{"mode":"proxy","url":"https://x/y.mp4"}"#).unwrap();
    assert_eq!(d.mode, PlaybackMode::Proxy);
    assert!(d.headers.is_empty());
    assert_eq!(d.expires_in_secs, None);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-core --test plugin_types_test`
Expected: FAIL to compile (helpers missing).

- [ ] **Step 3: Implement**

At the top of `crates/kadr-core/src/plugin.rs` change the imports to:

```rust
use crate::models::MediaItem;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PLUGIN_SCHEME: &str = "plugin://";
```

Append to the file:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginItemRef {
    pub plugin_id: String,
    pub remote_id: String,
}

pub fn plugin_item_path(plugin_id: &str, remote_id: &str) -> PathBuf {
    PathBuf::from(format!("{PLUGIN_SCHEME}{plugin_id}/{remote_id}"))
}

pub fn is_plugin_path(path: &Path) -> bool {
    path.to_str()
        .map(|s| s.starts_with(PLUGIN_SCHEME))
        .unwrap_or(false)
}

/// Parses `plugin://<plugin_id>/<remote_id>`. `remote_id` is everything after the first `/`.
pub fn parse_plugin_path(path: &Path) -> Option<PluginItemRef> {
    let rest = path.to_str()?.strip_prefix(PLUGIN_SCHEME)?;
    let (plugin_id, remote_id) = rest.split_once('/')?;
    if plugin_id.is_empty() || remote_id.is_empty() {
        return None;
    }
    Some(PluginItemRef {
        plugin_id: plugin_id.to_string(),
        remote_id: remote_id.to_string(),
    })
}

pub fn validate_remote_id(remote_id: &str) -> Result<(), String> {
    if remote_id.is_empty() {
        return Err("remote_id must not be empty".to_string());
    }
    if remote_id.len() > 256 {
        return Err("remote_id must be at most 256 bytes".to_string());
    }
    if remote_id.chars().any(|c| c.is_control()) {
        return Err("remote_id must not contain control characters".to_string());
    }
    Ok(())
}

impl MediaItem {
    /// The plugin reference for items published by a plugin, `None` for file-based items.
    pub fn plugin_ref(&self) -> Option<PluginItemRef> {
        parse_plugin_path(&self.file_path)
    }

    /// True when the plugin flagged the item as a live stream (`metadata.extra.live`).
    pub fn is_live(&self) -> bool {
        self.metadata
            .extra
            .get("live")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Live items are never seekable; otherwise `metadata.extra.seekable` (default true).
    pub fn is_seekable(&self) -> bool {
        if self.is_live() {
            return false;
        }
        self.metadata
            .extra
            .get("seekable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    Direct,
    Proxy,
    Transcode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackDescriptor {
    pub mode: PlaybackMode,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub expires_in_secs: Option<u64>,
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Where the transcoder reads its input from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackSource {
    File(PathBuf),
    Url {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub codecs: Vec<String>,
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub max_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub remote_id: String,
    pub user_id: String,
    #[serde(default)]
    pub client: ClientCapabilities,
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p kadr-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-core
git commit -m "feat(core): add plugin path helpers, MediaItem accessors and playback descriptor types"
```

---

### Task 3: Transcoding input from a URL (`PlaybackSource`)

**Files:**
- Modify: `crates/kadr-core/src/transcoding/builder.rs`, `crates/kadr-server/src/streaming/hls/session.rs`
- Test: `crates/kadr-core/tests/transcoding_test.rs` (append), `crates/kadr-server/tests/hls_session_model_test.rs` (append)

**Interfaces:**
- Consumes: `kadr_core::plugin::PlaybackSource`.
- Produces:
  - `build_ffmpeg_hls_args_for_source(source: &PlaybackSource, output_dir: &Path, decision: &TranscodeDecision, params: &TranscodeParameters) -> Vec<String>`
  - `HlsSessionManager::get_or_create_session_for_source(&self, media_id: i64, source: &PlaybackSource, tech: &TechnicalInfo, params: &TranscodeParameters) -> Result<Arc<TranscodeSession>, HlsError>`
  - `build_ffmpeg_hls_args` and `get_or_create_session` keep their exact current signatures.

- [ ] **Step 1: Write the failing builder test**

Look at the first existing test in `crates/kadr-core/tests/transcoding_test.rs` (`test_build_ffmpeg_hls_args_construction`, line ~86) and reuse the same way of constructing `TranscodeDecision` and `TranscodeParameters`. Append:

```rust
#[test]
fn test_build_args_for_url_source_adds_whitelist_and_headers_before_input() {
    use kadr_core::plugin::PlaybackSource;
    use kadr_core::transcoding::build_ffmpeg_hls_args_for_source;
    use std::collections::BTreeMap;

    let decision = TranscodeDecision {
        video: VideoAction::Copy,
        audio: AudioAction::Copy,
    };
    let params = TranscodeParameters {
        video_codec: "h264".into(),
        audio_codec: "aac".into(),
        max_height: None,
        audio_track_index: 0,
        start_time: 0.0,
    };
    let mut headers = BTreeMap::new();
    headers.insert("X-Token".to_string(), "abc".to_string());
    let source = PlaybackSource::Url {
        url: "https://example.com/live.ts".into(),
        headers,
    };

    let args = build_ffmpeg_hls_args_for_source(&source, std::path::Path::new("/tmp/out"), &decision, &params);

    let i = args.iter().position(|a| a == "-i").unwrap();
    assert_eq!(args[i + 1], "https://example.com/live.ts");
    let wl = args.iter().position(|a| a == "-protocol_whitelist").unwrap();
    assert!(wl < i, "protocol whitelist is an input option and must precede -i");
    assert_eq!(args[wl + 1], "http,https,tcp,tls,crypto");
    let h = args.iter().position(|a| a == "-headers").unwrap();
    assert!(h < i);
    assert_eq!(args[h + 1], "X-Token: abc\r\n");
}

#[test]
fn test_file_source_wrapper_matches_path_function() {
    use kadr_core::plugin::PlaybackSource;
    use kadr_core::transcoding::build_ffmpeg_hls_args_for_source;
    let decision = TranscodeDecision {
        video: VideoAction::Copy,
        audio: AudioAction::Copy,
    };
    let params = TranscodeParameters {
        video_codec: "h264".into(),
        audio_codec: "aac".into(),
        max_height: None,
        audio_track_index: 0,
        start_time: 0.0,
    };
    let path = std::path::Path::new("/media/a.mkv");
    let out = std::path::Path::new("/tmp/out");
    assert_eq!(
        build_ffmpeg_hls_args(path, out, &decision, &params),
        build_ffmpeg_hls_args_for_source(&PlaybackSource::File(path.to_path_buf()), out, &decision, &params)
    );
    assert!(!build_ffmpeg_hls_args(path, out, &decision, &params).contains(&"-protocol_whitelist".to_string()));
}
```

Make sure the file's existing `use` line imports `TranscodeDecision` (add it to the import list from `kadr_core::transcoding` if it is missing).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-core --test transcoding_test`
Expected: FAIL to compile (`build_ffmpeg_hls_args_for_source` missing).

- [ ] **Step 3: Implement the builder change**

In `crates/kadr-core/src/transcoding/builder.rs`:

1. Add `use crate::plugin::PlaybackSource;` to the imports.
2. Rename the existing `pub fn build_ffmpeg_hls_args(input_path: &Path, ...)` to `pub fn build_ffmpeg_hls_args_for_source(source: &PlaybackSource, output_dir: &Path, decision: &TranscodeDecision, params: &TranscodeParameters) -> Vec<String>`.
3. Replace these two lines inside it:

```rust
    args.push("-i".into());
    args.push(input_path.to_string_lossy().to_string());
```

with:

```rust
    match source {
        PlaybackSource::File(path) => {
            args.push("-i".into());
            args.push(path.to_string_lossy().to_string());
        }
        PlaybackSource::Url { url, headers } => {
            // Restrict ffmpeg to network protocols only (no file:, concat:, etc).
            args.push("-protocol_whitelist".into());
            args.push("http,https,tcp,tls,crypto".into());
            if !headers.is_empty() {
                let mut block = String::new();
                for (name, value) in headers {
                    block.push_str(&format!("{name}: {value}\r\n"));
                }
                args.push("-headers".into());
                args.push(block);
            }
            args.push("-i".into());
            args.push(url.clone());
        }
    }
```

4. Add the wrapper right above it:

```rust
pub fn build_ffmpeg_hls_args(
    input_path: &Path,
    output_dir: &Path,
    decision: &TranscodeDecision,
    params: &TranscodeParameters,
) -> Vec<String> {
    build_ffmpeg_hls_args_for_source(
        &PlaybackSource::File(input_path.to_path_buf()),
        output_dir,
        decision,
        params,
    )
}
```

`crates/kadr-core/src/transcoding/mod.rs` re-exports builder items; check how (`grep -n "build_ffmpeg_hls_args" crates/kadr-core/src/transcoding/mod.rs`). If it names items explicitly, add `build_ffmpeg_hls_args_for_source` next to `build_ffmpeg_hls_args`; if it uses `pub use builder::*;` nothing is needed.

- [ ] **Step 4: Implement the session manager change**

In `crates/kadr-server/src/streaming/hls/session.rs`:

1. Change the imports to `use kadr_core::plugin::PlaybackSource;` plus `build_ffmpeg_hls_args_for_source` (remove `build_ffmpeg_hls_args` from the `use kadr_core::transcoding::{...}` list).
2. Rename the existing method `get_or_create_session` to `get_or_create_session_for_source` and change its parameter `media_path: &Path` to `source: &PlaybackSource`.
3. Replace `build_ffmpeg_hls_args(media_path, &session_dir, &decision, params)` with `build_ffmpeg_hls_args_for_source(source, &session_dir, &decision, params)`.
4. Add the wrapper above it (inside `impl HlsSessionManager`):

```rust
    pub async fn get_or_create_session(
        &self,
        media_id: i64,
        media_path: &Path,
        tech: &TechnicalInfo,
        params: &TranscodeParameters,
    ) -> Result<Arc<TranscodeSession>, HlsError> {
        self.get_or_create_session_for_source(
            media_id,
            &PlaybackSource::File(media_path.to_path_buf()),
            tech,
            params,
        )
        .await
    }
```

- [ ] **Step 5: Add a session-level test**

Look at how `crates/kadr-server/tests/hls_session_model_test.rs` builds a manager and a fake ffmpeg (if it uses a fake script, reuse that helper). Append a test that creates a fake ffmpeg script recording its args:

```rust
#[tokio::test]
async fn test_session_for_url_source_passes_url_to_ffmpeg() {
    use kadr_core::models::TechnicalInfo;
    use kadr_core::plugin::PlaybackSource;
    use kadr_core::transcoding::TranscodeParameters;
    use kadr_server::config::TranscodingSettings;
    use kadr_server::HlsSessionManager;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("args.log");
    let script = dir.path().join("fake-ffmpeg.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {log}\nfor last; do :; done\nprintf '#EXTM3U\\n' > \"$last\"\n",
            log = log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let settings = TranscodingSettings {
        ffmpeg_path: script.to_string_lossy().to_string(),
        temp_dir: dir.path().join("tmp"),
        ..Default::default()
    };
    let mgr = HlsSessionManager::new(settings);
    let source = PlaybackSource::Url {
        url: "https://example.com/stream.ts".into(),
        headers: Default::default(),
    };
    let session = mgr
        .get_or_create_session_for_source(
            7,
            &source,
            &TechnicalInfo::default(),
            &TranscodeParameters::default(),
        )
        .await
        .unwrap();
    mgr.wait_for_playlist(&session, 5).await.unwrap();

    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("https://example.com/stream.ts"));
    assert!(logged.contains("-protocol_whitelist"));
}
```

If `TranscodeParameters` does not implement `Default`, construct it explicitly the way the existing tests in that file do. `tempfile` is already a dev-dependency.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p kadr-core && cargo test -p kadr-server --test hls_session_model_test --test hls_routes_test --test hls_process_reaper_test`
Expected: PASS (existing HLS tests unchanged).

- [ ] **Step 7: Commit**

```bash
git add crates/kadr-core crates/kadr-server/src/streaming crates/kadr-server/tests/hls_session_model_test.rs
git commit -m "feat(hls): allow transcoding from a URL source via PlaybackSource"
```


### Task 4: Config and plugin auth (tokens, derived secret, extractor)

**Files:**
- Modify: `crates/kadr-server/src/config.rs`, `crates/kadr-server/src/lib.rs`
- Create: `crates/kadr-server/src/plugins/mod.rs`, `crates/kadr-server/src/plugins/auth.rs`
- Test: `crates/kadr-server/tests/config_test.rs` (append), `crates/kadr-server/tests/plugin_auth_test.rs`

**Interfaces:**
- Produces:
  - `PluginSettings { enabled: bool, resolve_timeout_secs: u64, allow_private_proxy: bool }` with `Default` = `{ false, 5, false }`; `AppConfig.plugins: PluginSettings`.
  - `kadr_server::plugins::auth::{derive_plugin_secret(server_secret: &[u8], plugin_id: &str) -> String, PluginClaims, PluginTokenService { new(&[u8]), issue(&str) -> Result<String, jsonwebtoken::errors::Error>, verify(&str) -> Result<PluginClaims, jsonwebtoken::errors::Error> }, PluginAuth { plugin_id: String }}`

- [ ] **Step 1: Write the failing tests**

Append to `crates/kadr-server/tests/config_test.rs`:

```rust
#[test]
fn test_plugin_settings_defaults_and_parsing() {
    use kadr_server::config::AppConfig;

    let cfg: AppConfig = toml::from_str("").unwrap();
    assert!(!cfg.plugins.enabled);
    assert_eq!(cfg.plugins.resolve_timeout_secs, 5);
    assert!(!cfg.plugins.allow_private_proxy);

    let cfg: AppConfig = toml::from_str(
        "[plugins]\nenabled = true\nresolve_timeout_secs = 9\nallow_private_proxy = true\n",
    )
    .unwrap();
    assert!(cfg.plugins.enabled);
    assert_eq!(cfg.plugins.resolve_timeout_secs, 9);
    assert!(cfg.plugins.allow_private_proxy);
}
```

If `toml` is not a dev-dependency of `kadr-server` it is still available because it is a normal dependency of the crate.

Create `crates/kadr-server/tests/plugin_auth_test.rs`:

```rust
use kadr_core::models::{User, UserRole};
use kadr_server::auth::jwt::JwtService;
use kadr_server::plugins::auth::{derive_plugin_secret, PluginTokenService};

const SECRET: &[u8] = b"test-secret-with-sufficient-length-for-hmac-sha256";

#[test]
fn derived_secret_is_deterministic_and_scoped() {
    let a1 = derive_plugin_secret(SECRET, "iptv");
    let a2 = derive_plugin_secret(SECRET, "iptv");
    let b = derive_plugin_secret(SECRET, "hello");
    let c = derive_plugin_secret(b"another-server-secret-value-0123456789", "iptv");
    assert_eq!(a1, a2);
    assert_ne!(a1, b);
    assert_ne!(a1, c);
    assert!(a1.len() >= 43, "256-bit value, base64url without padding");
    assert!(!a1.contains('='));
}

#[test]
fn plugin_token_roundtrip() {
    let svc = PluginTokenService::new(SECRET);
    let token = svc.issue("iptv").unwrap();
    let claims = svc.verify(&token).unwrap();
    assert_eq!(claims.plugin_id, "iptv");
    assert_eq!(claims.sub, "plugin:iptv");
}

#[test]
fn tampered_plugin_token_is_rejected() {
    let svc = PluginTokenService::new(SECRET);
    let token = svc.issue("iptv").unwrap();
    let mut chars: Vec<char> = token.chars().collect();
    let last = chars.len() - 1;
    chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
    let tampered: String = chars.into_iter().collect();
    assert!(svc.verify(&tampered).is_err());
}

#[test]
fn user_tokens_are_not_plugin_tokens_and_vice_versa() {
    let jwt = JwtService::new(std::str::from_utf8(SECRET).unwrap(), 3600);
    let plugin = PluginTokenService::new(SECRET);

    let user = User {
        id: "u1".into(),
        username: "admin".into(),
        pin_hash: String::new(),
        role: UserRole::Admin,
        created_at: 0,
    };
    let user_token = jwt.generate_token(&user).unwrap();
    let plugin_token = plugin.issue("iptv").unwrap();

    assert!(plugin.verify(&user_token).is_err(), "user token must not verify as plugin token");
    assert!(jwt.verify_token(&plugin_token).is_err(), "plugin token must not verify as user token");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_auth_test --test config_test`
Expected: FAIL to compile.

- [ ] **Step 3: Implement config**

In `crates/kadr-server/src/config.rs` add after `TranscodingSettings`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_plugin_resolve_timeout_secs")]
    pub resolve_timeout_secs: u64,
    #[serde(default)]
    pub allow_private_proxy: bool,
}

fn default_plugin_resolve_timeout_secs() -> u64 {
    5
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            resolve_timeout_secs: default_plugin_resolve_timeout_secs(),
            allow_private_proxy: false,
        }
    }
}
```

and add to `AppConfig` (after `transcoding`):

```rust
    #[serde(default)]
    pub plugins: PluginSettings,
```

- [ ] **Step 4: Implement auth**

Create `crates/kadr-server/src/plugins/mod.rs`:

```rust
pub mod auth;
```

In `crates/kadr-server/src/lib.rs` add `pub mod plugins;` (alphabetically after `pub mod playback;`).

Create `crates/kadr-server/src/plugins/auth.rs`:

```rust
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::prelude::*;
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const TOKEN_TYP: &str = "kadr-plugin";
const TOKEN_TTL_SECS: usize = 365 * 24 * 3600;

fn hmac_bytes(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

/// Secret the server sends as `Authorization: Bearer <secret>` on every call to a plugin.
/// Derived (never stored): `base64url_nopad(HMAC-SHA256(server_secret, "kadr-plugin-secret-v1:" + plugin_id))`.
pub fn derive_plugin_secret(server_secret: &[u8], plugin_id: &str) -> String {
    let msg = format!("kadr-plugin-secret-v1:{plugin_id}");
    BASE64_URL_SAFE_NO_PAD.encode(hmac_bytes(server_secret, msg.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginClaims {
    pub sub: String,
    pub plugin_id: String,
    pub typ: String,
    pub exp: usize,
    pub iat: usize,
}

/// Issues and verifies tokens a plugin uses to call back into Kadr.
/// The signing key is derived from the server secret with a distinct label, and the claim
/// shape differs from `AuthClaims`, so user and plugin tokens can never be confused.
#[derive(Clone)]
pub struct PluginTokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl PluginTokenService {
    pub fn new(server_secret: &[u8]) -> Self {
        let key = hmac_bytes(server_secret, b"kadr-plugin-token-key-v1");
        Self {
            encoding: EncodingKey::from_secret(&key),
            decoding: DecodingKey::from_secret(&key),
        }
    }

    pub fn issue(&self, plugin_id: &str) -> Result<String, jsonwebtoken::errors::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as usize;
        let claims = PluginClaims {
            sub: format!("plugin:{plugin_id}"),
            plugin_id: plugin_id.to_string(),
            typ: TOKEN_TYP.to_string(),
            exp: now + TOKEN_TTL_SECS,
            iat: now,
        };
        encode(&Header::default(), &claims, &self.encoding)
    }

    pub fn verify(&self, token: &str) -> Result<PluginClaims, jsonwebtoken::errors::Error> {
        let data = decode::<PluginClaims>(token, &self.decoding, &Validation::default())?;
        let claims = data.claims;
        if claims.typ != TOKEN_TYP || claims.sub != format!("plugin:{}", claims.plugin_id) {
            return Err(jsonwebtoken::errors::ErrorKind::InvalidToken.into());
        }
        Ok(claims)
    }
}

/// Extractor for routes called by plugins. Only the `Authorization: Bearer` header is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginAuth {
    pub plugin_id: String,
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

impl<S> FromRequestParts<S> for PluginAuth
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let Some(token) = token else {
            return Err(unauthorized("Missing plugin token"));
        };
        let Some(svc) = parts.extensions.get::<PluginTokenService>() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Plugin token service not configured" })),
            )
                .into_response());
        };
        match svc.verify(&token) {
            Ok(claims) => Ok(PluginAuth {
                plugin_id: claims.plugin_id,
            }),
            Err(_) => Err(unauthorized("Invalid plugin token")),
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p kadr-server --test plugin_auth_test --test config_test && cargo build --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(plugins): add plugin settings, derived secrets and scoped plugin tokens"
```

---

### Task 5: URL guard, `PluginClient` and circuit breaker

**Files:**
- Create: `crates/kadr-server/src/plugins/url_guard.rs`, `crates/kadr-server/src/plugins/client.rs`, `crates/kadr-server/tests/plugin_support/mod.rs`
- Modify: `crates/kadr-server/src/plugins/mod.rs`
- Test: `crates/kadr-server/tests/plugin_url_guard_test.rs`, `crates/kadr-server/tests/plugin_client_test.rs`

**Interfaces:**
- Consumes: Task 2 descriptor and request types.
- Produces:
  - `url_guard::{is_private_ip(IpAddr) -> bool, validate_descriptor(&PlaybackDescriptor, allow_private: bool) -> Result<(), String> (async)}`
  - `client::PluginError { Unavailable(String), Timeout, BadResponse(String) }` with `status() -> StatusCode`, `code() -> &'static str`
  - `client::CircuitBreaker { allow(&mut self, Instant) -> bool, record_success(&mut self), record_failure(&mut self, Instant) }`
  - `client::PluginClient { new(timeout: Duration), fetch_manifest(&str) -> Result<PluginManifest, PluginError>, health(&str /*base_url*/, &str /*secret*/) -> Result<(), PluginError>, resolve(plugin_id: &str, base_url: &str, secret: &str, req: &ResolveRequest) -> Result<PlaybackDescriptor, PluginError> }`, all async.
  - Test-support `MockPlugin` (see Step 1).

- [ ] **Step 1: Create the mock plugin test support**

Create `crates/kadr-server/tests/plugin_support/mod.rs`:

```rust
#![allow(dead_code)]

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// In-process fake plugin bound to 127.0.0.1 on a random port.
#[derive(Clone)]
pub struct MockPlugin {
    pub base_url: String,
    pub manifest: Arc<Mutex<Value>>,
    pub descriptor: Arc<Mutex<Value>>,
    pub expected_secret: Arc<Mutex<Option<String>>>,
    pub healthy: Arc<AtomicBool>,
    pub resolve_calls: Arc<AtomicUsize>,
    pub last_resolve_body: Arc<Mutex<Option<Value>>>,
    pub resolve_delay_ms: Arc<AtomicUsize>,
    /// 0 = respond normally; otherwise respond with this HTTP status and an empty body.
    pub resolve_status: Arc<AtomicUsize>,
}

impl MockPlugin {
    pub fn set_descriptor(&self, v: Value) {
        *self.descriptor.lock().unwrap() = v;
    }
    pub fn set_secret(&self, s: &str) {
        *self.expected_secret.lock().unwrap() = Some(s.to_string());
    }
    pub fn set_manifest(&self, v: Value) {
        *self.manifest.lock().unwrap() = v;
    }
    pub fn media_url(&self) -> String {
        format!("{}/media", self.base_url)
    }
    pub fn calls(&self) -> usize {
        self.resolve_calls.load(Ordering::SeqCst)
    }
}

fn authorized(m: &MockPlugin, headers: &HeaderMap) -> bool {
    match &*m.expected_secret.lock().unwrap() {
        None => true,
        Some(secret) => {
            headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok())
                == Some(format!("Bearer {secret}").as_str())
        }
    }
}

async fn manifest(State(m): State<MockPlugin>) -> Json<Value> {
    Json(m.manifest.lock().unwrap().clone())
}

async fn health(State(m): State<MockPlugin>, headers: HeaderMap) -> StatusCode {
    if !authorized(&m, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    if m.healthy.load(Ordering::SeqCst) {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn resolve(
    State(m): State<MockPlugin>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    m.resolve_calls.fetch_add(1, Ordering::SeqCst);
    *m.last_resolve_body.lock().unwrap() = Some(body);
    if !authorized(&m, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let delay = m.resolve_delay_ms.load(Ordering::SeqCst);
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
    }
    let status = m.resolve_status.load(Ordering::SeqCst);
    if status != 0 {
        return StatusCode::from_u16(status as u16).unwrap().into_response();
    }
    Json(m.descriptor.lock().unwrap().clone()).into_response()
}

async fn media(headers: HeaderMap) -> Response {
    const BODY: &[u8] = b"0123456789";
    if let Some(range) = headers
        .get(header::RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|r| r.strip_prefix("bytes="))
    {
        if let Some((a, b)) = range.split_once('-') {
            let start: usize = a.parse().unwrap_or(0);
            let end: usize = b.parse().unwrap_or(BODY.len() - 1).min(BODY.len() - 1);
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "video/mp4")
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/10"))
                .body(Body::from(BODY[start..=end].to_vec()))
                .unwrap();
        }
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::from(BODY.to_vec()))
        .unwrap()
}

pub fn manifest_json(id: &str, permissions: &[&str]) -> Value {
    json!({
        "id": id,
        "name": format!("Mock {id}"),
        "version": "0.1.0",
        "api_version": 1,
        "permissions": permissions,
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    })
}

pub async fn spawn_mock(id: &str, permissions: &[&str]) -> MockPlugin {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mock = MockPlugin {
        base_url: format!("http://{addr}"),
        manifest: Arc::new(Mutex::new(manifest_json(id, permissions))),
        descriptor: Arc::new(Mutex::new(
            json!({ "mode": "direct", "url": "https://cdn.example.com/video.mp4" }),
        )),
        expected_secret: Arc::new(Mutex::new(None)),
        healthy: Arc::new(AtomicBool::new(true)),
        resolve_calls: Arc::new(AtomicUsize::new(0)),
        last_resolve_body: Arc::new(Mutex::new(None)),
        resolve_delay_ms: Arc::new(AtomicUsize::new(0)),
        resolve_status: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/manifest.json", get(manifest))
        .route("/health", get(health))
        .route("/playback/resolve", post(resolve))
        .route("/media", get(media))
        .with_state(mock.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    mock
}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/kadr-server/tests/plugin_url_guard_test.rs`:

```rust
use kadr_core::plugin::PlaybackDescriptor;
use kadr_server::plugins::url_guard::{is_private_ip, validate_descriptor};
use std::net::IpAddr;

fn desc(json: &str) -> PlaybackDescriptor {
    serde_json::from_str(json).unwrap()
}

#[test]
fn private_and_special_ranges_are_detected() {
    for ip in [
        "127.0.0.1", "10.0.0.1", "192.168.1.1", "172.16.0.1", "169.254.1.1", "0.0.0.0",
        "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1",
    ] {
        assert!(is_private_ip(ip.parse::<IpAddr>().unwrap()), "{ip} should be private");
    }
    for ip in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
        assert!(!is_private_ip(ip.parse::<IpAddr>().unwrap()), "{ip} should be public");
    }
}

#[tokio::test]
async fn rejects_non_http_schemes_and_garbage() {
    assert!(validate_descriptor(&desc(r#"{"mode":"direct","url":"ftp://x/y"}"#), false).await.is_err());
    assert!(validate_descriptor(&desc(r#"{"mode":"direct","url":"file:///etc/passwd"}"#), true).await.is_err());
    assert!(validate_descriptor(&desc(r#"{"mode":"direct","url":"not a url"}"#), false).await.is_err());
}

#[tokio::test]
async fn direct_descriptors_cannot_carry_headers() {
    let d = desc(r#"{"mode":"direct","url":"https://cdn.example.com/a.mp4","headers":{"X-A":"b"}}"#);
    assert!(validate_descriptor(&d, false).await.is_err());
}

#[tokio::test]
async fn header_injection_is_rejected() {
    let d = desc(r#"{"mode":"proxy","url":"http://8.8.8.8/a","headers":{"X-A":"b\r\nEvil: 1"}}"#);
    assert!(validate_descriptor(&d, true).await.is_err());
    let d = desc(r#"{"mode":"proxy","url":"http://8.8.8.8/a","headers":{"Bad:Name":"b"}}"#);
    assert!(validate_descriptor(&d, true).await.is_err());
}

#[tokio::test]
async fn proxy_and_transcode_block_private_hosts_unless_allowed() {
    for mode in ["proxy", "transcode"] {
        let ip = desc(&format!(r#"{{"mode":"{mode}","url":"http://127.0.0.1:9/x"}}"#));
        assert!(validate_descriptor(&ip, false).await.is_err());
        assert!(validate_descriptor(&ip, true).await.is_ok());

        let host = desc(&format!(r#"{{"mode":"{mode}","url":"http://localhost:9/x"}}"#));
        assert!(validate_descriptor(&host, false).await.is_err());

        let v6 = desc(&format!(r#"{{"mode":"{mode}","url":"http://[::1]:9/x"}}"#));
        assert!(validate_descriptor(&v6, false).await.is_err());

        let public = desc(&format!(r#"{{"mode":"{mode}","url":"http://8.8.8.8/x"}}"#));
        assert!(validate_descriptor(&public, false).await.is_ok());
    }
}

#[tokio::test]
async fn direct_urls_may_point_at_private_hosts() {
    let d = desc(r#"{"mode":"direct","url":"http://192.168.1.50/stream.m3u8"}"#);
    assert!(validate_descriptor(&d, false).await.is_ok());
}
```

Create `crates/kadr-server/tests/plugin_client_test.rs`:

```rust
mod plugin_support;

use kadr_core::plugin::{ClientCapabilities, PlaybackMode, ResolveRequest};
use kadr_server::plugins::client::{CircuitBreaker, PluginClient, PluginError};
use plugin_support::spawn_mock;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn req() -> ResolveRequest {
    ResolveRequest {
        remote_id: "ch1".into(),
        user_id: "u1".into(),
        client: ClientCapabilities::default(),
    }
}

#[test]
fn breaker_opens_after_three_failures_and_half_opens_after_cooldown() {
    let mut b = CircuitBreaker::default();
    let t0 = Instant::now();
    assert!(b.allow(t0));
    b.record_failure(t0);
    b.record_failure(t0);
    assert!(b.allow(t0), "two failures keep it closed");
    b.record_failure(t0);
    assert!(!b.allow(t0), "third failure opens it");
    assert!(!b.allow(t0 + Duration::from_secs(29)));
    assert!(b.allow(t0 + Duration::from_secs(31)), "half-open after cooldown");
    b.record_failure(t0 + Duration::from_secs(31));
    assert!(!b.allow(t0 + Duration::from_secs(32)), "one failure while half-open re-opens");
    assert!(b.allow(t0 + Duration::from_secs(62)));
    b.record_success();
    b.record_failure(t0 + Duration::from_secs(63));
    assert!(b.allow(t0 + Duration::from_secs(63)), "success fully resets the counter");
}

#[test]
fn error_status_and_code_mapping() {
    assert_eq!(PluginError::Unavailable("x".into()).status().as_u16(), 503);
    assert_eq!(PluginError::Unavailable("x".into()).code(), "PLUGIN_UNAVAILABLE");
    assert_eq!(PluginError::Timeout.status().as_u16(), 504);
    assert_eq!(PluginError::Timeout.code(), "PLUGIN_TIMEOUT");
    assert_eq!(PluginError::BadResponse("x".into()).status().as_u16(), 502);
    assert_eq!(PluginError::BadResponse("x".into()).code(), "PLUGIN_BAD_RESPONSE");
}

#[tokio::test]
async fn fetch_manifest_and_health() {
    let mock = spawn_mock("hello", &["items:publish"]).await;
    let client = PluginClient::new(Duration::from_secs(2));

    let m = client.fetch_manifest(&mock.base_url).await.unwrap();
    assert_eq!(m.id, "hello");

    client.health(&mock.base_url, "any").await.unwrap();
    mock.healthy.store(false, Ordering::SeqCst);
    assert!(matches!(
        client.health(&mock.base_url, "any").await,
        Err(PluginError::Unavailable(_))
    ));
}

#[tokio::test]
async fn unreachable_plugin_is_unavailable() {
    let client = PluginClient::new(Duration::from_secs(1));
    assert!(matches!(
        client.fetch_manifest("http://127.0.0.1:1").await,
        Err(PluginError::Unavailable(_))
    ));
}

#[tokio::test]
async fn resolve_sends_bearer_secret_and_parses_descriptor() {
    let mock = spawn_mock("hello", &["playback:resolve"]).await;
    mock.set_secret("s3cret");
    let client = PluginClient::new(Duration::from_secs(2));

    let d = client.resolve("hello", &mock.base_url, "s3cret", &req()).await.unwrap();
    assert_eq!(d.mode, PlaybackMode::Direct);
    assert_eq!(mock.last_resolve_body.lock().unwrap().as_ref().unwrap()["remote_id"], "ch1");

    // Wrong secret -> plugin answers 401 -> treated as a bad response, not an outage.
    let err = client.resolve("hello", &mock.base_url, "wrong", &req()).await.unwrap_err();
    assert!(matches!(err, PluginError::BadResponse(_)), "got {err:?}");
}

#[tokio::test]
async fn resolve_times_out() {
    let mock = spawn_mock("slow", &["playback:resolve"]).await;
    mock.resolve_delay_ms.store(1500, Ordering::SeqCst);
    let client = PluginClient::new(Duration::from_millis(300));
    assert!(matches!(
        client.resolve("slow", &mock.base_url, "s", &req()).await,
        Err(PluginError::Timeout)
    ));
}

#[tokio::test]
async fn resolve_maps_server_errors_and_bad_json() {
    let mock = spawn_mock("bad", &["playback:resolve"]).await;
    let client = PluginClient::new(Duration::from_secs(2));

    mock.set_descriptor(json!({ "nope": 1 }));
    assert!(matches!(
        client.resolve("bad", &mock.base_url, "s", &req()).await,
        Err(PluginError::BadResponse(_))
    ));

    mock.resolve_status.store(500, Ordering::SeqCst);
    assert!(matches!(
        client.resolve("bad", &mock.base_url, "s", &req()).await,
        Err(PluginError::Unavailable(_))
    ));
}

#[tokio::test]
async fn circuit_opens_after_repeated_failures_and_stops_calling_the_plugin() {
    let mock = spawn_mock("flaky", &["playback:resolve"]).await;
    mock.resolve_status.store(500, Ordering::SeqCst);
    let client = PluginClient::new(Duration::from_secs(2));

    for _ in 0..3 {
        assert!(client.resolve("flaky", &mock.base_url, "s", &req()).await.is_err());
    }
    assert_eq!(mock.calls(), 3);

    let err = client.resolve("flaky", &mock.base_url, "s", &req()).await.unwrap_err();
    assert!(matches!(err, PluginError::Unavailable(_)));
    assert_eq!(mock.calls(), 3, "open circuit must not reach the plugin");
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_url_guard_test --test plugin_client_test`
Expected: FAIL to compile.

- [ ] **Step 4: Implement `url_guard.rs`**

Create `crates/kadr-server/src/plugins/url_guard.rs`:

```rust
use kadr_core::plugin::{PlaybackDescriptor, PlaybackMode};
use std::net::IpAddr;

/// True for loopback, private, link-local, unspecified, CGNAT and unique-local addresses.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (seg0 & 0xfe00) == 0xfc00
                || (seg0 & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .map(|v4| is_private_ip(IpAddr::V4(v4)))
                    .unwrap_or(false)
        }
    }
}

fn header_text_ok(s: &str) -> bool {
    !s.chars().any(|c| c == '\r' || c == '\n' || c == '\0')
}

async fn ensure_public_host(url: &reqwest::Url) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "url has no host".to_string())?;
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return if is_private_ip(ip) {
            Err(format!("host {bare} is a private or loopback address"))
        } else {
            Ok(())
        };
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((bare, port))
        .await
        .map_err(|e| format!("cannot resolve host {bare}: {e}"))?;
    let mut seen = false;
    for addr in addrs {
        seen = true;
        if is_private_ip(addr.ip()) {
            return Err(format!("host {bare} resolves to a private or loopback address"));
        }
    }
    if seen {
        Ok(())
    } else {
        Err(format!("host {bare} did not resolve"))
    }
}

/// Validates a plugin-supplied playback descriptor.
///
/// Known limitation: DNS is re-resolved when the server (proxy) or ffmpeg (transcode)
/// connects, and ffmpeg follows redirects internally. Operators who need stronger guarantees
/// should keep `allow_private_proxy = false` and run plugins they trust.
pub async fn validate_descriptor(
    desc: &PlaybackDescriptor,
    allow_private: bool,
) -> Result<(), String> {
    let url = reqwest::Url::parse(&desc.url).map_err(|e| format!("invalid url: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("unsupported url scheme '{}'", url.scheme()));
    }
    for (name, value) in &desc.headers {
        if name.is_empty() || name.contains(':') || !header_text_ok(name) || !header_text_ok(value)
        {
            return Err(format!("invalid header '{name}'"));
        }
    }
    match desc.mode {
        PlaybackMode::Direct => {
            if !desc.headers.is_empty() {
                return Err(
                    "direct descriptors cannot carry headers; use proxy or transcode".to_string(),
                );
            }
            Ok(())
        }
        PlaybackMode::Proxy | PlaybackMode::Transcode => {
            if allow_private {
                Ok(())
            } else {
                ensure_public_host(&url).await
            }
        }
    }
}
```

- [ ] **Step 5: Implement `client.rs`**

Create `crates/kadr-server/src/plugins/client.rs`:

```rust
use axum::http::StatusCode;
use kadr_core::plugin::{PlaybackDescriptor, PluginManifest, ResolveRequest};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FAILURE_THRESHOLD: u32 = 3;
const COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin unavailable: {0}")]
    Unavailable(String),
    #[error("Plugin timed out")]
    Timeout,
    #[error("Plugin returned an invalid response: {0}")]
    BadResponse(String),
}

impl PluginError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::BadResponse(_) => StatusCode::BAD_GATEWAY,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "PLUGIN_UNAVAILABLE",
            Self::Timeout => "PLUGIN_TIMEOUT",
            Self::BadResponse(_) => "PLUGIN_BAD_RESPONSE",
        }
    }
}

/// Opens after `FAILURE_THRESHOLD` consecutive failures, half-opens after `COOLDOWN`
/// (a single failure in the half-open state re-opens it; a success resets it).
#[derive(Debug, Default)]
pub struct CircuitBreaker {
    failures: u32,
    open_until: Option<Instant>,
}

impl CircuitBreaker {
    pub fn allow(&mut self, now: Instant) -> bool {
        match self.open_until {
            Some(until) if now < until => false,
            Some(_) => {
                self.open_until = None;
                self.failures = FAILURE_THRESHOLD - 1;
                true
            }
            None => true,
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.open_until = None;
    }

    pub fn record_failure(&mut self, now: Instant) {
        self.failures += 1;
        if self.failures >= FAILURE_THRESHOLD {
            self.open_until = Some(now + COOLDOWN);
        }
    }
}

const CONNECT_ERROR_PREFIX: &str = "connection failed";

fn map_reqwest(e: reqwest::Error) -> PluginError {
    if e.is_timeout() {
        PluginError::Timeout
    } else if e.is_connect() {
        PluginError::Unavailable(format!("{CONNECT_ERROR_PREFIX}: {e}"))
    } else {
        PluginError::Unavailable(e.to_string())
    }
}

#[derive(Clone)]
pub struct PluginClient {
    http: reqwest::Client,
    timeout: Duration,
    breakers: Arc<Mutex<HashMap<String, CircuitBreaker>>>,
}

impl PluginClient {
    pub fn new(timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client builds with static configuration");
        Self {
            http,
            timeout,
            breakers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        rb: reqwest::RequestBuilder,
    ) -> Result<T, PluginError> {
        let resp = rb.timeout(self.timeout).send().await.map_err(map_reqwest)?;
        let status = resp.status();
        if status.is_server_error() {
            return Err(PluginError::Unavailable(format!("plugin returned {status}")));
        }
        if !status.is_success() {
            return Err(PluginError::BadResponse(format!("plugin returned {status}")));
        }
        resp.json::<T>().await.map_err(|e| {
            if e.is_timeout() {
                PluginError::Timeout
            } else {
                PluginError::BadResponse(e.to_string())
            }
        })
    }

    /// `GET <base_url>/manifest.json` (public, unauthenticated; not subject to the breaker).
    pub async fn fetch_manifest(&self, base_url: &str) -> Result<PluginManifest, PluginError> {
        let url = format!("{}/manifest.json", base_url.trim_end_matches('/'));
        self.send_json(self.http.get(url)).await
    }

    /// `GET <base_url>/health` (not subject to the breaker; used to detect recovery).
    pub async fn health(&self, base_url: &str, secret: &str) -> Result<(), PluginError> {
        let url = format!("{}/health", base_url.trim_end_matches('/'));
        let resp = self
            .http
            .get(url)
            .bearer_auth(secret)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(map_reqwest)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(PluginError::Unavailable(format!(
                "health check returned {}",
                resp.status()
            )))
        }
    }

    /// `POST <base_url>/playback/resolve`. Subject to the per-plugin circuit breaker and
    /// retried once on connection errors (never on timeouts).
    pub async fn resolve(
        &self,
        plugin_id: &str,
        base_url: &str,
        secret: &str,
        req: &ResolveRequest,
    ) -> Result<PlaybackDescriptor, PluginError> {
        {
            let mut breakers = self.breakers.lock().unwrap();
            let breaker = breakers.entry(plugin_id.to_string()).or_default();
            if !breaker.allow(Instant::now()) {
                return Err(PluginError::Unavailable(
                    "circuit open after repeated failures".to_string(),
                ));
            }
        }

        let url = format!("{}/playback/resolve", base_url.trim_end_matches('/'));
        let mut result = Err(PluginError::Unavailable("no attempt made".to_string()));
        for _attempt in 0..2 {
            let rb = self.http.post(&url).bearer_auth(secret).json(req);
            result = self.send_json::<PlaybackDescriptor>(rb).await;
            match &result {
                Err(PluginError::Unavailable(msg)) if msg.starts_with(CONNECT_ERROR_PREFIX) => {
                    continue;
                }
                _ => break,
            }
        }

        let mut breakers = self.breakers.lock().unwrap();
        let breaker = breakers.entry(plugin_id.to_string()).or_default();
        match &result {
            Ok(_) => breaker.record_success(),
            Err(_) => breaker.record_failure(Instant::now()),
        }
        result
    }
}
```

In `crates/kadr-server/src/plugins/mod.rs` replace the contents with:

```rust
pub mod auth;
pub mod client;
pub mod url_guard;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p kadr-server --test plugin_url_guard_test --test plugin_client_test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(plugins): add descriptor URL guard, plugin HTTP client and circuit breaker"
```

---

### Task 6: `PluginService`, admin API, router wiring and the shared test harness

**Files:**
- Create: `crates/kadr-server/src/plugins/service.rs`, `crates/kadr-server/src/plugins/admin_routes.rs`
- Modify: `crates/kadr-server/src/plugins/mod.rs`, `crates/kadr-server/src/api/mod.rs`, `crates/kadr-server/tests/plugin_support/mod.rs`
- Test: `crates/kadr-server/tests/plugin_admin_test.rs`

**Interfaces:**
- Consumes: Tasks 1, 2, 4, 5.
- Produces:
  - `PluginService` (Clone): `new(pool: deadpool_sqlite::Pool, server_secret: &[u8], config: Arc<tokio::sync::RwLock<AppConfig>>, events: Arc<EventBus>) -> Self`, and async methods `settings() -> PluginSettings`, `register(&str) -> Result<(PluginRecord, String), ServiceError>`, `approve(&str, Option<Vec<PluginPermission>>) -> Result<(PluginRecord, String), ServiceError>`, `set_enabled(&str, bool) -> Result<PluginRecord, ServiceError>`, `remove(&str, purge: bool) -> Result<(), ServiceError>`, `authorize(&PluginAuth, PluginPermission) -> Result<PluginRecord, ServiceError>`, `list() -> Result<Vec<PluginRecord>, ServiceError>`; sync accessors `libs() -> &LibraryRepository`, `media() -> &MediaItemRepository`, `events() -> &Arc<EventBus>`, `client() -> &PluginClient`, `secret_for(&str) -> String`, `repo() -> &PluginRepository`.
  - `ServiceError { Disabled, NotFound(String), Conflict(String), Invalid(String), Forbidden(String), Plugin(PluginError), Storage(StorageError), Internal(String) }` implementing `IntoResponse` and `From<PluginError>`, `From<StorageError>`.
  - `plugins::plugin_routes() -> axum::Router` (admin routes in this task; Task 7 merges the publish routes).
  - Test harness: `plugin_support::{Harness, build(PluginSettings), enabled_settings(), call(...), call_raw(...), register_and_approve(...)}`.

- [ ] **Step 1: Extend the test harness**

Append to `crates/kadr-server/tests/plugin_support/mod.rs`:

```rust
use axum::http::Request;
use http_body_util::BodyExt;
use kadr_core::models::{User, UserRole};
use kadr_server::api::create_router_with_ingest;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::{AppConfig, PluginSettings};
use kadr_server::events::EventBus;
use kadr_server::identity::ServerIdentity;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, PluginRepository,
    SubtitleRepository, UserRepository,
};
use kadr_storage::{create_in_memory_pool, initialize_database};
use std::time::Duration;
use tower::ServiceExt;

pub struct Harness {
    pub app: Router,
    pub media_repo: MediaItemRepository,
    pub lib_repo: LibraryRepository,
    pub plugin_repo: PluginRepository,
    pub playback_repo: PlaybackRepository,
    pub admin_token: String,
    pub user_token: String,
    pub jwt_secret: String,
    pub events: Arc<EventBus>,
}

pub fn enabled_settings() -> PluginSettings {
    PluginSettings {
        enabled: true,
        resolve_timeout_secs: 2,
        allow_private_proxy: true,
    }
}

pub async fn build(settings: PluginSettings) -> Harness {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let plugin_repo = PluginRepository::new(pool.clone());

    let jwt_secret = "test-secret-with-sufficient-length-for-hmac-sha256".to_string();
    let jwt_svc = JwtService::new(&jwt_secret, 3600);
    let mk = |role: UserRole, name: &str| User {
        id: name.to_string(),
        username: name.to_string(),
        pin_hash: String::new(),
        role,
        created_at: 0,
    };
    let admin_token = jwt_svc.generate_token(&mk(UserRole::Admin, "admin")).unwrap();
    let user_token = jwt_svc.generate_token(&mk(UserRole::Standard, "viewer")).unwrap();

    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        std::env::temp_dir().join(format!("kadr-plugin-test-{}", uuid::Uuid::new_v4())),
        SubtitleRepository::new(pool.clone()),
        media_repo.clone(),
    ));
    let telemetry = Arc::new(TelemetryCollector::new(
        std::path::PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let (ingest_tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut config = AppConfig::default();
    config.plugins = settings;

    let playback_repo_handle = playback_repo.clone();
    let app = create_router_with_ingest(
        user_repo,
        playback_repo,
        media_repo.clone(),
        lib_repo.clone(),
        jwt_svc,
        RateLimiter::new(50, Duration::from_secs(60), Duration::from_secs(60)),
        session_registry,
        LayoutRegistry::new(),
        widget_resolver,
        subtitle_service,
        Arc::new(OpenSubtitlesClient::new(None, None)),
        event_bus.clone(),
        telemetry,
        ingest_tx,
        Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None)),
        Arc::new(tokio::sync::RwLock::new(config)),
        Arc::new(ServerIdentity {
            id: "00000000-0000-0000-0000-000000000000".to_string(),
        }),
    );

    Harness {
        app,
        media_repo,
        lib_repo,
        plugin_repo,
        playback_repo: playback_repo_handle,
        admin_token,
        user_token,
        jwt_secret,
        events: event_bus,
    }
}

pub async fn call_raw(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = match body {
        Some(v) => builder
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, bytes)
}

/// JSON convenience wrapper: returns `Value::Null` for empty or non-JSON bodies.
pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, _, bytes) = call_raw(app, method, uri, token, &[], body).await;
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Registers the mock through the admin API, approves it with all requested permissions,
/// configures the mock's expected bearer secret, and returns `(plugin_token, secret)`.
pub async fn register_and_approve(h: &Harness, mock: &MockPlugin) -> (String, String) {
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    let secret = body["secret"].as_str().unwrap().to_string();
    let id = body["plugin"]["id"].as_str().unwrap().to_string();
    mock.set_secret(&secret);

    let (status, body) = call(
        &h.app,
        "POST",
        &format!("/api/v1/admin/plugins/{id}/approve"),
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "approve failed: {body}");
    (body["token"].as_str().unwrap().to_string(), secret)
}
```

(Adjust import lists if `Request`/`HeaderMap`/`Body` clash with names already imported at the top of the file. They are all from `axum` and are already imported there except `Request`, which this block adds.)

- [ ] **Step 2: Write the failing admin tests**

Create `crates/kadr-server/tests/plugin_admin_test.rs`:

```rust
mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::MediaType;
use kadr_server::config::PluginSettings;
use plugin_support::*;
use serde_json::json;

#[tokio::test]
async fn plugins_disabled_by_default() {
    let h = build(PluginSettings::default()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "PLUGINS_DISABLED");
}

#[tokio::test]
async fn admin_routes_require_admin() {
    let h = build(enabled_settings()).await;
    let (s, _) = call(&h.app, "GET", "/api/v1/admin/plugins", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.user_token), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_then_approve_flow() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": format!("{}/", mock.base_url) })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["plugin"]["id"], "hello");
    assert_eq!(body["plugin"]["status"], "pending");
    assert_eq!(body["plugin"]["base_url"], mock.base_url, "trailing slash is trimmed");
    assert!(!body["secret"].as_str().unwrap().is_empty());
    assert!(body["plugin"].get("secret").is_none());

    // Duplicate registration conflicts.
    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token), None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["plugin"]["status"], "enabled");
    assert_eq!(body["plugin"]["granted_permissions"], json!(["items:publish", "playback:resolve"]));
    assert!(!body["token"].as_str().unwrap().is_empty());

    let (status, body) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn approve_can_grant_a_subset_but_not_unrequested_permissions() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    call(&h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
         Some(json!({ "base_url": mock.base_url }))).await;

    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token),
        Some(json!({ "permissions": ["playback:resolve"] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token),
        Some(json!({ "permissions": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["plugin"]["granted_permissions"], json!([]));
}

#[tokio::test]
async fn register_rejects_unreachable_plugin_and_bad_manifests() {
    let h = build(enabled_settings()).await;

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": "http://127.0.0.1:1" })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "PLUGIN_UNAVAILABLE");

    let mock = spawn_mock("hello", &[]).await;
    let mut m = manifest_json("hello", &[]);
    m["api_version"] = json!(2);
    mock.set_manifest(m);
    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": "ftp://example.com" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn enable_disable_and_remove_with_and_without_purge() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    register_and_approve(&h, &mock).await;
    h.lib_repo
        .create_virtual("hello", "lib-1", "Hello", MediaType::LiveChannel)
        .await
        .unwrap();

    let (s, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/disable", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "disabled");
    let (s, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/enable", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "enabled");

    // Remove without purge keeps the library.
    let (s, _) = call(&h.app, "DELETE", "/api/v1/admin/plugins/hello", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(h.plugin_repo.get("hello").await.unwrap().is_none());
    assert!(h.lib_repo.get_by_id("lib-1").await.unwrap().is_some());

    // Re-register and remove with purge deletes it.
    register_and_approve(&h, &mock).await;
    let (s, _) = call(&h.app, "DELETE", "/api/v1/admin/plugins/hello?purge=true", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(h.lib_repo.get_by_id("lib-1").await.unwrap().is_none());

    let (s, _) = call(&h.app, "DELETE", "/api/v1/admin/plugins/hello", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pending_plugins_cannot_be_enabled_before_approval() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    call(&h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
         Some(json!({ "base_url": mock.base_url }))).await;
    let (s, _) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/enable", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_admin_test`
Expected: FAIL to compile (`PluginService`, routes missing).

- [ ] **Step 4: Implement `service.rs`**

Create `crates/kadr-server/src/plugins/service.rs`:

```rust
use crate::config::{AppConfig, PluginSettings};
use crate::events::EventBus;
use crate::plugins::auth::{derive_plugin_secret, PluginAuth, PluginTokenService};
use crate::plugins::client::{PluginClient, PluginError};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use kadr_core::plugin::{PlaybackDescriptor, PluginPermission, PluginRecord, PluginStatus};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PluginRepository};
use kadr_storage::StorageError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Plugins are disabled")]
    Disabled,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Forbidden(String),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Disabled => (StatusCode::NOT_FOUND, "PLUGINS_DISABLED".to_string()),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND".to_string()),
            Self::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT".to_string()),
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "INVALID_REQUEST".to_string()),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN".to_string()),
            Self::Plugin(e) => (e.status(), e.code().to_string()),
            Self::Storage(e) => {
                tracing::error!("Plugin storage error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR".to_string())
            }
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL".to_string()),
        };
        let message = match &self {
            Self::Storage(_) => "Database error".to_string(),
            other => other.to_string(),
        };
        (status, Json(serde_json::json!({ "error": code, "message": message }))).into_response()
    }
}

pub(crate) struct CachedDescriptor {
    pub descriptor: PlaybackDescriptor,
    pub expires_at: Instant,
}

struct Inner {
    repo: PluginRepository,
    libs: LibraryRepository,
    media: MediaItemRepository,
    tokens: PluginTokenService,
    server_secret: Vec<u8>,
    config: Arc<RwLock<AppConfig>>,
    events: Arc<EventBus>,
    client: PluginClient,
    /// Short-lived, in-memory playback descriptor cache keyed by (item id, user id).
    pub(crate) cache: Mutex<HashMap<(i64, String), CachedDescriptor>>,
}

#[derive(Clone)]
pub struct PluginService {
    inner: Arc<Inner>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn normalize_base_url(raw: &str) -> Result<String, ServiceError> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|e| ServiceError::Invalid(format!("invalid base_url: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ServiceError::Invalid(
            "base_url must use http or https".to_string(),
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

impl PluginService {
    pub fn new(
        pool: deadpool_sqlite::Pool,
        server_secret: &[u8],
        config: Arc<RwLock<AppConfig>>,
        events: Arc<EventBus>,
    ) -> Self {
        let settings = config
            .try_read()
            .map(|c| c.plugins.clone())
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                repo: PluginRepository::new(pool.clone()),
                libs: LibraryRepository::new(pool.clone()),
                media: MediaItemRepository::new(pool),
                tokens: PluginTokenService::new(server_secret),
                server_secret: server_secret.to_vec(),
                config,
                events,
                client: PluginClient::new(Duration::from_secs(settings.resolve_timeout_secs.max(1))),
                cache: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn repo(&self) -> &PluginRepository {
        &self.inner.repo
    }
    pub fn libs(&self) -> &LibraryRepository {
        &self.inner.libs
    }
    pub fn media(&self) -> &MediaItemRepository {
        &self.inner.media
    }
    pub fn events(&self) -> &Arc<EventBus> {
        &self.inner.events
    }
    pub fn client(&self) -> &PluginClient {
        &self.inner.client
    }
    pub(crate) fn cache(&self) -> &Mutex<HashMap<(i64, String), CachedDescriptor>> {
        &self.inner.cache
    }

    pub fn secret_for(&self, plugin_id: &str) -> String {
        derive_plugin_secret(&self.inner.server_secret, plugin_id)
    }

    pub async fn settings(&self) -> PluginSettings {
        self.inner.config.read().await.plugins.clone()
    }

    pub async fn ensure_enabled(&self) -> Result<(), ServiceError> {
        if self.settings().await.enabled {
            Ok(())
        } else {
            Err(ServiceError::Disabled)
        }
    }

    pub async fn list(&self) -> Result<Vec<PluginRecord>, ServiceError> {
        self.ensure_enabled().await?;
        Ok(self.inner.repo.list().await?)
    }

    async fn get_or_404(&self, id: &str) -> Result<PluginRecord, ServiceError> {
        self.inner
            .repo
            .get(id)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("plugin '{id}' not found")))
    }

    /// Fetches and validates the manifest, then stores the plugin as `pending`.
    /// Returns the record and the derived server-to-plugin secret.
    pub async fn register(&self, base_url: &str) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let base_url = normalize_base_url(base_url)?;
        let manifest = self.inner.client.fetch_manifest(&base_url).await?;
        manifest.validate().map_err(ServiceError::Invalid)?;
        if self.inner.repo.get(&manifest.id).await?.is_some() {
            return Err(ServiceError::Conflict(format!(
                "plugin '{}' is already registered",
                manifest.id
            )));
        }
        let record = PluginRecord {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            base_url,
            manifest,
            granted_permissions: vec![],
            status: PluginStatus::Pending,
            created_at: now_secs(),
        };
        self.inner.repo.insert(&record).await?;
        let secret = self.secret_for(&record.id);
        Ok((record, secret))
    }

    /// Grants permissions (default: everything the manifest requested), enables the plugin
    /// and issues its plugin-to-server token.
    pub async fn approve(
        &self,
        id: &str,
        requested: Option<Vec<PluginPermission>>,
    ) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        let mut granted = match requested {
            None => record.manifest.permissions.clone(),
            Some(perms) => {
                if let Some(bad) = perms
                    .iter()
                    .find(|p| !record.manifest.permissions.contains(p))
                {
                    return Err(ServiceError::Invalid(format!(
                        "permission '{}' was not requested in the manifest",
                        bad.as_str()
                    )));
                }
                perms
            }
        };
        granted.sort_by_key(|p| p.as_str());
        granted.dedup();

        self.inner.repo.set_granted(id, &granted).await?;
        self.inner.repo.set_status(id, PluginStatus::Enabled).await?;
        let token = self
            .inner
            .tokens
            .issue(id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))?;
        Ok((self.get_or_404(id).await?, token))
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        if record.status == PluginStatus::Pending {
            return Err(ServiceError::Invalid(
                "plugin has not been approved yet".to_string(),
            ));
        }
        let status = if enabled {
            PluginStatus::Enabled
        } else {
            PluginStatus::Disabled
        };
        self.inner.repo.set_status(id, status).await?;
        self.get_or_404(id).await
    }

    pub async fn remove(&self, id: &str, purge: bool) -> Result<(), ServiceError> {
        self.ensure_enabled().await?;
        self.get_or_404(id).await?;
        if purge {
            self.inner.libs.delete_by_plugin(id).await?;
        }
        self.inner.repo.delete(id).await?;
        Ok(())
    }

    /// Authorizes a plugin-initiated call: the plugin must exist, be enabled (or unhealthy)
    /// and hold `perm`.
    pub async fn authorize(
        &self,
        auth: &PluginAuth,
        perm: PluginPermission,
    ) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        let record = self
            .inner
            .repo
            .get(&auth.plugin_id)
            .await?
            .ok_or_else(|| ServiceError::Forbidden("plugin is not registered".to_string()))?;
        if !matches!(record.status, PluginStatus::Enabled | PluginStatus::Unhealthy) {
            return Err(ServiceError::Forbidden("plugin is not enabled".to_string()));
        }
        if !record.has_permission(perm) {
            return Err(ServiceError::Forbidden(format!(
                "missing permission '{}'",
                perm.as_str()
            )));
        }
        Ok(record)
    }
}
```

- [ ] **Step 5: Implement the admin routes**

Create `crates/kadr-server/src/plugins/admin_routes.rs`:

```rust
use crate::auth::jwt::RequireAdmin;
use crate::plugins::service::{PluginService, ServiceError};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use kadr_core::plugin::{PluginPermission, PluginRecord};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct RegisterBody {
    pub base_url: String,
}

#[derive(Deserialize, Default)]
pub struct ApproveBody {
    pub permissions: Option<Vec<PluginPermission>>,
}

#[derive(Deserialize)]
pub struct RemoveQuery {
    #[serde(default)]
    pub purge: bool,
}

pub fn admin_routes() -> Router {
    Router::new()
        .route(
            "/api/v1/admin/plugins",
            get(list_plugins).post(register_plugin),
        )
        .route("/api/v1/admin/plugins/{id}/approve", post(approve_plugin))
        .route("/api/v1/admin/plugins/{id}/enable", post(enable_plugin))
        .route("/api/v1/admin/plugins/{id}/disable", post(disable_plugin))
        .route("/api/v1/admin/plugins/{id}", delete(remove_plugin))
}

async fn list_plugins(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<Vec<PluginRecord>>, ServiceError> {
    Ok(Json(svc.list().await?))
}

async fn register_plugin(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<RegisterBody>,
) -> Result<(StatusCode, Json<Value>), ServiceError> {
    let (plugin, secret) = svc.register(&body.base_url).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "plugin": plugin, "secret": secret })),
    ))
}

async fn approve_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
    body: Option<Json<ApproveBody>>,
) -> Result<Json<Value>, ServiceError> {
    let requested = body.and_then(|Json(b)| b.permissions);
    let (plugin, token) = svc.approve(&id, requested).await?;
    Ok(Json(json!({ "plugin": plugin, "token": token })))
}

async fn enable_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<PluginRecord>, ServiceError> {
    Ok(Json(svc.set_enabled(&id, true).await?))
}

async fn disable_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<PluginRecord>, ServiceError> {
    Ok(Json(svc.set_enabled(&id, false).await?))
}

async fn remove_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Query(q): Query<RemoveQuery>,
    Extension(svc): Extension<PluginService>,
) -> Result<StatusCode, ServiceError> {
    svc.remove(&id, q.purge).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

Replace `crates/kadr-server/src/plugins/mod.rs` with:

```rust
pub mod admin_routes;
pub mod auth;
pub mod client;
pub mod service;
pub mod url_guard;

pub use service::{PluginService, ServiceError};

/// All plugin-related HTTP routes (admin registration and plugin publishing).
pub fn plugin_routes() -> axum::Router {
    admin_routes::admin_routes()
}
```

- [ ] **Step 6: Wire into the router**

In `crates/kadr-server/src/api/mod.rs`, inside `create_router_with_ingest`, immediately after the `hls_manager` block (before `let router = Router::new()`), add:

```rust
    let plugin_service = crate::plugins::PluginService::new(
        media_repo.pool().clone(),
        jwt_svc.secret(),
        config.clone(),
        event_bus.clone(),
    );
    let plugin_tokens = crate::plugins::auth::PluginTokenService::new(jwt_svc.secret());
```

Change `.merge(hls_routes::hls_routes())` to:

```rust
        .merge(hls_routes::hls_routes())
        .merge(crate::plugins::plugin_routes())
```

and add to the extension layers (next to `.layer(Extension(identity))`):

```rust
        .layer(Extension(plugin_service))
        .layer(Extension(plugin_tokens))
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p kadr-server --test plugin_admin_test && cargo test --workspace`
Expected: PASS, including every pre-existing test.

- [ ] **Step 8: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(plugins): add plugin service, admin registration API and router wiring"
```


### Task 7: Plugin publishing API (virtual libraries and items)

**Files:**
- Create: `crates/kadr-server/src/plugins/publish_routes.rs`
- Modify: `crates/kadr-server/src/plugins/mod.rs`
- Test: `crates/kadr-server/tests/plugin_publish_test.rs`

**Interfaces:**
- Consumes: `PluginService::{authorize, libs, media, events}`, `PluginAuth`, `plugin_item_path`, `validate_remote_id`.
- Produces routes (all require a plugin token; all require the `items:publish` permission):
  - `POST /api/v1/plugin/libraries` body `{ "id"?: string, "name": string, "media_type"?: MediaType }` returns `201 { id, name, media_type }`
  - `PUT /api/v1/plugin/libraries/{id}/items` body `{ "items": [PublishItem] }` returns `200 { upserted }`
  - `DELETE /api/v1/plugin/libraries/{id}/items/{remote_id}` returns `204`
  - `PublishItem { remote_id, title, item_type?, original_title?, release_year?, duration_seconds=0, live=false, seekable?, resolution?, video_codec?, audio_codec?, container?, poster_url?, backdrop_url?, overview?, genres=[] }`

- [ ] **Step 1: Write the failing tests**

Create `crates/kadr-server/tests/plugin_publish_test.rs`:

```rust
mod plugin_support;

use axum::http::StatusCode;
use kadr_core::events::SystemEvent;
use kadr_core::models::MediaType;
use kadr_core::plugin::plugin_item_path;
use plugin_support::*;
use serde_json::json;
use std::time::Duration;

async fn setup() -> (Harness, MockPlugin, String) {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;
    let (token, _) = register_and_approve(&h, &mock).await;
    (h, mock, token)
}

async fn create_lib(h: &Harness, token: &str, id: &str) {
    let (status, body) = call(
        &h.app, "POST", "/api/v1/plugin/libraries", Some(token),
        Some(json!({ "id": id, "name": "Hello", "media_type": "live_channel" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn publish_update_and_delete_items() {
    let (h, _mock, token) = setup().await;
    create_lib(&h, &token, "lib-1").await;
    assert_eq!(h.lib_repo.plugin_id_of("lib-1").await.unwrap().as_deref(), Some("hello"));

    let (status, body) = call(
        &h.app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token),
        Some(json!({ "items": [
            { "remote_id": "ch1", "title": "Channel 1", "live": true, "genres": ["news"] },
            { "remote_id": "group/ch2", "title": "Channel 2", "duration_seconds": 90, "poster_url": "https://img/x.png" }
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["upserted"], 2);

    let items = h.media_repo.list_by_library("lib-1", 100, 0).await.unwrap();
    assert_eq!(items.len(), 2);
    let ch1 = h.media_repo.find_by_path(plugin_item_path("hello", "ch1")).await.unwrap().unwrap();
    assert!(ch1.is_live());
    assert_eq!(ch1.item_type, MediaType::LiveChannel, "defaults to the library media type");
    assert_eq!(ch1.metadata.genres, vec!["news".to_string()]);
    let ch2 = h.media_repo.find_by_path(plugin_item_path("hello", "group/ch2")).await.unwrap().unwrap();
    assert_eq!(ch2.technical.duration_seconds, 90);
    assert_eq!(ch2.metadata.extra["poster_url"], "https://img/x.png");

    // Re-upserting updates in place instead of duplicating.
    call(
        &h.app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token),
        Some(json!({ "items": [{ "remote_id": "ch1", "title": "Renamed", "live": true }] })),
    )
    .await;
    assert_eq!(h.media_repo.count_by_library("lib-1").await.unwrap(), 2);
    let ch1 = h.media_repo.find_by_path(plugin_item_path("hello", "ch1")).await.unwrap().unwrap();
    assert_eq!(ch1.title, "Renamed");

    let (status, _) = call(&h.app, "DELETE", "/api/v1/plugin/libraries/lib-1/items/ch1", Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(h.media_repo.count_by_library("lib-1").await.unwrap(), 1);
    let (status, _) = call(&h.app, "DELETE", "/api/v1/plugin/libraries/lib-1/items/ch1", Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // remote ids containing '/' are addressed with percent-encoding.
    let (status, _) = call(&h.app, "DELETE", "/api/v1/plugin/libraries/lib-1/items/group%2Fch2", Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn publishing_emits_a_library_updated_event() {
    let (h, _mock, token) = setup().await;
    create_lib(&h, &token, "lib-1").await;
    let mut rx = h.events.subscribe();
    call(
        &h.app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token),
        Some(json!({ "items": [{ "remote_id": "ch1", "title": "C" }] })),
    )
    .await;
    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
    match ev {
        SystemEvent::LibraryUpdated { library_id, item_count, .. } => {
            assert_eq!(library_id, "lib-1");
            assert_eq!(item_count, 1);
        }
        other => panic!("unexpected event {other:?}"),
    }
}

#[tokio::test]
async fn tokens_are_not_interchangeable() {
    let (h, _mock, plugin_token) = setup().await;
    let body = Some(json!({ "name": "X" }));

    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", None, body.clone()).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&h.user_token), body.clone()).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&h.admin_token), body).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (s, _) = call(&h.app, "GET", "/api/v1/auth/me", Some(&plugin_token), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "plugin token must not authenticate as a user");
    let (s, _) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&plugin_token), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "plugin token must not reach admin routes");
}

#[tokio::test]
async fn missing_permission_and_disabled_plugin_are_forbidden() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("resolver-only", &["playback:resolve"]).await;
    let (token, _) = register_and_approve(&h, &mock).await;
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "X" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (h, _mock, token) = setup().await;
    call(&h.app, "POST", "/api/v1/admin/plugins/hello/disable", Some(&h.admin_token), None).await;
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "X" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn plugins_cannot_touch_other_libraries() {
    let (h, _mock, token) = setup().await;
    h.lib_repo.create_virtual("other", "other-lib", "Other", MediaType::Movie).await.unwrap();
    h.lib_repo.create("real", "Real", "/tmp/real", MediaType::Movie, false, None).await.unwrap();
    let items = Some(json!({ "items": [{ "remote_id": "x", "title": "X" }] }));

    for lib in ["other-lib", "real", "does-not-exist"] {
        let (s, _) = call(&h.app, "PUT", &format!("/api/v1/plugin/libraries/{lib}/items"), Some(&token), items.clone()).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "library {lib}");
        let (s, _) = call(&h.app, "DELETE", &format!("/api/v1/plugin/libraries/{lib}/items/x"), Some(&token), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "library {lib}");
    }
    assert_eq!(h.media_repo.count_by_library("other-lib").await.unwrap(), 0);
    assert_eq!(h.media_repo.count_by_library("real").await.unwrap(), 0);
}

#[tokio::test]
async fn validation_errors() {
    let (h, _mock, token) = setup().await;
    create_lib(&h, &token, "lib-1").await;

    // duplicate library id, bad characters, empty name
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "id": "lib-1", "name": "Again" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "id": "bad id/..", "name": "X" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "  " }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // id omitted -> generated
    let (s, body) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "Auto" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(!body["id"].as_str().unwrap().is_empty());

    // empty remote id, empty title, oversized batch
    let put = |items: serde_json::Value| {
        let app = h.app.clone();
        let token = token.clone();
        async move {
            call(&app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token), Some(json!({ "items": items }))).await
        }
    };
    assert_eq!(put(json!([{ "remote_id": "", "title": "X" }])).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(put(json!([{ "remote_id": "a", "title": " " }])).await.0, StatusCode::BAD_REQUEST);
    let many: Vec<_> = (0..1001).map(|i| json!({ "remote_id": format!("r{i}"), "title": "T" })).collect();
    assert_eq!(put(json!(many)).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(h.media_repo.count_by_library("lib-1").await.unwrap(), 0, "rejected batches write nothing");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_publish_test`
Expected: FAIL (routes return 404 or compile errors).

- [ ] **Step 3: Implement the publish routes**

Create `crates/kadr-server/src/plugins/publish_routes.rs`:

```rust
use crate::plugins::auth::PluginAuth;
use crate::plugins::service::{PluginService, ServiceError};
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, post, put},
    Extension, Json, Router,
};
use kadr_core::events::SystemEvent;
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_core::plugin::{plugin_item_path, validate_remote_id, PluginPermission};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_BATCH: usize = 1000;

#[derive(Deserialize)]
pub struct CreateLibraryBody {
    pub id: Option<String>,
    pub name: String,
    pub media_type: Option<MediaType>,
}

#[derive(Deserialize)]
pub struct PublishItem {
    pub remote_id: String,
    pub title: String,
    pub item_type: Option<MediaType>,
    pub original_title: Option<String>,
    pub release_year: Option<i32>,
    #[serde(default)]
    pub duration_seconds: i64,
    #[serde(default)]
    pub live: bool,
    pub seekable: Option<bool>,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub overview: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
}

#[derive(Deserialize)]
pub struct PublishBody {
    pub items: Vec<PublishItem>,
}

pub fn publish_routes() -> Router {
    Router::new()
        .route("/api/v1/plugin/libraries", post(create_library))
        .route("/api/v1/plugin/libraries/{id}/items", put(upsert_items))
        .route(
            "/api/v1/plugin/libraries/{id}/items/{remote_id}",
            delete(delete_item),
        )
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn valid_library_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Returns the library only when it exists and is owned by the calling plugin.
/// Other plugins' libraries and regular libraries look identical to "not found".
async fn owned_library(
    svc: &PluginService,
    auth: &PluginAuth,
    library_id: &str,
) -> Result<Library, ServiceError> {
    let not_found = || ServiceError::NotFound("library not found".to_string());
    match svc.libs().plugin_id_of(library_id).await? {
        Some(owner) if owner == auth.plugin_id => {}
        _ => return Err(not_found()),
    }
    svc.libs().get_by_id(library_id).await?.ok_or_else(not_found)
}

async fn create_library(
    auth: PluginAuth,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<CreateLibraryBody>,
) -> Result<(StatusCode, Json<Value>), ServiceError> {
    svc.authorize(&auth, PluginPermission::ItemsPublish).await?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ServiceError::Invalid("name must not be empty".to_string()));
    }
    let id = body
        .id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if !valid_library_id(&id) {
        return Err(ServiceError::Invalid(
            "library id must be 1-64 characters of [A-Za-z0-9_-]".to_string(),
        ));
    }
    if svc.libs().get_by_id(&id).await?.is_some() {
        return Err(ServiceError::Conflict(format!("library '{id}' already exists")));
    }
    let library = svc
        .libs()
        .create_virtual(
            &auth.plugin_id,
            &id,
            &name,
            body.media_type.unwrap_or(MediaType::Unknown),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": library.id,
            "name": library.name,
            "media_type": library.media_type,
        })),
    ))
}

fn build_item(
    plugin_id: &str,
    library: &Library,
    p: PublishItem,
) -> Result<MediaItem, ServiceError> {
    validate_remote_id(&p.remote_id).map_err(ServiceError::Invalid)?;
    let title = p.title.trim().to_string();
    if title.is_empty() {
        return Err(ServiceError::Invalid(format!(
            "title must not be empty (remote_id '{}')",
            p.remote_id
        )));
    }
    let mut metadata = MediaMetadata {
        overview: p.overview,
        genres: p.genres,
        ..Default::default()
    };
    // Artwork URLs are kept in `extra`, never in `poster_path`/`backdrop_path`, which the
    // artwork routes interpret as local files.
    if let Some(url) = p.poster_url {
        metadata.extra.insert("poster_url".into(), json!(url));
    }
    if let Some(url) = p.backdrop_url {
        metadata.extra.insert("backdrop_url".into(), json!(url));
    }
    if p.live {
        metadata.extra.insert("live".into(), json!(true));
    }
    if let Some(seekable) = p.seekable {
        metadata.extra.insert("seekable".into(), json!(seekable));
    }
    Ok(MediaItem {
        id: None,
        library_id: library.id.clone(),
        item_type: p.item_type.unwrap_or(library.media_type),
        title: title.clone(),
        original_title: p.original_title,
        release_year: p.release_year,
        added_at: now_secs(),
        file_path: plugin_item_path(plugin_id, &p.remote_id),
        file_name: title.replace('/', "_"),
        file_size: 0,
        technical: TechnicalInfo {
            duration_seconds: p.duration_seconds,
            resolution: p.resolution,
            video_codec: p.video_codec,
            audio_codec: p.audio_codec,
            audio_channels: None,
            container: p.container,
        },
        metadata,
    })
}

async fn upsert_items(
    auth: PluginAuth,
    Path(library_id): Path<String>,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<PublishBody>,
) -> Result<Json<Value>, ServiceError> {
    svc.authorize(&auth, PluginPermission::ItemsPublish).await?;
    let library = owned_library(&svc, &auth, &library_id).await?;
    if body.items.len() > MAX_BATCH {
        return Err(ServiceError::Invalid(format!(
            "at most {MAX_BATCH} items per request"
        )));
    }
    let items = body
        .items
        .into_iter()
        .map(|p| build_item(&auth.plugin_id, &library, p))
        .collect::<Result<Vec<_>, _>>()?;
    let upserted = svc.media().upsert_batch(&items).await?;
    let total = svc.media().count_by_library(&library_id).await?;
    svc.events().publish(SystemEvent::LibraryUpdated {
        library_id,
        item_count: total,
        timestamp: now_secs(),
    });
    Ok(Json(json!({ "upserted": upserted })))
}

async fn delete_item(
    auth: PluginAuth,
    Path((library_id, remote_id)): Path<(String, String)>,
    Extension(svc): Extension<PluginService>,
) -> Result<StatusCode, ServiceError> {
    svc.authorize(&auth, PluginPermission::ItemsPublish).await?;
    owned_library(&svc, &auth, &library_id).await?;
    let path = plugin_item_path(&auth.plugin_id, &remote_id);
    match svc.media().find_by_path(&path).await? {
        Some(item) if item.library_id == library_id => {
            svc.media().delete_by_path(&path).await?;
            let total = svc.media().count_by_library(&library_id).await?;
            svc.events().publish(SystemEvent::LibraryUpdated {
                library_id,
                item_count: total,
                timestamp: now_secs(),
            });
            Ok(StatusCode::NO_CONTENT)
        }
        _ => Err(ServiceError::NotFound("item not found".to_string())),
    }
}
```

In `crates/kadr-server/src/plugins/mod.rs` add `pub mod publish_routes;` and change `plugin_routes()` to:

```rust
pub fn plugin_routes() -> axum::Router {
    admin_routes::admin_routes().merge(publish_routes::publish_routes())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p kadr-server --test plugin_publish_test && cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(plugins): add plugin publishing API for virtual libraries and items"
```

---

### Task 8: Playback resolution (direct, proxy, transcode) for plugin items

**Files:**
- Create: `crates/kadr-server/src/plugins/playback.rs`
- Modify: `crates/kadr-server/src/plugins/mod.rs`, `crates/kadr-server/src/plugins/service.rs`, `crates/kadr-server/src/streaming/handler.rs`, `crates/kadr-server/src/api/hls_routes.rs`, `crates/kadr-server/tests/plugin_support/mod.rs`
- Test: `crates/kadr-server/tests/plugin_playback_test.rs`

**Interfaces:**
- Consumes: `PluginService`, `PluginClient::resolve`, `validate_descriptor`, `PlaybackSource`, `HlsSessionManager::get_or_create_session_for_source`.
- Produces:
  - `PluginService::resolve_item(&self, item: &MediaItem, user_id: &str, client: ClientCapabilities) -> Result<PlaybackDescriptor, PluginError>` (validates, caches by `(item id, user id)` for `expires_in_secs`)
  - `plugins::playback::{plugin_error_response(&PluginError) -> Response, stream_plugin_item(&PluginService, &MediaItem, user_id: &str, &HeaderMap) -> Response}`
  - `GET /api/v1/stream/{id}` for plugin items: `direct` returns `307` with `Location`; `proxy` streams the upstream (forwarding `Range` for seekable items); `transcode` returns `409 PLAYBACK_REQUIRES_TRANSCODE` with `hls_url`.
  - `GET /api/v1/media/{id}/hls/master.m3u8` for plugin items: `proxy` and `transcode` feed the URL to ffmpeg; `direct` returns `409 PLAYBACK_DIRECT_ONLY` with `url`.

- [ ] **Step 1: Add the fake ffmpeg helper to the test support**

Append to `crates/kadr-server/tests/plugin_support/mod.rs`:

```rust
/// Writes a fake `ffmpeg` shell script that logs its arguments and creates a playlist at the
/// last argument. Returns `(script_path, args_log_path)`.
pub fn fake_ffmpeg(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let log = dir.join("ffmpeg-args.log");
    let script = dir.join("fake-ffmpeg.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {log}\nfor last; do :; done\nprintf '#EXTM3U\\n' > \"$last\"\n",
            log = log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, log)
}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/kadr-server/tests/plugin_playback_test.rs`:

```rust
mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::{Library, MediaItem, MediaType};
use kadr_core::plugin::plugin_item_path;
use kadr_server::config::{PluginSettings, TranscodingSettings};
use kadr_server::HlsSessionManager;
use plugin_support::*;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;

async fn seed_item(h: &Harness, library_id: &str, plugin_id: &str, remote_id: &str, live: bool) -> MediaItem {
    if h.lib_repo.get_by_id(library_id).await.unwrap().is_none() {
        h.lib_repo
            .create_virtual(plugin_id, library_id, "Lib", MediaType::Movie)
            .await
            .unwrap();
    }
    let mut item = MediaItem {
        library_id: library_id.to_string(),
        item_type: MediaType::Movie,
        title: remote_id.to_string(),
        file_path: plugin_item_path(plugin_id, remote_id),
        file_name: remote_id.to_string(),
        added_at: 1,
        ..Default::default()
    };
    if live {
        item.metadata.extra.insert("live".into(), json!(true));
    }
    h.media_repo.upsert_batch(&[item]).await.unwrap();
    h.media_repo
        .find_by_path(plugin_item_path(plugin_id, remote_id))
        .await
        .unwrap()
        .unwrap()
}

async fn setup(settings: PluginSettings) -> (Harness, MockPlugin) {
    let h = build(settings).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;
    register_and_approve(&h, &mock).await;
    (h, mock)
}

fn stream_uri(item: &MediaItem) -> String {
    format!("/api/v1/stream/{}", item.id.unwrap())
}

async fn get(h: &Harness, uri: &str, headers: &[(&str, &str)]) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    call_raw(&h.app, "GET", uri, Some(&h.user_token), headers, None).await
}

fn json_body(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn direct_mode_redirects_to_the_descriptor_url() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, headers, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(headers["location"], "https://cdn.example.com/video.mp4");
    assert_eq!(mock.last_resolve_body.lock().unwrap().as_ref().unwrap()["remote_id"], "m1");
    assert_eq!(mock.last_resolve_body.lock().unwrap().as_ref().unwrap()["user_id"], "viewer");
}

#[tokio::test]
async fn proxy_mode_streams_the_upstream_body_and_forwards_ranges() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({ "mode": "proxy", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, headers, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"0123456789");
    assert_eq!(headers["content-type"], "video/mp4");

    let (status, headers, body) = get(&h, &stream_uri(&item), &[("range", "bytes=2-5")]).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, b"2345");
    assert_eq!(headers["content-range"], "bytes 2-5/10");
}

#[tokio::test]
async fn proxy_mode_ignores_ranges_for_live_items() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({ "mode": "proxy", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "live1", true).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[("range", "bytes=2-5")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"0123456789");
}

#[tokio::test]
async fn transcode_mode_points_clients_at_the_hls_endpoint() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({ "mode": "transcode", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let v = json_body(&body);
    assert_eq!(v["error"], "PLAYBACK_REQUIRES_TRANSCODE");
    assert_eq!(v["hls_url"], format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap()));
}

#[tokio::test]
async fn plugin_failures_map_to_503_504_502() {
    let (h, mock) = setup(PluginSettings { resolve_timeout_secs: 1, ..enabled_settings() }).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    mock.resolve_status.store(500, Ordering::SeqCst);
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(&body)["error"], "PLUGIN_UNAVAILABLE");

    mock.resolve_status.store(0, Ordering::SeqCst);
    mock.resolve_delay_ms.store(2500, Ordering::SeqCst);
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(json_body(&body)["error"], "PLUGIN_TIMEOUT");

    mock.resolve_delay_ms.store(0, Ordering::SeqCst);
    mock.set_descriptor(json!({ "nope": 1 }));
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json_body(&body)["error"], "PLUGIN_BAD_RESPONSE");
}

#[tokio::test]
async fn private_proxy_targets_are_rejected_unless_allowed() {
    let (h, mock) = setup(PluginSettings { allow_private_proxy: false, ..enabled_settings() }).await;
    mock.set_descriptor(json!({ "mode": "proxy", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json_body(&body)["error"], "PLUGIN_BAD_RESPONSE");
}

#[tokio::test]
async fn proxy_does_not_follow_upstream_redirects() {
    let (h, mock) = setup(enabled_settings()).await;
    // The mock's /health answers 200, so point at a path that does not exist: 404 -> 502.
    mock.set_descriptor(json!({ "mode": "proxy", "url": format!("{}/missing", mock.base_url) }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;
    let (status, _, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn descriptors_are_cached_only_when_the_plugin_sets_expires_in_secs() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    get(&h, &stream_uri(&item), &[]).await;
    get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(mock.calls(), 2, "no expires_in_secs -> never cached");

    mock.set_descriptor(json!({ "mode": "direct", "url": "https://cdn.example.com/a.mp4", "expires_in_secs": 60 }));
    get(&h, &stream_uri(&item), &[]).await;
    get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(mock.calls(), 3, "second request served from cache");

    // A different user has their own cache entry.
    call_raw(&h.app, "GET", &stream_uri(&item), Some(&h.admin_token), &[], None).await;
    assert_eq!(mock.calls(), 4);
}

#[tokio::test]
async fn unavailable_plugins_are_never_called() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    call(&h.app, "POST", "/api/v1/admin/plugins/hello/disable", Some(&h.admin_token), None).await;
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(&body)["error"], "PLUGIN_UNAVAILABLE");
    assert_eq!(mock.calls(), 0);

    // Plugin removed entirely (no purge): items remain but cannot be played.
    call(&h.app, "DELETE", "/api/v1/admin/plugins/hello", Some(&h.admin_token), None).await;
    let (status, _, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(mock.calls(), 0);
}

#[tokio::test]
async fn locked_libraries_never_trigger_a_resolve_call() {
    let (h, mock) = setup(enabled_settings()).await;
    h.lib_repo
        .insert(&Library {
            id: "priv".into(),
            name: "Private".into(),
            path: "plugin://hello/library/priv".into(),
            paths: vec![],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some(kadr_server::auth::pin::hash_pin("1234").unwrap()),
            created_at: 0,
        })
        .await
        .unwrap();
    let item = seed_item(&h, "priv", "hello", "secret", false).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json_body(&body)["error"], "LIBRARY_LOCKED");
    assert_eq!(mock.calls(), 0);
}

#[tokio::test]
async fn hls_feeds_proxy_and_transcode_urls_to_ffmpeg_and_refuses_direct() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let dir = tempfile::tempdir().unwrap();
    let (script, log) = fake_ffmpeg(dir.path());
    let mgr = Arc::new(HlsSessionManager::new(TranscodingSettings {
        ffmpeg_path: script.to_string_lossy().to_string(),
        temp_dir: dir.path().join("sessions"),
        ..Default::default()
    }));
    let app = h.app.clone().layer(axum::Extension(mgr));
    let hls = format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap());

    // direct -> refuse (direct URLs are not SSRF-validated, so they are never given to ffmpeg)
    let (status, _, body) = call_raw(&app, "GET", &hls, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json_body(&body)["error"], "PLAYBACK_DIRECT_ONLY");
    assert!(!log.exists());

    mock.set_descriptor(json!({ "mode": "transcode", "url": "http://8.8.8.8/live.ts", "headers": { "X-Token": "abc" } }));
    let (status, _, _) = call_raw(&app, "GET", &hls, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::OK);
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("http://8.8.8.8/live.ts"), "{logged}");
    assert!(logged.contains("-protocol_whitelist"));
    assert!(logged.contains("X-Token: abc"));
}

#[tokio::test]
async fn file_items_are_unaffected() {
    let (h, _mock) = setup(enabled_settings()).await;
    h.lib_repo.create("real", "Real", "/tmp/real", MediaType::Movie, false, None).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("movie.mp4");
    std::fs::write(&file, b"abcdefghij").unwrap();
    h.media_repo
        .upsert_batch(&[MediaItem {
            library_id: "real".into(),
            item_type: MediaType::Movie,
            title: "Movie".into(),
            file_path: file.clone(),
            file_name: "movie.mp4".into(),
            file_size: 10,
            added_at: 1,
            ..Default::default()
        }])
        .await
        .unwrap();
    let item = h.media_repo.find_by_path(&file).await.unwrap().unwrap();

    let (status, _, body) = get(&h, &stream_uri(&item), &[("range", "bytes=0-3")]).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, b"abcd");
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_playback_test`
Expected: FAIL (plugin items are still opened as files, so they return 404 `Media file missing from disk`).

- [ ] **Step 4: Add the proxy HTTP client to the service**

In `crates/kadr-server/src/plugins/service.rs`:

1. Add a field to `struct Inner`, below `client`:

```rust
    /// Used to fetch upstream media in `proxy` mode: never follows redirects, no total timeout.
    proxy_http: reqwest::Client,
```

2. Initialize it in `PluginService::new` next to `client:`:

```rust
                proxy_http: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(Duration::from_secs(10))
                    .build()
                    .expect("reqwest client builds with static configuration"),
```

3. Add an accessor next to `client()`:

```rust
    pub fn proxy_client(&self) -> &reqwest::Client {
        &self.inner.proxy_http
    }
```

- [ ] **Step 5: Implement resolution and streaming**

Create `crates/kadr-server/src/plugins/playback.rs`:

```rust
use crate::plugins::client::PluginError;
use crate::plugins::service::{CachedDescriptor, PluginService};
use crate::plugins::url_guard::validate_descriptor;
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use kadr_core::models::MediaItem;
use kadr_core::plugin::{
    ClientCapabilities, PlaybackDescriptor, PlaybackMode, PluginPermission, PluginStatus,
    ResolveRequest,
};
use std::time::{Duration, Instant};

const MAX_CACHE_ENTRIES: usize = 1024;

pub fn plugin_error_response(err: &PluginError) -> Response {
    (
        err.status(),
        Json(serde_json::json!({ "error": err.code(), "message": err.to_string() })),
    )
        .into_response()
}

impl PluginService {
    /// Asks the owning plugin how to play `item`, validates the answer and caches it for
    /// `expires_in_secs` (per item and user). Never called for plugins that are missing,
    /// disabled, pending or unhealthy.
    pub async fn resolve_item(
        &self,
        item: &MediaItem,
        user_id: &str,
        client: ClientCapabilities,
    ) -> Result<PlaybackDescriptor, PluginError> {
        let Some(pref) = item.plugin_ref() else {
            return Err(PluginError::BadResponse(
                "item is not owned by a plugin".to_string(),
            ));
        };
        let settings = self.settings().await;
        if !settings.enabled {
            return Err(PluginError::Unavailable("plugins are disabled".to_string()));
        }
        let record = self
            .repo()
            .get(&pref.plugin_id)
            .await
            .map_err(|e| PluginError::Unavailable(format!("storage error: {e}")))?
            .ok_or_else(|| PluginError::Unavailable("plugin is not installed".to_string()))?;
        if record.status != PluginStatus::Enabled {
            return Err(PluginError::Unavailable(format!(
                "plugin is {}",
                record.status.as_str()
            )));
        }
        if !record.has_permission(PluginPermission::PlaybackResolve) {
            return Err(PluginError::Unavailable(
                "plugin lacks the playback:resolve permission".to_string(),
            ));
        }

        let key = (item.id.unwrap_or_default(), user_id.to_string());
        {
            let mut cache = self.cache().lock().await;
            if let Some(cached) = cache.get(&key) {
                if cached.expires_at > Instant::now() {
                    return Ok(cached.descriptor.clone());
                }
            }
            cache.remove(&key);
        }

        let request = ResolveRequest {
            remote_id: pref.remote_id,
            user_id: user_id.to_string(),
            client,
        };
        let descriptor = self
            .client()
            .resolve(
                &record.id,
                &record.base_url,
                &self.secret_for(&record.id),
                &request,
            )
            .await?;
        validate_descriptor(&descriptor, settings.allow_private_proxy)
            .await
            .map_err(PluginError::BadResponse)?;

        if let Some(ttl) = descriptor.expires_in_secs.filter(|t| *t > 0) {
            let now = Instant::now();
            let mut cache = self.cache().lock().await;
            if cache.len() >= MAX_CACHE_ENTRIES {
                cache.retain(|_, c| c.expires_at > now);
            }
            if cache.len() < MAX_CACHE_ENTRIES {
                cache.insert(
                    key,
                    CachedDescriptor {
                        descriptor: descriptor.clone(),
                        expires_at: now + Duration::from_secs(ttl),
                    },
                );
            }
        }
        Ok(descriptor)
    }
}

/// Direct-play endpoint behavior for plugin items.
pub async fn stream_plugin_item(
    svc: &PluginService,
    item: &MediaItem,
    user_id: &str,
    request_headers: &HeaderMap,
) -> Response {
    let descriptor = match svc
        .resolve_item(item, user_id, ClientCapabilities::default())
        .await
    {
        Ok(d) => d,
        Err(e) => return plugin_error_response(&e),
    };
    match descriptor.mode {
        PlaybackMode::Direct => {
            // Re-serialize through the URL parser so the header value is always valid ASCII.
            let location = reqwest::Url::parse(&descriptor.url)
                .ok()
                .and_then(|u| HeaderValue::from_str(u.as_str()).ok());
            match location {
                Some(value) => Response::builder()
                    .status(StatusCode::TEMPORARY_REDIRECT)
                    .header(header::LOCATION, value)
                    .body(Body::empty())
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
                None => plugin_error_response(&PluginError::BadResponse(
                    "descriptor url is not a valid location".to_string(),
                )),
            }
        }
        PlaybackMode::Transcode => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "PLAYBACK_REQUIRES_TRANSCODE",
                "hls_url": format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap_or_default()),
            })),
        )
            .into_response(),
        PlaybackMode::Proxy => proxy_upstream(svc, item, &descriptor, request_headers).await,
    }
}

async fn proxy_upstream(
    svc: &PluginService,
    item: &MediaItem,
    descriptor: &PlaybackDescriptor,
    request_headers: &HeaderMap,
) -> Response {
    let mut upstream_req = svc.proxy_client().get(&descriptor.url);
    for (name, value) in &descriptor.headers {
        upstream_req = upstream_req.header(name, value);
    }
    if item.is_seekable() {
        if let Some(range) = request_headers.get(header::RANGE) {
            upstream_req = upstream_req.header(header::RANGE, range.clone());
        }
    }
    let upstream = match upstream_req.send().await {
        Ok(r) => r,
        Err(e) if e.is_timeout() => return plugin_error_response(&PluginError::Timeout),
        Err(e) => {
            return plugin_error_response(&PluginError::Unavailable(format!(
                "upstream request failed: {e}"
            )))
        }
    };

    let status = upstream.status();
    if !matches!(status.as_u16(), 200 | 206 | 416) {
        return plugin_error_response(&PluginError::BadResponse(format!(
            "upstream returned {status} (redirects are not followed)"
        )));
    }

    let content_type = descriptor
        .content_type
        .clone()
        .or_else(|| {
            upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let mut builder = Response::builder()
        .status(status.as_u16())
        .header(header::CONTENT_TYPE, content_type);
    for name in [header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES] {
        if let Some(v) = upstream.headers().get(&name) {
            builder = builder.header(name, v.clone());
        }
    }

    let stream = futures::stream::unfold(upstream, |mut resp| async move {
        match resp.chunk().await {
            Ok(Some(bytes)) => Some((Ok::<_, std::io::Error>(bytes), resp)),
            Ok(None) => None,
            Err(e) => Some((Err(std::io::Error::other(e)), resp)),
        }
    });
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
```

Add `pub mod playback;` to `crates/kadr-server/src/plugins/mod.rs`.

- [ ] **Step 6: Branch the direct-play handler**

In `crates/kadr-server/src/streaming/handler.rs`:

1. Add imports: `use crate::plugins::PluginService;`.
2. Change the signature: rename `_auth_user: Option<AuthUser>` to `auth_user: Option<AuthUser>` and add the parameter `plugins: Option<Extension<PluginService>>,` after `Extension(lib_repo): Extension<LibraryRepository>,`.
3. Immediately after the library-lock `match lib_repo.get_by_id(...)` block (which ends just before `let file = match File::open(&item.file_path).await {`), insert:

```rust
    if item.plugin_ref().is_some() {
        let Some(Extension(plugins)) = plugins else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "PLUGIN_UNAVAILABLE", "message": "Plugins are not available" })),
            )
                .into_response();
        };
        let user_id = auth_user
            .as_ref()
            .map(|u| u.id.clone())
            .unwrap_or_else(|| "anonymous".to_string());
        return crate::plugins::playback::stream_plugin_item(&plugins, &item, &user_id, &headers)
            .await;
    }
```

- [ ] **Step 7: Branch the HLS handler**

In `crates/kadr-server/src/api/hls_routes.rs`:

1. Add imports:

```rust
use crate::auth::jwt::AuthUser;
use crate::plugins::playback::plugin_error_response;
use crate::plugins::PluginService;
use kadr_core::plugin::{ClientCapabilities, PlaybackMode, PlaybackSource};
```

2. Add two parameters to `master_playlist` (after `unlocked: UnlockedLibraries,`):

```rust
    auth_user: Option<AuthUser>,
    plugins: Option<Extension<PluginService>>,
```

3. Replace this block (the `try_exists` check, `let params = ...`, and the `get_or_create_session(...)` call):

```rust
    if !tokio::fs::try_exists(&item.file_path)
        .await
        .unwrap_or(false)
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media file missing from disk" })),
        )
            .into_response();
    }

    let params = TranscodeParameters::from(query);
    let session = match session_mgr
        .get_or_create_session(
            item.id.unwrap_or(item_id),
            &item.file_path,
            &item.technical,
            &params,
        )
        .await
    {
```

with:

```rust
    let source = if item.plugin_ref().is_some() {
        let Some(Extension(plugins)) = plugins else {
            return plugin_error_response(&crate::plugins::client::PluginError::Unavailable(
                "Plugins are not available".to_string(),
            ));
        };
        let user_id = auth_user
            .as_ref()
            .map(|u| u.id.clone())
            .unwrap_or_else(|| "anonymous".to_string());
        let capabilities = ClientCapabilities {
            codecs: [query.video_codec.clone(), query.audio_codec.clone()]
                .into_iter()
                .flatten()
                .collect(),
            containers: Vec::new(),
            max_height: query.max_height,
        };
        match plugins.resolve_item(&item, &user_id, capabilities).await {
            // Direct descriptors are not SSRF-validated, so they are never handed to ffmpeg.
            Ok(d) if d.mode == PlaybackMode::Direct => {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "PLAYBACK_DIRECT_ONLY", "url": d.url })),
                )
                    .into_response();
            }
            Ok(d) => PlaybackSource::Url {
                url: d.url,
                headers: d.headers,
            },
            Err(e) => return plugin_error_response(&e),
        }
    } else {
        if !tokio::fs::try_exists(&item.file_path)
            .await
            .unwrap_or(false)
        {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media file missing from disk" })),
            )
                .into_response();
        }
        PlaybackSource::File(item.file_path.clone())
    };

    let params = TranscodeParameters::from(query);
    let session = match session_mgr
        .get_or_create_session_for_source(
            item.id.unwrap_or(item_id),
            &source,
            &item.technical,
            &params,
        )
        .await
    {
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p kadr-server --test plugin_playback_test && cargo test --workspace`
Expected: PASS. The existing streaming, private-streaming and HLS tests must still pass unchanged.

- [ ] **Step 9: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(plugins): resolve plugin item playback via direct, proxy and transcode modes"
```

---

### Task 9: Live items, health monitor, plugin events, watcher skip

**Files:**
- Create: `crates/kadr-server/src/plugins/health.rs`
- Modify: `crates/kadr-core/src/events.rs`, `crates/kadr-server/src/plugins/mod.rs`, `crates/kadr-server/src/playback/handler.rs`, `crates/kadr-server/src/api/mod.rs`, `crates/kadr-server/src/main.rs`
- Test: `crates/kadr-server/tests/plugin_health_test.rs`, `crates/kadr-server/tests/plugin_live_playback_test.rs`

**Interfaces:**
- Produces:
  - `SystemEvent::PluginError { plugin_id: String, message: String, timestamp: i64 }` (serialized as type `plugin:error`)
  - `PluginService::check_all(&self) -> Result<(), ServiceError>`: probes `enabled` and `unhealthy` plugins; moves enabled to unhealthy on failure (and publishes the event) and unhealthy back to enabled on success.
  - `plugins::health::spawn_health_monitor(PluginService, std::time::Duration) -> tokio::task::JoinHandle<()>`
  - Playback sessions for live items: resume position is always 0, and heartbeats do not persist watch state.

- [ ] **Step 1: Write the failing tests**

Create `crates/kadr-server/tests/plugin_health_test.rs`:

```rust
mod plugin_support;

use kadr_core::events::SystemEvent;
use kadr_core::plugin::PluginStatus;
use kadr_server::config::AppConfig;
use kadr_server::plugins::PluginService;
use plugin_support::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn health_check_flips_status_and_publishes_an_event() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    register_and_approve(&h, &mock).await;

    let mut config = AppConfig::default();
    config.plugins = enabled_settings();
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        Arc::new(tokio::sync::RwLock::new(config)),
        h.events.clone(),
    );
    let mut rx = h.events.subscribe();

    svc.check_all().await.unwrap();
    assert_eq!(h.plugin_repo.get("hello").await.unwrap().unwrap().status, PluginStatus::Enabled);

    mock.healthy.store(false, Ordering::SeqCst);
    svc.check_all().await.unwrap();
    assert_eq!(h.plugin_repo.get("hello").await.unwrap().unwrap().status, PluginStatus::Unhealthy);
    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
    match ev {
        SystemEvent::PluginError { plugin_id, .. } => assert_eq!(plugin_id, "hello"),
        other => panic!("unexpected event {other:?}"),
    }

    mock.healthy.store(true, Ordering::SeqCst);
    svc.check_all().await.unwrap();
    assert_eq!(h.plugin_repo.get("hello").await.unwrap().unwrap().status, PluginStatus::Enabled);
}

#[tokio::test]
async fn disabled_and_pending_plugins_are_not_probed() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    register_and_approve(&h, &mock).await;
    call(&h.app, "POST", "/api/v1/admin/plugins/hello/disable", Some(&h.admin_token), None).await;
    mock.healthy.store(false, Ordering::SeqCst);

    let mut config = AppConfig::default();
    config.plugins = enabled_settings();
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        Arc::new(tokio::sync::RwLock::new(config)),
        h.events.clone(),
    );
    svc.check_all().await.unwrap();
    assert_eq!(h.plugin_repo.get("hello").await.unwrap().unwrap().status, PluginStatus::Disabled);
}
```

Create `crates/kadr-server/tests/plugin_live_playback_test.rs`:

```rust
mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::plugin_item_path;
use plugin_support::*;
use serde_json::json;

async fn seed(h: &Harness, live: bool) -> MediaItem {
    h.lib_repo.create_virtual("hello", "lib-1", "Lib", MediaType::LiveChannel).await.unwrap();
    let mut item = MediaItem {
        library_id: "lib-1".into(),
        item_type: MediaType::LiveChannel,
        title: "Ch".into(),
        file_path: plugin_item_path("hello", "ch1"),
        file_name: "ch1".into(),
        added_at: 1,
        ..Default::default()
    };
    if live {
        item.metadata.extra.insert("live".into(), json!(true));
    }
    h.media_repo.upsert_batch(&[item]).await.unwrap();
    h.media_repo.find_by_path(plugin_item_path("hello", "ch1")).await.unwrap().unwrap()
}

#[tokio::test]
async fn live_items_never_resume_and_never_persist_progress() {
    let h = build(enabled_settings()).await;
    let item = seed(&h, true).await;
    let id = item.id.unwrap();

    // Pre-existing state must not be used for resume, and must not be overwritten.
    h.playback_repo
        .upsert_progress("viewer", id, 400, kadr_core::models::WatchState::InProgress, 1)
        .await
        .unwrap();

    let (status, body) = call(&h.app, "POST", "/api/v1/playback/sessions", Some(&h.user_token), Some(json!({ "media_item_id": id }))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["resume_position_seconds"], 0);
    let session_id = body["session_id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &h.app, "POST", &format!("/api/v1/playback/{session_id}/progress"),
        Some(&h.user_token), Some(json!({ "position_seconds": 500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let state = h.playback_repo.get_state("viewer", id).await.unwrap().unwrap();
    assert_eq!(state.playback_position_seconds, 400, "live heartbeats must not persist progress");
}

#[tokio::test]
async fn non_live_plugin_items_persist_progress_like_files() {
    let h = build(enabled_settings()).await;
    let item = seed(&h, false).await;
    let id = item.id.unwrap();

    let (_, body) = call(&h.app, "POST", "/api/v1/playback/sessions", Some(&h.user_token), Some(json!({ "media_item_id": id }))).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();
    call(
        &h.app, "POST", &format!("/api/v1/playback/{session_id}/progress"),
        Some(&h.user_token), Some(json!({ "position_seconds": 30 })),
    )
    .await;
    let state = h.playback_repo.get_state("viewer", id).await.unwrap().unwrap();
    assert_eq!(state.playback_position_seconds, 30);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_health_test --test plugin_live_playback_test`
Expected: FAIL to compile.

- [ ] **Step 3: Add the event variant**

In `crates/kadr-core/src/events.rs`, add to `enum SystemEvent` (after `SystemTelemetry`):

```rust
    #[serde(rename = "plugin:error")]
    PluginError {
        plugin_id: String,
        message: String,
        timestamp: i64,
    },
```

and to `event_type`:

```rust
            Self::PluginError { .. } => "plugin:error",
```

Run `cargo build --workspace` and fix any other exhaustive `match` on `SystemEvent` the compiler reports (add a `PluginError { .. }` arm that mirrors how the neighboring variants are handled).

- [ ] **Step 4: Implement the health monitor**

Create `crates/kadr-server/src/plugins/health.rs`:

```rust
use crate::plugins::service::{PluginService, ServiceError};
use kadr_core::events::SystemEvent;
use kadr_core::plugin::PluginStatus;
use std::time::Duration;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl PluginService {
    /// Probes every `enabled` and `unhealthy` plugin once. A failing enabled plugin becomes
    /// `unhealthy` (and a `plugin:error` event is published); a recovering one becomes `enabled`.
    /// Disabled and pending plugins are never contacted. Does nothing when plugins are off.
    pub async fn check_all(&self) -> Result<(), ServiceError> {
        if self.ensure_enabled().await.is_err() {
            return Ok(());
        }
        for record in self.repo().list().await? {
            if !matches!(record.status, PluginStatus::Enabled | PluginStatus::Unhealthy) {
                continue;
            }
            let result = self
                .client()
                .health(&record.base_url, &self.secret_for(&record.id))
                .await;
            match (record.status, result) {
                (PluginStatus::Enabled, Err(e)) => {
                    self.repo()
                        .set_status(&record.id, PluginStatus::Unhealthy)
                        .await?;
                    tracing::warn!(plugin = %record.id, "Plugin became unhealthy: {e}");
                    self.events().publish(SystemEvent::PluginError {
                        plugin_id: record.id.clone(),
                        message: e.to_string(),
                        timestamp: now_secs(),
                    });
                }
                (PluginStatus::Unhealthy, Ok(())) => {
                    self.repo()
                        .set_status(&record.id, PluginStatus::Enabled)
                        .await?;
                    tracing::info!(plugin = %record.id, "Plugin recovered");
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Runs `check_all` every `interval`. The first probe happens after one full interval.
pub fn spawn_health_monitor(svc: PluginService, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = svc.check_all().await {
                tracing::error!("Plugin health check failed: {e}");
            }
        }
    })
}
```

Add `pub mod health;` to `crates/kadr-server/src/plugins/mod.rs`.

In `crates/kadr-server/src/api/mod.rs`, right after `let plugin_tokens = ...;` add:

```rust
    if tokio::runtime::Handle::try_current().is_ok() {
        crate::plugins::health::spawn_health_monitor(
            plugin_service.clone(),
            std::time::Duration::from_secs(30),
        );
    }
```

(The loop is a no-op while `[plugins].enabled = false`, so it is safe to always spawn, matching how the HLS reaper is started.)

- [ ] **Step 5: Handle live items in playback sessions**

In `crates/kadr-server/src/playback/handler.rs`:

1. In `create_session`, replace the whole `let resume_pos = match playback_repo.get_state(...).await { ... };` expression with:

```rust
    let resume_pos = if item.is_live() {
        0
    } else {
        match playback_repo
            .get_state(&auth_user.id, payload.media_item_id)
            .await
        {
            Ok(Some(s)) => {
                if s.watch_state == WatchState::Completed {
                    0
                } else {
                    s.playback_position_seconds
                }
            }
            _ => 0,
        }
    };
```

2. In `progress_heartbeat`, add a parameter `media_repo: Option<Extension<MediaItemRepository>>,` (after `Extension(sessions): ...`) and, immediately after the `let session = match sessions.get(...)` block, insert:

```rust
    // Live items have no duration: skip scrobbling and never persist watch state.
    if session.duration_seconds == 0 {
        if let Some(Extension(media_repo)) = media_repo {
            if let Ok(Some(item)) = media_repo.get_by_id(session.media_item_id).await {
                if item.is_live() {
                    sessions
                        .update_progress(&session_id, payload.position_seconds)
                        .await;
                    return StatusCode::OK.into_response();
                }
            }
        }
    }
```

- [ ] **Step 6: Skip virtual libraries when starting watchers**

In `crates/kadr-server/src/main.rs`, at the top of the `for lib in &active_libraries {` loop that starts watchers (the one containing `let has_existing_path = ...`), add as the first statement:

```rust
        if kadr_core::plugin::is_plugin_path(&lib.path) {
            continue; // virtual plugin library: nothing on disk to watch
        }
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p kadr-server --test plugin_health_test --test plugin_live_playback_test --test playback_test && cargo test --workspace`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/kadr-core crates/kadr-server
git commit -m "feat(plugins): add health monitor, live-item playback handling and plugin error events"
```

---

### Task 10: Example plugin, documentation and final verification

**Files:**
- Create: `examples/plugin-hello/Cargo.toml`, `examples/plugin-hello/src/main.rs`, `examples/plugin-hello/README.md`, `docs/PLUGIN_GUIDE.md`
- Modify: `CHANGELOG.md`, `kadr.toml`

**Interfaces:**
- Consumes: the whole HTTP contract from Tasks 6 to 8.
- Produces: a standalone example plugin (its own Cargo workspace, not part of the main workspace) and the plugin author guide.

- [ ] **Step 1: Create the example plugin**

Create `examples/plugin-hello/Cargo.toml`:

```toml
[package]
name = "plugin-hello"
version = "0.1.0"
edition = "2021"
description = "Minimal example Kadr plugin: publishes three sample videos and resolves them as direct URLs"

[dependencies]
axum = "0.8"
tokio = { version = "1.43", features = ["full"] }
serde_json = "1.0"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

# Keep this example out of the main Kadr workspace.
[workspace]
```

Create `examples/plugin-hello/src/main.rs`:

```rust
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

#[derive(Clone)]
struct Config {
    secret: String,
}

const SAMPLES: &[(&str, &str, &str)] = &[
    ("big-buck-bunny", "Big Buck Bunny", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4"),
    ("elephants-dream", "Elephants Dream", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4"),
    ("sintel", "Sintel", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/Sintel.mp4"),
];

fn authorized(cfg: &Config, headers: &HeaderMap) -> bool {
    headers.get("authorization").and_then(|h| h.to_str().ok())
        == Some(format!("Bearer {}", cfg.secret).as_str())
}

async fn manifest() -> Json<Value> {
    Json(json!({
        "id": "hello",
        "name": "Hello Plugin",
        "version": "0.1.0",
        "api_version": 1,
        "permissions": ["items:publish", "playback:resolve"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    }))
}

async fn health(State(cfg): State<Config>, headers: HeaderMap) -> StatusCode {
    if authorized(&cfg, &headers) {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

async fn resolve(
    State(cfg): State<Config>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !authorized(&cfg, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let remote_id = req["remote_id"].as_str().unwrap_or_default();
    let (_, _, url) = SAMPLES
        .iter()
        .find(|(id, _, _)| *id == remote_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({ "mode": "direct", "url": url, "expires_in_secs": 300 })))
}

async fn publish(kadr_url: &str, token: &str) -> Result<(), reqwest::Error> {
    let http = reqwest::Client::new();
    // 201 on first run, 409 afterwards: both are fine.
    http.post(format!("{kadr_url}/api/v1/plugin/libraries"))
        .bearer_auth(token)
        .json(&json!({ "id": "hello-videos", "name": "Hello Videos", "media_type": "movie" }))
        .send()
        .await?;
    let items: Vec<Value> = SAMPLES
        .iter()
        .map(|(id, title, _)| json!({ "remote_id": id, "title": title, "duration_seconds": 600 }))
        .collect();
    let resp = http
        .put(format!("{kadr_url}/api/v1/plugin/libraries/hello-videos/items"))
        .bearer_auth(token)
        .json(&json!({ "items": items }))
        .send()
        .await?;
    println!("published {} items: HTTP {}", SAMPLES.len(), resp.status());
    Ok(())
}

#[tokio::main]
async fn main() {
    let secret = std::env::var("KADR_PLUGIN_SECRET").unwrap_or_else(|_| "unset".to_string());
    let bind = std::env::var("PLUGIN_BIND").unwrap_or_else(|_| "127.0.0.1:9100".to_string());
    let kadr_url = std::env::var("KADR_URL").unwrap_or_else(|_| "http://127.0.0.1:8492".to_string());

    if let Ok(token) = std::env::var("KADR_PLUGIN_TOKEN") {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Err(e) = publish(&kadr_url, &token).await {
                eprintln!("publish failed: {e}");
            }
        });
    }

    let app = Router::new()
        .route("/manifest.json", get(manifest))
        .route("/health", get(health))
        .route("/playback/resolve", post(resolve))
        .with_state(Config { secret });
    let listener = tokio::net::TcpListener::bind(&bind).await.expect("bind failed");
    println!("plugin-hello listening on http://{bind}");
    axum::serve(listener, app).await.expect("server failed");
}
```

Create `examples/plugin-hello/README.md`:

````markdown
# plugin-hello

Minimal Kadr plugin. It publishes three public sample videos into a virtual library and resolves
them as `direct` playback URLs. Use it as a template; see [`docs/PLUGIN_GUIDE.md`](../../docs/PLUGIN_GUIDE.md).

## Run it

1. Enable plugins in `kadr.toml` and restart Kadr:

   ```toml
   [plugins]
   enabled = true
   ```

2. Start the plugin. The manifest is public, so it can start before it knows its secret:

   ```bash
   cd examples/plugin-hello && cargo run
   ```

3. As an admin, register and approve it (the registration response contains `secret`, the approval response contains `token`):

   ```bash
   curl -s -X POST localhost:8492/api/v1/admin/plugins \
     -H "Authorization: Bearer $ADMIN_JWT" -H 'Content-Type: application/json' \
     -d '{"base_url":"http://127.0.0.1:9100"}'

   curl -s -X POST localhost:8492/api/v1/admin/plugins/hello/approve \
     -H "Authorization: Bearer $ADMIN_JWT"
   ```

4. Restart the plugin with both values so it can authenticate Kadr and publish its items:

   ```bash
   KADR_PLUGIN_SECRET=<secret> KADR_PLUGIN_TOKEN=<token> cargo run
   ```

The "Hello Videos" library now appears in Kadr, and `GET /api/v1/stream/{id}` redirects to the sample file.
````

- [ ] **Step 2: Verify the example builds**

Run: `cargo build --manifest-path examples/plugin-hello/Cargo.toml`
Expected: builds. (It has its own `Cargo.lock` and its own empty `[workspace]`, so it does not affect the main workspace. If the sandbox has no network, this requires the registry cache to already contain `axum`, `tokio`, `reqwest` and `serde_json`, which the main workspace already uses.) Add `examples/plugin-hello/target/` to `.gitignore` if the repo's `.gitignore` does not already cover `target/` at any depth.

- [ ] **Step 3: Write the plugin guide**

Create `docs/PLUGIN_GUIDE.md`:

````markdown
# Kadr Plugin Guide

Kadr plugins are separate processes that talk to the server over HTTP. A plugin can:

- publish **virtual items** (items with no file on disk) into libraries it owns, and
- **resolve playback** for those items (`direct`, `proxy` or `transcode`).

The server never starts or stops plugins. API version: `1`.

## Enabling plugins

```toml
[plugins]
enabled = true              # default false
resolve_timeout_secs = 5    # default 5
allow_private_proxy = false # default false; see "Security"
```

## Manifest

Serve `GET <base_url>/manifest.json` (public):

```json
{
  "id": "iptv",
  "name": "IPTV",
  "version": "0.1.0",
  "api_version": 1,
  "permissions": ["items:publish", "playback:resolve"],
  "capabilities": { "virtual_library": true, "playback_resolver": true }
}
```

`id` must match `[a-z0-9-]{1,32}` and be unique.

## Registration and approval (admin)

| Call | Result |
|---|---|
| `POST /api/v1/admin/plugins` `{"base_url": "http://host:port"}` | `201 { plugin, secret }`; plugin is `pending` |
| `POST /api/v1/admin/plugins/{id}/approve` `{"permissions": [...]}` (body optional) | `{ plugin, token }`; grants the requested permissions (or a subset) and enables it |
| `POST /api/v1/admin/plugins/{id}/enable` / `disable` | toggles a previously approved plugin |
| `DELETE /api/v1/admin/plugins/{id}?purge=true` | removes the plugin; `purge` also deletes its libraries and items |
| `GET /api/v1/admin/plugins` | lists plugins |

- `secret` is what **Kadr sends to the plugin**: `Authorization: Bearer <secret>` on `/health` and `/playback/resolve`. The plugin must reject calls without it. It is derived from the server secret, so it is never stored.
- `token` is what **the plugin sends to Kadr** on `/api/v1/plugin/*` calls. It is not a user token and cannot be used on any other route.

## Publishing (plugin to Kadr)

All calls need `Authorization: Bearer <token>` and the `items:publish` permission.

- `POST /api/v1/plugin/libraries` `{"id"?: "...", "name": "...", "media_type"?: "live_channel"}` returns `201`.
- `PUT /api/v1/plugin/libraries/{id}/items` `{"items": [...]}` upserts up to 1000 items per call.
- `DELETE /api/v1/plugin/libraries/{id}/items/{remote_id}` returns `204`. Percent-encode `/` inside `remote_id`.

Item fields: `remote_id`, `title` (required); `item_type`, `original_title`, `release_year`, `duration_seconds`, `live`, `seekable`, `resolution`, `video_codec`, `audio_codec`, `container`, `poster_url`, `backdrop_url`, `overview`, `genres`.

A plugin can only touch libraries it created. `live: true` items have no resume position and never record watch progress.

## Resolving playback (Kadr to plugin)

`POST <base_url>/playback/resolve`

```json
{ "remote_id": "ch1", "user_id": "opaque-id", "client": { "codecs": ["h264"], "containers": [], "max_height": 1080 } }
```

Respond with a descriptor:

```json
{ "mode": "proxy", "url": "https://...", "headers": {"User-Agent": "x"}, "expires_in_secs": 300, "content_type": "video/mp4" }
```

| `mode` | Behavior |
|---|---|
| `direct` | `GET /api/v1/stream/{id}` redirects the client (307) to `url`. Must not set `headers`. Not usable with HLS (`409 PLAYBACK_DIRECT_ONLY`). |
| `proxy` | Kadr fetches `url` (with `headers`) and streams it to the client, forwarding `Range` for seekable items. Upstream redirects are not followed. |
| `transcode` | The HLS endpoint `GET /api/v1/media/{id}/hls/master.m3u8` feeds `url` to ffmpeg. The direct-play endpoint answers `409 PLAYBACK_REQUIRES_TRANSCODE` with the `hls_url`. |

`expires_in_secs` lets Kadr cache the descriptor per item and user. Omit it to resolve on every request.

## Errors seen by clients

| Status | Code | Meaning |
|---|---|---|
| 503 | `PLUGIN_UNAVAILABLE` | plugin is down, unhealthy, disabled, removed, or its circuit is open |
| 504 | `PLUGIN_TIMEOUT` | resolve exceeded `resolve_timeout_secs` |
| 502 | `PLUGIN_BAD_RESPONSE` | invalid descriptor, rejected URL, or bad upstream response |

Three consecutive failures open a per-plugin circuit for 30 seconds. A health check runs every 30 seconds; a failing plugin becomes `unhealthy` and a `plugin:error` event is published on `/api/v1/events`.

## Security notes

- Descriptor URLs must be `http` or `https`. For `proxy` and `transcode`, hosts that resolve to loopback, private, link-local or CGNAT addresses are rejected unless `allow_private_proxy = true`.
- Known limitations: DNS is resolved again when Kadr or ffmpeg connects, and ffmpeg follows redirects internally. Only register plugins you trust, and keep `allow_private_proxy = false` unless a plugin must reach your LAN.
- Private (PIN-locked) libraries are checked before any resolve call, so a plugin is never asked about an item the user has not unlocked.
- Plugins never receive user credentials, only an opaque user id.

## Example

See [`examples/plugin-hello`](../examples/plugin-hello).
````

- [ ] **Step 4: Update the changelog and config sample**

In `CHANGELOG.md`, add directly above `## [v0.1.3-alpha] - 2026-11-07`:

```markdown
## [Unreleased]

### Added
- **Plugin framework (HTTP)**: out-of-process plugins can publish virtual media items into plugin-owned libraries and resolve playback (`direct`, `proxy`, `transcode`) for them. Includes admin registration and approval (`/api/v1/admin/plugins`), scoped plugin tokens, a derived server-to-plugin secret, a circuit-breaker client, health monitoring with `plugin:error` events, SSRF protection for stream URLs, and live-item handling. Disabled by default; enable with `[plugins] enabled = true`. See `docs/PLUGIN_GUIDE.md` and `examples/plugin-hello`.
- `MediaType::LiveChannel` and `HlsSessionManager::get_or_create_session_for_source` / `build_ffmpeg_hls_args_for_source` for transcoding from a URL.

```

Append to `kadr.toml`:

```toml

[plugins]
enabled = false
resolve_timeout_secs = 5
allow_private_proxy = false
```

- [ ] **Step 5: Final verification**

Run each and confirm:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --manifest-path examples/plugin-hello/Cargo.toml
```

Expected: formatting clean, no clippy warnings, every test passes (the pre-existing suites included), example builds. Fix any clippy findings in the new code (for example `too_many_arguments` is already allowed on the router builders; for `PluginService::new` there are only four parameters).

- [ ] **Step 6: Manual smoke test (requires ffmpeg only for the HLS part)**

1. Set `[plugins] enabled = true` in a scratch `kadr.toml`, run `cargo run -p kadr-server`, then follow `examples/plugin-hello/README.md`.
2. `curl -i -H "Authorization: Bearer $USER_JWT" localhost:8492/api/v1/stream/<id>` returns `307` with a `Location` header.
3. Stop the plugin: within about 30 seconds its status becomes `unhealthy`, and playback returns `503 PLUGIN_UNAVAILABLE`.

- [ ] **Step 7: Commit**

```bash
git add examples docs CHANGELOG.md kadr.toml .gitignore
git commit -m "docs(plugins): add plugin guide, example plugin and changelog entry"
```

---

## Self-Review

**Spec coverage** (spec section → task):
- §2 transport/trust, registration by URL, admin approval, `[plugins]` settings, derived secret, scoped token → Tasks 4, 6, 10.
- §3 `PluginRegistry`/`PluginClient`/`PluginHealth`, manifest, v1 API → Tasks 1, 5, 6, 7, 8, 9.
- §4 data model (migration 007, no `MediaItem`/`Library` field changes, `LiveChannel`) → Tasks 1, 2.
- §5 playback modes, descriptor expiry cache, `PlaybackSource`, HLS/stream/session changes, locked-library ordering → Tasks 3, 8, 9.
- §6 permissions, token isolation, revoke/disable/purge → Tasks 4, 6, 7 (tests: `tokens_are_not_interchangeable`, `missing_permission_and_disabled_plugin_are_forbidden`, `plugins_cannot_touch_other_libraries`, purge test).
- §7 error mapping, SSRF, header injection, no-redirect proxy, event bus → Tasks 5, 8, 9.
- §8 testing (unit, migration, mock-plugin integration, regression, permissions) → every task plus the full `cargo test --workspace` gate.
- §9 example plugin → Task 10.
- Out-of-scope items (admin UI, webhooks, widgets, provider traits, secrets store, IPTV/music) are not planned.

**Known deviations from the approved spec (already folded into the spec file):** the secret is derived instead of stored/hashed; `media_items` is not altered and `MediaItem`/`Library` get no new fields (derived accessors instead); direct-mode descriptors cannot carry headers; `direct` is refused by HLS.

**Type consistency check:** `PluginRecord`, `PluginManifest`, `PluginPermission`, `PluginStatus` (Task 1) are used unchanged in Tasks 5 to 9. `PlaybackDescriptor`/`PlaybackSource`/`ClientCapabilities`/`ResolveRequest` (Task 2) match their use in the client (Task 5), service (Task 8) and builder (Task 3). `PluginService` accessors defined in Task 6 (`repo`, `libs`, `media`, `events`, `client`, `secret_for`, `settings`, `ensure_enabled`) are the ones used in Tasks 7 to 9; `proxy_client` is added in Task 8 Step 4 before use.

**Items for the executor to watch** (compile-level details that could not be verified without building):
- Task 3 Step 3: confirm how `transcoding/mod.rs` re-exports builder items (`pub use builder::*;` was observed, so no change is expected).
- Task 1 Step 8: update any hard-coded migration-count assertion from 6 to 7.
