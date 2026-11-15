# Git-Based Plugin Installation & Process Supervision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an admin install, supervise, and update prebuilt Kadr plugins directly from public GitHub, GitLab, and Gitea/Forgejo repository URLs without compiling on the server or manually copying credentials.

**Architecture:** A `ForgeClient` resolves release metadata and downloads platform-matched `*-<os>-<arch>.tar.gz` bundles (with optional SHA-256 verification) across GitHub, GitLab v4, and Gitea v1 APIs. A pure-Rust `installer` unpacks `manifest.json` and `plugin` into `<data_dir>/plugins/<id>/` as `pending` without executing the binary; once approved, an in-process `PluginSupervisor` allocates a loopback port, clears parent environment variables, injects `KADR_*` credentials, supervises crashes with exponential backoff, and supports atomic in-place updates.

**Tech Stack:** Rust (`axum 0.8`, `reqwest 0.12`, `flate2` with `rust_backend`, `tar 0.4`, `sha2`, `libc` on Unix, `rusqlite` via `deadpool_sqlite`), React 19 + TypeScript, Tailwind 4, `lucide-react`, Vitest + React Testing Library. Spec: `docs/superpowers/specs/2026-11-15-git-plugin-install-design.md`.

## Global Constraints

- **Never execute unapproved binaries:** Installing from Git downloads and unpacks `manifest.json` + `bin/plugin` and inserts the plugin with `status = Pending`. The binary is spawned only after admin approval (`approve`), `enable`, `update` (when permissions are already granted), or server startup (for `Enabled`/`Unhealthy` managed plugins).
- **Zero C dependencies (musl compatibility):** `flate2` MUST use `default-features = false, features = ["rust_backend"]` so `cargo check --workspace --target x86_64-unknown-linux-musl` continues to pass without a C toolchain.
- **Archive & download bounds:** Compressed archive download cap is `100 MiB`; checksum asset cap is `64 KiB`; cumulative uncompressed tar extraction cap is `250 MiB`. Reject tar entries with `..`, absolute paths, symlinks, hardlinks, FIFOs, or device nodes.
- **SSRF & transport policy:** Follow at most `5` redirect hops on forge/asset requests. When `plugins.allow_private_proxy == false`, require `https://` and reject private/loopback IPs at every hop via `ensure_public_host`. When `plugins.allow_private_proxy == true`, allow `http://` and private/loopback IPs.
- **Reserved plugin IDs:** Both `"settings"` and `"install"` must be rejected by `PluginManifest::validate()`.
- **Environment isolation:** Supervised child processes MUST call `Command::env_clear()`, pass only safe OS variables (`PATH`, `LANG`, `LC_ALL`, `TZ`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, `SYSTEMROOT`), and inject `PLUGIN_BIND`, `KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`, and `PLUGIN_DATA_DIR`.
- **No new npm packages.** Preserve backwards compatibility with existing external-URL plugins and router-builder signatures.
- **Quality gates:** Run `cargo test --workspace` after Rust tasks, `cd web && npm test -- --run` after web tasks, and conventional commits after each task.

## File Structure

| File | Responsibility |
|---|---|
| `crates/kadr-core/src/plugin.rs` | `PluginInstallKind`, `ForgeKind`, extended `PluginRecord`, reserve `"install"` manifest ID |
| `crates/kadr-storage/src/migrations/010_managed_plugins.sql` (new) | Add `install_kind`, `repo_url`, `forge_kind`, `installed_tag` columns to `plugins` |
| `crates/kadr-storage/src/migrations.rs` | Register migration `010_managed_plugins.sql` |
| `crates/kadr-storage/src/repos/plugin_repo.rs` | Persist new `PluginRecord` fields; add `set_base_url` and `update_release` |
| `crates/kadr-server/src/plugins/url_guard.rs` | Expose `pub(crate) async fn ensure_public_host` for forge SSRF checks |
| `crates/kadr-server/src/plugins/forge.rs` (new) | GitHub, GitLab v4, and Gitea v1 release discovery, redirect-checked HTTP, target asset selection, SHA-256 verification |
| `crates/kadr-server/src/plugins/installer.rs` (new) | Safe `.tar.gz` unpacking into staging dir, manifest validation, fresh & atomic-update file commits, file removal |
| `crates/kadr-server/src/plugins/supervisor.rs` (new) | Loopback port allocation, env-isolated `tokio::process::Command` spawning, `/health` readiness polling, crash backoff, `SIGTERM`→`SIGKILL` shutdown |
| `crates/kadr-server/src/plugins/service.rs` | `install_from_git`, `update_managed`, per-plugin lifecycle mutex, supervisor hooks in `approve`/`set_enabled`/`update_settings`/`remove`/`start_managed_plugins`/`shutdown_managed_plugins` |
| `crates/kadr-server/src/plugins/admin_routes.rs` | `POST /api/v1/admin/plugins/install` and `POST /api/v1/admin/plugins/{id}/update` |
| `crates/kadr-server/src/api/mod.rs`, `crates/kadr-server/src/main.rs` | Pass `data_dir/plugins` to `PluginService`; share single `PluginService` in `main.rs` across startup, routes, health monitor, and graceful shutdown |
| `web/src/types/index.ts`, `web/src/api/client.ts` | Managed plugin TS types, `installPlugin`, `updatePlugin` |
| `web/src/components/admin/RegisterPluginForm.tsx`, `PluginList.tsx`, `RemovePluginDialog.tsx`, `PluginsPanel.tsx` | Git install mode toggle, managed badges, one-click Update + re-approval flow, updated purge label |
| `docs/PLUGIN_GUIDE.md`, `examples/plugin-hello/README.md`, `examples/plugin-hello/manifest.json` (new) | Release `.tar.gz` packaging specification and example instructions |

---

### Task 1: Core Managed-Plugin Types, Reserved `"install"` ID & SQLite Migration `010`

**Files:**
- Modify: `crates/kadr-core/src/plugin.rs:57-121`
- Modify: `crates/kadr-core/tests/plugin_types_test.rs:99-110`
- Create: `crates/kadr-storage/src/migrations/010_managed_plugins.sql`
- Modify: `crates/kadr-storage/src/migrations.rs:5-17`
- Modify: `crates/kadr-storage/src/repos/plugin_repo.rs:1-135`
- Modify: `crates/kadr-server/src/plugins/service.rs:252-260`
- Test: `crates/kadr-storage/tests/plugin_repo_test.rs`

**Interfaces:**
- Produces in `kadr_core::plugin`:
  - `pub enum PluginInstallKind { External, Managed }` (`Default = External`, `as_str`, `parse`)
  - `pub enum ForgeKind { Github, Gitlab, Gitea }` (`as_str`, `parse`)
  - `PluginRecord` fields: `pub install_kind: PluginInstallKind`, `pub repo_url: Option<String>`, `pub forge_kind: Option<ForgeKind>`, `pub installed_tag: Option<String>` (all `#[serde(default)]`)
- Produces in `kadr_storage::repos::PluginRepository`:
  - `pub async fn set_base_url(&self, id: &str, base_url: &str) -> Result<bool>`
  - `pub async fn update_release(&self, id: &str, name: &str, manifest: &PluginManifest, installed_tag: &str) -> Result<bool>`

- [ ] **Step 1: Write the failing core & storage tests**

Append to `crates/kadr-core/tests/plugin_types_test.rs`:

```rust
#[test]
fn manifest_id_install_is_reserved() {
    let manifest = PluginManifest {
        id: "install".into(),
        name: "Install".into(),
        version: "0.1.0".into(),
        api_version: 1,
        permissions: vec![],
        capabilities: Default::default(),
    };
    assert!(manifest.validate().unwrap_err().contains("reserved"));
}

#[test]
fn plugin_record_managed_fields_default_for_legacy_json() {
    let rec: PluginRecord = serde_json::from_str(
        r#"{
            "id": "iptv",
            "name": "IPTV",
            "base_url": "http://127.0.0.1:9000",
            "manifest": {"id":"iptv","name":"IPTV","version":"0.1.0","api_version":1},
            "granted_permissions": [],
            "status": "enabled",
            "created_at": 1
        }"#,
    )
    .unwrap();
    assert_eq!(rec.install_kind, PluginInstallKind::External);
    assert_eq!(rec.repo_url, None);
    assert_eq!(rec.forge_kind, None);
    assert_eq!(rec.installed_tag, None);
}
```

Update `record(id: &str)` in `crates/kadr-storage/tests/plugin_repo_test.rs` and add managed-plugin persistence + migration tests:

```rust
// Update imports at top of crates/kadr-storage/tests/plugin_repo_test.rs:
use kadr_core::plugin::{
    ForgeKind, PluginCapabilities, PluginInstallKind, PluginManifest, PluginPermission,
    PluginRecord, PluginStatus,
};

// Update record(id: &str) to include the new fields:
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
        install_kind: PluginInstallKind::External,
        repo_url: None,
        forge_kind: None,
        installed_tag: None,
    }
}

// Append at bottom of crates/kadr-storage/tests/plugin_repo_test.rs:
#[tokio::test]
async fn managed_plugin_metadata_base_url_and_release_updates_roundtrip() {
    let (repo, _, _) = setup().await;
    let mut rec = record("hello");
    rec.install_kind = PluginInstallKind::Managed;
    rec.repo_url = Some("https://github.com/acme/plugin-hello".to_string());
    rec.forge_kind = Some(ForgeKind::Github);
    rec.installed_tag = Some("v0.1.0".to_string());
    repo.insert(&rec).await.unwrap();

    let got = repo.get("hello").await.unwrap().unwrap();
    assert_eq!(got, rec);

    assert!(repo
        .set_base_url("hello", "http://127.0.0.1:45123")
        .await
        .unwrap());
    let mut new_manifest = got.manifest.clone();
    new_manifest.name = "Hello v2".to_string();
    new_manifest.version = "0.2.0".to_string();
    assert!(repo
        .update_release("hello", "Hello v2", &new_manifest, "v0.2.0")
        .await
        .unwrap());

    let updated = repo.get("hello").await.unwrap().unwrap();
    assert_eq!(updated.base_url, "http://127.0.0.1:45123");
    assert_eq!(updated.name, "Hello v2");
    assert_eq!(updated.manifest.version, "0.2.0");
    assert_eq!(updated.installed_tag.as_deref(), Some("v0.2.0"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kadr-core --test plugin_types_test && cargo test -p kadr-storage --test plugin_repo_test`
Expected: FAIL to compile (`PluginInstallKind`, `ForgeKind`, `set_base_url`, `update_release` missing).

- [ ] **Step 3: Implement core types, migration `010`, and repository methods**

In `crates/kadr-core/src/plugin.rs`, reserve `"install"` in `PluginManifest::validate()` and add `PluginInstallKind`, `ForgeKind`, and the new `PluginRecord` fields:

```rust
        if matches!(self.id.as_str(), "settings" | "install") {
            return Err(format!("manifest id '{}' is reserved", self.id));
        }
```

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginInstallKind {
    #[default]
    External,
    Managed,
}

impl PluginInstallKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::External => "external",
            Self::Managed => "managed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "external" => Some(Self::External),
            "managed" => Some(Self::Managed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeKind {
    Github,
    Gitlab,
    Gitea,
}

impl ForgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Gitea => "gitea",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "github" => Some(Self::Github),
            "gitlab" => Some(Self::Gitlab),
            "gitea" => Some(Self::Gitea),
            _ => None,
        }
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
    #[serde(default)]
    pub install_kind: PluginInstallKind,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub forge_kind: Option<ForgeKind>,
    #[serde(default)]
    pub installed_tag: Option<String>,
}
```

Create `crates/kadr-storage/src/migrations/010_managed_plugins.sql`:

```sql
ALTER TABLE plugins ADD COLUMN install_kind TEXT NOT NULL DEFAULT 'external';
ALTER TABLE plugins ADD COLUMN repo_url TEXT DEFAULT NULL;
ALTER TABLE plugins ADD COLUMN forge_kind TEXT DEFAULT NULL;
ALTER TABLE plugins ADD COLUMN installed_tag TEXT DEFAULT NULL;
```

Register `010_managed_plugins.sql` in `crates/kadr-storage/src/migrations.rs`:

```rust
        M::up(include_str!("migrations/008_settings.sql")),
        M::up(include_str!("migrations/009_playlists.sql")),
        M::up(include_str!("migrations/010_managed_plugins.sql")),
```

Update `crates/kadr-storage/src/repos/plugin_repo.rs`:

```rust
use crate::error::Result;
use deadpool_sqlite::Pool;
use kadr_core::plugin::{
    ForgeKind, PluginInstallKind, PluginManifest, PluginPermission, PluginRecord, PluginStatus,
};
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
    let install_kind_s: String = row.get(7)?;
    let forge_kind_s: Option<String> = row.get(9)?;
    let manifest: PluginManifest =
        serde_json::from_str(&manifest_s).map_err(|e| conversion_err(3, e))?;
    let granted_permissions: Vec<PluginPermission> =
        serde_json::from_str(&perms_s).map_err(|e| conversion_err(4, e))?;
    let status = PluginStatus::parse(&status_s).unwrap_or(PluginStatus::Disabled);
    let install_kind =
        PluginInstallKind::parse(&install_kind_s).unwrap_or(PluginInstallKind::External);
    let forge_kind = forge_kind_s.as_deref().and_then(ForgeKind::parse);
    Ok(PluginRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        manifest,
        granted_permissions,
        status,
        created_at: row.get(6)?,
        install_kind,
        repo_url: row.get(8)?,
        forge_kind,
        installed_tag: row.get(10)?,
    })
}

const COLUMNS: &str = "id, name, base_url, manifest, granted_permissions, status, created_at, install_kind, repo_url, forge_kind, installed_tag";
```

Update `insert` in `crates/kadr-storage/src/repos/plugin_repo.rs` to write all 11 columns, and add `set_base_url` and `update_release`:

```rust
    pub async fn insert(&self, rec: &PluginRecord) -> Result<()> {
        let rec = rec.clone();
        let manifest = serde_json::to_string(&rec.manifest)?;
        let perms = serde_json::to_string(&rec.granted_permissions)?;
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO plugins (id, name, base_url, manifest, granted_permissions, status, created_at, install_kind, repo_url, forge_kind, installed_tag)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    rec.id,
                    rec.name,
                    rec.base_url,
                    manifest,
                    perms,
                    rec.status.as_str(),
                    rec.created_at,
                    rec.install_kind.as_str(),
                    rec.repo_url,
                    rec.forge_kind.map(|f| f.as_str()),
                    rec.installed_tag,
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn set_base_url(&self, id: &str, base_url: &str) -> Result<bool> {
        let id = id.to_string();
        let base_url = base_url.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute(
                "UPDATE plugins SET base_url = ?1 WHERE id = ?2",
                params![base_url, id],
            )?;
            Ok(n > 0)
        })
        .await?
    }

    pub async fn update_release(
        &self,
        id: &str,
        name: &str,
        manifest: &PluginManifest,
        installed_tag: &str,
    ) -> Result<bool> {
        let id = id.to_string();
        let name = name.to_string();
        let manifest = serde_json::to_string(manifest)?;
        let installed_tag = installed_tag.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute(
                "UPDATE plugins SET name = ?1, manifest = ?2, installed_tag = ?3 WHERE id = ?4",
                params![name, manifest, installed_tag, id],
            )?;
            Ok(n > 0)
        })
        .await?
    }
```

In `crates/kadr-server/src/plugins/service.rs` (`register`), initialize the four new fields on `PluginRecord`:

```rust
        let record = PluginRecord {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            base_url,
            manifest,
            granted_permissions: vec![],
            status: PluginStatus::Pending,
            created_at: now_secs(),
            install_kind: kadr_core::plugin::PluginInstallKind::External,
            repo_url: None,
            forge_kind: None,
            installed_tag: None,
        };
