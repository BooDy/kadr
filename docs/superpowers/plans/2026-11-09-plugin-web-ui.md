# Plugin Web UI Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an admin enable and fully manage plugins from the web UI (enable switch, settings, register, approve, enable/disable, remove, connection details) with settings persisted in the database instead of `kadr.toml`.

**Architecture:** A `settings` key-value table (migration 008) stores plugin settings that override `kadr.toml` at startup; new admin API routes read and write them (and work while plugins are off), plus connection-detail and token routes. `PluginClient` reads its timeout from a shared, updatable value so changes apply without a restart. The web UI gets a new `PluginsPanel` built from small leaf components (dialogs, form, list), wired into `AdminDashboard` as a new tab.

**Tech Stack:** Rust (axum 0.8, rusqlite via deadpool_sqlite), React 19 + TypeScript, Tailwind 4, lucide-react, Vitest + React Testing Library. Spec: `docs/superpowers/specs/2026-11-09-plugin-web-ui-design.md`. Builds on the merged plugin framework (`docs/superpowers/plans/2026-11-07-plugin-framework.md`).

## Global Constraints

- Settings keys: `plugins.enabled` (`"true"`/`"false"`), `plugins.resolve_timeout_secs` (integer string), `plugins.allow_private_proxy` (`"true"`/`"false"`).
- Precedence: stored DB value > `kadr.toml` > built-in default (`enabled=false`, `resolve_timeout_secs=5`, `allow_private_proxy=false`). Unparseable or out-of-range stored values are ignored with a warning.
- `resolve_timeout_secs` must be within `1..=60`; violations return `400 INVALID_REQUEST`.
- Settings routes are admin-only (`RequireAdmin`) and work while plugins are disabled. All other plugin admin routes keep returning `404 PLUGINS_DISABLED` while disabled.
- Persist first, then apply: a failed DB write must leave the in-memory config unchanged.
- The server-to-plugin secret stays derived (never stored, never logged). `GET /api/v1/admin/plugins/{id}/connection` may return it to admins; list/other responses must never contain it.
- The plugin id `settings` is reserved (it would collide with `/api/v1/admin/plugins/settings`).
- Existing public router-builder function signatures, existing plugin routes and the `kadr.toml` `[plugins]` section must keep working unchanged. Existing tests must pass.
- Only crates already in `Cargo.lock`; no new npm dependencies.
- Web: follow the existing admin styling (Tailwind classes used in `AdminDashboard.tsx`), keep `AdminDashboard.tsx` changes to tab wiring only, show API error `message` (not just the `error` code) to the user.
- Commit after every task with a conventional-commit message. Run `cargo test --workspace` after each Rust task and `cd web && npm test` after each web task.

## File Structure

| File | Responsibility |
|---|---|
| `crates/kadr-storage/src/migrations/008_settings.sql` (new) | `settings` table |
| `crates/kadr-storage/src/repos/settings_repo.rs` (new) | `SettingsRepository` |
| `crates/kadr-server/src/plugins/settings.rs` (new) | settings keys, validation, partial update, `load_plugin_settings` |
| `crates/kadr-server/src/plugins/client.rs` | dynamic timeout (`set_timeout`) |
| `crates/kadr-server/src/plugins/service.rs` | `get_settings`/`update_settings`/`connection`/`issue_token`, timeout sync |
| `crates/kadr-server/src/plugins/admin_routes.rs` | settings, connection and token routes |
| `crates/kadr-server/src/main.rs` | load stored settings at startup |
| `crates/kadr-core/src/plugin.rs` | reserve manifest id `settings` |
| `web/src/types/index.ts`, `web/src/api/client.ts` | plugin types, API methods, `describeApiError` |
| `web/src/components/admin/ApprovePluginDialog.tsx`, `RemovePluginDialog.tsx`, `ConnectionDialog.tsx`, `RegisterPluginForm.tsx`, `PluginList.tsx`, `PluginsPanel.tsx` (new) | UI components |
| `web/src/components/admin/AdminDashboard.tsx` | `'plugins'` tab wiring only |
| `docs/PLUGIN_GUIDE.md`, `CHANGELOG.md` | docs |

---

### Task 1: `settings` table and `SettingsRepository`

**Files:**
- Create: `crates/kadr-storage/src/migrations/008_settings.sql`, `crates/kadr-storage/src/repos/settings_repo.rs`
- Modify: `crates/kadr-storage/src/migrations.rs`, `crates/kadr-storage/src/repos/mod.rs`, `crates/kadr-storage/src/lib.rs`
- Test: `crates/kadr-storage/tests/settings_repo_test.rs`

**Interfaces:**
- Produces `kadr_storage::repos::SettingsRepository` (also re-exported from `kadr_storage`): `new(Pool)`, `get(&str) -> Result<Option<String>>`, `set(&str, &str) -> Result<()>` (upsert), `set_many(&[(String, String)]) -> Result<()>` (one transaction), `get_all() -> Result<HashMap<String, String>>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/kadr-storage/tests/settings_repo_test.rs`:

```rust
use kadr_storage::repos::SettingsRepository;
use kadr_storage::{create_in_memory_pool, initialize_database};

async fn repo() -> SettingsRepository {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    SettingsRepository::new(pool)
}

#[tokio::test]
async fn get_returns_none_for_missing_keys() {
    let repo = repo().await;
    assert_eq!(repo.get("plugins.enabled").await.unwrap(), None);
    assert!(repo.get_all().await.unwrap().is_empty());
}

#[tokio::test]
async fn set_inserts_then_overwrites() {
    let repo = repo().await;
    repo.set("plugins.enabled", "true").await.unwrap();
    assert_eq!(repo.get("plugins.enabled").await.unwrap().as_deref(), Some("true"));
    repo.set("plugins.enabled", "false").await.unwrap();
    assert_eq!(repo.get("plugins.enabled").await.unwrap().as_deref(), Some("false"));
}

#[tokio::test]
async fn set_many_writes_all_entries_and_get_all_returns_them() {
    let repo = repo().await;
    repo.set_many(&[
        ("a".to_string(), "1".to_string()),
        ("b".to_string(), "2".to_string()),
    ])
    .await
    .unwrap();
    let all = repo.get_all().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all["a"], "1");
    assert_eq!(all["b"], "2");

    // Empty input is a no-op.
    repo.set_many(&[]).await.unwrap();
    assert_eq!(repo.get_all().await.unwrap().len(), 2);
}

#[tokio::test]
async fn set_many_upserts_existing_keys() {
    let repo = repo().await;
    repo.set("a", "old").await.unwrap();
    repo.set_many(&[("a".to_string(), "new".to_string())]).await.unwrap();
    assert_eq!(repo.get("a").await.unwrap().as_deref(), Some("new"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-storage --test settings_repo_test`
Expected: FAIL to compile (`SettingsRepository` missing).

- [ ] **Step 3: Implement**

Create `crates/kadr-storage/src/migrations/008_settings.sql`:

```sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

In `crates/kadr-storage/src/migrations.rs` append to the `vec![...]` after the 007 entry:

```rust
        M::up(include_str!("migrations/008_settings.sql")),
```

Create `crates/kadr-storage/src/repos/settings_repo.rs`:

```rust
use crate::error::Result;
use deadpool_sqlite::Pool;
use rusqlite::params;
use std::collections::HashMap;

/// Small key-value store for settings changed from the web UI.
#[derive(Clone)]
pub struct SettingsRepository {
    pool: Pool,
}

impl SettingsRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT value FROM settings WHERE key = ?1")?;
            let mut rows = stmt.query(params![key])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get::<_, String>(0)?)),
                None => Ok(None),
            }
        })
        .await?
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        self.set_many(&[(key.to_string(), value.to_string())]).await
    }

    /// Upserts all entries in a single transaction (all or nothing).
    pub async fn set_many(&self, entries: &[(String, String)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let entries = entries.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                )?;
                for (key, value) in &entries {
                    stmt.execute(params![key, value])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn get_all(&self) -> Result<HashMap<String, String>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT key, value FROM settings")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out = HashMap::new();
            for r in rows {
                let (k, v) = r?;
                out.insert(k, v);
            }
            Ok(out)
        })
        .await?
    }
}
```

In `crates/kadr-storage/src/repos/mod.rs` add `pub mod settings_repo;` and `pub use settings_repo::SettingsRepository;`. In `crates/kadr-storage/src/lib.rs` add `SettingsRepository` to the `pub use repos::{...}` list.

- [ ] **Step 4: Run tests**

Run: `cargo test -p kadr-storage && cargo build --workspace`
Expected: PASS. If a migration test asserts the schema version count, update the expected value from 7 to 8.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-storage
git commit -m "feat(storage): add settings table and SettingsRepository"
```

---

### Task 2: Plugin settings persistence, API, live timeout, startup load, reserved id

**Files:**
- Create: `crates/kadr-server/src/plugins/settings.rs`
- Modify: `crates/kadr-server/src/plugins/mod.rs`, `crates/kadr-server/src/plugins/client.rs`, `crates/kadr-server/src/plugins/service.rs`, `crates/kadr-server/src/plugins/admin_routes.rs`, `crates/kadr-server/src/plugins/health.rs` (one call), `crates/kadr-server/src/plugins/playback.rs` (one call), `crates/kadr-server/src/main.rs`, `crates/kadr-core/src/plugin.rs`
- Test: `crates/kadr-server/tests/plugin_settings_test.rs`, `crates/kadr-core/tests/plugin_types_test.rs` (append)

**Interfaces:**
- Consumes: `SettingsRepository` (Task 1), `PluginSettings` (config), `PluginService`, `RequireAdmin`.
- Produces:
  - `plugins::settings::{KEY_ENABLED, KEY_RESOLVE_TIMEOUT_SECS, KEY_ALLOW_PRIVATE_PROXY, MIN_TIMEOUT_SECS (1), MAX_TIMEOUT_SECS (60), PluginSettingsUpdate { enabled: Option<bool>, resolve_timeout_secs: Option<u64>, allow_private_proxy: Option<bool> } (Deserialize, Default) with validate() -> Result<(), String>, entries() -> Vec<(String, String)>, apply_to(&self, &mut PluginSettings), load_plugin_settings(&SettingsRepository, PluginSettings) -> PluginSettings (async)}`
  - `PluginClient::set_timeout(&self, Duration)` (the existing `PluginClient::new(Duration)` is unchanged).
  - `PluginService::{get_settings() -> PluginSettings, update_settings(PluginSettingsUpdate) -> Result<PluginSettings, ServiceError>}`.
  - Routes: `GET /api/v1/admin/plugins/settings` returns `{ enabled, resolve_timeout_secs, allow_private_proxy }`; `PUT /api/v1/admin/plugins/settings` (partial body) returns the same shape.

- [ ] **Step 1: Write the failing tests**

Append to `crates/kadr-core/tests/plugin_types_test.rs`:

```rust
#[test]
fn manifest_id_settings_is_reserved() {
    let manifest = PluginManifest {
        id: "settings".into(),
        name: "Settings".into(),
        version: "0.1.0".into(),
        api_version: 1,
        permissions: vec![],
        capabilities: Default::default(),
    };
    assert!(manifest.validate().unwrap_err().contains("reserved"));
}
```

Create `crates/kadr-server/tests/plugin_settings_test.rs`:

```rust
mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::plugin_item_path;
use kadr_server::config::PluginSettings;
use kadr_server::plugins::settings::load_plugin_settings;
use kadr_storage::repos::SettingsRepository;
use plugin_support::*;
use serde_json::json;
use std::sync::atomic::Ordering;

const SETTINGS: &str = "/api/v1/admin/plugins/settings";

#[tokio::test]
async fn settings_get_works_while_plugins_are_disabled() {
    let h = build(PluginSettings::default()).await;
    let (status, body) = call(&h.app, "GET", SETTINGS, Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "enabled": false, "resolve_timeout_secs": 5, "allow_private_proxy": false }));

    // The other plugin admin routes are still blocked while disabled.
    let (status, body) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "PLUGINS_DISABLED");
}

#[tokio::test]
async fn settings_routes_require_admin() {
    let h = build(PluginSettings::default()).await;
    for method in ["GET", "PUT"] {
        let body = if method == "PUT" { Some(json!({ "enabled": true })) } else { None };
        let (s, _) = call(&h.app, method, SETTINGS, None, body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = call(&h.app, method, SETTINGS, Some(&h.user_token), body).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn enabling_from_the_api_takes_effect_immediately() {
    let h = build(PluginSettings::default()).await;
    let (status, body) = call(&h.app, "PUT", SETTINGS, Some(&h.admin_token), Some(json!({ "enabled": true }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["resolve_timeout_secs"], 5, "untouched keys keep their value");

    let (status, body) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!([]));

    let (_, body) = call(&h.app, "PUT", SETTINGS, Some(&h.admin_token), Some(json!({ "enabled": false }))).await;
    assert_eq!(body["enabled"], false);
    let (status, _) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn partial_updates_only_change_supplied_keys() {
    let h = build(PluginSettings::default()).await;
    let (_, body) = call(&h.app, "PUT", SETTINGS, Some(&h.admin_token), Some(json!({ "allow_private_proxy": true }))).await;
    assert_eq!(body, json!({ "enabled": false, "resolve_timeout_secs": 5, "allow_private_proxy": true }));
    let (_, body) = call(&h.app, "PUT", SETTINGS, Some(&h.admin_token), Some(json!({ "resolve_timeout_secs": 12 }))).await;
    assert_eq!(body, json!({ "enabled": false, "resolve_timeout_secs": 12, "allow_private_proxy": true }));
}

#[tokio::test]
async fn invalid_timeouts_are_rejected_and_change_nothing() {
    let h = build(PluginSettings::default()).await;
    for bad in [0u64, 61, 1000] {
        let (status, body) = call(
            &h.app, "PUT", SETTINGS, Some(&h.admin_token),
            Some(json!({ "enabled": true, "resolve_timeout_secs": bad })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "timeout {bad}");
        assert_eq!(body["error"], "INVALID_REQUEST");
    }
    let (_, body) = call(&h.app, "GET", SETTINGS, Some(&h.admin_token), None).await;
    assert_eq!(body["enabled"], false, "a rejected update must not apply any of its keys");
    assert_eq!(body["resolve_timeout_secs"], 5);

    for ok in [1u64, 60] {
        let (status, _) = call(&h.app, "PUT", SETTINGS, Some(&h.admin_token), Some(json!({ "resolve_timeout_secs": ok }))).await;
        assert_eq!(status, StatusCode::OK, "timeout {ok}");
    }
}

#[tokio::test]
async fn settings_are_persisted_and_override_the_config_default_on_load() {
    let h = build(PluginSettings::default()).await;
    let (_, put) = call(
        &h.app, "PUT", SETTINGS, Some(&h.admin_token),
        Some(json!({ "enabled": true, "resolve_timeout_secs": 9, "allow_private_proxy": true })),
    )
    .await;
    assert_eq!(put["resolve_timeout_secs"], 9);

    let repo = SettingsRepository::new(h.media_repo.pool().clone());
    let loaded = load_plugin_settings(&repo, PluginSettings::default()).await;
    assert_eq!(
        loaded,
        PluginSettings { enabled: true, resolve_timeout_secs: 9, allow_private_proxy: true }
    );
}

#[tokio::test]
async fn load_falls_back_to_the_base_for_missing_and_unparseable_values() {
    let h = build(PluginSettings::default()).await;
    let repo = SettingsRepository::new(h.media_repo.pool().clone());
    let base = PluginSettings { enabled: true, resolve_timeout_secs: 7, allow_private_proxy: false };

    // Nothing stored: the base is returned untouched.
    assert_eq!(load_plugin_settings(&repo, base.clone()).await, base);

    // Garbage and out-of-range values are ignored; valid ones still apply.
    repo.set("plugins.enabled", "yes").await.unwrap();
    repo.set("plugins.resolve_timeout_secs", "abc").await.unwrap();
    repo.set("plugins.allow_private_proxy", "true").await.unwrap();
    let loaded = load_plugin_settings(&repo, base.clone()).await;
    assert_eq!(loaded, PluginSettings { allow_private_proxy: true, ..base.clone() });

    repo.set("plugins.resolve_timeout_secs", "999").await.unwrap();
    assert_eq!(load_plugin_settings(&repo, base.clone()).await.resolve_timeout_secs, 7);
}

#[tokio::test]
async fn the_settings_path_is_not_captured_as_a_plugin_id() {
    let h = build(enabled_settings()).await;
    let (status, _) = call(&h.app, "DELETE", SETTINGS, Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "must not reach the remove-plugin handler");

    // And a plugin cannot register under the reserved id.
    let mock = spawn_mock("settings", &["items:publish"]).await;
    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn a_changed_timeout_applies_to_the_next_resolve_without_rebuilding() {
    let h = build(PluginSettings { resolve_timeout_secs: 3, ..enabled_settings() }).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;
    register_and_approve(&h, &mock).await;
    h.lib_repo.create_virtual("hello", "lib-1", "Lib", MediaType::Movie).await.unwrap();
    h.media_repo
        .upsert_batch(&[MediaItem {
            library_id: "lib-1".into(),
            item_type: MediaType::Movie,
            title: "m1".into(),
            file_path: plugin_item_path("hello", "m1"),
            file_name: "m1".into(),
            added_at: 1,
            ..Default::default()
        }])
        .await
        .unwrap();
    let item = h.media_repo.find_by_path(plugin_item_path("hello", "m1")).await.unwrap().unwrap();
    let uri = format!("/api/v1/stream/{}", item.id.unwrap());

    mock.resolve_delay_ms.store(1500, Ordering::SeqCst);
    let (status, _, _) = call_raw(&h.app, "GET", &uri, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT, "1.5s is inside the 3s timeout");

    call(&h.app, "PUT", SETTINGS, Some(&h.admin_token), Some(json!({ "resolve_timeout_secs": 1 }))).await;
    let (status, _, _) = call_raw(&h.app, "GET", &uri, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "1.5s now exceeds the new 1s timeout");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-core --test plugin_types_test && cargo test -p kadr-server --test plugin_settings_test`
Expected: FAIL (reserved id not enforced; settings module and routes missing).

- [ ] **Step 3: Reserve the manifest id**

In `crates/kadr-core/src/plugin.rs`, in `PluginManifest::validate`, right after the `id_ok` check block (before the `name` check), add:

```rust
        if self.id == "settings" {
            return Err("manifest id 'settings' is reserved".to_string());
        }
```

- [ ] **Step 4: Implement the settings module**

Create `crates/kadr-server/src/plugins/settings.rs`:

```rust
use crate::config::PluginSettings;
use kadr_storage::repos::SettingsRepository;

pub const KEY_ENABLED: &str = "plugins.enabled";
pub const KEY_RESOLVE_TIMEOUT_SECS: &str = "plugins.resolve_timeout_secs";
pub const KEY_ALLOW_PRIVATE_PROXY: &str = "plugins.allow_private_proxy";

pub const MIN_TIMEOUT_SECS: u64 = 1;
pub const MAX_TIMEOUT_SECS: u64 = 60;

/// Partial update of the plugin settings (every field optional).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PluginSettingsUpdate {
    pub enabled: Option<bool>,
    pub resolve_timeout_secs: Option<u64>,
    pub allow_private_proxy: Option<bool>,
}

impl PluginSettingsUpdate {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(secs) = self.resolve_timeout_secs {
            if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&secs) {
                return Err(format!(
                    "resolve_timeout_secs must be between {MIN_TIMEOUT_SECS} and {MAX_TIMEOUT_SECS}"
                ));
            }
        }
        Ok(())
    }

    /// Key/value pairs to persist for the supplied fields only.
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(v) = self.enabled {
            out.push((KEY_ENABLED.to_string(), v.to_string()));
        }
        if let Some(v) = self.resolve_timeout_secs {
            out.push((KEY_RESOLVE_TIMEOUT_SECS.to_string(), v.to_string()));
        }
        if let Some(v) = self.allow_private_proxy {
            out.push((KEY_ALLOW_PRIVATE_PROXY.to_string(), v.to_string()));
        }
        out
    }

    pub fn apply_to(&self, settings: &mut PluginSettings) {
        if let Some(v) = self.enabled {
            settings.enabled = v;
        }
        if let Some(v) = self.resolve_timeout_secs {
            settings.resolve_timeout_secs = v;
        }
        if let Some(v) = self.allow_private_proxy {
            settings.allow_private_proxy = v;
        }
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Starts from `base` (the `kadr.toml` / default values) and applies every valid stored
/// override. Unparseable or out-of-range stored values are ignored with a warning, and a
/// storage failure falls back to `base`.
pub async fn load_plugin_settings(
    repo: &SettingsRepository,
    base: PluginSettings,
) -> PluginSettings {
    let stored = match repo.get_all().await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!("Could not read stored plugin settings, using defaults: {e}");
            return base;
        }
    };
    let mut settings = base;

    if let Some(raw) = stored.get(KEY_ENABLED) {
        match parse_bool(raw) {
            Some(v) => settings.enabled = v,
            None => tracing::warn!(key = KEY_ENABLED, value = %raw, "Ignoring invalid stored setting"),
        }
    }
    if let Some(raw) = stored.get(KEY_RESOLVE_TIMEOUT_SECS) {
        match raw.parse::<u64>() {
            Ok(v) if (MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&v) => {
                settings.resolve_timeout_secs = v
            }
            _ => tracing::warn!(
                key = KEY_RESOLVE_TIMEOUT_SECS, value = %raw, "Ignoring invalid stored setting"
            ),
        }
    }
    if let Some(raw) = stored.get(KEY_ALLOW_PRIVATE_PROXY) {
        match parse_bool(raw) {
            Some(v) => settings.allow_private_proxy = v,
            None => tracing::warn!(
                key = KEY_ALLOW_PRIVATE_PROXY, value = %raw, "Ignoring invalid stored setting"
            ),
        }
    }
    settings
}
```

Add `pub mod settings;` to `crates/kadr-server/src/plugins/mod.rs`.

- [ ] **Step 5: Make the client timeout updatable**

In `crates/kadr-server/src/plugins/client.rs`:

1. Add `use std::sync::atomic::{AtomicU64, Ordering};` to the imports.
2. Change the struct field `timeout: Duration,` to `timeout_ms: Arc<AtomicU64>,`.
3. In `PluginClient::new` replace `timeout,` with `timeout_ms: Arc::new(AtomicU64::new(timeout.as_millis() as u64)),`.
4. Add these methods inside `impl PluginClient`:

```rust
    /// Updates the request timeout used by every clone of this client from now on.
    pub fn set_timeout(&self, timeout: Duration) {
        self.timeout_ms
            .store(timeout.as_millis().max(1) as u64, Ordering::Relaxed);
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.load(Ordering::Relaxed))
    }
```

5. Replace every use of `self.timeout` (in `send_json`: `rb.timeout(self.timeout)`, and in `health`: `.timeout(self.timeout)`) with `self.timeout()`.

- [ ] **Step 6: Extend the service**

In `crates/kadr-server/src/plugins/service.rs`:

1. Add imports: `use crate::plugins::settings::PluginSettingsUpdate;` and `use kadr_storage::repos::SettingsRepository;` (merge into the existing `kadr_storage::repos::{...}` import).
2. Add a field to `struct Inner` (below `libs`): `settings_repo: SettingsRepository,` and initialize it in `PluginService::new`: `settings_repo: SettingsRepository::new(pool.clone()),` (place it before the `media: MediaItemRepository::new(pool)` line that consumes `pool`).
3. Add these methods to `impl PluginService` (below `ensure_enabled`):

```rust
    pub async fn get_settings(&self) -> PluginSettings {
        self.settings().await
    }

    /// Persists the supplied keys first, then applies them to the live config. A failed
    /// write leaves the in-memory config unchanged.
    pub async fn update_settings(
        &self,
        update: PluginSettingsUpdate,
    ) -> Result<PluginSettings, ServiceError> {
        update.validate().map_err(ServiceError::Invalid)?;
        self.inner.settings_repo.set_many(&update.entries()).await?;
        let applied = {
            let mut cfg = self.inner.config.write().await;
            update.apply_to(&mut cfg.plugins);
            cfg.plugins.clone()
        };
        self.apply_timeout(&applied);
        Ok(applied)
    }

    fn apply_timeout(&self, settings: &PluginSettings) {
        self.inner
            .client
            .set_timeout(Duration::from_secs(settings.resolve_timeout_secs.max(1)));
    }

    /// Re-reads `resolve_timeout_secs` from the live config and pushes it into the client.
    /// Called before every outbound plugin call so every service instance stays current.
    pub(crate) async fn sync_client_timeout(&self) {
        let settings = self.settings().await;
        self.apply_timeout(&settings);
    }
```

4. In `register`, call `self.sync_client_timeout().await;` right after `self.ensure_enabled().await?;`.
5. In `crates/kadr-server/src/plugins/health.rs` `check_all`, call `self.sync_client_timeout().await;` right after the early-return `ensure_enabled` check.
6. In `crates/kadr-server/src/plugins/playback.rs` `resolve_item`, call `self.sync_client_timeout().await;` right before the `let request = ResolveRequest {` statement (after the cache lookup block).