```

- [ ] **Step 4: Run workspace tests to verify everything passes**

Run: `cargo test --workspace`
Expected: PASS across all crates.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-core/src/plugin.rs crates/kadr-core/tests/plugin_types_test.rs \
  crates/kadr-storage/src/migrations/010_managed_plugins.sql crates/kadr-storage/src/migrations.rs \
  crates/kadr-storage/src/repos/plugin_repo.rs crates/kadr-storage/tests/plugin_repo_test.rs \
  crates/kadr-server/src/plugins/service.rs
git commit -m "feat(plugins): add managed plugin metadata types and migration 010"
```

---

### Task 2: Forge Release Resolver (`plugins/forge.rs`) & SSRF Redirect Guard

**Files:**
- Modify: `crates/kadr-server/src/plugins/url_guard.rs:35-65`
- Modify: `crates/kadr-server/src/plugins/mod.rs:1-16`
- Create: `crates/kadr-server/src/plugins/forge.rs`
- Test: `crates/kadr-server/tests/plugin_forge_test.rs`

**Interfaces:**
- Consumes: `kadr_core::plugin::ForgeKind`, `url_guard::ensure_public_host(&reqwest::Url) -> Result<(), String>`.
- Produces in `kadr_server::plugins::forge`:
  - `pub const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;`
  - `pub const MAX_CHECKSUM_BYTES: usize = 64 * 1024;`
  - `pub enum ForgeError { InvalidRequest(String), ChecksumMismatch(String), ForgeError(String), Timeout }`
  - `pub struct ParsedRepo { pub canonical_url: String, pub origin: String, pub host: String, pub segments: Vec<String> }`
  - `pub struct ReleaseAsset { pub name: String, pub download_url: String }`
  - `pub struct DownloadedBundle { pub forge_kind: ForgeKind, pub canonical_repo_url: String, pub tag_name: String, pub asset_name: String, pub archive_bytes: Vec<u8> }`
  - `pub fn parse_repo_url(raw: &str, allow_private: bool) -> Result<ParsedRepo, ForgeError>`
  - `pub fn select_target_asset<'a>(assets: &'a [ReleaseAsset], os: &str, arch: &str) -> Result<&'a ReleaseAsset, ForgeError>`
  - `pub fn verify_checksum(archive_name: &str, checksum_asset_name: &str, checksum_text: &str, actual_sha256_hex: &str) -> Result<(), ForgeError>`
  - `pub struct ForgeClient` with `new(timeout: Duration) -> Self`, `with_github_api_base(self, base: &str) -> Self` (for tests), and `fetch_release_bundle(&self, repo_url: &str, tag: Option<&str>, forge_hint: Option<ForgeKind>, allow_private: bool) -> Result<DownloadedBundle, ForgeError>`

- [ ] **Step 1: Write the failing forge tests**

Create `crates/kadr-server/tests/plugin_forge_test.rs`:

```rust
use axum::{
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::get,
    Json, Router,
};
use kadr_core::plugin::ForgeKind;
use kadr_server::plugins::forge::{
    parse_repo_url, select_target_asset, verify_checksum, ForgeClient, ForgeError, ReleaseAsset,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Duration;

fn asset(name: &str) -> ReleaseAsset {
    ReleaseAsset {
        name: name.to_string(),
        download_url: format!("https://example.com/{name}"),
    }
}

#[test]
fn repo_url_parsing_strips_git_and_enforces_scheme() {
    let r = parse_repo_url("https://github.com/acme/plugin-iptv.git/", false).unwrap();
    assert_eq!(r.canonical_url, "https://github.com/acme/plugin-iptv");
    assert_eq!(r.segments, vec!["acme", "plugin-iptv"]);

    let gl = parse_repo_url("https://gitlab.com/group/sub/plugin-iptv", false).unwrap();
    assert_eq!(gl.segments, vec!["group", "sub", "plugin-iptv"]);

    assert!(parse_repo_url("http://github.com/acme/repo", false).is_err());
    assert!(parse_repo_url("http://127.0.0.1:8080/acme/repo", true).is_ok());
    assert!(parse_repo_url("https://github.com/only-owner", false).is_err());
    assert!(parse_repo_url("https://github.com/acme/repo?foo=1", false).is_err());
}

#[test]
fn target_asset_prefers_canonical_then_aliases_and_rejects_ambiguous() {
    let assets = vec![
        asset("hello-linux-amd64.tar.gz"),
        asset("hello-linux-x86_64.tar.gz"),
        asset("hello-darwin-arm64.tar.gz"),
    ];
    let exact = select_target_asset(&assets, "linux", "x86_64").unwrap();
    assert_eq!(exact.name, "hello-linux-x86_64.tar.gz");

    let alias = select_target_asset(&assets, "macos", "aarch64").unwrap();
    assert_eq!(alias.name, "hello-darwin-arm64.tar.gz");

    let missing = select_target_asset(&assets, "linux", "riscv64").unwrap_err();
    assert!(matches!(missing, ForgeError::InvalidRequest(msg) if msg.contains("linux-riscv64")));

    let dup = vec![asset("a-linux-x86_64.tar.gz"), asset("b-linux-x86_64.tar.gz")];
    assert!(matches!(
        select_target_asset(&dup, "linux", "x86_64").unwrap_err(),
        ForgeError::InvalidRequest(_)
    ));
}

#[test]
fn checksum_verification_handles_single_and_multi_file_formats() {
    let digest = "a".repeat(64);
    verify_checksum(
        "p-linux-x86_64.tar.gz",
        "p-linux-x86_64.tar.gz.sha256",
        &format!("{digest}\n"),
        &digest,
    )
    .unwrap();

    let sums = format!(
        "{}  other.tar.gz\n{} *p-linux-x86_64.tar.gz\n",
        "b".repeat(64),
        digest
    );
    verify_checksum("p-linux-x86_64.tar.gz", "checksums.txt", &sums, &digest).unwrap();

    assert!(matches!(
        verify_checksum("p-linux-x86_64.tar.gz", "checksums.txt", &sums, &"c".repeat(64)),
        Err(ForgeError::ChecksumMismatch(_))
    ));
    assert!(matches!(
        verify_checksum("missing.tar.gz", "checksums.txt", &sums, &digest),
        Err(ForgeError::ChecksumMismatch(_))
    ));
}

#[tokio::test]
async fn forge_client_fetches_github_gitea_and_gitlab_subgroup_with_redirects_and_checksums() {
    let payload = b"fake-tar-gz-bytes".to_vec();
    let hex_hash = format!("{:x}", Sha256::digest(&payload));
    let target_name = format!(
        "hello-{}-{}.tar.gz",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let b_gh = base.clone();
    let t_gh = target_name.clone();
    let b_gt = base.clone();
    let t_gt = target_name.clone();
    let b_gl = base.clone();
    let t_gl = target_name.clone();
    let sums_body = format!("{hex_hash}  {target_name}\n");
    let archive_body = payload.clone();

    let app = Router::new()
        // GitHub latest release (checks User-Agent is present)
        .route(
            "/repos/acme/hello/releases/latest",
            get(move |headers: HeaderMap| {
                let b = b_gh.clone();
                let t = t_gh.clone();
                async move {
                    if !headers.contains_key(header::USER_AGENT) {
                        return StatusCode::FORBIDDEN.into_response();
                    }
                    Json(json!({
                        "tag_name": "v1.0.0",
                        "assets": [
                            { "name": t, "browser_download_url": format!("{b}/cdn/redirect") },
                            { "name": "checksums.txt", "browser_download_url": format!("{b}/files/checksums.txt") }
                        ]
                    }))
                    .into_response()
                }
            }),
        )
        // Gitea v1 release (only /gitea-owner/hello exists; /group/hello returns 404 to test fallback)
        .route(
            "/api/v1/repos/{owner}/{repo}/releases/latest",
            get(move |Path((owner, _repo)): Path<(String, String)>| {
                let b = b_gt.clone();
                let t = t_gt.clone();
                async move {
                    if owner != "gitea-owner" {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    Json(json!({
                        "tag_name": "v2.0.0",
                        "assets": [
                            { "name": t, "browser_download_url": format!("{b}/files/archive.tar.gz") }
                        ]
                    }))
                    .into_response()
                }
            }),
        )
        // GitLab v4 permalink redirect -> tagged release (supports %2F encoded path)
        .route(
            "/api/v4/projects/{project}/releases/permalink/latest",
            get({
                let b = base.clone();
                move |Path(project): Path<String>| {
                    let b = b.clone();
                    async move {
                        let enc = urlencoding::encode(&project);
                        Redirect::temporary(&format!("{b}/api/v4/projects/{enc}/releases/v3.0.0"))
                    }
                }
            }),
        )
        .route(
            "/api/v4/projects/{project}/releases/{tag}",
            get(move |Path((_project, tag)): Path<(String, String)>| {
                let b = b_gl.clone();
                let t = t_gl.clone();
                async move {
                    Json(json!({
                        "tag_name": tag,
                        "assets": {
                            "links": [
                                { "name": t, "direct_asset_url": format!("{b}/files/archive.tar.gz") }
                            ]
                        }
                    }))
                }
            }),
        )
        .route(
            "/cdn/redirect",
            get({
                let b = base.clone();
                move || {
                    let b = b.clone();
                    async move { Redirect::temporary(&format!("{b}/files/archive.tar.gz")) }
                }
            }),
        )
        .route(
            "/files/archive.tar.gz",
            get(move || {
                let body = archive_body.clone();
                async move { body }
            }),
        )
        .route(
            "/files/checksums.txt",
            get(move || {
                let s = sums_body.clone();
                async move { s }
            }),
        );

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = ForgeClient::new(Duration::from_secs(5)).with_github_api_base(&base);

    // 1. GitHub (via hint + overridden API base) with CDN redirect + checksum verification
    let gh = client
        .fetch_release_bundle(
            &format!("{base}/acme/hello"),
            None,
            Some(ForgeKind::Github),
            true,
        )
        .await
        .unwrap();
    assert_eq!(gh.forge_kind, ForgeKind::Github);
    assert_eq!(gh.tag_name, "v1.0.0");
    assert_eq!(gh.archive_bytes, payload);

    // 2. Self-hosted Gitea auto-detected on 2-segment path
    let gt = client
        .fetch_release_bundle(&format!("{base}/gitea-owner/hello"), None, None, true)
        .await
        .unwrap();
    assert_eq!(gt.forge_kind, ForgeKind::Gitea);
    assert_eq!(gt.tag_name, "v2.0.0");

    // 3. Self-hosted GitLab auto-detected via 404 fallback on 2-segment path
    let gl2 = client
        .fetch_release_bundle(&format!("{base}/group/hello"), None, None, true)
        .await
        .unwrap();
    assert_eq!(gl2.forge_kind, ForgeKind::Gitlab);
    assert_eq!(gl2.tag_name, "v3.0.0");

    // 4. Self-hosted GitLab nested subgroup (>2 segments goes straight to GitLab v4)
    let gl3 = client
        .fetch_release_bundle(&format!("{base}/group/sub/hello"), None, None, true)
        .await
        .unwrap();
    assert_eq!(gl3.forge_kind, ForgeKind::Gitlab);
    assert_eq!(gl3.tag_name, "v3.0.0");

    // 5. SSRF block when allow_private = false
    let blocked = client
        .fetch_release_bundle(&format!("{base}/gitea-owner/hello"), None, None, false)
        .await
        .unwrap_err();
    assert!(matches!(blocked, ForgeError::InvalidRequest(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test plugin_forge_test`
Expected: FAIL to compile (`kadr_server::plugins::forge` missing).

- [ ] **Step 3: Expose `ensure_public_host` and implement `plugins/forge.rs`**

In `crates/kadr-server/src/plugins/url_guard.rs`, change line 35 from `async fn ensure_public_host` to:

```rust
pub(crate) async fn ensure_public_host(url: &reqwest::Url) -> Result<(), String> {
```

Export `pub mod forge;` in `crates/kadr-server/src/plugins/mod.rs`.

Create `crates/kadr-server/src/plugins/forge.rs`:

```rust
use crate::plugins::url_guard::ensure_public_host;
use axum::http::header;
use kadr_core::plugin::ForgeKind;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_CHECKSUM_BYTES: usize = 64 * 1024;
const MAX_REDIRECTS: usize = 5;
const USER_AGENT_VALUE: &str = concat!("kadr/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, thiserror::Error)]
pub enum ForgeError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    ChecksumMismatch(String),
    #[error("{0}")]
    ForgeError(String),
    #[error("Forge request timed out")]
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRepo {
    pub canonical_url: String,
    pub origin: String,
    pub host: String,
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone)]
pub struct DownloadedBundle {
    pub forge_kind: ForgeKind,
    pub canonical_repo_url: String,
    pub tag_name: String,
    pub asset_name: String,
    pub archive_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ResolvedRelease {
    forge_kind: ForgeKind,
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

pub fn parse_repo_url(raw: &str, allow_private: bool) -> Result<ParsedRepo, ForgeError> {
    let trimmed = raw.trim();
    let url = reqwest::Url::parse(trimmed)
        .map_err(|e| ForgeError::InvalidRequest(format!("invalid repo_url: {e}")))?;
    let scheme = url.scheme();
    if scheme != "https" && !(allow_private && scheme == "http") {
        return Err(ForgeError::InvalidRequest(
            "repo_url must use https:// (or http:// when allow_private_proxy is enabled)"
                .to_string(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ForgeError::InvalidRequest(
            "repo_url must not include query parameters or fragments".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ForgeError::InvalidRequest("repo_url has no host".to_string()))?
        .to_ascii_lowercase();
    let mut segments: Vec<String> = url
        .path_segments()
        .map(|it| {
            it.filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    if let Some(last) = segments.last_mut() {
        if let Some(stripped) = last.strip_suffix(".git") {
            *last = stripped.to_string();
        }
    }
    if segments.len() < 2 || segments.iter().any(|s| s.is_empty() || s == "." || s == "..") {
        return Err(ForgeError::InvalidRequest(
            "repo_url must include at least owner and repository path segments".to_string(),
        ));
    }
    let origin = match url.port() {
        Some(port) => format!("{}://{}:{}", scheme, host, port),
        None => format!("{}://{}", scheme, host),
    };
    let canonical_url = format!("{origin}/{}", segments.join("/"));
    Ok(ParsedRepo {
        canonical_url,
        origin,
        host,
        segments,
    })
}

fn os_aliases(os: &str) -> Vec<&str> {
    match os {
        "macos" => vec!["darwin"],
        _ => vec![],
    }
}

fn arch_aliases(arch: &str) -> Vec<&str> {
    match arch {
        "x86_64" => vec!["amd64"],
        "aarch64" => vec!["arm64"],
        _ => vec![],
    }
}

pub fn select_target_asset<'a>(
    assets: &'a [ReleaseAsset],
    os: &str,
    arch: &str,
) -> Result<&'a ReleaseAsset, ForgeError> {
    let canonical_suffix = format!("-{os}-{arch}.tar.gz");
    let exact: Vec<&ReleaseAsset> = assets
        .iter()
        .filter(|a| a.name.to_ascii_lowercase().ends_with(&canonical_suffix))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    if exact.len() > 1 {
        let names: Vec<&str> = exact.iter().map(|a| a.name.as_str()).collect();
        return Err(ForgeError::InvalidRequest(format!(
            "multiple release assets match '{canonical_suffix}': {}",
            names.join(", ")
        )));
    }

    let mut alias_suffixes = Vec::new();
    let mut os_list = vec![os];
    os_list.extend(os_aliases(os));
    let mut arch_list = vec![arch];
    arch_list.extend(arch_aliases(arch));
    for o in &os_list {
        for a in &arch_list {
            if *o == os && *a == arch {
                continue;
            }
            alias_suffixes.push(format!("-{o}-{a}.tar.gz"));
        }
    }

    let aliased: Vec<&ReleaseAsset> = assets
        .iter()
        .filter(|asset| {
            let lower = asset.name.to_ascii_lowercase();
            alias_suffixes.iter().any(|s| lower.ends_with(s))
        })
        .collect();
    if aliased.len() == 1 {
        return Ok(aliased[0]);
    }
    if aliased.len() > 1 {
        let names: Vec<&str> = aliased.iter().map(|a| a.name.as_str()).collect();
        return Err(ForgeError::InvalidRequest(format!(
            "multiple release assets match target '{os}-{arch}': {}",
            names.join(", ")
        )));
    }

    Err(ForgeError::InvalidRequest(format!(
        "no release asset matching '*{canonical_suffix}' found for host target {os}-{arch}"
    )))
}

fn find_checksum_asset<'a>(assets: &'a [ReleaseAsset], archive_name: &str) -> Option<&'a ReleaseAsset> {
    let exact_sha = format!("{archive_name}.sha256");
    if let Some(a) = assets.iter().find(|a| a.name == exact_sha) {
        return Some(a);
    }
    for name in ["checksums.txt", "SHA256SUMS"] {
        if let Some(a) = assets.iter().find(|a| a.name == name) {
            return Some(a);
        }
    }
    None
}

pub fn verify_checksum(
    archive_name: &str,
    checksum_asset_name: &str,
    checksum_text: &str,
    actual_sha256_hex: &str,
) -> Result<(), ForgeError> {
    let single_file_sha = checksum_asset_name.ends_with(".sha256");
    let mut expected_hex: Option<String> = None;

    for raw_line in checksum_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        match parts.next() {
            None if single_file_sha => {
                expected_hex = Some(hash.to_ascii_lowercase());
                break;
            }
            Some(file_token) => {
                let fname = file_token.trim_start_matches('*');
                let basename = fname.rsplit('/').next().unwrap_or(fname);
                if basename == archive_name {
                    expected_hex = Some(hash.to_ascii_lowercase());
                    break;
                }
            }
            None => {}
        }
    }

    let Some(expected) = expected_hex else {
        return Err(ForgeError::ChecksumMismatch(format!(
            "checksum asset '{checksum_asset_name}' does not contain an entry for '{archive_name}'"
        )));
    };
    if expected != actual_sha256_hex.to_ascii_lowercase() {
        return Err(ForgeError::ChecksumMismatch(format!(
            "SHA-256 mismatch for '{archive_name}' (expected {expected}, got {actual_sha256_hex})"
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct GithubOrGiteaRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubOrGiteaAsset>,
}

#[derive(Deserialize)]
struct GithubOrGiteaAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GitlabRelease {
    tag_name: String,
    #[serde(default)]
    assets: GitlabAssets,
}

#[derive(Default, Deserialize)]
struct GitlabAssets {
    #[serde(default)]
    links: Vec<GitlabAssetLink>,
}

#[derive(Deserialize)]
struct GitlabAssetLink {
    name: String,
    direct_asset_url: Option<String>,
    url: String,
}

#[derive(Clone)]
pub struct ForgeClient {
    http: reqwest::Client,
    timeout_ms: Arc<AtomicU64>,
    github_api_base: String,
}

impl ForgeClient {
    pub fn new(timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client builds with static configuration");
        Self {
            http,
            timeout_ms: Arc::new(AtomicU64::new(timeout.as_millis().max(1000) as u64)),
            github_api_base: "https://api.github.com".to_string(),
        }
    }

    pub fn with_github_api_base(mut self, base: &str) -> Self {
        self.github_api_base = base.trim_end_matches('/').to_string();
        self
    }

    pub fn set_timeout(&self, timeout: Duration) {
        self.timeout_ms
            .store(timeout.as_millis().max(1000) as u64, Ordering::Relaxed);
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.load(Ordering::Relaxed))
    }

    async fn validate_hop(&self, url: &reqwest::Url, allow_private: bool) -> Result<(), ForgeError> {
        let scheme = url.scheme();
        if scheme != "https" && !(allow_private && scheme == "http") {
            return Err(ForgeError::InvalidRequest(format!(
                "disallowed URL scheme '{scheme}'"
            )));
        }
        if !allow_private {
            ensure_public_host(url)
                .await
                .map_err(ForgeError::InvalidRequest)?;
        }
        Ok(())
    }

    async fn get_following_redirects(
        &self,
        raw_url: &str,
        accept: Option<&str>,
        timeout: Duration,
        allow_private: bool,
    ) -> Result<reqwest::Response, ForgeError> {
        let mut current = reqwest::Url::parse(raw_url)
            .map_err(|e| ForgeError::InvalidRequest(format!("invalid url '{raw_url}': {e}")))?;

        for _hop in 0..=MAX_REDIRECTS {
            self.validate_hop(&current, allow_private).await?;
            let mut req = self
                .http
                .get(current.clone())
                .header(header::USER_AGENT, USER_AGENT_VALUE)
                .timeout(timeout);
            if let Some(a) = accept {
                req = req.header(header::ACCEPT, a);
            }
            let resp = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    ForgeError::Timeout
                } else {
                    ForgeError::ForgeError(format!("forge request failed: {e}"))
                }
            })?;
            if resp.status().is_redirection() {
                let loc = resp
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        ForgeError::ForgeError("redirect missing Location header".to_string())
                    })?;
                current = current.join(loc).map_err(|e| {
                    ForgeError::ForgeError(format!("invalid redirect Location '{loc}': {e}"))
                })?;
                continue;
            }
            return Ok(resp);
        }
        Err(ForgeError::ForgeError(
            "too many redirects from forge".to_string(),
        ))
    }

    async fn download_bounded(
        &self,
        raw_url: &str,
        max_bytes: usize,
        allow_private: bool,
    ) -> Result<Vec<u8>, ForgeError> {
        let download_timeout = self.timeout().max(Duration::from_secs(60));
        let mut resp = self
            .get_following_redirects(raw_url, None, download_timeout, allow_private)
            .await?;
        let status = resp.status();
        if status.is_server_error() {
            return Err(ForgeError::ForgeError(format!(
                "asset download returned {status}"
            )));
        }
        if !status.is_success() {
            return Err(ForgeError::InvalidRequest(format!(
                "asset download returned {status}"
            )));
        }
        if let Some(len) = resp.content_length() {
            if len > max_bytes as u64 {
                return Err(ForgeError::InvalidRequest(format!(
                    "release asset exceeds maximum size of {max_bytes} bytes"
                )));
            }
        }
        let mut buf = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| {
            if e.is_timeout() {
                ForgeError::Timeout
            } else {
                ForgeError::ForgeError(format!("error reading asset body: {e}"))
            }
        })? {
            if buf.len() + chunk.len() > max_bytes {
                return Err(ForgeError::InvalidRequest(format!(
                    "release asset exceeds maximum size of {max_bytes} bytes"
                )));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }

    async fn fetch_github_or_gitea(
        &self,
        forge_kind: ForgeKind,
        api_url: &str,
        accept: Option<&str>,
        allow_private: bool,
    ) -> Result<Option<ResolvedRelease>, ForgeError> {
        let resp = self
            .get_following_redirects(api_url, accept, self.timeout(), allow_private)
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_server_error() {
            return Err(ForgeError::ForgeError(format!(
                "{} API returned {status}",
                forge_kind.as_str()
            )));
        }
        if !status.is_success() {
            return Err(ForgeError::InvalidRequest(format!(
                "{} API returned {status}",
                forge_kind.as_str()
            )));
        }
        let parsed: GithubOrGiteaRelease = resp
            .json()
            .await
            .map_err(|e| ForgeError::ForgeError(format!("invalid release JSON: {e}")))?;
        Ok(Some(ResolvedRelease {
            forge_kind,
            tag_name: parsed.tag_name,
            assets: parsed
                .assets
                .into_iter()
                .map(|a| ReleaseAsset {
                    name: a.name,
                    download_url: a.browser_download_url,
                })
                .collect(),
        }))
    }

    async fn fetch_gitlab(
        &self,
        repo: &ParsedRepo,
        tag: Option<&str>,
        allow_private: bool,
    ) -> Result<Option<ResolvedRelease>, ForgeError> {
        let project = urlencoding::encode(&repo.segments.join("/"));
        let api_url = match tag {
            Some(t) => format!(
                "{}/api/v4/projects/{project}/releases/{}",
                repo.origin,
                urlencoding::encode(t)
            ),
            None => format!("{}/api/v4/projects/{project}/releases/permalink/latest", repo.origin),
        };
        let resp = self
            .get_following_redirects(&api_url, Some("application/json"), self.timeout(), allow_private)
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_server_error() {
            return Err(ForgeError::ForgeError(format!(
                "gitlab API returned {status}"
            )));
        }
        if !status.is_success() {
            return Err(ForgeError::InvalidRequest(format!(
                "gitlab API returned {status}"
            )));
        }
        let parsed: GitlabRelease = resp
            .json()
            .await
            .map_err(|e| ForgeError::ForgeError(format!("invalid gitlab release JSON: {e}")))?;
        Ok(Some(ResolvedRelease {
            forge_kind: ForgeKind::Gitlab,
            tag_name: parsed.tag_name,
            assets: parsed
                .assets
                .links
                .into_iter()
                .map(|l| ReleaseAsset {
                    name: l.name,
                    download_url: l.direct_asset_url.unwrap_or(l.url),
                })
                .collect(),
        }))
    }

    async fn resolve_release(
        &self,
        repo: &ParsedRepo,
        tag: Option<&str>,
        forge_hint: Option<ForgeKind>,
        allow_private: bool,
    ) -> Result<ResolvedRelease, ForgeError> {
        let detected = forge_hint.or(match repo.host.as_str() {
            "github.com" => Some(ForgeKind::Github),
            "gitlab.com" => Some(ForgeKind::Gitlab),
            "codeberg.org" => Some(ForgeKind::Gitea),
            _ if repo.segments.len() > 2 => Some(ForgeKind::Gitlab),
            _ => None,
        });

        let not_found = || {
            ForgeError::InvalidRequest(match tag {
                Some(t) => format!("release '{t}' not found at {}", repo.canonical_url),
                None => format!("no published release found at {}", repo.canonical_url),
            })
        };

        match detected {
            Some(ForgeKind::Github) => {
                if repo.segments.len() != 2 {
                    return Err(ForgeError::InvalidRequest(
                        "GitHub repository URL must have exact <owner>/<repo> path".to_string(),
                    ));
                }
                let (owner, name) = (&repo.segments[0], &repo.segments[1]);
                let url = match tag {
                    Some(t) => format!(
                        "{}/repos/{owner}/{name}/releases/tags/{}",
                        self.github_api_base,
                        urlencoding::encode(t)
                    ),
                    None => format!("{}/repos/{owner}/{name}/releases/latest", self.github_api_base),
                };
                self.fetch_github_or_gitea(
                    ForgeKind::Github,
                    &url,
                    Some("application/vnd.github+json"),
                    allow_private,
                )
                .await?
                .ok_or_else(not_found)
            }
            Some(ForgeKind::Gitea) => {
                if repo.segments.len() != 2 {
                    return Err(ForgeError::InvalidRequest(
                        "Gitea repository URL must have exact <owner>/<repo> path".to_string(),
                    ));
                }
                let (owner, name) = (&repo.segments[0], &repo.segments[1]);
                let url = match tag {
                    Some(t) => format!(
                        "{}/api/v1/repos/{owner}/{name}/releases/tags/{}",
                        repo.origin,
                        urlencoding::encode(t)
                    ),
                    None => format!("{}/api/v1/repos/{owner}/{name}/releases/latest", repo.origin),
                };
                self.fetch_github_or_gitea(
                    ForgeKind::Gitea,
                    &url,
                    Some("application/json"),
                    allow_private,
                )
                .await?
                .ok_or_else(not_found)
            }
            Some(ForgeKind::Gitlab) => self
                .fetch_gitlab(repo, tag, allow_private)
                .await?
                .ok_or_else(not_found),
            None => {
                let (owner, name) = (&repo.segments[0], &repo.segments[1]);
                let gitea_url = match tag {
                    Some(t) => format!(
                        "{}/api/v1/repos/{owner}/{name}/releases/tags/{}",
                        repo.origin,
                        urlencoding::encode(t)
                    ),
                    None => format!("{}/api/v1/repos/{owner}/{name}/releases/latest", repo.origin),
                };
                if let Some(rel) = self
                    .fetch_github_or_gitea(
                        ForgeKind::Gitea,
                        &gitea_url,
                        Some("application/json"),
                        allow_private,
                    )
                    .await?
                {
                    return Ok(rel);
                }
                self.fetch_gitlab(repo, tag, allow_private)
                    .await?
                    .ok_or_else(not_found)
            }
        }
    }

    pub async fn fetch_release_bundle(
        &self,
        repo_url: &str,
        tag: Option<&str>,
        forge_hint: Option<ForgeKind>,
        allow_private: bool,
    ) -> Result<DownloadedBundle, ForgeError> {
        let parsed = parse_repo_url(repo_url, allow_private)?;
        let tag_clean = tag.map(str::trim).filter(|t| !t.is_empty());
        let release = self
            .resolve_release(&parsed, tag_clean, forge_hint, allow_private)
            .await?;

        let asset = select_target_asset(
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
        )?;
        let archive_bytes = self
            .download_bounded(&asset.download_url, MAX_ARCHIVE_BYTES, allow_private)
            .await?;

        if let Some(cs_asset) = find_checksum_asset(&release.assets, &asset.name) {
            let cs_bytes = self
                .download_bounded(&cs_asset.download_url, MAX_CHECKSUM_BYTES, allow_private)
                .await?;
            let cs_text = String::from_utf8(cs_bytes).map_err(|_| {
                ForgeError::ChecksumMismatch("checksum asset is not valid UTF-8".to_string())
            })?;
            let actual_hex = format!("{:x}", Sha256::digest(&archive_bytes));
            verify_checksum(&asset.name, &cs_asset.name, &cs_text, &actual_hex)?;
        }

        Ok(DownloadedBundle {
            forge_kind: release.forge_kind,
            canonical_repo_url: parsed.canonical_url,
            tag_name: release.tag_name,
            asset_name: asset.name.clone(),
            archive_bytes,
        })
    }
}
```

- [ ] **Step 4: Run forge tests to verify they pass**