- [ ] **Step 7: Add the routes**

In `crates/kadr-server/src/plugins/admin_routes.rs`:

1. Add imports: `use crate::config::PluginSettings;` and `use crate::plugins::settings::PluginSettingsUpdate;`.
2. In `admin_routes()` add this route as the FIRST `.route(...)` call so it is registered next to the collection route:

```rust
        .route(
            "/api/v1/admin/plugins/settings",
            get(get_plugin_settings).put(put_plugin_settings),
        )
```

3. Add the handlers (settings must NOT call `ensure_enabled`):

```rust
async fn get_plugin_settings(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
) -> Json<PluginSettings> {
    Json(svc.get_settings().await)
}

async fn put_plugin_settings(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
    Json(update): Json<PluginSettingsUpdate>,
) -> Result<Json<PluginSettings>, ServiceError> {
    Ok(Json(svc.update_settings(update).await?))
}
```

- [ ] **Step 8: Load stored settings at startup**

In `crates/kadr-server/src/main.rs`, immediately after the line `initialize_database(&pool).await?;` add:

```rust
    let settings_repo = kadr_storage::repos::SettingsRepository::new(pool.clone());
    config.plugins =
        kadr_server::plugins::settings::load_plugin_settings(&settings_repo, config.plugins.clone())
            .await;
    info!(plugins_enabled = config.plugins.enabled, "Plugin settings loaded");
```

(`config` is already declared `let mut config = config;` above this point.)

- [ ] **Step 9: Run tests**

Run: `cargo test -p kadr-core --test plugin_types_test && cargo test -p kadr-server --test plugin_settings_test --test plugin_client_test --test plugin_admin_test && cargo test --workspace`
Expected: PASS (existing plugin tests unchanged). If axum answers `404` instead of `405` for the `DELETE` on the settings path, keep the assertion that the request is not handled by the remove handler (not 204) and adjust the expected status to what axum returns, noting it in your report.

- [ ] **Step 10: Commit**

```bash
git add crates/kadr-core crates/kadr-server
git commit -m "feat(plugins): persist plugin settings in the database and expose them via the admin API"
```

---

### Task 3: Connection details and token endpoints

**Files:**
- Modify: `crates/kadr-server/src/plugins/service.rs`, `crates/kadr-server/src/plugins/admin_routes.rs`
- Test: `crates/kadr-server/tests/plugin_connection_test.rs`

**Interfaces:**
- Consumes: `PluginService::{ensure_enabled, secret_for, repo}`, `PluginTokenService` (held in `Inner.tokens`).
- Produces:
  - `PluginService::connection(&self, id: &str) -> Result<(PluginRecord, String /*secret*/), ServiceError>` and `PluginService::issue_token(&self, id: &str) -> Result<String, ServiceError>`.
  - `GET /api/v1/admin/plugins/{id}/connection` returns `{ base_url, secret }`; `POST /api/v1/admin/plugins/{id}/token` returns `{ token }` (400 when the plugin is still `pending`).

- [ ] **Step 1: Write the failing tests**

Create `crates/kadr-server/tests/plugin_connection_test.rs`:

```rust
mod plugin_support;

use axum::http::StatusCode;
use kadr_server::config::PluginSettings;
use plugin_support::*;
use serde_json::json;

async fn registered(h: &Harness, id: &str) -> (MockPlugin, String) {
    let mock = spawn_mock(id, &["items:publish", "playback:resolve"]).await;
    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    (mock, body["secret"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn connection_returns_the_same_secret_as_registration() {
    let h = build(enabled_settings()).await;
    let (mock, secret) = registered(&h, "hello").await;

    let (status, body) = call(&h.app, "GET", "/api/v1/admin/plugins/hello/connection", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["secret"], secret.as_str());
    assert_eq!(body["base_url"], mock.base_url);
}

#[tokio::test]
async fn connection_is_available_before_approval_too() {
    // The secret is derived, so the admin can configure the plugin before approving it.
    let h = build(enabled_settings()).await;
    registered(&h, "hello").await;
    let (status, _) = call(&h.app, "GET", "/api/v1/admin/plugins/hello/connection", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn issued_tokens_work_on_plugin_routes() {
    let h = build(enabled_settings()).await;
    registered(&h, "hello").await;
    call(&h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token), None).await;

    let (status, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/token", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token = body["token"].as_str().unwrap().to_string();
    assert!(!token.is_empty());

    let (status, body) = call(
        &h.app, "POST", "/api/v1/plugin/libraries", Some(&token),
        Some(json!({ "id": "lib-1", "name": "Hello" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn tokens_cannot_be_issued_for_pending_or_unknown_plugins() {
    let h = build(enabled_settings()).await;
    registered(&h, "hello").await;

    let (status, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/token", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "INVALID_REQUEST");

    let (status, _) = call(&h.app, "POST", "/api/v1/admin/plugins/nope/token", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&h.app, "GET", "/api/v1/admin/plugins/nope/connection", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn both_routes_are_blocked_while_plugins_are_disabled() {
    let h = build(PluginSettings::default()).await;
    for (method, uri) in [
        ("GET", "/api/v1/admin/plugins/hello/connection"),
        ("POST", "/api/v1/admin/plugins/hello/token"),
    ] {
        let (status, body) = call(&h.app, method, uri, Some(&h.admin_token), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "PLUGINS_DISABLED");
    }
}

#[tokio::test]
async fn both_routes_require_an_admin_and_reject_plugin_tokens() {
    let h = build(enabled_settings()).await;
    let (mock, _) = registered(&h, "hello").await;
    let (plugin_token, _) = {
        let (_, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token), None).await;
        (body["token"].as_str().unwrap().to_string(), mock)
    };

    for (method, uri) in [
        ("GET", "/api/v1/admin/plugins/hello/connection"),
        ("POST", "/api/v1/admin/plugins/hello/token"),
    ] {
        let (s, _) = call(&h.app, method, uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = call(&h.app, method, uri, Some(&h.user_token), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = call(&h.app, method, uri, Some(&plugin_token), None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "a plugin token must not reach admin routes");
    }
}

#[tokio::test]
async fn the_secret_never_appears_in_the_plugin_list() {
    let h = build(enabled_settings()).await;
    let (_, secret) = registered(&h, "hello").await;
    let (_, raw, _) = {
        let (status, headers, bytes) = call_raw(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.admin_token), &[], None).await;
        (status, String::from_utf8(bytes).unwrap(), headers)
    };
    assert!(!raw.contains(&secret), "list response must not leak the derived secret");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p kadr-server --test plugin_connection_test`
Expected: FAIL (routes return 404/405).

- [ ] **Step 3: Implement the service methods**

In `crates/kadr-server/src/plugins/service.rs` add to `impl PluginService` (after `set_enabled`):

```rust
    /// Connection details an admin copies into the plugin process. The secret is derived, so it
    /// can be shown again at any time (including before approval).
    pub async fn connection(&self, id: &str) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        let secret = self.secret_for(&record.id);
        Ok((record, secret))
    }

    /// Issues a fresh plugin-to-server token for an approved plugin.
    pub async fn issue_token(&self, id: &str) -> Result<String, ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        if record.status == PluginStatus::Pending {
            return Err(ServiceError::Invalid(
                "plugin has not been approved yet".to_string(),
            ));
        }
        self.inner
            .tokens
            .issue(&record.id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))
    }
```

- [ ] **Step 4: Add the routes**

In `crates/kadr-server/src/plugins/admin_routes.rs` add to `admin_routes()` (next to the other `{id}` routes):

```rust
        .route("/api/v1/admin/plugins/{id}/connection", get(plugin_connection))
        .route("/api/v1/admin/plugins/{id}/token", post(issue_plugin_token))
```

and the handlers:

```rust
async fn plugin_connection(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<Value>, ServiceError> {
    let (record, secret) = svc.connection(&id).await?;
    Ok(Json(json!({ "base_url": record.base_url, "secret": secret })))
}

async fn issue_plugin_token(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
) -> Result<Json<Value>, ServiceError> {
    let token = svc.issue_token(&id).await?;
    Ok(Json(json!({ "token": token })))
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p kadr-server --test plugin_connection_test && cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/kadr-server
git commit -m "feat(plugins): add connection details and token endpoints for admins"
```


### Task 4: Web plugin types, API client methods and `describeApiError`

**Files:**
- Modify: `web/src/types/index.ts`, `web/src/api/client.ts`
- Test: `web/src/api/client.test.ts` (append a `describe` block)

**Interfaces:**
- Produces types: `PluginStatus`, `PluginPermission`, `PluginManifest`, `PluginRecord`, `PluginSettings`, `UpdatePluginSettingsPayload`, `PluginConnection`, `RegisterPluginResponse`, `ApprovePluginResponse`.
- Produces on `ApiClient` (and the exported `api` singleton): `getPluginSettings()`, `updatePluginSettings(payload)`, `listPlugins()`, `registerPlugin(baseUrl)`, `approvePlugin(id, permissions?)`, `setPluginEnabled(id, enabled)`, `removePlugin(id, purge?)`, `getPluginConnection(id)`, `issuePluginToken(id)`.
- Produces `describeApiError(err: unknown): string` exported from `web/src/api/client.ts`: prefers the API's `message` field, then the `ApiError` message, then `Error.message`, else `'Unexpected error'`.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/api/client.test.ts` (inside the file, as a new top-level `describe`; reuse the imports at the top and add `describeApiError` to the `./client` import):

```ts
describe('ApiClient plugin management', () => {
  let client: ApiClient;
  let mockFetch: ReturnType<typeof vi.fn>;

  const jsonResponse = (body: unknown, status = 200) => ({
    ok: status >= 200 && status < 300,
    status,
    statusText: 'x',
    headers: new Headers({ 'content-type': 'application/json' }),
    json: async () => body,
  });

  beforeEach(() => {
    localStorage.clear();
    mockFetch = vi.fn();
    globalThis.fetch = mockFetch;
    client = new ApiClient();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('gets and updates plugin settings', async () => {
    const settings = { enabled: false, resolve_timeout_secs: 5, allow_private_proxy: false };
    mockFetch.mockResolvedValueOnce(jsonResponse(settings));
    expect(await client.getPluginSettings()).toEqual(settings);
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins/settings');

    mockFetch.mockResolvedValueOnce(jsonResponse({ ...settings, enabled: true }));
    const updated = await client.updatePluginSettings({ enabled: true });
    expect(updated.enabled).toBe(true);
    const [url, init] = mockFetch.mock.calls[1];
    expect(url).toBe('/api/v1/admin/plugins/settings');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body)).toEqual({ enabled: true });
  });

  it('lists and registers plugins', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse([]));
    expect(await client.listPlugins()).toEqual([]);
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins');

    mockFetch.mockResolvedValueOnce(jsonResponse({ plugin: { id: 'hello' }, secret: 's3' }, 201));
    const res = await client.registerPlugin('http://127.0.0.1:9100');
    expect(res.secret).toBe('s3');
    const [url, init] = mockFetch.mock.calls[1];
    expect(url).toBe('/api/v1/admin/plugins');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ base_url: 'http://127.0.0.1:9100' });
  });

  it('approves with and without an explicit permission list', async () => {
    mockFetch.mockResolvedValue(jsonResponse({ plugin: { id: 'hello' }, token: 't' }));
    await client.approvePlugin('hello', ['items:publish']);
    let [url, init] = mockFetch.mock.calls[0];
    expect(url).toBe('/api/v1/admin/plugins/hello/approve');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ permissions: ['items:publish'] });

    await client.approvePlugin('hello');
    [url, init] = mockFetch.mock.calls[1];
    expect(init.body).toBeUndefined();
  });

  it('enables, disables and removes plugins', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ id: 'hello', status: 'disabled' }));
    await client.setPluginEnabled('hello', false);
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins/hello/disable');
    mockFetch.mockResolvedValueOnce(jsonResponse({ id: 'hello', status: 'enabled' }));
    await client.setPluginEnabled('hello', true);
    expect(mockFetch.mock.calls[1][0]).toBe('/api/v1/admin/plugins/hello/enable');

    mockFetch.mockResolvedValueOnce({ ok: true, status: 204, statusText: 'No Content', headers: new Headers() });
    await client.removePlugin('hello');
    expect(mockFetch.mock.calls[2][0]).toBe('/api/v1/admin/plugins/hello');
    expect(mockFetch.mock.calls[2][1].method).toBe('DELETE');

    mockFetch.mockResolvedValueOnce({ ok: true, status: 204, statusText: 'No Content', headers: new Headers() });
    await client.removePlugin('hello', true);
    expect(mockFetch.mock.calls[3][0]).toBe('/api/v1/admin/plugins/hello?purge=true');
  });

  it('fetches connection details and issues tokens, encoding the id', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ base_url: 'http://x', secret: 's' }));
    const conn = await client.getPluginConnection('a b');
    expect(conn.secret).toBe('s');
    expect(mockFetch.mock.calls[0][0]).toBe('/api/v1/admin/plugins/a%20b/connection');

    mockFetch.mockResolvedValueOnce(jsonResponse({ token: 'tok' }));
    expect((await client.issuePluginToken('hello')).token).toBe('tok');
    expect(mockFetch.mock.calls[1][0]).toBe('/api/v1/admin/plugins/hello/token');
    expect(mockFetch.mock.calls[1][1].method).toBe('POST');
  });

  it('describeApiError prefers the API message over the error code', async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: 'PLUGIN_UNAVAILABLE', message: 'Plugin unavailable: connection failed' }, 503)
    );
    const err = await client.registerPlugin('http://x').catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.message).toBe('PLUGIN_UNAVAILABLE');
    expect(describeApiError(err)).toBe('Plugin unavailable: connection failed');

    expect(describeApiError(new ApiError(500, 'boom'))).toBe('boom');
    expect(describeApiError(new Error('plain'))).toBe('plain');
    expect(describeApiError('weird')).toBe('Unexpected error');
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd web && npx vitest run src/api/client.test.ts`
Expected: FAIL (methods and `describeApiError` do not exist).

- [ ] **Step 3: Add the types**

Append to `web/src/types/index.ts`:

```ts
// Plugin Types
export type PluginStatus = 'pending' | 'enabled' | 'disabled' | 'unhealthy';
export type PluginPermission = 'items:publish' | 'playback:resolve';

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  api_version: number;
  permissions: PluginPermission[];
  capabilities: {
    virtual_library: boolean;
    playback_resolver: boolean;
  };
}

export interface PluginRecord {
  id: string;
  name: string;
  base_url: string;
  manifest: PluginManifest;
  granted_permissions: PluginPermission[];
  status: PluginStatus;
  created_at: number;
}

export interface PluginSettings {
  enabled: boolean;
  resolve_timeout_secs: number;
  allow_private_proxy: boolean;
}

export type UpdatePluginSettingsPayload = Partial<PluginSettings>;

export interface PluginConnection {
  base_url: string;
  secret: string;
}

export interface RegisterPluginResponse {
  plugin: PluginRecord;
  secret: string;
}

export interface ApprovePluginResponse {
  plugin: PluginRecord;
  token: string;
}
```

- [ ] **Step 4: Add the client methods and helper**

In `web/src/api/client.ts`:

1. Add to the `import type { ... } from '../types';` list: `ApprovePluginResponse, PluginConnection, PluginPermission, PluginRecord, PluginSettings, RegisterPluginResponse, UpdatePluginSettingsPayload`.
2. Add inside `class ApiClient`, after `createUser` (before the closing `}` of the class):

```ts
  // Plugin Management
  public async getPluginSettings(): Promise<PluginSettings> {
    return this.request<PluginSettings>('/api/v1/admin/plugins/settings');
  }

  public async updatePluginSettings(payload: UpdatePluginSettingsPayload): Promise<PluginSettings> {
    return this.request<PluginSettings>('/api/v1/admin/plugins/settings', {
      method: 'PUT',
      body: JSON.stringify(payload),
    });
  }

  public async listPlugins(): Promise<PluginRecord[]> {
    return this.request<PluginRecord[]>('/api/v1/admin/plugins');
  }

  public async registerPlugin(baseUrl: string): Promise<RegisterPluginResponse> {
    return this.request<RegisterPluginResponse>('/api/v1/admin/plugins', {
      method: 'POST',
      body: JSON.stringify({ base_url: baseUrl }),
    });
  }

  public async approvePlugin(
    id: string,
    permissions?: PluginPermission[]
  ): Promise<ApprovePluginResponse> {
    return this.request<ApprovePluginResponse>(
      `/api/v1/admin/plugins/${encodeURIComponent(id)}/approve`,
      {
        method: 'POST',
        body: permissions ? JSON.stringify({ permissions }) : undefined,
      }
    );
  }

  public async setPluginEnabled(id: string, enabled: boolean): Promise<PluginRecord> {
    return this.request<PluginRecord>(
      `/api/v1/admin/plugins/${encodeURIComponent(id)}/${enabled ? 'enable' : 'disable'}`,
      { method: 'POST' }
    );
  }

  public async removePlugin(id: string, purge = false): Promise<void> {
    await this.request<void>(
      `/api/v1/admin/plugins/${encodeURIComponent(id)}${purge ? '?purge=true' : ''}`,
      { method: 'DELETE' }
    );
  }

  public async getPluginConnection(id: string): Promise<PluginConnection> {
    return this.request<PluginConnection>(
      `/api/v1/admin/plugins/${encodeURIComponent(id)}/connection`
    );
  }

  public async issuePluginToken(id: string): Promise<{ token: string }> {
    return this.request<{ token: string }>(
      `/api/v1/admin/plugins/${encodeURIComponent(id)}/token`,
      { method: 'POST' }
    );
  }
```

3. Add below the class (above `export const api = new ApiClient();`):

```ts
/** Human-readable text for an API error: prefers the server's `message` over its `error` code. */
export function describeApiError(err: unknown): string {
  if (err instanceof ApiError) {
    const details = err.details;
    if (
      details &&
      typeof details === 'object' &&
      'message' in details &&
      typeof (details as { message: unknown }).message === 'string'
    ) {
      return (details as { message: string }).message;
    }
    return err.message;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return 'Unexpected error';
}
```

- [ ] **Step 5: Run tests**

Run: `cd web && npx vitest run src/api/client.test.ts && npx tsc -b`
Expected: PASS, no type errors.

- [ ] **Step 6: Commit**

```bash
git add web/src/types web/src/api
git commit -m "feat(web): add plugin types, API client methods and describeApiError"
```

---

### Task 5: Approve, Remove and Connection dialogs

**Files:**
- Create: `web/src/components/admin/ApprovePluginDialog.tsx`, `web/src/components/admin/RemovePluginDialog.tsx`, `web/src/components/admin/ConnectionDialog.tsx`
- Test: `web/src/components/admin/PluginDialogs.test.tsx`

**Interfaces:**
- Consumes: Task 4 types and `api.getPluginConnection`, `api.issuePluginToken`, `describeApiError`.
- Produces:
  - `ApprovePluginDialog({ plugin: PluginRecord, busy?: boolean, onConfirm(permissions: PluginPermission[]): void, onCancel(): void })`: dialog labelled `Approve <plugin name>`, one checkbox per requested permission (all checked), sends only the checked ones in manifest order.
  - `RemovePluginDialog({ plugin, busy?, onConfirm(purge: boolean): void, onCancel(): void })`: dialog labelled `Remove <plugin name>`, an unchecked "Also delete this plugin's libraries and items" checkbox.
  - `ConnectionDialog({ plugin, onClose(): void })`: dialog labelled `Connection details for <plugin name>`; loads the connection on mount; copy buttons; "Generate token" button; an environment-variable block.

- [ ] **Step 1: Write the failing tests**

Create `web/src/components/admin/PluginDialogs.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ApprovePluginDialog } from './ApprovePluginDialog';
import { RemovePluginDialog } from './RemovePluginDialog';
import { ConnectionDialog } from './ConnectionDialog';
import { api, ApiError } from '../../api/client';
import type { PluginRecord } from '../../types';

const plugin: PluginRecord = {
  id: 'hello',
  name: 'Hello Plugin',
  base_url: 'http://127.0.0.1:9100',
  manifest: {
    id: 'hello',
    name: 'Hello Plugin',
    version: '0.1.0',
    api_version: 1,
    permissions: ['items:publish', 'playback:resolve'],
    capabilities: { virtual_library: true, playback_resolver: true },
  },
  granted_permissions: [],
  status: 'pending',
  created_at: 1700000000,
};

describe('ApprovePluginDialog', () => {
  it('lists every requested permission, all checked by default', () => {
    render(<ApprovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole('dialog', { name: /Approve Hello Plugin/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/Publish items/i)).toBeChecked();
    expect(screen.getByLabelText(/Resolve playback/i)).toBeChecked();
  });

  it('confirms only the checked permissions, in manifest order', () => {
    const onConfirm = vi.fn();
    render(<ApprovePluginDialog plugin={plugin} onConfirm={onConfirm} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByLabelText(/Resolve playback/i));
    fireEvent.click(screen.getByRole('button', { name: /Approve and enable/i }));
    expect(onConfirm).toHaveBeenCalledWith(['items:publish']);
  });

  it('can approve with no permissions and can be cancelled', () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ApprovePluginDialog plugin={plugin} onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByLabelText(/Publish items/i));
    fireEvent.click(screen.getByLabelText(/Resolve playback/i));
    fireEvent.click(screen.getByRole('button', { name: /Approve and enable/i }));
    expect(onConfirm).toHaveBeenCalledWith([]);
    fireEvent.click(screen.getByRole('button', { name: /^Cancel$/i }));
    expect(onCancel).toHaveBeenCalled();
  });

  it('explains plugins that request nothing and disables the button while busy', () => {
    const none = { ...plugin, manifest: { ...plugin.manifest, permissions: [] } };
    render(<ApprovePluginDialog plugin={none} busy onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByText(/does not request any permissions/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Approve and enable/i })).toBeDisabled();
  });
});

describe('RemovePluginDialog', () => {
  it('defaults to keeping the data and confirms with purge only when checked', () => {
    const onConfirm = vi.fn();
    render(<RemovePluginDialog plugin={plugin} onConfirm={onConfirm} onCancel={vi.fn()} />);
    const purge = screen.getByLabelText(/Also delete this plugin's libraries and items/i);
    expect(purge).not.toBeChecked();
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    expect(onConfirm).toHaveBeenLastCalledWith(false);

    fireEvent.click(purge);
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    expect(onConfirm).toHaveBeenLastCalledWith(true);
  });

  it('can be cancelled', () => {
    const onCancel = vi.fn();
    render(<RemovePluginDialog plugin={plugin} onConfirm={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole('button', { name: /^Cancel$/i }));
    expect(onCancel).toHaveBeenCalled();
  });
});

describe('ConnectionDialog', () => {
  const writeText = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    vi.spyOn(api, 'getPluginConnection').mockResolvedValue({
      base_url: 'http://127.0.0.1:9100',
      secret: 'SECRET-VALUE',
    });
    vi.spyOn(api, 'issuePluginToken').mockResolvedValue({ token: 'TOKEN-VALUE' });
  });

  it('shows the base URL and secret and copies the secret', async () => {
    render(<ConnectionDialog plugin={plugin} onClose={vi.fn()} />);
    expect(screen.getByRole('dialog', { name: /Connection details for Hello Plugin/i })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(screen.getByDisplayValue('http://127.0.0.1:9100')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Copy secret/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('SECRET-VALUE'));
  });

  it('generates a token on demand and builds the environment block', async () => {
    render(<ConnectionDialog plugin={plugin} onClose={vi.fn()} />);
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(api.issuePluginToken).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: /Generate token/i }));
    await waitFor(() => expect(screen.getByDisplayValue('TOKEN-VALUE')).toBeInTheDocument());
    expect(api.issuePluginToken).toHaveBeenCalledWith('hello');

    const env = screen.getByTestId('connection-env');
    expect(env.textContent).toContain('KADR_PLUGIN_SECRET=SECRET-VALUE');
    expect(env.textContent).toContain('KADR_PLUGIN_TOKEN=TOKEN-VALUE');
    expect(env.textContent).toContain(`KADR_URL=${window.location.origin}`);

    fireEvent.click(screen.getByRole('button', { name: /Copy environment variables/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(env.textContent));
  });

  it('shows API error messages and closes', async () => {
    vi.spyOn(api, 'getPluginConnection').mockRejectedValue(
      new ApiError(404, 'NOT_FOUND', { error: 'NOT_FOUND', message: "plugin 'hello' not found" })
    );
    const onClose = vi.fn();
    render(<ConnectionDialog plugin={plugin} onClose={onClose} />);
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent("plugin 'hello' not found"));
    fireEvent.click(screen.getByRole('button', { name: /^Close$/i }));
    expect(onClose).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd web && npx vitest run src/components/admin/PluginDialogs.test.tsx`
Expected: FAIL (components missing).

- [ ] **Step 3: Implement `ApprovePluginDialog`**

Create `web/src/components/admin/ApprovePluginDialog.tsx`:

```tsx
import React, { useState } from 'react';
import { X } from 'lucide-react';
import type { PluginPermission, PluginRecord } from '../../types';

const PERMISSION_INFO: Record<PluginPermission, { title: string; description: string }> = {
  'items:publish': {
    title: 'Publish items',
    description: 'Create its own libraries and add or remove items in them.',
  },
  'playback:resolve': {
    title: 'Resolve playback',
    description: 'Be asked how to play the items it published (stream URL, proxy or transcode).',
  },
};

export interface ApprovePluginDialogProps {
  plugin: PluginRecord;
  busy?: boolean;
  onConfirm: (permissions: PluginPermission[]) => void;
  onCancel: () => void;
}

export const ApprovePluginDialog: React.FC<ApprovePluginDialogProps> = ({
  plugin,
  busy = false,
  onConfirm,
  onCancel,
}) => {
  const requested = plugin.manifest.permissions;
  const [selected, setSelected] = useState<PluginPermission[]>(requested);

  const toggle = (permission: PluginPermission) => {
    setSelected((current) =>
      current.includes(permission)
        ? current.filter((p) => p !== permission)
        : [...current, permission]
    );
  };

  const confirm = () => onConfirm(requested.filter((p) => selected.includes(p)));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Approve ${plugin.name}`}
        className="w-full max-w-lg bg-panel border border-border-subtle rounded-xl p-6 shadow-xl space-y-4"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h3 className="text-lg font-semibold text-text-main">Approve {plugin.name}</h3>
            <p className="text-xs text-muted mt-0.5">
              v{plugin.manifest.version} · {plugin.base_url}
            </p>
          </div>
          <button
            type="button"
            onClick={onCancel}
            aria-label="Close"
            className="p-1.5 text-muted hover:text-text-main rounded-lg hover:bg-panel-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <p className="text-sm text-muted">
          This plugin asks for the permissions below. Nothing is granted until you confirm.
        </p>

        {requested.length === 0 ? (
          <p className="text-sm text-text-main">This plugin does not request any permissions.</p>
        ) : (
          <ul className="space-y-3">
            {requested.map((permission) => (
              <li key={permission}>
                <label className="flex items-start gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={selected.includes(permission)}
                    onChange={() => toggle(permission)}
                    className="mt-1 accent-accent"
                  />
                  <span>
                    <span className="block text-sm font-medium text-text-main">
                      {PERMISSION_INFO[permission].title}
                    </span>
                    <span className="block text-xs text-muted">
                      {PERMISSION_INFO[permission].description}
                    </span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}

        <div className="flex justify-end gap-3 pt-2">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-sm text-muted hover:text-text-main rounded-xl hover:bg-panel-hover cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirm}
            disabled={busy}
            className="px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
          >
            Approve and enable
          </button>
        </div>
      </div>
    </div>
  );
};
```

- [ ] **Step 4: Implement `RemovePluginDialog`**

Create `web/src/components/admin/RemovePluginDialog.tsx`:

```tsx
import React, { useState } from 'react';
import { X } from 'lucide-react';
import type { PluginRecord } from '../../types';

export interface RemovePluginDialogProps {
  plugin: PluginRecord;
  busy?: boolean;
  onConfirm: (purge: boolean) => void;
  onCancel: () => void;
}

export const RemovePluginDialog: React.FC<RemovePluginDialogProps> = ({
  plugin,
  busy = false,
  onConfirm,
  onCancel,
}) => {
  const [purge, setPurge] = useState(false);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Remove ${plugin.name}`}
        className="w-full max-w-md bg-panel border border-border-subtle rounded-xl p-6 shadow-xl space-y-4"
      >
        <div className="flex items-start justify-between gap-4">
          <h3 className="text-lg font-semibold text-text-main">Remove {plugin.name}?</h3>
          <button
            type="button"
            onClick={onCancel}
            aria-label="Close"
            className="p-1.5 text-muted hover:text-text-main rounded-lg hover:bg-panel-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <p className="text-sm text-muted">
          Kadr will stop talking to this plugin. Items it published stay in your libraries but cannot
          be played unless you delete them too.
        </p>

        <label className="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            checked={purge}
            onChange={(e) => setPurge(e.target.checked)}
            className="mt-1 accent-accent"
          />
          <span className="text-sm text-text-main">
            Also delete this plugin&apos;s libraries and items
          </span>
        </label>

        <div className="flex justify-end gap-3 pt-2">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-sm text-muted hover:text-text-main rounded-xl hover:bg-panel-hover cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => onConfirm(purge)}
            disabled={busy}
            className="px-4 py-2 text-sm font-semibold bg-cta text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
          >
            Remove plugin
          </button>
        </div>
      </div>
    </div>
  );
};
```

- [ ] **Step 5: Implement `ConnectionDialog`**

Create `web/src/components/admin/ConnectionDialog.tsx`:

```tsx
import React, { useEffect, useState } from 'react';
import { Copy, X } from 'lucide-react';
import { api, describeApiError } from '../../api/client';
import type { PluginConnection, PluginRecord } from '../../types';

export interface ConnectionDialogProps {
  plugin: PluginRecord;
  onClose: () => void;
}

const inputClass =
  'flex-1 min-w-0 bg-canvas border border-border-subtle rounded-lg px-3 py-2 text-sm text-text-main font-mono';
const copyClass =
  'shrink-0 flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-text-main bg-panel-hover rounded-lg hover:bg-accent hover:text-canvas cursor-pointer';

export const ConnectionDialog: React.FC<ConnectionDialogProps> = ({ plugin, onClose }) => {
  const [connection, setConnection] = useState<PluginConnection | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getPluginConnection(plugin.id)
      .then((c) => {
        if (!cancelled) setConnection(c);
      })
      .catch((err) => {
        if (!cancelled) setError(describeApiError(err));
      });
    return () => {
      cancelled = true;
    };
  }, [plugin.id]);

  const generateToken = async () => {
    setGenerating(true);
    setError(null);
    try {
      const res = await api.issuePluginToken(plugin.id);
      setToken(res.token);
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setGenerating(false);
    }
  };

  const copy = async (label: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(label);
    } catch {
      setError('Copy failed. Select the text and copy it manually.');
    }
  };

  const envBlock = connection
    ? [
        `KADR_URL=${window.location.origin}`,
        `KADR_PLUGIN_SECRET=${connection.secret}`,
        `KADR_PLUGIN_TOKEN=${token ?? '<click "Generate token">'}`,
      ].join('\n')
    : '';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Connection details for ${plugin.name}`}
        className="w-full max-w-2xl bg-panel border border-border-subtle rounded-xl p-6 shadow-xl space-y-4"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h3 className="text-lg font-semibold text-text-main">
              Connection details for {plugin.name}
            </h3>
            <p className="text-xs text-muted mt-0.5">
              Give these values to the plugin process. Treat the secret and token like passwords.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close dialog"
            className="p-1.5 text-muted hover:text-text-main rounded-lg hover:bg-panel-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {error && (
          <div role="alert" className="text-sm text-cta bg-cta/15 border border-cta/30 rounded-lg px-3 py-2">
            {error}
          </div>
        )}

        {connection && (
          <div className="space-y-4">
            <div>
              <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-1">
                Plugin base URL
              </label>
              <input readOnly value={connection.base_url} aria-label="Plugin base URL" className={inputClass} />
            </div>

            <div>
              <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-1">
                Secret (Kadr authenticates to the plugin with this)
              </label>
              <div className="flex gap-2">
                <input readOnly value={connection.secret} aria-label="Secret" className={inputClass} />
                <button type="button" className={copyClass} onClick={() => copy('secret', connection.secret)}>
                  <Copy className="w-3.5 h-3.5" /> Copy secret
                </button>
              </div>
            </div>

            <div>
              <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-1">
                Token (the plugin authenticates to Kadr with this)
              </label>
              {token ? (
                <div className="flex gap-2">
                  <input readOnly value={token} aria-label="Token" className={inputClass} />
                  <button type="button" className={copyClass} onClick={() => copy('token', token)}>
                    <Copy className="w-3.5 h-3.5" /> Copy token
                  </button>
                </div>
              ) : (
                <p className="text-xs text-muted mb-2">No token generated yet.</p>
              )}
              <button
                type="button"
                onClick={generateToken}
                disabled={generating || plugin.status === 'pending'}
                title={plugin.status === 'pending' ? 'Approve the plugin first' : undefined}
                className="mt-2 px-3 py-2 text-xs font-semibold bg-accent text-canvas rounded-lg disabled:opacity-50 cursor-pointer"
              >
                Generate token
              </button>
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-muted uppercase tracking-wider">
                  Environment variables
                </span>
                <button type="button" className={copyClass} onClick={() => copy('env', envBlock)}>
                  <Copy className="w-3.5 h-3.5" /> Copy environment variables
                </button>
              </div>
              <pre
                data-testid="connection-env"
                className="bg-canvas border border-border-subtle rounded-lg p-3 text-xs text-text-main font-mono overflow-x-auto whitespace-pre-wrap break-all"
              >
                {envBlock}
              </pre>
            </div>
            {copied && <p className="text-xs text-emerald-300">Copied {copied}.</p>}
          </div>
        )}

        <div className="flex justify-end pt-2">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 text-sm text-muted hover:text-text-main rounded-xl hover:bg-panel-hover cursor-pointer"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
```

Note: the X icon button is named "Close dialog" and the footer button "Close" so the footer button can be selected unambiguously with `name: /^Close$/i`.

- [ ] **Step 6: Run tests**

Run: `cd web && npx vitest run src/components/admin/PluginDialogs.test.tsx && npx tsc -b`
Expected: PASS. If `ApprovePluginDialog`'s two buttons named "Close"/"Cancel" make `getByRole` ambiguous, keep the exact names used in the tests above (`Close` aria-label on the X button of Approve/Remove only matters for ambiguity with `name: /^Cancel$/i`, which does not match it).

- [ ] **Step 7: Commit**

```bash
git add web/src/components/admin
git commit -m "feat(web): add approve, remove and connection dialogs for plugins"
```

---

### Task 6: `RegisterPluginForm` and `PluginList`

**Files:**
- Create: `web/src/components/admin/RegisterPluginForm.tsx`, `web/src/components/admin/PluginList.tsx`
- Test: `web/src/components/admin/PluginList.test.tsx`

**Interfaces:**
- Consumes: `PluginRecord`, `PluginStatus`.
- Produces:
  - `RegisterPluginForm({ onRegister(baseUrl: string): Promise<boolean>, disabled?: boolean })`: URL input labelled `Plugin base URL`, button `Register plugin`; trims the URL, ignores empty input, clears the field only when `onRegister` resolves `true`.
  - `PluginList({ plugins, busy?: boolean, onApprove(p), onEnable(p), onDisable(p), onConnection(p), onRemove(p) })`: one row per plugin with `data-testid="plugin-row-<id>"`; empty state `No plugins registered yet.`; buttons per status: pending → `Review & approve`; enabled or unhealthy → `Connection details`, `Disable`; disabled → `Enable`, `Connection details`; every row → `Remove`.

- [ ] **Step 1: Write the failing tests**

Create `web/src/components/admin/PluginList.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { PluginList } from './PluginList';
import { RegisterPluginForm } from './RegisterPluginForm';
import type { PluginRecord, PluginStatus } from '../../types';

const make = (id: string, status: PluginStatus, granted: PluginRecord['granted_permissions'] = []): PluginRecord => ({
  id,
  name: `Plugin ${id}`,
  base_url: `http://127.0.0.1:9${id.length}00`,
  manifest: {
    id,
    name: `Plugin ${id}`,
    version: '1.2.3',
    api_version: 1,
    permissions: ['items:publish', 'playback:resolve'],
    capabilities: { virtual_library: true, playback_resolver: true },
  },
  granted_permissions: granted,
  status,
  created_at: 1,
});

const handlers = () => ({
  onApprove: vi.fn(),
  onEnable: vi.fn(),
  onDisable: vi.fn(),
  onConnection: vi.fn(),
  onRemove: vi.fn(),
});