Run: `cargo test -p kadr-server --test plugin_forge_test`
Expected: PASS (all 4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/plugins/url_guard.rs crates/kadr-server/src/plugins/mod.rs \
  crates/kadr-server/src/plugins/forge.rs crates/kadr-server/tests/plugin_forge_test.rs
git commit -m "feat(plugins): add GitHub, GitLab, and Gitea release resolver"
```

---

### Task 3: Safe Pure-Rust `.tar.gz` Bundle Installer (`plugins/installer.rs`)

**Files:**
- Modify: `crates/kadr-server/Cargo.toml:39-65`
- Modify: `crates/kadr-server/src/plugins/mod.rs`
- Create: `crates/kadr-server/src/plugins/installer.rs`
- Test: `crates/kadr-server/tests/plugin_installer_test.rs`

**Interfaces:**
- Consumes: `kadr_core::plugin::PluginManifest`.
- Produces in `kadr_server::plugins::installer`:
  - `pub const MAX_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;`
  - `pub const BINARY_FILENAME: &str` (`"plugin.exe"` on Windows, `"plugin"` otherwise)
  - `pub struct PluginPaths { pub plugin_dir: PathBuf, pub bin_path: PathBuf, pub manifest_path: PathBuf, pub data_dir: PathBuf }`
  - `pub struct StagedBundle` (with `pub fn manifest(&self) -> &PluginManifest`, `pub fn commit_fresh(self, plugins_dir: &Path) -> Result<PluginPaths, String>`, `pub fn commit_update(self, plugins_dir: &Path, expected_id: &str) -> Result<PluginPaths, String>`; RAII `Drop` removes `.staging-<uuid>` if uncommitted)
  - `pub fn plugin_paths(plugins_dir: &Path, plugin_id: &str) -> PluginPaths`
  - `pub fn unpack_bundle(plugins_dir: &Path, archive_bytes: &[u8]) -> Result<StagedBundle, String>`
  - `pub fn remove_plugin_files(plugins_dir: &Path, plugin_id: &str, purge: bool) -> std::io::Result<()>`

- [ ] **Step 1: Add pure-Rust `flate2` + `tar` dependencies and write failing installer tests**

Add to `[dependencies]` in `crates/kadr-server/Cargo.toml`:

```toml
flate2 = { version = "1.0", default-features = false, features = ["rust_backend"] }
tar = "0.4"
```

Create `crates/kadr-server/tests/plugin_installer_test.rs`:

```rust
use flate2::{write::GzEncoder, Compression};
use kadr_server::plugins::installer::{remove_plugin_files, unpack_bundle};
use serde_json::json;

fn make_tar_gz(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut gz);
        for (path, content, kind) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_mode(0o644);
            if *kind == tar::EntryType::Symlink {
                header.set_size(0);
                builder
                    .append_link(&mut header, *path, "/etc/passwd")
                    .unwrap();
            } else {
                header.set_size(content.len() as u64);
                header.set_cksum();
                builder.append_data(&mut header, *path, *content).unwrap();
            }
        }
        builder.finish().unwrap();
    }
    gz.finish().unwrap()
}

fn manifest_bytes(id: &str, version: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "id": id,
        "name": format!("Plugin {id}"),
        "version": version,
        "api_version": 1,
        "permissions": ["items:publish"],
        "capabilities": { "virtual_library": true }
    }))
    .unwrap()
}

#[test]
fn unpacks_root_and_single_prefix_layouts_and_commits_fresh_and_update_preserving_data() {
    let dir = tempfile::tempdir().unwrap();
    let m1 = manifest_bytes("hello", "0.1.0");
    let flat = make_tar_gz(&[
        ("manifest.json", &m1, tar::EntryType::Regular),
        ("plugin", b"#!/bin/sh\necho v1\n", tar::EntryType::Regular),
    ]);

    let staged = unpack_bundle(dir.path(), &flat).unwrap();
    assert_eq!(staged.manifest().id, "hello");
    let paths = staged.commit_fresh(dir.path()).unwrap();
    assert!(paths.bin_path.exists());
    assert!(paths.manifest_path.exists());
    assert!(paths.data_dir.exists());

    // Write plugin runtime state in data_dir — update and non-purge remove must preserve it.
    let state_file = paths.data_dir.join("state.txt");
    std::fs::write(&state_file, b"keep-me").unwrap();

    // Update using a nested top-level directory layout (`hello-0.2.0/...`)
    let m2 = manifest_bytes("hello", "0.2.0");
    let nested = make_tar_gz(&[
        ("hello-0.2.0/manifest.json", &m2, tar::EntryType::Regular),
        (
            "hello-0.2.0/plugin",
            b"#!/bin/sh\necho v2\n",
            tar::EntryType::Regular,
        ),
    ]);
    let staged_v2 = unpack_bundle(dir.path(), &nested).unwrap();
    assert_eq!(staged_v2.manifest().version, "0.2.0");
    let updated_paths = staged_v2.commit_update(dir.path(), "hello").unwrap();
    assert_eq!(
        std::fs::read(&updated_paths.bin_path).unwrap(),
        b"#!/bin/sh\necho v2\n"
    );
    assert_eq!(std::fs::read(&state_file).unwrap(), b"keep-me");

    // Reject manifest id mismatch on update and verify staging directory is cleaned up.
    let wrong_id = make_tar_gz(&[
        ("manifest.json", &manifest_bytes("other", "0.3.0"), tar::EntryType::Regular),
        ("plugin", b"bin", tar::EntryType::Regular),
    ]);
    let staged_bad = unpack_bundle(dir.path(), &wrong_id).unwrap();
    assert!(staged_bad.commit_update(dir.path(), "hello").is_err());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".staging-"))
        .collect();
    assert!(leftovers.is_empty(), "staging directories must be cleaned up");

    // Non-purge remove deletes bin + manifest but preserves data/state.txt
    remove_plugin_files(dir.path(), "hello", false).unwrap();
    assert!(!updated_paths.bin_path.exists());
    assert!(!updated_paths.manifest_path.exists());
    assert!(state_file.exists());

    // Purge remove deletes everything
    remove_plugin_files(dir.path(), "hello", true).unwrap();
    assert!(!updated_paths.plugin_dir.exists());
}

#[test]
fn rejects_symlinks_missing_files_and_invalid_manifests_without_leaving_staging_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let m = manifest_bytes("hello", "0.1.0");

    let with_symlink = make_tar_gz(&[
        ("manifest.json", &m, tar::EntryType::Regular),
        ("plugin", b"bin", tar::EntryType::Regular),
        ("escape", b"", tar::EntryType::Symlink),
    ]);
    assert!(unpack_bundle(dir.path(), &with_symlink).is_err());

    let missing_bin = make_tar_gz(&[("manifest.json", &m, tar::EntryType::Regular)]);
    assert!(unpack_bundle(dir.path(), &missing_bin).is_err());

    let reserved_manifest = serde_json::to_vec(&json!({
        "id": "install",
        "name": "Bad",
        "version": "0.1.0",
        "api_version": 1
    }))
    .unwrap();
    let reserved = make_tar_gz(&[
        ("manifest.json", &reserved_manifest, tar::EntryType::Regular),
        ("plugin", b"bin", tar::EntryType::Regular),
    ]);
    assert!(unpack_bundle(dir.path(), &reserved).is_err());

    let count = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count, 0, "failed unpacks must leave zero staging dirs");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test plugin_installer_test`
Expected: FAIL to compile (`kadr_server::plugins::installer` missing).

- [ ] **Step 3: Implement `plugins/installer.rs`**

Export `pub mod installer;` in `crates/kadr-server/src/plugins/mod.rs` and create `crates/kadr-server/src/plugins/installer.rs`:

```rust
use flate2::read::GzDecoder;
use kadr_core::plugin::PluginManifest;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const MAX_UNCOMPRESSED_BYTES: u64 = 250 * 1024 * 1024;

#[cfg(windows)]
pub const BINARY_FILENAME: &str = "plugin.exe";
#[cfg(not(windows))]
pub const BINARY_FILENAME: &str = "plugin";

#[derive(Debug, Clone)]
pub struct PluginPaths {
    pub plugin_dir: PathBuf,
    pub bin_path: PathBuf,
    pub manifest_path: PathBuf,
    pub data_dir: PathBuf,
}

pub fn plugin_paths(plugins_dir: &Path, plugin_id: &str) -> PluginPaths {
    let plugin_dir = plugins_dir.join(plugin_id);
    PluginPaths {
        bin_path: plugin_dir.join("bin").join(BINARY_FILENAME),
        manifest_path: plugin_dir.join("manifest.json"),
        data_dir: plugin_dir.join("data"),
        plugin_dir,
    }
}

pub struct StagedBundle {
    staging_dir: PathBuf,
    manifest_src: PathBuf,
    bin_src: PathBuf,
    manifest: PluginManifest,
}

impl Drop for StagedBundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.staging_dir);
    }
}

fn validate_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "archive contains unsafe entry path '{}'",
                    path.display()
                ));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err("archive contains empty entry path".to_string());
    }
    Ok(clean)
}

fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to set executable permissions: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn locate_bundle_root(staging_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let root_manifest = staging_dir.join("manifest.json");
    let root_bin = staging_dir.join(BINARY_FILENAME);
    if root_manifest.is_file() && root_bin.is_file() {
        return Ok((root_manifest, root_bin));
    }

    let mut subdirs = Vec::new();
    let entries = std::fs::read_dir(staging_dir)
        .map_err(|e| format!("failed to read staging dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to inspect staging entry: {e}"))?;
        if entry
            .file_type()
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            subdirs.push(entry.path());
        }
    }

    if subdirs.len() == 1 {
        let sub_manifest = subdirs[0].join("manifest.json");
        let sub_bin = subdirs[0].join(BINARY_FILENAME);
        if sub_manifest.is_file() && sub_bin.is_file() {
            return Ok((sub_manifest, sub_bin));
        }
    }

    Err(format!(
        "release archive must contain 'manifest.json' and '{BINARY_FILENAME}' at root or in a single top-level directory"
    ))
}

pub fn unpack_bundle(plugins_dir: &Path, archive_bytes: &[u8]) -> Result<StagedBundle, String> {
    std::fs::create_dir_all(plugins_dir)
        .map_err(|e| format!("failed to create plugins directory: {e}"))?;
    let staging_dir = plugins_dir.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("failed to create staging directory: {e}"))?;

    // Guard ensures staging_dir is removed on any error before StagedBundle is constructed.
    struct DirGuard(Option<PathBuf>);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            if let Some(ref p) = self.0 {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
    let mut guard = DirGuard(Some(staging_dir.clone()));

    let gz = GzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(gz);
    let mut total_unpacked: u64 = 0;

    let entries = archive
        .entries()
        .map_err(|e| format!("invalid tar.gz archive: {e}"))?;
    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| format!("corrupt tar entry: {e}"))?;
        let entry_type = entry.header().entry_type();
        let raw_path = entry
            .path()
            .map_err(|e| format!("invalid tar entry path: {e}"))?
            .into_owned();
        if raw_path.as_os_str() == "." || raw_path.as_os_str().is_empty() {
            continue;
        }
        let rel_path = validate_relative_path(&raw_path)?;
        let dest = staging_dir.join(&rel_path);

        match entry_type {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&dest)
                    .map_err(|e| format!("failed to create directory: {e}"))?;
            }
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create parent directory: {e}"))?;
                }
                let remaining = MAX_UNCOMPRESSED_BYTES
                    .checked_sub(total_unpacked)
                    .ok_or_else(|| "uncompressed archive exceeds 250 MiB limit".to_string())?;
                let mut limited = (&mut entry).take(remaining.saturating_add(1));
                let mut out = std::fs::File::create(&dest)
                    .map_err(|e| format!("failed to create extracted file: {e}"))?;
                let copied = std::io::copy(&mut limited, &mut out)
                    .map_err(|e| format!("failed to decompress archive entry: {e}"))?;
                total_unpacked = total_unpacked.saturating_add(copied);
                if total_unpacked > MAX_UNCOMPRESSED_BYTES {
                    return Err("uncompressed archive exceeds 250 MiB limit".to_string());
                }
            }
            other => {
                return Err(format!(
                    "archive contains unsupported entry type '{other:?}' at '{}'",
                    rel_path.display()
                ));
            }
        }
    }

    let (manifest_src, bin_src) = locate_bundle_root(&staging_dir)?;
    let manifest_bytes = std::fs::read(&manifest_src)
        .map_err(|e| format!("failed to read manifest.json: {e}"))?;
    let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("invalid manifest.json: {e}"))?;
    manifest.validate()?;
    set_executable(&bin_src)?;

    guard.0 = None;
    Ok(StagedBundle {
        staging_dir,
        manifest_src,
        bin_src,
        manifest,
    })
}

impl StagedBundle {
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub fn commit_fresh(self, plugins_dir: &Path) -> Result<PluginPaths, String> {
        let paths = plugin_paths(plugins_dir, &self.manifest.id);
        let bin_dir = paths
            .bin_path
            .parent()
            .expect("bin_path always has parent");
        std::fs::create_dir_all(bin_dir)
            .map_err(|e| format!("failed to create plugin bin dir: {e}"))?;
        std::fs::create_dir_all(&paths.data_dir)
            .map_err(|e| format!("failed to create plugin data dir: {e}"))?;

        std::fs::rename(&self.bin_src, &paths.bin_path)
            .map_err(|e| format!("failed to install plugin binary: {e}"))?;
        std::fs::rename(&self.manifest_src, &paths.manifest_path)
            .map_err(|e| format!("failed to install manifest.json: {e}"))?;
        set_executable(&paths.bin_path)?;
        Ok(paths)
    }

    pub fn commit_update(
        self,
        plugins_dir: &Path,
        expected_id: &str,
    ) -> Result<PluginPaths, String> {
        if self.manifest.id != expected_id {
            return Err(format!(
                "updated manifest id '{}' does not match installed plugin '{}'",
                self.manifest.id, expected_id
            ));
        }
        let paths = plugin_paths(plugins_dir, expected_id);
        let bin_dir = paths
            .bin_path
            .parent()
            .expect("bin_path always has parent");
        std::fs::create_dir_all(bin_dir)
            .map_err(|e| format!("failed to create plugin bin dir: {e}"))?;
        std::fs::create_dir_all(&paths.data_dir)
            .map_err(|e| format!("failed to create plugin data dir: {e}"))?;

        let tmp_bin = bin_dir.join(format!(".{BINARY_FILENAME}.new"));
        let tmp_manifest = paths.plugin_dir.join(".manifest.json.new");
        std::fs::copy(&self.bin_src, &tmp_bin)
            .map_err(|e| format!("failed to stage updated binary: {e}"))?;
        set_executable(&tmp_bin)?;
        std::fs::copy(&self.manifest_src, &tmp_manifest)
            .map_err(|e| format!("failed to stage updated manifest: {e}"))?;

        std::fs::rename(&tmp_bin, &paths.bin_path)
            .map_err(|e| format!("failed to swap updated binary: {e}"))?;
        std::fs::rename(&tmp_manifest, &paths.manifest_path)
            .map_err(|e| format!("failed to swap updated manifest: {e}"))?;
        Ok(paths)
    }
}

pub fn remove_plugin_files(plugins_dir: &Path, plugin_id: &str, purge: bool) -> std::io::Result<()> {
    let paths = plugin_paths(plugins_dir, plugin_id);
    if purge {
        if paths.plugin_dir.exists() {
            std::fs::remove_dir_all(&paths.plugin_dir)?;
        }
    } else {
        if let Some(bin_dir) = paths.bin_path.parent() {
            if bin_dir.exists() {
                std::fs::remove_dir_all(bin_dir)?;
            }
        }
        if paths.manifest_path.exists() {
            std::fs::remove_file(&paths.manifest_path)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run installer tests and musl compilation check**

Run: `cargo test -p kadr-server --test plugin_installer_test && cargo check --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.lock crates/kadr-server/Cargo.toml crates/kadr-server/src/plugins/mod.rs \
  crates/kadr-server/src/plugins/installer.rs crates/kadr-server/tests/plugin_installer_test.rs
git commit -m "feat(plugins): add safe tar.gz bundle installer"
```

---

### Task 4: Child Process Supervisor (`plugins/supervisor.rs`)

**Files:**
- Modify: `crates/kadr-server/Cargo.toml`
- Modify: `crates/kadr-server/src/plugins/mod.rs`
- Create: `crates/kadr-server/src/plugins/supervisor.rs`
- Test: `crates/kadr-server/tests/plugin_supervisor_test.rs`

**Interfaces:**
- Produces in `kadr_server::plugins::supervisor`:
  - `pub fn kadr_local_url(host: &str, port: u16) -> String`
  - `pub struct SpawnSpec { pub plugin_id: String, pub bin_path: PathBuf, pub data_dir: PathBuf, pub kadr_url: String, pub secret: String, pub token: String }`
  - `pub enum StartOutcome { Healthy { base_url: String }, Unhealthy { base_url: String, reason: String } }`
  - `pub type OnPortChanged = Arc<dyn Fn(String, String) + Send + Sync>;` (`(plugin_id, new_base_url)`)
  - `pub struct PluginSupervisor` with:
    - `pub fn new() -> Self`
    - `pub async fn start(&self, spec: SpawnSpec, ready_timeout: Duration, on_port_changed: Option<OnPortChanged>) -> StartOutcome`
    - `pub async fn stop(&self, plugin_id: &str)`
    - `pub async fn stop_all(&self)`
    - `pub async fn is_running(&self, plugin_id: &str) -> bool`

- [ ] **Step 1: Add `libc` Unix dependency and write failing supervisor tests**

Add to `crates/kadr-server/Cargo.toml`:

```toml
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

Create `crates/kadr-server/tests/plugin_supervisor_test.rs`:

```rust
#![cfg(unix)]

use kadr_server::plugins::supervisor::{
    kadr_local_url, PluginSupervisor, SpawnSpec, StartOutcome,
};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn kadr_local_url_formats_wildcard_ipv4_and_ipv6_hosts() {
    assert_eq!(kadr_local_url("0.0.0.0", 8492), "http://127.0.0.1:8492");
    assert_eq!(kadr_local_url("::", 8492), "http://127.0.0.1:8492");
    assert_eq!(kadr_local_url("[::]", 8492), "http://127.0.0.1:8492");
    assert_eq!(kadr_local_url("::1", 8492), "http://[::1]:8492");
    assert_eq!(kadr_local_url("127.0.0.1", 9000), "http://127.0.0.1:9000");
}

#[tokio::test]
async fn supervisor_spawns_isolated_env_waits_for_health_restarts_on_crash_and_stops_cleanly() {
    // Require python3 (standard on Linux/macOS CI/dev) for a tiny real HTTP server process.
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("plugin");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    // Parent secret env that MUST NOT leak to the child process:
    std::env::set_var("LEAK_CANARY_TEST", "should-be-stripped");

    let script = r#"#!/usr/bin/env python3
import http.server, json, os, pathlib, signal, sys

pathlib.Path(os.environ["PLUGIN_DATA_DIR"], "env.json").write_text(json.dumps({
    "bind": os.environ.get("PLUGIN_BIND"),
    "kadr_url": os.environ.get("KADR_URL"),
    "secret": os.environ.get("KADR_PLUGIN_SECRET"),
    "token": os.environ.get("KADR_PLUGIN_TOKEN"),
    "canary": os.environ.get("LEAK_CANARY_TEST"),
}))

expected = "Bearer " + os.environ["KADR_PLUGIN_SECRET"]
host, port = os.environ["PLUGIN_BIND"].split(":")

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        if self.path == "/health":
            if self.headers.get("Authorization") == expected:
                self.send_response(200); self.end_headers(); self.wfile.write(b"ok")
            else:
                self.send_response(401); self.end_headers()
        elif self.path == "/crash":
            self.send_response(200); self.end_headers()
            os._exit(1)

signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;
    std::fs::write(&bin_path, script).unwrap();
    std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let sup = PluginSupervisor::new();
    let ports: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let ports_cb = ports.clone();

    let outcome = sup
        .start(
            SpawnSpec {
                plugin_id: "hello".to_string(),
                bin_path: bin_path.clone(),
                data_dir: data_dir.clone(),
                kadr_url: "http://127.0.0.1:8492".to_string(),
                secret: "sec-123".to_string(),
                token: "tok-456".to_string(),
            },
            Duration::from_secs(5),
            Some(Arc::new(move |_id, url| {
                ports_cb.lock().unwrap().push(url);
            })),
        )
        .await;

    let base_url = match outcome {
        StartOutcome::Healthy { base_url } => base_url,
        StartOutcome::Unhealthy { reason, .. } => panic!("expected healthy, got: {reason}"),
    };
    assert!(sup.is_running("hello").await);

    let env_dump: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data_dir.join("env.json")).unwrap()).unwrap();
    assert_eq!(env_dump["kadr_url"], "http://127.0.0.1:8492");
    assert_eq!(env_dump["secret"], "sec-123");
    assert_eq!(env_dump["token"], "tok-456");
    assert!(env_dump["canary"].is_null(), "parent env must be cleared");

    // Trigger an unexpected crash and verify supervisor restarts it (~1s backoff).
    let _ = reqwest::get(format!("{base_url}/crash")).await;
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let recorded = ports.lock().unwrap().clone();
    assert!(
        recorded.len() >= 2,
        "expected respawn port callback, got {recorded:?}"
    );
    let new_url = recorded.last().unwrap().clone();
    let resp = reqwest::Client::new()
        .get(format!("{new_url}/health"))
        .bearer_auth("sec-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Stop cleanly and confirm the HTTP port is no longer serving.
    sup.stop("hello").await;
    assert!(!sup.is_running("hello").await);
    assert!(reqwest::get(format!("{new_url}/health")).await.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test plugin_supervisor_test`
Expected: FAIL to compile (`kadr_server::plugins::supervisor` missing).

- [ ] **Step 3: Implement `plugins/supervisor.rs`**

Export `pub mod supervisor;` in `crates/kadr-server/src/plugins/mod.rs` and create `crates/kadr-server/src/plugins/supervisor.rs`:

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};

const SAFE_OS_ENV_KEYS: &[&str] = &[
    "PATH",
    "LANG",
    "LC_ALL",
    "TZ",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "SYSTEMROOT",
];

pub fn kadr_local_url(host: &str, port: u16) -> String {
    let trimmed = host.trim();
    if matches!(trimmed, "0.0.0.0" | "::" | "[::]" | "") {
        format!("http://127.0.0.1:{port}")
    } else if trimmed.contains(':') && !trimmed.starts_with('[') {
        format!("http://[{trimmed}]:{port}")
    } else {
        format!("http://{trimmed}:{port}")
    }
}

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub plugin_id: String,
    pub bin_path: PathBuf,
    pub data_dir: PathBuf,
    pub kadr_url: String,
    pub secret: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub enum StartOutcome {
    Healthy { base_url: String },
    Unhealthy { base_url: String, reason: String },
}

pub type OnPortChanged = Arc<dyn Fn(String, String) + Send + Sync>;

struct ManagedEntry {
    stop_flag: Arc<AtomicBool>,
    stop_notify: Arc<Notify>,
    child: Arc<Mutex<Option<Child>>>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct PluginSupervisor {
    entries: Arc<Mutex<HashMap<String, ManagedEntry>>>,
    http: reqwest::Client,
}

fn reserve_loopback_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to allocate loopback port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to inspect loopback port: {e}"))?
        .port();
    Ok(port)
}

fn spawn_child(spec: &SpawnSpec, port: u16) -> Result<Child, String> {
    std::fs::create_dir_all(&spec.data_dir)
        .map_err(|e| format!("failed to create plugin data dir: {e}"))?;

    let mut cmd = Command::new(&spec.bin_path);
    cmd.env_clear();
    for key in SAFE_OS_ENV_KEYS {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    cmd.env("PLUGIN_BIND", format!("127.0.0.1:{port}"))
        .env("KADR_URL", &spec.kadr_url)
        .env("KADR_PLUGIN_SECRET", &spec.secret)
        .env("KADR_PLUGIN_TOKEN", &spec.token)
        .env("PLUGIN_DATA_DIR", &spec.data_dir)
        .current_dir(&spec.data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn plugin '{}': {e}", spec.plugin_id))?;

    if let Some(stdout) = child.stdout.take() {
        let id = spec.plugin_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::info!(target: "plugin", plugin = %id, "{line}");
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let id = spec.plugin_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(target: "plugin", plugin = %id, "{line}");
            }
        });
    }

    Ok(child)
}

async fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: Sending SIGTERM to a direct child PID owned by this process before waiting.
        unsafe {
            let _ = libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        if tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .is_ok()
        {
            return;
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

impl PluginSupervisor {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .build()
            .expect("reqwest client builds");
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            http,
        }
    }

    async fn wait_for_health(
        &self,
        child: &mut Child,
        base_url: &str,
        secret: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout.max(Duration::from_millis(300));
        let health_url = format!("{base_url}/health");
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("plugin exited prematurely with {status}"));
            }
            if let Ok(resp) = self
                .http
                .get(&health_url)
                .bearer_auth(secret)
                .timeout(Duration::from_millis(500))
                .send()
                .await
            {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "plugin did not become healthy within {}s",
                    timeout.as_secs().max(1)
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn start(
        &self,
        spec: SpawnSpec,
        ready_timeout: Duration,
        on_port_changed: Option<OnPortChanged>,
    ) -> StartOutcome {
        self.stop(&spec.plugin_id).await;

        let mut last_err = String::new();
        let mut started: Option<(Child, String, bool)> = None;

        for _attempt in 0..3 {
            let port = match reserve_loopback_port() {
                Ok(p) => p,
                Err(e) => return StartOutcome::Unhealthy {
                    base_url: "http://127.0.0.1:0".to_string(),
                    reason: e,
                },
            };
            let base_url = format!("http://127.0.0.1:{port}");
            if let Some(ref cb) = on_port_changed {
                cb(spec.plugin_id.clone(), base_url.clone());
            }
            let mut child = match spawn_child(&spec, port) {
                Ok(c) => c,
                Err(e) => {
                    return StartOutcome::Unhealthy {
                        base_url,
                        reason: e,
                    };
                }
            };
            match self
                .wait_for_health(&mut child, &base_url, &spec.secret, ready_timeout)
                .await
            {
                Ok(()) => {
                    started = Some((child, base_url, true));
                    break;
                }
                Err(e) => {
                    last_err = e;
                    // Retry another port only if the process exited immediately (e.g., port race).
                    if child.try_wait().ok().flatten().is_some() {
                        continue;
                    }
                    started = Some((child, base_url, false));
                    break;
                }
            }
        }

        let (initial_child, base_url, is_healthy) = match started {
            Some((c, url, ok)) => (Some(c), url, ok),
            None => (None, "http://127.0.0.1:0".to_string(), false),
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_notify = Arc::new(Notify::new());
        let child_slot = Arc::new(Mutex::new(initial_child));

        let flag_bg = stop_flag.clone();
        let notify_bg = stop_notify.clone();
        let slot_bg = child_slot.clone();
        let spec_bg = spec.clone();
        let cb_bg = on_port_changed.clone();

        let task = tokio::spawn(async move {
            let mut backoff_secs = 1u64;
            let mut spawned_at = Instant::now();
            loop {
                let exited = {
                    let mut guard = slot_bg.lock().await;
                    match guard.as_mut() {
                        Some(ch) => {
                            let wait_fut = ch.wait();
                            tokio::select! {
                                _ = notify_bg.notified() => false,
                                _ = wait_fut => true,
                            }
                        }
                        None => true,
                    }
                };
                if flag_bg.load(Ordering::SeqCst) || !exited {
                    break;
                }
                if spawned_at.elapsed() >= Duration::from_secs(60) {
                    backoff_secs = 1;
                }
                tracing::warn!(
                    plugin = %spec_bg.plugin_id,
                    backoff_secs,
                    "Managed plugin exited unexpectedly; scheduling restart"
                );
                tokio::select! {
                    _ = notify_bg.notified() => break,
                    _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                }
                if flag_bg.load(Ordering::SeqCst) {
                    break;
                }
                backoff_secs = (backoff_secs * 2).min(30);

                if let Ok(port) = reserve_loopback_port() {
                    let url = format!("http://127.0.0.1:{port}");
                    if let Ok(new_child) = spawn_child(&spec_bg, port) {
                        spawned_at = Instant::now();
                        if let Some(ref cb) = cb_bg {
                            cb(spec_bg.plugin_id.clone(), url);
                        }
                        *slot_bg.lock().await = Some(new_child);
                    }
                }
            }
        });

        self.entries.lock().await.insert(
            spec.plugin_id.clone(),
            ManagedEntry {
                stop_flag,
                stop_notify,
                child: child_slot,
                task,
            },
        );

        if is_healthy {
            StartOutcome::Healthy { base_url }
        } else {
            StartOutcome::Unhealthy {
                base_url,
                reason: last_err,
            }
        }
    }

    pub async fn stop(&self, plugin_id: &str) {
        let entry = self.entries.lock().await.remove(plugin_id);
        if let Some(entry) = entry {
            entry.stop_flag.store(true, Ordering::SeqCst);
            entry.stop_notify.notify_waiters();
            let _ = entry.task.await;
            if let Some(mut child) = entry.child.lock().await.take() {
                terminate_child(&mut child).await;
            }
        }
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self.entries.lock().await.keys().cloned().collect();
        for id in ids {
            self.stop(&id).await;
        }
    }

    pub async fn is_running(&self, plugin_id: &str) -> bool {
        let entries = self.entries.lock().await;
        let Some(entry) = entries.get(plugin_id) else {
            return false;
        };
        let mut guard = entry.child.lock().await;
        match guard.as_mut() {
            Some(ch) => matches!(ch.try_wait(), Ok(None)),
            None => false,
        }
    }
}
```

- [ ] **Step 4: Run supervisor tests and clippy**

Run: `cargo test -p kadr-server --test plugin_supervisor_test && cargo clippy -p kadr-server --all-targets -- -D warnings`
Expected: PASS with zero warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.lock crates/kadr-server/Cargo.toml crates/kadr-server/src/plugins/mod.rs \
  crates/kadr-server/src/plugins/supervisor.rs crates/kadr-server/tests/plugin_supervisor_test.rs
git commit -m "feat(plugins): add isolated child process supervisor"
```

---

### Task 5: `PluginService` Managed Lifecycle, Admin Routes & `main.rs` Wiring

**Files:**
- Modify: `crates/kadr-server/src/plugins/service.rs`
- Modify: `crates/kadr-server/src/plugins/admin_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs:95-110`
- Modify: `crates/kadr-server/src/main.rs:399-490`
- Modify: `crates/kadr-server/tests/plugin_support/mod.rs:184-295`
- Create: `crates/kadr-server/tests/plugin_install_test.rs`

**Interfaces:**
- Consumes: `ForgeClient`, `ForgeError`, `installer::{plugin_paths, remove_plugin_files, unpack_bundle}`, `supervisor::{kadr_local_url, PluginSupervisor, SpawnSpec, StartOutcome}`, `PluginRepository::{set_base_url, update_release}`.
- Produces in `kadr_server::plugins::service`:
  - `ServiceError` variants: `ChecksumMismatch(String)` (`502 CHECKSUM_MISMATCH`), `ForgeError(String)` (`502 FORGE_ERROR`), `ForgeTimeout` (`504 FORGE_TIMEOUT`)
  - `pub struct UpdateOutcome { pub updated: bool, pub previous_version: String, pub requires_approval: bool, pub plugin: PluginRecord }`
  - `PluginService::with_forge_client(self, forge: ForgeClient) -> Self`
  - `PluginService::supervisor(&self) -> &PluginSupervisor`
  - `PluginService::plugins_dir(&self) -> PathBuf`
  - `PluginService::install_from_git(&self, repo_url: &str, tag: Option<&str>, forge: Option<ForgeKind>) -> Result<PluginRecord, ServiceError>`
  - `PluginService::update_managed(&self, id: &str, tag: Option<&str>) -> Result<UpdateOutcome, ServiceError>`
  - `PluginService::start_managed_plugins(&self)` and `PluginService::shutdown_managed_plugins(&self)`
- Produces HTTP routes in `admin_routes.rs`:
  - `POST /api/v1/admin/plugins/install` → `201 { "plugin": PluginRecord }`
  - `POST /api/v1/admin/plugins/{id}/update` → `200 UpdateOutcome`

- [ ] **Step 1: Write the failing end-to-end managed plugin integration test**

Update `crates/kadr-server/tests/plugin_support/mod.rs` so `Harness` sets a temporary `data_dir` in `AppConfig` (cleaned via `tempfile::TempDir` stored on `Harness`) so parallel integration tests never share `./data/plugins`:

```rust
// Add field to Harness in crates/kadr-server/tests/plugin_support/mod.rs:
pub struct Harness {
    pub app: Router,
    pub data_dir: tempfile::TempDir,
    pub media_repo: MediaItemRepository,
    pub lib_repo: LibraryRepository,
    pub plugin_repo: PluginRepository,
    pub playback_repo: PlaybackRepository,
    pub admin_token: String,
    pub user_token: String,
    pub jwt_secret: String,
    pub events: Arc<EventBus>,
}

// In build(settings: PluginSettings):
    let data_dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig {
        plugins: settings,
        ..Default::default()
    };
    config.server.data_dir = data_dir.path().to_path_buf();
    // ... and include `data_dir` in the returned Harness { ... }.
```

Create `crates/kadr-server/tests/plugin_install_test.rs`:

```rust
#![cfg(unix)]

mod plugin_support;

use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Extension, Json, Router,
};
use flate2::{write::GzEncoder, Compression};
use kadr_core::plugin::ForgeKind;
use kadr_server::plugins::{forge::ForgeClient, PluginService};
use plugin_support::{build, call, call_raw, enabled_settings};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn build_plugin_tar_gz(manifest: &Value, script: &str) -> Vec<u8> {
    let manifest_bytes = serde_json::to_vec(manifest).unwrap();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut tar = tar::Builder::new(&mut gz);
        for (name, data, mode) in [
            ("manifest.json", manifest_bytes.as_slice(), 0o644),
            ("plugin", script.as_bytes(), 0o755),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(mode);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, name, data).unwrap();
        }
        tar.finish().unwrap();
    }
    gz.finish().unwrap()
}

#[derive(Clone)]
struct ForgeState {
    tag: Arc<Mutex<String>>,
    archive: Arc<Mutex<Vec<u8>>>,
}

#[tokio::test]
async fn managed_plugin_install_approve_publish_resolve_update_and_remove_lifecycle() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let mut h = build(enabled_settings()).await;

    // Bind real TCP listener for Kadr so the spawned child plugin can call POST /api/v1/plugin/*
    let kadr_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let kadr_port = kadr_listener.local_addr().unwrap().port();

    // Build shared PluginService pointing at a mock GitHub server + Kadr's real bound port
    let plugin_script = r#"#!/usr/bin/env python3
import http.server, json, os, pathlib, signal, sys, threading, urllib.request

data_dir = pathlib.Path(os.environ["PLUGIN_DATA_DIR"])
(data_dir / "boot_marker.txt").write_text("ran")
(data_dir / "persistent.txt").write_text("user-state")

expected = "Bearer " + os.environ["KADR_PLUGIN_SECRET"]
kadr_url = os.environ["KADR_URL"]
token = os.environ["KADR_PLUGIN_TOKEN"]
host, port = os.environ["PLUGIN_BIND"].split(":")

def publish():
    for method, path, body in [
        ("POST", "/api/v1/plugin/libraries", {"id": "git-hello-lib", "name": "Git Hello", "media_type": "movie"}),
        ("PUT", "/api/v1/plugin/libraries/git-hello-lib/items", {"items": [{"remote_id": "vid1", "title": "Git Video 1", "duration_seconds": 120}]}),
    ]:
        req = urllib.request.Request(
            kadr_url + path,
            data=json.dumps(body).encode(),
            headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
            method=method,
        )
        try:
            urllib.request.urlopen(req, timeout=3)
        except Exception:
            pass

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        if self.path == "/health":
            code = 200 if self.headers.get("Authorization") == expected else 401
            self.send_response(code); self.end_headers()
    def do_POST(self):
        if self.path == "/playback/resolve":
            if self.headers.get("Authorization") != expected:
                self.send_response(401); self.end_headers(); return
            body = json.dumps({"mode": "direct", "url": "https://cdn.example.com/git-vid1.mp4"}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)

signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
srv = http.server.HTTPServer((host, int(port)), Handler)
threading.Thread(target=publish, daemon=True).start()
srv.serve_forever()
"#;

    let v1_manifest = json!({
        "id": "githello",
        "name": "Git Hello",
        "version": "0.1.0",
        "api_version": 1,
        "permissions": ["items:publish"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    });
    let v1_archive = build_plugin_tar_gz(&v1_manifest, plugin_script);

    let forge_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let forge_base = format!("http://{}", forge_listener.local_addr().unwrap());
    let target_asset = format!(
        "githello-{}-{}.tar.gz",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let forge_state = ForgeState {
        tag: Arc::new(Mutex::new("v0.1.0".to_string())),
        archive: Arc::new(Mutex::new(v1_archive)),
    };
    let fb = forge_base.clone();
    let ta = target_asset.clone();
    let ta2 = target_asset.clone();
    let forge_app = Router::new()
        .route(
            "/repos/acme/githello/releases/latest",
            get(move |State(st): State<ForgeState>| {
                let fb = fb.clone();
                let ta = ta.clone();
                async move {
                    let tag = st.tag.lock().unwrap().clone();
                    Json(json!({
                        "tag_name": tag,
                        "assets": [
                            { "name": ta, "browser_download_url": format!("{fb}/download/bundle.tar.gz") },
                            { "name": "checksums.txt", "browser_download_url": format!("{fb}/download/checksums.txt") }
                        ]
                    }))
                }
            }),
        )
        .route(
            "/download/bundle.tar.gz",
            get(|State(st): State<ForgeState>| async move { st.archive.lock().unwrap().clone() }),
        )
        .route(
            "/download/checksums.txt",
            get(move |State(st): State<ForgeState>| {
                let ta2 = ta2.clone();
                async move {
                    let bytes = st.archive.lock().unwrap().clone();
                    format!("{:x}  {ta2}\n", Sha256::digest(&bytes))
                }
            }),
        )
        .with_state(forge_state.clone());
    tokio::spawn(async move {
        axum::serve(forge_listener, forge_app).await.unwrap();
    });

    let mut cfg = kadr_server::config::AppConfig {
        plugins: enabled_settings(),
        ..Default::default()
    };
    cfg.server.host = "127.0.0.1".to_string();
    cfg.server.port = kadr_port;
    cfg.server.data_dir = h.data_dir.path().to_path_buf();
    let shared_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        shared_cfg,
        h.events.clone(),
    )
    .with_forge_client(ForgeClient::new(Duration::from_secs(5)).with_github_api_base(&forge_base));

    h.app = h.app.layer(Extension(svc.clone()));
    let app_for_server = h.app.clone();
    tokio::spawn(async move {
        axum::serve(kadr_listener, app_for_server).await.unwrap();
    });

    // 1. Install from Git -> status must be pending, and binary MUST NOT have executed yet!
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/install",
        Some(&h.admin_token),
        Some(json!({
            "repo_url": format!("{forge_base}/acme/githello"),
            "forge": "github"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "install failed: {body}");
    assert_eq!(body["plugin"]["status"], "pending");
    assert_eq!(body["plugin"]["install_kind"], "managed");
    assert_eq!(body["plugin"]["installed_tag"], "v0.1.0");
    let boot_marker = h
        .data_dir
        .path()
        .join("plugins/githello/data/boot_marker.txt");
    assert!(
        !boot_marker.exists(),
        "binary must never run before admin permission approval"
    );

    // 2. Approve -> spawns supervised child, which publishes 'Git Video 1' to Kadr!
    let (status, approve_body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/approve",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "approve failed: {approve_body}");
    assert_eq!(approve_body["plugin"]["status"], "enabled");
    assert!(svc.supervisor().is_running("githello").await);
    assert!(boot_marker.exists());

    // Wait briefly for async publish thread in the plugin to finish creating its virtual library
    for _ in 0..30 {
        if h.media_repo
            .count_by_library("git-hello-lib")
            .await
            .unwrap_or(0)
            == 1
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        h.media_repo.count_by_library("git-hello-lib").await.unwrap(),
        1
    );

    // 3. Update when tag unchanged -> updated: false
    let (status, noop) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/update",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noop["updated"], false);

    // 4. Update v0.2.0 with same permissions -> seamless restart, requires_approval: false
    let v2_manifest = json!({
        "id": "githello",
        "name": "Git Hello",
        "version": "0.2.0",
        "api_version": 1,
        "permissions": ["items:publish"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    });
    *forge_state.tag.lock().unwrap() = "v0.2.0".to_string();
    *forge_state.archive.lock().unwrap() = build_plugin_tar_gz(&v2_manifest, plugin_script);

    let (status, up_v2) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/update",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update v2 failed: {up_v2}");
    assert_eq!(up_v2["updated"], true);
    assert_eq!(up_v2["previous_version"], "0.1.0");
    assert_eq!(up_v2["requires_approval"], false);
    assert_eq!(up_v2["plugin"]["manifest"]["version"], "0.2.0");
    assert!(svc.supervisor().is_running("githello").await);

    // 5. Update v0.3.0 requesting NEW permission playback:resolve -> stops child & drops to pending!
    let v3_manifest = json!({
        "id": "githello",
        "name": "Git Hello",
        "version": "0.3.0",
        "api_version": 1,
        "permissions": ["items:publish", "playback:resolve"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    });
    *forge_state.tag.lock().unwrap() = "v0.3.0".to_string();
    *forge_state.archive.lock().unwrap() = build_plugin_tar_gz(&v3_manifest, plugin_script);

    let (status, up_v3) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/update",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(up_v3["updated"], true);
    assert_eq!(up_v3["requires_approval"], true);
    assert_eq!(up_v3["plugin"]["status"], "pending");
    assert!(!svc.supervisor().is_running("githello").await);

    // Re-approve v0.3.0 with playback:resolve and verify direct stream redirect works!
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/approve",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let item = h
        .media_repo
        .find_by_path(std::path::Path::new("plugin://githello/vid1"))
        .await
        .unwrap()
        .unwrap();
    let (stream_status, headers, _) = call_raw(
        &h.app,
        "GET",
        &format!("/api/v1/stream/{}", item.id.unwrap()),
        Some(&h.user_token),
        &[],
        None,
    )
    .await;
    assert_eq!(stream_status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        headers.get("location").unwrap(),
        "https://cdn.example.com/git-vid1.mp4"
    );

    // 6. Disable stops process; Remove with purge=true cleans up entire plugin directory.
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/disable",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!svc.supervisor().is_running("githello").await);

    let (status, _) = call(
        &h.app,
        "DELETE",
        "/api/v1/admin/plugins/githello?purge=true",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!h.data_dir.path().join("plugins/githello").exists());

    svc.shutdown_managed_plugins().await;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test plugin_install_test`
Expected: FAIL to compile (`with_forge_client`, `supervisor`, `shutdown_managed_plugins` missing).

- [ ] **Step 3: Implement `PluginService` lifecycle methods, admin routes, and `main.rs` wiring**

In `crates/kadr-server/src/plugins/service.rs`:

1. Add imports (`ForgeClient`, `ForgeError`, `installer::{plugin_paths, remove_plugin_files, unpack_bundle}`, `supervisor::{kadr_local_url, OnPortChanged, PluginSupervisor, SpawnSpec, StartOutcome}`, `ForgeKind`, `PluginInstallKind`, `SystemEvent`).
2. Add `ChecksumMismatch(String)`, `ForgeError(String)`, and `ForgeTimeout` to `ServiceError` (mapping to `(StatusCode::BAD_GATEWAY, "CHECKSUM_MISMATCH")`, `(StatusCode::BAD_GATEWAY, "FORGE_ERROR")`, and `(StatusCode::GATEWAY_TIMEOUT, "FORGE_TIMEOUT")`), plus `From<ForgeError> for ServiceError`:

```rust
impl From<ForgeError> for ServiceError {
    fn from(e: ForgeError) -> Self {
        match e {
            ForgeError::InvalidRequest(m) => Self::Invalid(m),
            ForgeError::ChecksumMismatch(m) => Self::ChecksumMismatch(m),
            ForgeError::ForgeError(m) => Self::ForgeError(m),
            ForgeError::Timeout => Self::ForgeTimeout,
        }
    }
}
```

3. Add `UpdateOutcome`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateOutcome {
    pub updated: bool,
    pub previous_version: String,
    pub requires_approval: bool,
    pub plugin: PluginRecord,
}
```

4. Extend `Inner` in `service.rs` with `forge: ForgeClient`, `supervisor: PluginSupervisor`, and `op_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>`. Construct `ForgeClient::new(Duration::from_secs(settings.resolve_timeout_secs.max(1)))` and `PluginSupervisor::new()` in `PluginService::new`, and add:

```rust
    pub fn with_forge_client(mut self, forge: ForgeClient) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_forge_client must be called before cloning PluginService")
            .forge = forge;
        self
    }

    pub fn supervisor(&self) -> &PluginSupervisor {
        &self.inner.supervisor
    }

    pub async fn plugins_dir(&self) -> std::path::PathBuf {
        self.inner.config.read().await.server.data_dir.join("plugins")
    }
```
5. Add helper `lock_for(&self, id: &str) -> Arc<Mutex<()>>` and `spawn_managed(&self, record: &PluginRecord) -> Result<(), ServiceError>`:

```rust
    async fn lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        let mut map = self.inner.op_locks.lock().await;
        map.entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn spawn_managed(&self, record: &PluginRecord) -> Result<(), ServiceError> {
        let (plugins_dir, kadr_url, timeout_secs) = {
            let cfg = self.inner.config.read().await;
            (
                cfg.server.data_dir.join("plugins"),
                kadr_local_url(&cfg.server.host, cfg.server.port),
                cfg.plugins.resolve_timeout_secs.max(1),
            )
        };
        let paths = plugin_paths(&plugins_dir, &record.id);
        let token = self
            .inner
            .tokens
            .issue(&record.id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))?;
        let secret = self.secret_for(&record.id);
        let repo = self.inner.repo.clone();
        let on_port: OnPortChanged = Arc::new(move |id, url| {
            let repo = repo.clone();
            tokio::spawn(async move {
                let _ = repo.set_base_url(&id, &url).await;
            });
        });

        let outcome = self
            .inner
            .supervisor
            .start(
                SpawnSpec {
                    plugin_id: record.id.clone(),
                    bin_path: paths.bin_path,
                    data_dir: paths.data_dir,
                    kadr_url,
                    secret,
                    token,
                },
                Duration::from_secs(timeout_secs),
                Some(on_port),
            )
            .await;

        match outcome {
            StartOutcome::Healthy { base_url } => {
                self.inner.repo.set_base_url(&record.id, &base_url).await?;
                self.inner
                    .repo
                    .set_status(&record.id, PluginStatus::Enabled)
                    .await?;
            }
            StartOutcome::Unhealthy { base_url, reason } => {
                self.inner.repo.set_base_url(&record.id, &base_url).await?;
                self.inner
                    .repo
                    .set_status(&record.id, PluginStatus::Unhealthy)
                    .await?;
                self.inner.events.publish(SystemEvent::PluginError {
                    plugin_id: record.id.clone(),
                    message: reason,
                    timestamp: now_secs(),
                });
            }
        }
        Ok(())
    }
```

6. Implement `install_from_git`, `update_managed`, `start_managed_plugins`, `shutdown_managed_plugins`, and hook `supervisor` into `approve`, `set_enabled`, `update_settings`, and `remove`:

```rust
    pub async fn install_from_git(
        &self,
        repo_url: &str,
        tag: Option<&str>,
        forge: Option<ForgeKind>,
    ) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        self.sync_client_timeout().await;
        let settings = self.settings().await;
        let plugins_dir = self.plugins_dir().await;

        let bundle = self
            .inner
            .forge
            .fetch_release_bundle(repo_url, tag, forge, settings.allow_private_proxy)
            .await?;
        let staged = unpack_bundle(&plugins_dir, &bundle.archive_bytes)
            .map_err(ServiceError::Invalid)?;
        let manifest = staged.manifest().clone();

        let lock = self.lock_for(&manifest.id).await;
        let _guard = lock.lock().await;

        if self.inner.repo.get(&manifest.id).await?.is_some() {
            return Err(ServiceError::Conflict(format!(
                "plugin '{}' is already registered",
                manifest.id
            )));
        }
        staged
            .commit_fresh(&plugins_dir)
            .map_err(ServiceError::Internal)?;

        let record = PluginRecord {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            base_url: "http://127.0.0.1:0".to_string(),
            manifest,
            granted_permissions: vec![],
            status: PluginStatus::Pending,
            created_at: now_secs(),
            install_kind: PluginInstallKind::Managed,
            repo_url: Some(bundle.canonical_repo_url),
            forge_kind: Some(bundle.forge_kind),
            installed_tag: Some(bundle.tag_name),
        };
        self.inner.repo.insert(&record).await?;
        Ok(record)
    }

    pub async fn update_managed(
        &self,
        id: &str,
        tag: Option<&str>,
    ) -> Result<UpdateOutcome, ServiceError> {
        self.ensure_enabled().await?;
        self.sync_client_timeout().await;
        let lock = self.lock_for(id).await;
        let _guard = lock.lock().await;

        let existing = self.get_or_404(id).await?;
        if existing.install_kind != PluginInstallKind::Managed {
            return Err(ServiceError::Invalid(
                "only Git-installed managed plugins can be updated".to_string(),
            ));
        }
        let repo_url = existing.repo_url.clone().ok_or_else(|| {
            ServiceError::Invalid("managed plugin has no repository URL".to_string())
        })?;
        let settings = self.settings().await;
        let bundle = self
            .inner
            .forge
            .fetch_release_bundle(
                &repo_url,
                tag,
                existing.forge_kind,
                settings.allow_private_proxy,
            )
            .await?;

        let previous_version = existing.manifest.version.clone();
        let explicit_tag = tag.map(str::trim).filter(|t| !t.is_empty());
        if explicit_tag.is_none()
            && existing.installed_tag.as_deref() == Some(bundle.tag_name.as_str())
        {
            return Ok(UpdateOutcome {
                updated: false,
                previous_version,
                requires_approval: false,
                plugin: existing,
            });
        }

        let plugins_dir = self.plugins_dir().await;
        let staged = unpack_bundle(&plugins_dir, &bundle.archive_bytes)
            .map_err(ServiceError::Invalid)?;
        let new_manifest = staged.manifest().clone();
        if new_manifest.id != existing.id {
            return Err(ServiceError::Invalid(format!(
                "updated manifest id '{}' does not match installed plugin '{}'",
                new_manifest.id, existing.id
            )));
        }

        self.inner.supervisor.stop(id).await;
        staged
            .commit_update(&plugins_dir, id)
            .map_err(ServiceError::Internal)?;
        self.inner
            .repo
            .update_release(id, &new_manifest.name, &new_manifest, &bundle.tag_name)
            .await?;

        let needs_new_perms = existing.status == PluginStatus::Pending
            || new_manifest
                .permissions
                .iter()
                .any(|p| !existing.granted_permissions.contains(p));

        if needs_new_perms {
            self.inner
                .repo
                .set_status(id, PluginStatus::Pending)
                .await?;
            return Ok(UpdateOutcome {
                updated: true,
                previous_version,
                requires_approval: true,
                plugin: self.get_or_404(id).await?,
            });
        }

        let refreshed = self.get_or_404(id).await?;
        if matches!(
            existing.status,
            PluginStatus::Enabled | PluginStatus::Unhealthy
        ) {
            self.spawn_managed(&refreshed).await?;
        }
        Ok(UpdateOutcome {
            updated: true,
            previous_version,
            requires_approval: false,
            plugin: self.get_or_404(id).await?,
        })
    }

    pub async fn start_managed_plugins(&self) {
        if self.ensure_enabled().await.is_err() {
            return;
        }
        let Ok(records) = self.inner.repo.list().await else {
            return;
        };
        for rec in records {
            if rec.install_kind == PluginInstallKind::Managed
                && matches!(rec.status, PluginStatus::Enabled | PluginStatus::Unhealthy)
            {
                let lock = self.lock_for(&rec.id).await;
                let _guard = lock.lock().await;
                if let Err(e) = self.spawn_managed(&rec).await {
                    tracing::error!(plugin = %rec.id, "Failed to start managed plugin: {e}");
                }
            }
        }
    }

    pub async fn shutdown_managed_plugins(&self) {
        self.inner.supervisor.stop_all().await;
    }
```

Also update `apply_timeout`, `update_settings`, `approve`, `set_enabled`, and `remove` in `service.rs`:
- In `apply_timeout`: also call `self.inner.forge.set_timeout(Duration::from_secs(settings.resolve_timeout_secs.max(1)));`.
- In `update_settings`: capture `was_enabled = self.settings().await.enabled` before applying; after applying, if `was_enabled && !applied.enabled`, call `self.shutdown_managed_plugins().await`; if `!was_enabled && applied.enabled`, call `self.start_managed_plugins().await`.
- In `approve`: acquire `lock_for(id)`; after persisting `granted_permissions` and `PluginStatus::Enabled`, if `record.install_kind == PluginInstallKind::Managed`, call `self.spawn_managed(&self.get_or_404(id).await?).await?`.
- In `set_enabled`: acquire `lock_for(id)`; if `record.install_kind == PluginInstallKind::Managed`, call `self.spawn_managed(&record).await?` when `enabled == true` or set status `Disabled` and call `self.inner.supervisor.stop(id).await` when `enabled == false`.
- In `remove`: acquire `lock_for(id)`; if `record.install_kind == PluginInstallKind::Managed`, call `self.inner.supervisor.stop(id).await`, delete DB records, and call `let _ = remove_plugin_files(&self.plugins_dir().await, id, purge);`.

In `crates/kadr-server/src/plugins/admin_routes.rs`, add the two new endpoints (keeping `/api/v1/admin/plugins/settings` and `/api/v1/admin/plugins/install` registered alongside `{id}` routes):

```rust
#[derive(Deserialize)]
pub struct InstallBody {
    pub repo_url: String,
    pub tag: Option<String>,
    pub forge: Option<kadr_core::plugin::ForgeKind>,
}

#[derive(Deserialize, Default)]
pub struct UpdateBody {
    pub tag: Option<String>,
}

// Add to admin_routes():
        .route("/api/v1/admin/plugins/install", post(install_plugin))
        .route("/api/v1/admin/plugins/{id}/update", post(update_plugin))

async fn install_plugin(
    _admin: RequireAdmin,
    Extension(svc): Extension<PluginService>,
    Json(body): Json<InstallBody>,
) -> Result<(StatusCode, Json<Value>), ServiceError> {
    let plugin = svc
        .install_from_git(&body.repo_url, body.tag.as_deref(), body.forge)
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "plugin": plugin }))))
}

async fn update_plugin(
    _admin: RequireAdmin,
    Path(id): Path<String>,
    Extension(svc): Extension<PluginService>,
    body: Option<Json<UpdateBody>>,
) -> Result<Json<crate::plugins::service::UpdateOutcome>, ServiceError> {
    let tag = body.and_then(|Json(b)| b.tag);
    Ok(Json(svc.update_managed(&id, tag.as_deref()).await?))
}
```

In `crates/kadr-server/src/main.rs` (lines 400-489):
1. Pass `.layer(axum::Extension(plugin_service.clone()))` on `app` right next to `.layer(axum::Extension(storyboard_service))` so all Axum routes share the exact `PluginService` instance as `spawn_health_monitor`.
2. Right after `let listener = tokio::net::TcpListener::bind(&addr).await?;` (and updating `shared_config` port if `0` was used), spawn startup of managed plugins:
   ```rust
   let startup_plugins = plugin_service.clone();
   tokio::spawn(async move {
       startup_plugins.start_managed_plugins().await;
   });
   ```
3. During graceful shutdown (right after `health_monitor_handle.abort();`), call:
   ```rust
   plugin_service.shutdown_managed_plugins().await;
   ```

- [ ] **Step 4: Run full Rust test suite and clippy**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS across all tests and zero clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/plugins/service.rs crates/kadr-server/src/plugins/admin_routes.rs \
  crates/kadr-server/src/main.rs crates/kadr-server/tests/plugin_support/mod.rs \
  crates/kadr-server/tests/plugin_install_test.rs
git commit -m "feat(plugins): wire git plugin install, update, and process lifecycle"
```

---

### Task 6: Web UI — Git Install Mode, Managed Badges & One-Click Update

**Files:**
- Modify: `web/src/types/index.ts:394-443`
- Modify: `web/src/api/client.ts:573-636`
- Modify: `web/src/components/admin/RegisterPluginForm.tsx`
- Modify: `web/src/components/admin/PluginList.tsx`
- Modify: `web/src/components/admin/RemovePluginDialog.tsx`
- Modify: `web/src/components/admin/PluginsPanel.tsx`
- Test: `web/src/components/admin/PluginList.test.tsx`, `web/src/components/admin/PluginDialogs.test.tsx`, `web/src/components/admin/PluginsPanel.test.tsx`

**Interfaces:**
- Produces in `web/src/types/index.ts`:
  - `export type PluginInstallKind = 'external' | 'managed';`
  - `export type ForgeKind = 'github' | 'gitlab' | 'gitea';`
  - Optional fields on `PluginRecord`: `install_kind?: PluginInstallKind; repo_url?: string | null; forge_kind?: ForgeKind | null; installed_tag?: string | null;`
  - `export interface InstallPluginPayload { repo_url: string; tag?: string; forge?: ForgeKind; }`
  - `export interface InstallPluginResponse { plugin: PluginRecord; }`
  - `export interface UpdatePluginResponse { updated: boolean; previous_version: string; requires_approval: boolean; plugin: PluginRecord; }`
- Produces in `web/src/api/client.ts`:
  - `api.installPlugin(payload: InstallPluginPayload): Promise<InstallPluginResponse>`
  - `api.updatePlugin(id: string, tag?: string): Promise<UpdatePluginResponse>`

- [ ] **Step 1: Write the failing frontend tests**

Append to `describe('PluginList')` and `describe('RegisterPluginForm')` in `web/src/components/admin/PluginList.test.tsx`:

```tsx
  it('renders managed badge, repo link, and Update action while hiding Connection details', () => {
    const onUpdate = vi.fn();
    const managed: PluginRecord = {
      ...make('m', 'enabled', ['items:publish']),
      install_kind: 'managed',
      repo_url: 'https://github.com/acme/hello',
      installed_tag: 'v1.2.3',
    };
    render(<PluginList plugins={[managed]} {...handlers()} onUpdate={onUpdate} />);
    const row = screen.getByTestId('plugin-row-m');
    expect(within(row).getByText(/Managed · v1\.2\.3/)).toBeInTheDocument();
    const link = within(row).getByRole('link', { name: 'https://github.com/acme/hello' });
    expect(link).toHaveAttribute('href', 'https://github.com/acme/hello');
    expect(within(row).queryByRole('button', { name: /Connection details/i })).toBeNull();

    fireEvent.click(within(row).getByRole('button', { name: /^Update$/i }));
    expect(onUpdate).toHaveBeenCalledWith(managed);
  });
```

```tsx
  it('defaults to Git install mode when onInstall is supplied and switches to External URL', async () => {
    const onInstall = vi.fn().mockResolvedValue(true);
    const onRegister = vi.fn().mockResolvedValue(true);
    render(<RegisterPluginForm onRegister={onRegister} onInstall={onInstall} />);

    const repoInput = screen.getByLabelText(/Repository URL/i) as HTMLInputElement;
    const tagInput = screen.getByLabelText(/Release tag/i) as HTMLInputElement;
    fireEvent.change(repoInput, { target: { value: ' https://github.com/acme/hello ' } });
    fireEvent.change(tagInput, { target: { value: ' v0.2.0 ' } });
    fireEvent.click(screen.getByRole('button', { name: /Install plugin/i }));
    await waitFor(() =>
      expect(onInstall).toHaveBeenCalledWith('https://github.com/acme/hello', 'v0.2.0')
    );
    await waitFor(() => expect(repoInput.value).toBe(''));
    expect(tagInput.value).toBe('');

    fireEvent.click(screen.getByRole('tab', { name: /External URL/i }));
    fireEvent.change(screen.getByLabelText(/Plugin base URL/i), {
      target: { value: 'http://127.0.0.1:9100' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Register plugin/i }));
    await waitFor(() => expect(onRegister).toHaveBeenCalledWith('http://127.0.0.1:9100'));
  });
```

Append to `describe('RemovePluginDialog')` in `web/src/components/admin/PluginDialogs.test.tsx`:

```tsx
  it('mentions deleting the local data directory when removing a managed plugin', () => {
    render(
      <RemovePluginDialog
        plugin={{ ...plugin, install_kind: 'managed' }}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(
      screen.getByLabelText(
        /Also delete this plugin's local data directory, virtual libraries, and items/i
      )
    ).toBeInTheDocument();
  });
```

In `web/src/components/admin/PluginsPanel.test.tsx`:
- In `beforeEach`, add spies:
  ```tsx
  vi.spyOn(api, 'installPlugin').mockResolvedValue({
    plugin: { ...pendingPlugin, install_kind: 'managed', repo_url: 'https://github.com/acme/hello', installed_tag: 'v0.1.0' },
  });
  vi.spyOn(api, 'updatePlugin').mockResolvedValue({
    updated: false,
    previous_version: '0.1.0',
    requires_approval: false,
    plugin: { ...enabledPlugin, install_kind: 'managed', repo_url: 'https://github.com/acme/hello', installed_tag: 'v0.1.0' },
  });
  ```
- In `starts disabled`, assert `expect(screen.getByLabelText(/Repository URL/i)).toBeDisabled();`; in `registers a plugin`, `shows the API message when registration fails`, and `drops out-of-order refresh responses`, click `fireEvent.click(screen.getByRole('tab', { name: /External URL/i }));` before typing into `Plugin base URL`.
- Append tests to `describe('PluginsPanel')`:
  ```tsx
  it('installs a plugin from Git and opens the approval dialog immediately', async () => {
    const managedPending: PluginRecord = {
      ...pendingPlugin,
      install_kind: 'managed',
      repo_url: 'https://github.com/acme/hello',
      installed_tag: 'v0.1.0',
    };
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValueOnce([]).mockResolvedValue([managedPending]);
    vi.mocked(api.installPlugin).mockResolvedValue({ plugin: managedPending });

    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByLabelText(/Repository URL/i)).toBeEnabled());

    fireEvent.change(screen.getByLabelText(/Repository URL/i), {
      target: { value: 'https://github.com/acme/hello' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Install plugin/i }));

    await waitFor(() =>
      expect(api.installPlugin).toHaveBeenCalledWith({
        repo_url: 'https://github.com/acme/hello',
        tag: undefined,
      })
    );
    expect(await screen.findByRole('dialog', { name: /Approve Hello Plugin/i })).toBeInTheDocument();
  });

  it('updates a managed plugin across noop, seamless upgrade, and permission re-approval', async () => {
    const managedEnabled: PluginRecord = {
      ...enabledPlugin,
      install_kind: 'managed',
      repo_url: 'https://github.com/acme/hello',
      installed_tag: 'v0.1.0',
    };
    vi.mocked(api.getPluginSettings).mockResolvedValue(enabledSettings);
    vi.mocked(api.listPlugins).mockResolvedValue([managedEnabled]);
    render(<PluginsPanel />);
    await waitFor(() => expect(screen.getByTestId('plugin-row-hello')).toBeInTheDocument());

    // 1. Already up to date
    fireEvent.click(screen.getByRole('button', { name: /^Update$/i }));
    await waitFor(() => expect(api.updatePlugin).toHaveBeenCalledWith('hello'));
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent(/already up to date \(v0\.1\.0\)/i)
    );

    // 2. Seamless update
    const v2: PluginRecord = {
      ...managedEnabled,
      installed_tag: 'v0.2.0',
      manifest: { ...managedEnabled.manifest, version: '0.2.0' },
    };
    vi.mocked(api.updatePlugin).mockResolvedValueOnce({
      updated: true,
      previous_version: '0.1.0',
      requires_approval: false,
      plugin: v2,
    });
    fireEvent.click(screen.getByRole('button', { name: /^Update$/i }));
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent(/Updated "Hello Plugin" from v0\.1\.0 to v0\.2\.0/i)
    );

    // 3. Update requiring new permission approval
    const v3Pending: PluginRecord = {
      ...v2,
      status: 'pending',
      installed_tag: 'v0.3.0',
      manifest: { ...v2.manifest, version: '0.3.0' },
    };
    vi.mocked(api.updatePlugin).mockResolvedValueOnce({
      updated: true,
      previous_version: '0.2.0',
      requires_approval: true,
      plugin: v3Pending,
    });
    fireEvent.click(screen.getByRole('button', { name: /^Update$/i }));
    expect(await screen.findByRole('dialog', { name: /Approve Hello Plugin/i })).toBeInTheDocument();
  });
  ```

- [ ] **Step 2: Run Vitest to verify new tests fail**

Run: `cd web && npm test -- --run`
Expected: FAIL (`installPlugin`, `updatePlugin`, `onUpdate`, and Git mode UI not implemented yet).

- [ ] **Step 3: Implement TypeScript types, API client methods, and admin components**

In `web/src/types/index.ts`, extend the Plugin Types block:

```ts
export type PluginStatus = 'pending' | 'enabled' | 'disabled' | 'unhealthy';
export type PluginPermission = 'items:publish' | 'playback:resolve';
export type PluginInstallKind = 'external' | 'managed';
export type ForgeKind = 'github' | 'gitlab' | 'gitea';

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
  install_kind?: PluginInstallKind;
  repo_url?: string | null;
  forge_kind?: ForgeKind | null;
  installed_tag?: string | null;
}

export interface InstallPluginPayload {
  repo_url: string;
  tag?: string;
  forge?: ForgeKind;
}

export interface InstallPluginResponse {
  plugin: PluginRecord;
}

export interface UpdatePluginResponse {
  updated: boolean;
  previous_version: string;
  requires_approval: boolean;
  plugin: PluginRecord;
}
```

In `web/src/api/client.ts` (import the 3 new types at top and add methods in the `Plugin Management` section):

```ts
  public async installPlugin(payload: InstallPluginPayload): Promise<InstallPluginResponse> {
    return this.request<InstallPluginResponse>('/api/v1/admin/plugins/install', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  public async updatePlugin(id: string, tag?: string): Promise<UpdatePluginResponse> {
    return this.request<UpdatePluginResponse>(
      `/api/v1/admin/plugins/${encodeURIComponent(id)}/update`,
      {
        method: 'POST',
        body: tag ? JSON.stringify({ tag }) : undefined,
      }
    );
  }
```

Update `web/src/components/admin/RegisterPluginForm.tsx` (supports standalone `onRegister`-only usage in existing unit tests AND two-tab `Install from Git` / `External URL` when `onInstall` is provided):

```tsx
import React, { useState } from 'react';
import { Download, Plus } from 'lucide-react';

export interface RegisterPluginFormProps {
  /** Resolves `true` when registration succeeded (the field is then cleared). */
  onRegister: (baseUrl: string) => Promise<boolean>;
  /** Optional Git repository installer; when supplied, shows the Git / External mode switch. */
  onInstall?: (repoUrl: string, tag?: string) => Promise<boolean>;
  disabled?: boolean;
}

export const RegisterPluginForm: React.FC<RegisterPluginFormProps> = ({
  onRegister,
  onInstall,
  disabled = false,
}) => {
  const [mode, setMode] = useState<'git' | 'external'>(onInstall ? 'git' : 'external');
  const [repoUrl, setRepoUrl] = useState('');
  const [tag, setTag] = useState('');
  const [url, setUrl] = useState('');

  const trimmedUrl = url.trim();
  const trimmedRepo = repoUrl.trim();
  const trimmedTag = tag.trim();

  const submitExternal = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!trimmedUrl || disabled) return;
    try {
      const ok = await onRegister(trimmedUrl);
      if (ok) setUrl('');
    } catch {
      // Keep typed input on error
    }
  };

  const submitGit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!onInstall || !trimmedRepo || disabled) return;
    try {
      const ok = await onInstall(trimmedRepo, trimmedTag || undefined);
      if (ok) {
        setRepoUrl('');
        setTag('');
      }
    } catch {
      // Keep typed input on error
    }
  };

  if (!onInstall) {
    return (
      <form onSubmit={submitExternal} className="flex flex-col sm:flex-row gap-3">
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
          disabled={disabled || !trimmedUrl}
          className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
        >
          <Plus className="w-4 h-4" aria-hidden="true" />
          Register plugin
        </button>
      </form>
    );
  }

  return (
    <div className="bg-panel border border-border-subtle rounded-xl p-4 space-y-4">
      <div role="tablist" aria-label="Plugin installation method" className="inline-flex rounded-lg bg-canvas p-1 border border-border-subtle">
        <button
          type="button"
          role="tab"
          aria-selected={mode === 'git'}
          disabled={disabled}
          onClick={() => setMode('git')}
          className={`px-3 py-1.5 text-xs font-semibold rounded-md transition-colors cursor-pointer disabled:opacity-50 ${
            mode === 'git' ? 'bg-accent text-canvas' : 'text-muted hover:text-text-main'
          }`}
        >
          Install from Git
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === 'external'}
          disabled={disabled}
          onClick={() => setMode('external')}
          className={`px-3 py-1.5 text-xs font-semibold rounded-md transition-colors cursor-pointer disabled:opacity-50 ${
            mode === 'external' ? 'bg-accent text-canvas' : 'text-muted hover:text-text-main'
          }`}
        >
          External URL
        </button>
      </div>

      {mode === 'git' ? (
        <form onSubmit={submitGit} className="space-y-3">
          <div className="flex flex-col sm:flex-row gap-3">
            <input
              type="text"
              value={repoUrl}
              onChange={(e) => setRepoUrl(e.target.value)}
              disabled={disabled}
              aria-label="Repository URL"
              placeholder="https://github.com/owner/kadr-plugin-example"
              className="flex-1 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
            />
            <input
              type="text"
              value={tag}
              onChange={(e) => setTag(e.target.value)}
              disabled={disabled}
              aria-label="Release tag (optional)"
              placeholder="Tag (optional, e.g. v0.1.0)"
              className="sm:w-48 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
            />
            <button
              type="submit"
              disabled={disabled || !trimmedRepo}
              className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
            >
              <Download className="w-4 h-4" aria-hidden="true" />
              Install plugin
            </button>
          </div>
          <p className="text-xs text-muted">
            Supports public GitHub, GitLab, and Gitea repositories with prebuilt release archives.
          </p>
        </form>
      ) : (
        <form onSubmit={submitExternal} className="flex flex-col sm:flex-row gap-3">
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
            disabled={disabled || !trimmedUrl}
            className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
          >
            <Plus className="w-4 h-4" aria-hidden="true" />
            Register plugin
          </button>
        </form>
      )}
    </div>
  );
};
```

Update `web/src/components/admin/PluginList.tsx`:

```tsx
export interface PluginListProps {
  plugins: PluginRecord[];
  busy?: boolean;
  onApprove: (plugin: PluginRecord) => void;
  onEnable: (plugin: PluginRecord) => void;
  onDisable: (plugin: PluginRecord) => void;
  onConnection: (plugin: PluginRecord) => void;
  onUpdate?: (plugin: PluginRecord) => void;
  onRemove: (plugin: PluginRecord) => void;
}
```

Inside `plugins.map((plugin) => { ... })` in `web/src/components/admin/PluginList.tsx`:

```tsx
        const pending = plugin.status === 'pending';
        const isManaged = plugin.install_kind === 'managed';
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
                {isManaged && (
                  <span className="text-xs font-medium px-2 py-0.5 rounded-full border border-accent/40 bg-accent/10 text-accent">
                    {plugin.installed_tag ? `Managed · ${plugin.installed_tag}` : 'Managed'}
                  </span>
                )}
                <span
                  className={`text-xs font-medium px-2 py-0.5 rounded-full border ${STATUS_CLASS[plugin.status]}`}
                >
                  {STATUS_LABEL[plugin.status]}
                </span>
              </div>
              {isManaged && plugin.repo_url ? (
                <p className="text-xs text-muted break-all">
                  <a
                    href={plugin.repo_url}
                    target="_blank"
                    rel="noreferrer"
                    className="hover:text-accent underline decoration-border-subtle"
                  >
                    {plugin.repo_url}
                  </a>
                </p>
              ) : (
                <p className="text-xs text-muted break-all">{plugin.base_url}</p>
              )}
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
              {isManaged && onUpdate && (
                <button type="button" className={buttonClass} disabled={busy} onClick={() => onUpdate(plugin)}>
                  Update
                </button>
              )}
              {!pending && !isManaged && (
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
```

In `web/src/components/admin/RemovePluginDialog.tsx`, replace the static checkbox span (lines 84-86) with:

```tsx
          <span className="text-sm text-text-main">
            {plugin.install_kind === 'managed'
              ? "Also delete this plugin's local data directory, virtual libraries, and items"
              : "Also delete this plugin's libraries and items"}
          </span>
```

Update `web/src/components/admin/PluginsPanel.tsx`:
- Change section heading `Register a plugin` to `Add a plugin` and pass `onInstall={installFromGit}` to `<RegisterPluginForm onRegister={register} onInstall={installFromGit} disabled={!enabled || busy} />`.
- Implement `installFromGit`:
  ```ts
  const installFromGit = async (repoUrl: string, tag?: string): Promise<boolean> => {
    let ok = false;
    await run(async () => {
      const res = await api.installPlugin({ repo_url: repoUrl, tag });
      ok = true;
      await refresh(true);
      if (isMountedRef.current) {
        setDialogError(null);
        setApproving(res.plugin);
        setBanner({
          type: 'success',
          text: `Downloaded "${res.plugin.name}". Review and approve its permissions to start it.`,
        });
      }
    });
    return ok;
  };
  ```
- Implement `updateManagedPlugin` and pass `onUpdate={(p) => void updateManagedPlugin(p)}` to `<PluginList>`:
  ```ts
  const updateManagedPlugin = (plugin: PluginRecord) =>
    run(async () => {
      const res = await api.updatePlugin(plugin.id);
      await refresh(true);
      if (!isMountedRef.current) return;
      if (!res.updated) {
        return `"${res.plugin.name}" is already up to date (v${res.plugin.manifest.version}).`;
      }
      if (res.requires_approval) {
        setDialogError(null);
        setApproving(res.plugin);
        setBanner({
          type: 'warning',
          text: `Updated "${res.plugin.name}" to v${res.plugin.manifest.version}. Approve its new permissions to start it.`,
        });
        return;
      }
      return `Updated "${res.plugin.name}" from v${res.previous_version} to v${res.plugin.manifest.version}.`;
    });
  ```

- [ ] **Step 4: Run Vitest and production TypeScript + Vite build**

Run: `cd web && npm test -- --run && npm run build`
Expected: PASS across all test suites and clean production build.

- [ ] **Step 5: Commit**

```bash
git add web/src/types/index.ts web/src/api/client.ts \
  web/src/components/admin/RegisterPluginForm.tsx web/src/components/admin/PluginList.tsx \
  web/src/components/admin/RemovePluginDialog.tsx web/src/components/admin/PluginsPanel.tsx \
  web/src/components/admin/PluginList.test.tsx web/src/components/admin/PluginDialogs.test.tsx \
  web/src/components/admin/PluginsPanel.test.tsx
git commit -m "feat(web): add git plugin installation and one-click updates to admin UI"
```

---

### Task 7: Documentation & `plugin-hello` Release Packaging Guide

**Files:**
- Modify: `docs/PLUGIN_GUIDE.md`
- Modify: `examples/plugin-hello/README.md`
- Create: `examples/plugin-hello/manifest.json`

**Interfaces:**
- Documents the exact `.tar.gz` bundle layout, target suffixes, `checksums.txt`, supervised environment variables (`PLUGIN_BIND`, `KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`, `PLUGIN_DATA_DIR`), and CI release workflow for plugin authors.

- [ ] **Step 1: Create `examples/plugin-hello/manifest.json`**

Create `examples/plugin-hello/manifest.json` matching the manifest served by `examples/plugin-hello/src/main.rs`:

```json
{
  "id": "hello",
  "name": "Hello Plugin",
  "version": "0.1.0",
  "api_version": 1,
  "permissions": ["items:publish", "playback:resolve"],
  "capabilities": {
    "virtual_library": true,
    "playback_resolver": true
  }
}
```

- [ ] **Step 2: Update `docs/PLUGIN_GUIDE.md` and `examples/plugin-hello/README.md`**

In `docs/PLUGIN_GUIDE.md`:
1. Update the intro to note that Kadr supports **managed plugins** (installed directly from GitHub, GitLab, or Gitea releases and supervised by Kadr) as well as **external plugins** (started separately and registered by HTTP URL).
2. Add a **Packaging for Git Installation** section documenting:
   - Supported hosts: public GitHub (`github.com/<owner>/<repo>`), GitLab (`gitlab.com/<group>/.../<repo>` or self-hosted), and Gitea / Forgejo (`codeberg.org/<owner>/<repo>` or self-hosted).
   - Release asset naming: `<name>-<os>-<arch>.tar.gz` (`linux-x86_64`, `linux-aarch64`, `macos-x86_64`, `macos-aarch64`, plus `darwin`/`amd64`/`arm64` aliases).
   - Bundle contents: `manifest.json` and `plugin` (`plugin.exe` on Windows), plus optional `checksums.txt` (`sha256sum` format).
   - Environment variables automatically injected into managed plugins (`PLUGIN_BIND`, `KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`, `PLUGIN_DATA_DIR`) with stripped parent environment.
   - Admin API routes `POST /api/v1/admin/plugins/install` and `POST /api/v1/admin/plugins/{id}/update`, plus reserved IDs `settings` and `install`.
   - A concise GitHub Actions snippet building `plugin-<id>-linux-x86_64.tar.gz` + `checksums.txt`.

In `examples/plugin-hello/README.md`, add a **Package a release archive** section:

```bash
cargo build --release
cp target/release/plugin-hello plugin
tar -czf plugin-hello-linux-x86_64.tar.gz manifest.json plugin
sha256sum plugin-hello-linux-x86_64.tar.gz > checksums.txt
rm plugin
```

- [ ] **Step 3: Run full CI verification suite**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && npm test -- --run && npm run build
```
Expected: All checks and tests PASS cleanly.

- [ ] **Step 4: Commit**

```bash
git add docs/PLUGIN_GUIDE.md examples/plugin-hello/README.md examples/plugin-hello/manifest.json
git commit -m "docs(plugins): document git release bundle format and managed supervision"
```