describe('PluginList', () => {
  it('shows an empty state', () => {
    render(<PluginList plugins={[]} {...handlers()} />);
    expect(screen.getByText(/No plugins registered yet/i)).toBeInTheDocument();
  });

  it('renders name, version, status and granted permissions', () => {
    render(
      <PluginList
        plugins={[make('a', 'enabled', ['items:publish'])]}
        {...handlers()}
      />
    );
    const row = screen.getByTestId('plugin-row-a');
    expect(within(row).getByText('Plugin a')).toBeInTheDocument();
    expect(within(row).getByText(/v1\.2\.3/)).toBeInTheDocument();
    expect(within(row).getByText(/Enabled/)).toBeInTheDocument();
    expect(within(row).getByText(/items:publish/)).toBeInTheDocument();
    expect(within(row).queryByText(/playback:resolve/)).toBeNull();
  });

  it('shows the requested permissions for pending plugins', () => {
    render(<PluginList plugins={[make('p', 'pending')]} {...handlers()} />);
    const row = screen.getByTestId('plugin-row-p');
    expect(within(row).getByText(/Awaiting approval/i)).toBeInTheDocument();
    expect(within(row).getByText(/items:publish/)).toBeInTheDocument();
    expect(within(row).getByText(/playback:resolve/)).toBeInTheDocument();
  });

  it('offers the right actions for each status', () => {
    const plugins = [
      make('p', 'pending'),
      make('e', 'enabled'),
      make('u', 'unhealthy'),
      make('d', 'disabled'),
    ];
    render(<PluginList plugins={plugins} {...handlers()} />);
    const names = (id: string) =>
      within(screen.getByTestId(`plugin-row-${id}`))
        .getAllByRole('button')
        .map((b) => b.textContent?.trim());

    expect(names('p')).toEqual(['Review & approve', 'Remove']);
    expect(names('e')).toEqual(['Connection details', 'Disable', 'Remove']);
    expect(names('u')).toEqual(['Connection details', 'Disable', 'Remove']);
    expect(names('d')).toEqual(['Enable', 'Connection details', 'Remove']);
  });

  it('invokes the matching handler with the plugin', () => {
    const h = handlers();
    const plugins = [make('p', 'pending'), make('e', 'enabled'), make('d', 'disabled')];
    render(<PluginList plugins={plugins} {...h} />);

    fireEvent.click(within(screen.getByTestId('plugin-row-p')).getByRole('button', { name: /Review & approve/i }));
    expect(h.onApprove).toHaveBeenCalledWith(plugins[0]);
    fireEvent.click(within(screen.getByTestId('plugin-row-e')).getByRole('button', { name: /^Disable$/i }));
    expect(h.onDisable).toHaveBeenCalledWith(plugins[1]);
    fireEvent.click(within(screen.getByTestId('plugin-row-e')).getByRole('button', { name: /Connection details/i }));
    expect(h.onConnection).toHaveBeenCalledWith(plugins[1]);
    fireEvent.click(within(screen.getByTestId('plugin-row-d')).getByRole('button', { name: /^Enable$/i }));
    expect(h.onEnable).toHaveBeenCalledWith(plugins[2]);
    fireEvent.click(within(screen.getByTestId('plugin-row-d')).getByRole('button', { name: /^Remove$/i }));
    expect(h.onRemove).toHaveBeenCalledWith(plugins[2]);
  });

  it('disables every action while busy', () => {
    render(<PluginList plugins={[make('e', 'enabled')]} busy {...handlers()} />);
    for (const button of within(screen.getByTestId('plugin-row-e')).getAllByRole('button')) {
      expect(button).toBeDisabled();
    }
  });
});

describe('RegisterPluginForm', () => {
  it('submits the trimmed URL and clears the field on success', async () => {
    const onRegister = vi.fn().mockResolvedValue(true);
    render(<RegisterPluginForm onRegister={onRegister} />);
    const input = screen.getByLabelText(/Plugin base URL/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: '  http://127.0.0.1:9100  ' } });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(onRegister).toHaveBeenCalledWith('http://127.0.0.1:9100'));
    await waitFor(() => expect(input.value).toBe(''));
  });

  it('keeps the field when registration fails', async () => {
    const onRegister = vi.fn().mockResolvedValue(false);
    render(<RegisterPluginForm onRegister={onRegister} />);
    const input = screen.getByLabelText(/Plugin base URL/i) as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'http://bad' } });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(onRegister).toHaveBeenCalled());
    expect(input.value).toBe('http://bad');
  });

  it('ignores empty input and respects disabled', () => {
    const onRegister = vi.fn();
    const { rerender } = render(<RegisterPluginForm onRegister={onRegister} />);
    expect(screen.getByRole('button', { name: /Register plugin/i })).toBeDisabled();
    fireEvent.submit(screen.getByRole('button', { name: /Register plugin/i }).closest('form')!);
    expect(onRegister).not.toHaveBeenCalled();

    rerender(<RegisterPluginForm onRegister={onRegister} disabled />);
    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), { target: { value: 'http://x' } });
    expect(screen.getByRole('button', { name: /Register plugin/i })).toBeDisabled();
    expect(screen.getByLabelText(/Plugin base URL/i)).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd web && npx vitest run src/components/admin/PluginList.test.tsx`
Expected: FAIL (components missing).

- [ ] **Step 3: Implement `RegisterPluginForm`**

Create `web/src/components/admin/RegisterPluginForm.tsx`:

```tsx
import React, { useState } from 'react';
import { Plus } from 'lucide-react';

export interface RegisterPluginFormProps {
  /** Resolves `true` when registration succeeded (the field is then cleared). */
  onRegister: (baseUrl: string) => Promise<boolean>;
  disabled?: boolean;
}

export const RegisterPluginForm: React.FC<RegisterPluginFormProps> = ({
  onRegister,
  disabled = false,
}) => {
  const [url, setUrl] = useState('');
  const trimmed = url.trim();

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!trimmed || disabled) return;
    const ok = await onRegister(trimmed);
    if (ok) setUrl('');
  };

  return (
    <form onSubmit={submit} className="flex flex-col sm:flex-row gap-3">
      <input
        type="text"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        disabled={disabled}
        aria-label="Plugin base URL"
        placeholder="http://127.0.0.1:9100"
        className="flex-1 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
      />
      <button
        type="submit"
        disabled={disabled || !trimmed}
        className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
      >
        <Plus className="w-4 h-4" />
        Register plugin
      </button>
    </form>
  );
};
```

- [ ] **Step 4: Implement `PluginList`**

Create `web/src/components/admin/PluginList.tsx`:

```tsx
import React from 'react';
import type { PluginRecord, PluginStatus } from '../../types';

export interface PluginListProps {
  plugins: PluginRecord[];
  busy?: boolean;
  onApprove: (plugin: PluginRecord) => void;
  onEnable: (plugin: PluginRecord) => void;
  onDisable: (plugin: PluginRecord) => void;
  onConnection: (plugin: PluginRecord) => void;
  onRemove: (plugin: PluginRecord) => void;
}

const STATUS_LABEL: Record<PluginStatus, string> = {
  pending: 'Pending',
  enabled: 'Enabled',
  disabled: 'Disabled',
  unhealthy: 'Unhealthy',
};

const STATUS_CLASS: Record<PluginStatus, string> = {
  pending: 'bg-amber-950/80 border-amber-500/50 text-amber-300',
  enabled: 'bg-emerald-950/80 border-emerald-500/50 text-emerald-300',
  disabled: 'bg-panel-hover border-border-subtle text-muted',
  unhealthy: 'bg-cta/15 border-cta/30 text-cta',
};

const buttonClass =
  'px-3 py-1.5 text-xs font-medium rounded-lg border border-border-subtle text-text-main hover:bg-panel-hover disabled:opacity-50 cursor-pointer';
const primaryClass =
  'px-3 py-1.5 text-xs font-semibold rounded-lg bg-accent text-canvas disabled:opacity-50 cursor-pointer';
const dangerClass =
  'px-3 py-1.5 text-xs font-medium rounded-lg border border-cta/30 text-cta hover:bg-cta/15 disabled:opacity-50 cursor-pointer';

export const PluginList: React.FC<PluginListProps> = ({
  plugins,
  busy = false,
  onApprove,
  onEnable,
  onDisable,
  onConnection,
  onRemove,
}) => {
  if (plugins.length === 0) {
    return <p className="text-sm text-muted">No plugins registered yet.</p>;
  }

  return (
    <ul className="space-y-3">
      {plugins.map((plugin) => {
        const pending = plugin.status === 'pending';
        const permissions = pending ? plugin.manifest.permissions : plugin.granted_permissions;
        return (
          <li
            key={plugin.id}
            data-testid={`plugin-row-${plugin.id}`}
            className="bg-panel border border-border-subtle rounded-xl p-4 flex flex-col md:flex-row md:items-center justify-between gap-4"
          >
            <div className="min-w-0 space-y-1">
              <div className="flex items-center gap-3 flex-wrap">
                <span className="text-sm font-semibold text-text-main">{plugin.name}</span>
                <span className="text-xs text-muted">v{plugin.manifest.version}</span>
                <span
                  className={`text-xs font-medium px-2 py-0.5 rounded-full border ${STATUS_CLASS[plugin.status]}`}
                >
                  {STATUS_LABEL[plugin.status]}
                </span>
              </div>
              <p className="text-xs text-muted break-all">{plugin.base_url}</p>
              <p className="text-xs text-muted">
                {pending ? 'Awaiting approval. Requests: ' : 'Granted: '}
                {permissions.length === 0 ? 'none' : permissions.join(', ')}
              </p>
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              {pending && (
                <button type="button" className={primaryClass} disabled={busy} onClick={() => onApprove(plugin)}>
                  Review &amp; approve
                </button>
              )}
              {plugin.status === 'disabled' && (
                <button type="button" className={primaryClass} disabled={busy} onClick={() => onEnable(plugin)}>
                  Enable
                </button>
              )}
              {!pending && (
                <button type="button" className={buttonClass} disabled={busy} onClick={() => onConnection(plugin)}>
                  Connection details
                </button>
              )}
              {(plugin.status === 'enabled' || plugin.status === 'unhealthy') && (
                <button type="button" className={buttonClass} disabled={busy} onClick={() => onDisable(plugin)}>
                  Disable
                </button>
              )}
              <button type="button" className={dangerClass} disabled={busy} onClick={() => onRemove(plugin)}>
                Remove
              </button>
            </div>
          </li>
        );
      })}
    </ul>
  );
};
```

Button order check against the tests: pending → `Review & approve`, `Remove`; enabled/unhealthy → `Connection details`, `Disable`, `Remove`; disabled → `Enable`, `Connection details`, `Remove`. The JSX above renders Enable before Connection details and Disable after it, which matches.

- [ ] **Step 5: Run tests**

Run: `cd web && npx vitest run src/components/admin/PluginList.test.tsx && npx tsc -b`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/components/admin
git commit -m "feat(web): add plugin registration form and plugin list"
```

---

### Task 7: `PluginsPanel` and the Plugins tab

**Files:**
- Create: `web/src/components/admin/PluginsPanel.tsx`
- Modify: `web/src/components/admin/AdminDashboard.tsx` (tab wiring only)
- Test: `web/src/components/admin/PluginsPanel.test.tsx`, `web/src/components/admin/AdminDashboard.test.tsx` (append one test)

**Interfaces:**
- Consumes: Tasks 4 to 6 (`api.*`, `describeApiError`, dialogs, form, list).
- Produces: `PluginsPanel` (no props) and the `'plugins'` `AdminTab`.
- Behavior: loads settings on mount; loads the plugin list only when enabled; the master switch has `role="switch"`, `aria-label="Enable plugins"`; settings form (timeout 1 to 60 validated client side, private-address toggle, `Save plugin settings`); register, approve, enable, disable, remove and connection flows; refreshes the list every 15 seconds while enabled; errors show the API `message`. While plugins are off, the register form and list are dimmed and disabled and the text "Plugins are disabled" is shown; the settings form stays editable so the timeout can be set before enabling.

- [ ] **Step 1: Write the failing panel tests**

Create `web/src/components/admin/PluginsPanel.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { PluginsPanel } from './PluginsPanel';
import { api, ApiError } from '../../api/client';
import type { PluginRecord, PluginSettings } from '../../types';

const disabledSettings: PluginSettings = {
  enabled: false,
  resolve_timeout_secs: 5,
  allow_private_proxy: false,
};
const enabledSettings: PluginSettings = { ...disabledSettings, enabled: true };

const pendingPlugin: PluginRecord = {
  id: 'hello',
  name: 'Hello Plugin',
  base_url: 'http://127.0.0.1:9100',
  manifest: {
    id: 'hello',
    name: 'Hello Plugin',
    version: '0.1.0',
    api_version: 1,
    permissions: ['items:publish', 'playback:resolve'],
    capabilities: { virtual_library: true, playback_resolver: true },
  },
  granted_permissions: [],
  status: 'pending',
  created_at: 1,
};
const enabledPlugin: PluginRecord = {
  ...pendingPlugin,
  status: 'enabled',
  granted_permissions: ['items:publish', 'playback:resolve'],
};
const disabledPlugin: PluginRecord = { ...enabledPlugin, status: 'disabled' };

describe('PluginsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(api, 'getPluginSettings').mockResolvedValue(disabledSettings);
    vi.spyOn(api, 'updatePluginSettings').mockImplementation(async (payload) => ({
      ...disabledSettings,
      ...payload,
    }));
    vi.spyOn(api, 'listPlugins').mockResolvedValue([]);
    vi.spyOn(api, 'registerPlugin').mockResolvedValue({ plugin: pendingPlugin, secret: 's' });
    vi.spyOn(api, 'approvePlugin').mockResolvedValue({ plugin: enabledPlugin, token: 't' });
    vi.spyOn(api, 'setPluginEnabled').mockResolvedValue(enabledPlugin);
    vi.spyOn(api, 'removePlugin').mockResolvedValue();
    vi.spyOn(api, 'getPluginConnection').mockResolvedValue({
      base_url: pendingPlugin.base_url,
      secret: 'SECRET-VALUE',
    });
    vi.spyOn(api, 'issuePluginToken').mockResolvedValue({ token: 'TOKEN-VALUE' });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const enableSwitch = () => screen.getByRole('switch', { name: /Enable plugins/i });

  it('starts disabled: shows the notice and does not load plugins', async () => {
    render(<PluginsPanel />);
    await waitFor(() => expect(api.getPluginSettings).toHaveBeenCalled());
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));
    expect(screen.getByText(/Plugins are disabled/i)).toBeInTheDocument();
    expect(api.listPlugins).not.toHaveBeenCalled();
    expect(screen.getByLabelText(/Plugin base URL/i)).toBeDisabled();
  });

  it('enables plugins from the switch and loads the list', async () => {
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));

    fireEvent.click(enableSwitch());
    await waitFor(() => expect(api.updatePluginSettings).toHaveBeenCalledWith({ enabled: true }));
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'true'));
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(screen.queryByText(/Plugins are disabled/i)).toBeNull();
    expect(screen.getByRole('status')).toHaveTextContent(/Plugins enabled/i);
  });

  it('can disable again and clears the list', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(enableSwitch());
    await waitFor(() => expect(api.updatePluginSettings).toHaveBeenCalledWith({ enabled: false }));
    await waitFor(() => expect(screen.queryByTestId('plugin-row-hello')).toBeNull());
    expect(screen.getByText(/Plugins are disabled/i)).toBeInTheDocument();
  });

  it('loads the plugin list on mount when already enabled', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(api.listPlugins).toHaveBeenCalledTimes(1);
  });

  it('saves settings and validates the timeout client side', async () => {
    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));

    const timeout = screen.getByLabelText(/Resolve timeout/i);
    fireEvent.change(timeout, { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: /Save plugin settings/i }));
    expect(screen.getByRole('alert')).toHaveTextContent(/between 1 and 60/i);
    expect(api.updatePluginSettings).not.toHaveBeenCalled();

    fireEvent.change(timeout, { target: { value: '12' } });
    fireEvent.click(screen.getByLabelText(/Allow plugins to stream from private/i));
    fireEvent.click(screen.getByRole('button', { name: /Save plugin settings/i }));
    await waitFor(() =>
      expect(api.updatePluginSettings).toHaveBeenCalledWith({
        resolve_timeout_secs: 12,
        allow_private_proxy: true,
      })
    );
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent(/settings saved/i));
  });

  it('registers a plugin and refreshes the list', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValueOnce([]).mockResolvedValue([pendingPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByLabelText(/Plugin base URL/i)).toBeEnabled());

    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), {
      target: { value: 'http://127.0.0.1:9100' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(api.registerPlugin).toHaveBeenCalledWith('http://127.0.0.1:9100'));
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(screen.getByRole('status')).toHaveTextContent(/Registered "Hello Plugin"/);
  });

  it('shows the API message when registration fails', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.registerPlugin).mockRejectedValue(
      new ApiError(503, 'PLUGIN_UNAVAILABLE', {
        error: 'PLUGIN_UNAVAILABLE',
        message: 'Plugin unavailable: connection failed',
      })
    );
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByLabelText(/Plugin base URL/i)).toBeEnabled());
    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), { target: { value: 'http://bad' } });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Plugin unavailable: connection failed')
    );
  });

  it('approves with a permission subset', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([pendingPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /Review & approve/i }));
    const dialog = screen.getByRole('dialog', { name: /Approve Hello Plugin/i });
    fireEvent.click(within(dialog).getByLabelText(/Resolve playback/i));
    fireEvent.click(within(dialog).getByRole('button', { name: /Approve and enable/i }));

    await waitFor(() => expect(api.approvePlugin).toHaveBeenCalledWith('hello', ['items:publish']));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.getByRole('status')).toHaveTextContent(/approved and enabled/i);
  });

  it('enables and disables a plugin', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([disabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /^Enable$/i }));
    await waitFor(() => expect(api.setPluginEnabled).toHaveBeenCalledWith('hello', true));

    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    fireEvent.click(await screen.findByRole('button', { name: /^Disable$/i }));
    await waitFor(() => expect(api.setPluginEnabled).toHaveBeenCalledWith('hello', false));
  });

  it('removes a plugin with and without purging data', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /^Remove$/i }));
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenLastCalledWith('hello', false));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

    fireEvent.click(screen.getByRole('button', { name: /^Remove$/i }));
    fireEvent.click(screen.getByLabelText(/Also delete this plugin's libraries and items/i));
    fireEvent.click(screen.getByRole('button', { name: /Remove plugin/i }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenLastCalledWith('hello', true));
  });

  it('opens the connection details dialog', async () => {
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Connection details/i }));
    await waitFor(() => expect(screen.getByDisplayValue('SECRET-VALUE')).toBeInTheDocument());
    expect(api.getPluginConnection).toHaveBeenCalledWith('hello');
  });

  it('refreshes statuses every 15 seconds while enabled and keeps the last list on failure', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([enabledPlugin]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());
    expect(api.listPlugins).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(15000);
    expect(api.listPlugins).toHaveBeenCalledTimes(2);

    vi.mocked(api.listPlugins).mockRejectedValue(new Error('network down'));
    await vi.advanceTimersByTimeAsync(15000);
    expect(api.listPlugins).toHaveBeenCalledTimes(3);
    expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/last known state/i));
  });

  it('does not poll while plugins are disabled', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    render(<PluginsPanel />);
    await waitFor(() => expect(enableSwitch()).toHaveAttribute('aria-checked', 'false'));
    await vi.advanceTimersByTimeAsync(45000);
    expect(api.listPlugins).not.toHaveBeenCalled();
  });
});
```

Append this test inside the `describe('AdminDashboard Component', ...)` block of `web/src/components/admin/AdminDashboard.test.tsx` (before its closing `});`):

```tsx
  it('shows the Plugins tab and renders the plugins panel', async () => {
    vi.spyOn(api, 'getPluginSettings').mockResolvedValue({
      enabled: false,
      resolve_timeout_secs: 5,
      allow_private_proxy: false,
    });
    render(<AdminDashboard />);
    await waitFor(() => expect(screen.getByText('Featured Movies')).toBeDefined());

    fireEvent.click(screen.getByRole('button', { name: /^Plugins$/i }));
    await waitFor(() =>
      expect(screen.getByRole('switch', { name: /Enable plugins/i })).toBeDefined()
    );
    expect(api.getPluginSettings).toHaveBeenCalled();
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `cd web && npx vitest run src/components/admin/PluginsPanel.test.tsx src/components/admin/AdminDashboard.test.tsx`
Expected: FAIL (`PluginsPanel` missing, no Plugins tab).

- [ ] **Step 3: Implement `PluginsPanel`**

Create `web/src/components/admin/PluginsPanel.tsx`:

```tsx
import React, { useCallback, useEffect, useState } from 'react';
import { AlertCircle, CheckCircle2, Puzzle } from 'lucide-react';
import { api, describeApiError } from '../../api/client';
import type { PluginPermission, PluginRecord, PluginSettings } from '../../types';
import { ApprovePluginDialog } from './ApprovePluginDialog';
import { ConnectionDialog } from './ConnectionDialog';
import { PluginList } from './PluginList';
import { RegisterPluginForm } from './RegisterPluginForm';
import { RemovePluginDialog } from './RemovePluginDialog';

const REFRESH_INTERVAL_MS = 15000;

type Banner = { type: 'success' | 'error' | 'warning'; text: string } | null;

export const PluginsPanel: React.FC = () => {
  const [settings, setSettings] = useState<PluginSettings | null>(null);
  const [timeoutInput, setTimeoutInput] = useState('5');
  const [allowPrivate, setAllowPrivate] = useState(false);
  const [plugins, setPlugins] = useState<PluginRecord[]>([]);
  const [banner, setBanner] = useState<Banner>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [approving, setApproving] = useState<PluginRecord | null>(null);
  const [removing, setRemoving] = useState<PluginRecord | null>(null);
  const [connecting, setConnecting] = useState<PluginRecord | null>(null);

  const enabled = settings?.enabled ?? false;

  const applySettings = useCallback((s: PluginSettings) => {
    setSettings(s);
    setTimeoutInput(String(s.resolve_timeout_secs));
    setAllowPrivate(s.allow_private_proxy);
  }, []);

  const refresh = useCallback(async (silent = false) => {
    try {
      setPlugins(await api.listPlugins());
    } catch (err) {
      setBanner(
        silent
          ? { type: 'warning', text: 'Could not refresh plugin status; showing the last known state.' }
          : { type: 'error', text: describeApiError(err) }
      );
    }
  }, []);

  // Initial load: settings first, the plugin list only when plugins are enabled.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await api.getPluginSettings();
        if (cancelled) return;
        applySettings(s);
        if (s.enabled) await refresh();
      } catch (err) {
        if (!cancelled) setBanner({ type: 'error', text: describeApiError(err) });
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [applySettings, refresh]);

  // Keep statuses fresh (health changes) while plugins are enabled.
  useEffect(() => {
    if (!enabled) return;
    const id = setInterval(() => {
      void refresh(true);
    }, REFRESH_INTERVAL_MS);
    return () => clearInterval(id);
  }, [enabled, refresh]);

  const run = async (action: () => Promise<string | void>) => {
    setBusy(true);
    try {
      const message = await action();
      if (message) setBanner({ type: 'success', text: message });
    } catch (err) {
      setBanner({ type: 'error', text: describeApiError(err) });
    } finally {
      setBusy(false);
    }
  };

  const toggleEnabled = () =>
    run(async () => {
      const updated = await api.updatePluginSettings({ enabled: !enabled });
      applySettings(updated);
      if (updated.enabled) {
        await refresh();
      } else {
        setPlugins([]);
      }
      return updated.enabled ? 'Plugins enabled.' : 'Plugins disabled.';
    });

  const saveSettings = (e: React.FormEvent) => {
    e.preventDefault();
    const secs = Number(timeoutInput);
    if (!Number.isInteger(secs) || secs < 1 || secs > 60) {
      setBanner({
        type: 'error',
        text: 'Resolve timeout must be a whole number between 1 and 60 seconds.',
      });
      return;
    }
    void run(async () => {
      const updated = await api.updatePluginSettings({
        resolve_timeout_secs: secs,
        allow_private_proxy: allowPrivate,
      });
      applySettings(updated);
      return 'Plugin settings saved.';
    });
  };

  const register = async (baseUrl: string): Promise<boolean> => {
    let ok = false;
    await run(async () => {
      const res = await api.registerPlugin(baseUrl);
      ok = true;
      await refresh();
      return `Registered "${res.plugin.name}". Review and approve its permissions to enable it.`;
    });
    return ok;
  };

  const confirmApprove = (permissions: PluginPermission[]) => {
    const target = approving;
    if (!target) return;
    void run(async () => {
      const res = await api.approvePlugin(target.id, permissions);
      setApproving(null);
      await refresh();
      return `"${res.plugin.name}" approved and enabled.`;
    });
  };

  const setPluginEnabled = (plugin: PluginRecord, value: boolean) =>
    run(async () => {
      const updated = await api.setPluginEnabled(plugin.id, value);
      await refresh();
      return `"${updated.name}" ${value ? 'enabled' : 'disabled'}.`;
    });

  const confirmRemove = (purge: boolean) => {
    const target = removing;
    if (!target) return;
    void run(async () => {
      await api.removePlugin(target.id, purge);
      setRemoving(null);
      await refresh();
      return `Removed "${target.name}".`;
    });
  };

  return (
    <div className="mt-8 max-w-3xl space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-text-main flex items-center gap-2">
          <Puzzle className="w-5 h-5" />
          Plugins
        </h2>
        <p className="text-muted text-sm mt-0.5">
          Plugins add virtual libraries (live TV, streaming sources) and resolve playback for them.
        </p>
      </div>

      {banner && (
        <div
          role={banner.type === 'success' ? 'status' : 'alert'}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium ${
            banner.type === 'success'
              ? 'bg-emerald-950/80 border border-emerald-500/50 text-emerald-300'
              : banner.type === 'warning'
              ? 'bg-amber-950/80 border border-amber-500/50 text-amber-300'
              : 'bg-cta/15 border border-cta/30 text-cta'
          }`}
        >
          {banner.type === 'success' ? (
            <CheckCircle2 className="w-4 h-4 shrink-0" />
          ) : (
            <AlertCircle className="w-4 h-4 shrink-0" />
          )}
          <span>{banner.text}</span>
        </div>
      )}

      {loading ? (
        <p className="text-sm text-muted">Loading plugin settings…</p>
      ) : (
        <>
          <div className="bg-panel border border-border-subtle rounded-xl p-6 flex items-center justify-between gap-4 shadow-sm">
            <div>
              <h3 className="text-sm font-semibold text-text-main">Enable plugins</h3>
              <p className="text-xs text-muted mt-0.5">
                Turning plugins off keeps their data; their items just cannot be played.
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={enabled}
              aria-label="Enable plugins"
              disabled={busy}
              onClick={toggleEnabled}
              className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors cursor-pointer disabled:opacity-50 ${
                enabled ? 'bg-accent' : 'bg-panel-hover border border-border-subtle'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 rounded-full bg-canvas transition-transform ${
                  enabled ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </button>
          </div>

          <form
            onSubmit={saveSettings}
            className="bg-panel border border-border-subtle rounded-xl p-6 space-y-4 shadow-sm"
          >
            <h3 className="text-sm font-semibold text-accent uppercase tracking-wider">Settings</h3>
            <div>
              <label htmlFor="plugin-timeout" className="block text-xs font-medium text-text-main mb-1">
                Resolve timeout (seconds)
              </label>
              <input
                id="plugin-timeout"
                type="number"
                min={1}
                max={60}
                value={timeoutInput}
                onChange={(e) => setTimeoutInput(e.target.value)}
                className="w-32 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
              />
              <p className="text-xs text-muted mt-1">
                How long Kadr waits for a plugin to answer (1 to 60). Applies immediately.
              </p>
            </div>
            <label className="flex items-start gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={allowPrivate}
                onChange={(e) => setAllowPrivate(e.target.checked)}
                aria-label="Allow plugins to stream from private/LAN addresses"
                className="mt-1 accent-accent"
              />
              <span>
                <span className="block text-sm text-text-main">
                  Allow plugins to stream from private/LAN addresses
                </span>
                <span className="block text-xs text-amber-300">
                  Only enable this for plugins you trust: it lets them make Kadr connect to devices
                  on your network.
                </span>
              </span>
            </label>
            <button
              type="submit"
              disabled={busy}
              className="px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
            >
              Save plugin settings
            </button>
          </form>

          {!enabled && (
            <p className="text-sm text-muted">
              Plugins are disabled. Turn them on to register and manage plugins.
            </p>
          )}

          <div
            aria-disabled={!enabled}
            className={`space-y-6 ${enabled ? '' : 'opacity-50 pointer-events-none'}`}
          >
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider">
                Register a plugin
              </h3>
              <RegisterPluginForm onRegister={register} disabled={!enabled || busy} />
            </div>

            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider">
                Installed plugins
              </h3>
              <PluginList
                plugins={plugins}
                busy={busy}
                onApprove={setApproving}
                onEnable={(p) => void setPluginEnabled(p, true)}
                onDisable={(p) => void setPluginEnabled(p, false)}
                onConnection={setConnecting}
                onRemove={setRemoving}
              />
            </div>
          </div>
        </>
      )}

      {approving && (
        <ApprovePluginDialog
          plugin={approving}
          busy={busy}
          onConfirm={confirmApprove}
          onCancel={() => setApproving(null)}
        />
      )}
      {removing && (
        <RemovePluginDialog
          plugin={removing}
          busy={busy}
          onConfirm={confirmRemove}
          onCancel={() => setRemoving(null)}
        />
      )}
      {connecting && (
        <ConnectionDialog plugin={connecting} onClose={() => setConnecting(null)} />
      )}
    </div>
  );
};
```

- [ ] **Step 4: Wire the tab into `AdminDashboard.tsx`**

Make exactly these three changes in `web/src/components/admin/AdminDashboard.tsx`:

1. Add `Puzzle,` to the `lucide-react` import list, and add `import { PluginsPanel } from './PluginsPanel';` after the `TelemetryDashboard` import.
2. Change `export type AdminTab = 'libraries' | 'users' | 'studio' | 'telemetry' | 'config';` to include the new tab:

```ts
export type AdminTab = 'libraries' | 'users' | 'studio' | 'telemetry' | 'config' | 'plugins';
```

3. In the tab navigation, add this button directly after the "Server Configuration" button (before the closing `</div>` of the tab bar):

```tsx
        <button
          onClick={() => setActiveTab('plugins')}
          className={`flex items-center gap-2 px-4 py-2.5 text-sm transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight shrink-0 ${
            activeTab === 'plugins'
              ? 'bg-accent text-canvas font-semibold rounded-xl shadow-md'
              : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl'
          }`}
        >
          <Puzzle className="w-4 h-4" />
          Plugins
        </button>
```

and render the panel next to the other tab bodies (for example right before `{activeTab === 'config' && (`):

```tsx
      {activeTab === 'plugins' && <PluginsPanel />}
```

- [ ] **Step 5: Run tests**

Run: `cd web && npx vitest run src/components/admin && npx tsc -b`
Expected: PASS (the full existing `AdminDashboard` suite still passes).

- [ ] **Step 6: Commit**

```bash
git add web/src/components/admin
git commit -m "feat(web): add Plugins admin tab with enable switch, settings and plugin management"
```

---

### Task 8: Docs, changelog and final verification

**Files:**
- Modify: `docs/PLUGIN_GUIDE.md`, `CHANGELOG.md`, `kadr.toml`

**Interfaces:** documents everything from Tasks 1 to 7.

- [ ] **Step 1: Update the guide**

In `docs/PLUGIN_GUIDE.md`:

1. Replace the whole "Enabling plugins" section (the `[plugins]` TOML block and its lead-in) with:

````markdown
## Enabling plugins

The easiest way is the web UI: open **Administration, Plugins** and turn on **Enable plugins**. The
setting is stored in Kadr's database, takes effect immediately (no restart) and survives restarts.

`kadr.toml` still works and provides the **default** for a fresh install. A value saved from the web
UI takes precedence over `kadr.toml`.

```toml
[plugins]
enabled = true              # default false
resolve_timeout_secs = 5    # default 5, allowed range 1-60
allow_private_proxy = false # default false; see "Security notes"
```

## Managing plugins in the web UI

The **Plugins** tab lets an admin:

1. enable or disable plugin support and change the settings above;
2. **register** a plugin by URL (Kadr fetches its public manifest; the plugin appears as *pending*);
3. **review and approve** the permissions the plugin requested (you can approve a subset);
4. **enable / disable** an approved plugin, or **remove** it (optionally deleting its libraries and items);
5. open **Connection details** to copy the plugin's secret, generate a token, and copy ready-made
   `KADR_URL`, `KADR_PLUGIN_SECRET` and `KADR_PLUGIN_TOKEN` lines for the plugin's environment.

Plugin statuses refresh every 15 seconds while the tab is open.
````

2. In the "Registration and approval (admin)" table, add these rows:

````markdown
| `GET /api/v1/admin/plugins/settings` | `{ enabled, resolve_timeout_secs, allow_private_proxy }`; works while plugins are disabled |
| `PUT /api/v1/admin/plugins/settings` | partial body; validates `resolve_timeout_secs` (1-60); persists and applies immediately; works while plugins are disabled |
| `GET /api/v1/admin/plugins/{id}/connection` | `{ base_url, secret }` for configuring the plugin process |
| `POST /api/v1/admin/plugins/{id}/token` | `{ token }`: issues a fresh plugin-to-server token (approved plugins only) |
````

3. Replace the sentence in that section that says the secret is "never stored" so it also says it can be viewed again: "`secret` is derived from the server secret, never stored, and can be viewed again at any time by an admin (web UI: Connection details, or `GET .../connection`)."
4. Add under "Security notes":

````markdown
- The plugin secret is derived from Kadr's server secret. If it leaks, rotate the server secret: stop Kadr, delete the `jwt_secret` file in the data directory (find the exact path in the startup log line "Generated and saved new JWT secret"), and start Kadr again. This invalidates every user session, plugin token and derived plugin secret, so users must sign in again and plugins must be given their new secret and a fresh token from **Connection details**.
- The plugin id `settings` is reserved.
````

Before writing item 4, verify the real secret file name and location in `crates/kadr-server/src/main.rs` (search for `jwt_secret_path`) and use the exact file name and directory you find there instead of the placeholder wording if it differs.

- [ ] **Step 2: Changelog and sample config**

In `CHANGELOG.md`, in the existing `## [Unreleased]` section under `### Added`, append:

```markdown
- **Plugin management from the web UI**: new Plugins admin tab to enable plugin support, change plugin settings, register plugins by URL, approve their permissions, enable/disable/remove them, and copy connection details. Plugin settings are now stored in the database (new `settings` table) and override `kadr.toml`; new admin endpoints `GET/PUT /api/v1/admin/plugins/settings`, `GET /api/v1/admin/plugins/{id}/connection` and `POST /api/v1/admin/plugins/{id}/token`. The plugin id `settings` is reserved. `kadr.toml` `[plugins]` still provides the defaults.
```

In `kadr.toml`, add a comment line above the `[plugins]` section: `# Defaults only: values saved from the web UI (Administration > Plugins) take precedence.`

- [ ] **Step 3: Final verification**

Run each and confirm:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && npm test && npm run build
```

Expected: formatting clean, no clippy warnings, all Rust tests pass, all web tests pass, the web build succeeds. Fix any formatting or lint findings in code from this branch (behavior-preserving) and list them in your report.

- [ ] **Step 4: Manual smoke test (optional, report if skipped)**

Start Kadr with a fresh data directory and no `[plugins]` section, sign in as admin, open Administration, Plugins, turn on **Enable plugins**, register `examples/plugin-hello` (see its README), approve it, open **Connection details**, generate a token, restart Kadr and confirm the plugin is still enabled.

- [ ] **Step 5: Commit**

```bash
git add docs CHANGELOG.md kadr.toml
git commit -m "docs(plugins): document web UI plugin management and persisted settings"
```

---

## Self-Review

**Spec coverage** (spec section → task):
- §2 persistence (migration 008, `SettingsRepository`, keys, precedence, ignore-unparseable) → Tasks 1, 2.
- §3 API (settings GET/PUT, connection, token; admin-only; works while disabled; route ordering; secret re-viewable) → Tasks 2, 3.
- §4 live apply (timeout shared with client, `enabled`/`allow_private_proxy` already dynamic, monitor unchanged, stored settings loaded in `main.rs` before the router) → Task 2 (steps 5, 6, 8).
- §5 UI (switch, settings, register, list with per-status actions, approve with permission checkboxes, remove with purge, connection dialog with env lines, 15 s refresh, existing conventions) → Tasks 4 to 7.
- §6 error handling (400 on bad settings, persist-then-apply, 404/400 on connection/token, UI shows `message` and keeps the last list on refresh failure) → Tasks 2, 3, 4, 7.
- §7 testing (backend integration, storage, web Vitest) → every task has its own tests.
- §8 risks: dynamic timeout covered by the dedicated test; the rotation guidance is documented in Task 8; `AdminDashboard.tsx` changes are tab wiring only.

**Deviations/clarifications versus the spec (all small):** the settings form stays editable while plugins are off (only registration and the list are dimmed) so the timeout can be set before enabling; `connection` also works for pending plugins (the secret is derived, so there is no reason to hide it), while the UI offers Connection details only after approval; the manifest id `settings` is reserved to avoid a route collision.

**Placeholder scan:** no TBD/TODO; the only conditional instruction is the guide's rotation wording, which tells the implementer exactly how to confirm the real file name.

**Type consistency:** `PluginSettings` (TS) matches the server JSON (`enabled`, `resolve_timeout_secs`, `allow_private_proxy`); `PluginRecord` matches the Rust `PluginRecord` serialization; `approvePlugin(id, permissions?)` matches the existing `{ permissions }` body; `describeApiError` is used by the dialogs and the panel; component props defined in Tasks 5 and 6 are used unchanged in Task 7.
