# Git-Based Plugin Installation & Process Supervision — Design

Date: 2026-11-15
Status: Draft, pending user review
Builds on: `2026-11-07-plugin-framework-design.md`, `2026-11-09-plugin-web-ui-design.md`

## 1. Goal and Scope

Let an admin install a Kadr plugin directly from a public **GitHub**, **GitLab**, or **Gitea / Forgejo** repository URL, without compiling code on the server, manually copying `KADR_*` secrets/tokens, or managing a separate systemd unit. Kadr downloads a prebuilt platform release bundle, waits for admin permission approval before executing the binary, supervises the plugin child process across restarts, and supports one-click updates.

**In scope**
- Standard `.tar.gz` release bundle convention (`manifest.json` + `plugin` binary, optional SHA-256 checksum verification).
- Native release discovery for GitHub, GitLab (including nested subgroups), and Gitea / Forgejo public repositories (latest release or pinned tag).
- Safe pure-Rust archive extraction (`flate2` + `tar`) into `<data_dir>/plugins/<id>/` with size, symlink, and path-traversal guards.
- In-process child supervision (`PluginSupervisor`): ephemeral loopback port allocation, clean environment injection (`PLUGIN_BIND`, `KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`, `PLUGIN_DATA_DIR`), crash backoff, startup auto-boot, and graceful shutdown.
- One-click **Update** for managed plugins, automatically gating on admin re-approval if a newer version requests previously ungranted permissions.
- Admin API routes and Web UI updates (`Install from Git` alongside existing `External URL` registration, plus `Update` action).
- Updating `docs/PLUGIN_GUIDE.md` and `examples/plugin-hello` with release packaging instructions.

**Out of scope**
- Private Git repositories requiring personal access tokens / SSH keys.
- Building plugins from source (`git clone` / `cargo build`) on the Kadr host.
- Per-plugin custom configuration / environment-variable UI (left for a dedicated follow-up spec).
- Automatic background auto-updates without admin action.

---

## 2. Release Bundle & Forge Discovery

### 2.1 Release Archive Convention
Plugin authors attach one `.tar.gz` archive per supported OS/architecture to a Git release:

- **Canonical filename suffix:** `-<os>-<arch>.tar.gz` where `<os>` and `<arch>` match Rust's `std::env::consts::{OS, ARCH}` (e.g., `kadr-plugin-iptv-linux-x86_64.tar.gz`, `kadr-plugin-iptv-linux-aarch64.tar.gz`, `kadr-plugin-iptv-macos-aarch64.tar.gz`).
- **Accepted target aliases** (used only if no exact canonical match exists in the release):
  - OS: `darwin` for `macos`.
  - Arch: `amd64` for `x86_64`; `arm64` for `aarch64`.
  - If zero assets match the host target, installation fails with `400 INVALID_REQUEST` naming the expected `<os>-<arch>` suffix. If multiple assets match ambiguously, installation fails with `400 INVALID_REQUEST` listing the conflicting asset names.
- **Required archive contents** (either at the archive root or under a single shared top-level directory `<prefix>/`):
  - `manifest.json` — valid `PluginManifest` (`api_version: 1`).
  - `plugin` (`plugin.exe` on Windows) — executable plugin binary (`chmod 0755` applied on Unix).
- **Optional SHA-256 verification:**
  - Kadr checks the release assets for `<archive_name>.sha256`, `checksums.txt`, or `SHA256SUMS` (in that precedence order, max `64 KiB`).
  - `<archive_name>.sha256` may contain either `<64-hex>` or `<64-hex>  <archive_name>`. Multi-file `checksums.txt` / `SHA256SUMS` must contain a `<64-hex> [* ]<archive_name>` line for the selected archive.
  - If a checksum asset is present and either lacks an entry for the archive or does not match the downloaded archive's SHA-256 digest, installation fails with `502 CHECKSUM_MISMATCH`.

### 2.2 Forge Adapter (`crates/kadr-server/src/plugins/forge.rs`)
Input: `repo_url` (`http://` or `https://`, trailing `/` and `.git` stripped; query strings and fragments rejected), optional `tag`, and optional `forge` hint (`github | gitlab | gitea`).

| Forge | Host Detection (when `forge` omitted) | Release API Endpoint |
|---|---|---|
| `github` | `github.com` (requires `<owner>/<repo>`) | Latest: `GET https://api.github.com/repos/{owner}/{repo}/releases/latest`<br>Tag: `GET https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}`<br>Headers: `Accept: application/vnd.github+json`, `User-Agent: kadr/<version>` |
| `gitlab` | `gitlab.com` (requires `>= 2` path segments, e.g. `group/sub/repo`) | Latest: `GET {origin}/api/v4/projects/{url_encoded_path}/releases/permalink/latest`<br>Tag: `GET {origin}/api/v4/projects/{url_encoded_path}/releases/{url_encoded_tag}`<br>Assets read from `assets.links[]` (`direct_asset_url` preferred over `url`) |
| `gitea` | `codeberg.org` (requires `<owner>/<repo>`) | Latest: `GET {origin}/api/v1/repos/{owner}/{repo}/releases/latest`<br>Tag: `GET {origin}/api/v1/repos/{owner}/{repo}/releases/tags/{tag}` |
| Self-hosted | Any other host | If path has 2 segments, try Gitea v1 first; on `404`, fall back to GitLab v4. If path has `> 2` segments, use GitLab v4 directly. Persist the resolved `ForgeKind` in SQLite so updates skip probing. |

All forge metadata requests and asset downloads enforce a bounded redirect loop (max 5 hops, required for GitHub S3 CDN and GitLab `permalink/latest` redirects):
- When `plugins.allow_private_proxy == false` (default): every hop must use `https://` and resolve strictly to public IP addresses (reusing the IP guard in `plugins/url_guard.rs`).
- When `plugins.allow_private_proxy == true`: `http://` and private/loopback/LAN addresses are allowed so self-hosted homelab Gitea/GitLab instances and local integration test servers work.

---

## 3. Data Model & Disk Layout

### 3.1 Core Types (`crates/kadr-core/src/plugin.rs`)
```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginInstallKind {
    #[default]
    External,
    Managed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeKind {
    Github,
    Gitlab,
    Gitea,
}
```

`PluginRecord` gains four `#[serde(default)]` fields:
- `install_kind: PluginInstallKind`
- `repo_url: Option<String>`
- `forge_kind: Option<ForgeKind>`
- `installed_tag: Option<String>`

`PluginManifest::validate()` additionally reserves `"install"` alongside `"settings"` so static admin sub-routes (`/api/v1/admin/plugins/settings` and `/api/v1/admin/plugins/install`) can never collide with `{id}` routes.

### 3.2 Database Migration (`010_managed_plugins.sql`)
```sql
ALTER TABLE plugins ADD COLUMN install_kind TEXT NOT NULL DEFAULT 'external';
ALTER TABLE plugins ADD COLUMN repo_url TEXT DEFAULT NULL;
ALTER TABLE plugins ADD COLUMN forge_kind TEXT DEFAULT NULL;
ALTER TABLE plugins ADD COLUMN installed_tag TEXT DEFAULT NULL;
```
Existing externally registered plugins default cleanly to `install_kind = 'external'` with `NULL` forge metadata.

### 3.3 Disk Layout (`<config.server.data_dir>/plugins/`)
```text
<data_dir>/plugins/
  .staging-<uuid>/          # temporary unpack directory (auto-removed on drop)
  <plugin_id>/
    manifest.json           # unpacked manifest
    bin/
      plugin                # unpacked executable (0755)
    data/                   # writable working directory (PLUGIN_DATA_DIR)
```

---

## 4. Safe Archive Installer (`crates/kadr-server/src/plugins/installer.rs`)

1. Stream the archive body with a hard cap of **`100 MiB` compressed** (aborting immediately with `400 INVALID_REQUEST` if exceeded) while computing its SHA-256 hash.
2. Verify SHA-256 if a checksum asset was found in the release.
3. Unpack via `flate2::read::GzDecoder` + `tar::Archive` into `<data_dir>/plugins/.staging-<uuid>/` (guarded by an RAII cleanup struct so any error deletes the staging directory):
   - Enforce a cumulative **`250 MiB` uncompressed** byte cap across all entries.
   - Reject any tar entry whose path is absolute, contains `..`, or whose entry type is anything other than regular file or directory (symlinks, hardlinks, FIFOs, and device nodes are rejected).
   - Locate `manifest.json` and `plugin` (`plugin.exe` on Windows) either at root or inside a single top-level directory; parse and validate `PluginManifest`.
4. Commit files:
   - **Fresh install:** verify no plugin with `manifest.id` exists in SQLite (`409 CONFLICT`), create `<data_dir>/plugins/<id>/bin/` and `<data_dir>/plugins/<id>/data/` (`create_dir_all`, preserving any existing `data/` left behind by a prior `purge = false` removal), move `manifest.json` and `bin/plugin` into place, and insert `PluginRecord` with `status = Pending`, `install_kind = Managed`, and placeholder `base_url = "http://127.0.0.1:0"`. **The binary is never executed during install.**
   - **Update:** verify `manifest.id == existing.id` (`400 INVALID_REQUEST` on mismatch). While the supervised child is stopped, atomically replace `<data_dir>/plugins/<id>/bin/plugin` and `manifest.json` via same-directory temp-file renames so `<data_dir>/plugins/<id>/data/` is preserved untouched.

---

## 5. Process Supervisor & Lifecycle (`crates/kadr-server/src/plugins/supervisor.rs`)

### 5.1 Single Shared `PluginService`
`create_router_with_ingest` continues to build a default `PluginService` (rooted at `config.server.data_dir.join("plugins")`) so existing router tests work unchanged, while `main.rs` constructs one shared `PluginService` and passes it across:
- `spawn_health_monitor`,
- Axum routes via `.layer(axum::Extension(plugin_service.clone()))` (matching `StoryboardService`),
- server startup (`plugin_service.start_managed_plugins().await`), and
- graceful shutdown (`plugin_service.shutdown_managed_plugins().await`).

Per-plugin lifecycle operations (`install`, `approve`, `set_enabled`, `update`, `remove`) acquire a per-plugin async mutex inside `PluginService` so concurrent admin calls cannot race process or filesystem state.

### 5.2 Spawning & Environment Isolation
To start a managed plugin `<id>`:
1. Bind `std::net::TcpListener::bind("127.0.0.1:0")` to obtain an available loopback port `<port>`, drop the listener, set `base_url = "http://127.0.0.1:<port>"` in SQLite, and mint a fresh plugin JWT token + derived secret. (Retry port selection up to 3 times if the child exits immediately due to a port race.)
2. Compute `KADR_URL` from `AppConfig.server`: if `server.host` is `0.0.0.0`, `::`, or `[::]`, use `http://127.0.0.1:<server.port>`; if `server.host` is an unbracketed IPv6 literal, use `http://[<host>]:<server.port>`; otherwise use `http://<host>:<server.port>`.
3. Spawn `<data_dir>/plugins/<id>/bin/plugin` with `tokio::process::Command`:
   - `env_clear()` so the child never inherits server secrets from `/etc/kadr/kadr.env`.
   - Pass safe OS environment variables if set in the parent: `PATH`, `LANG`, `LC_ALL`, `TZ`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, `SYSTEMROOT`.
   - Inject `PLUGIN_BIND=127.0.0.1:<port>`, `KADR_URL`, `KADR_PLUGIN_SECRET`, `KADR_PLUGIN_TOKEN`, and `PLUGIN_DATA_DIR=<data_dir>/plugins/<id>/data`.
   - Set `current_dir` to `<data_dir>/plugins/<id>/data`, `stdin(Stdio::null())`, and pipe `stdout` / `stderr` line-by-line into `tracing` under target `plugin::<id>`.
4. Poll `GET http://127.0.0.1:<port>/health` (with `Authorization: Bearer <secret>`) every `100ms` up to `resolve_timeout_secs`:
   - If `2xx`: mark plugin `Enabled`.
   - If timeout or early exit: mark plugin `Unhealthy`, emit `SystemEvent::PluginError`, and allow the background supervisor loop to retry with exponential backoff.

### 5.3 Crash Backoff, Toggle, Update & Shutdown
- **Crash supervision:** While a managed plugin's desired state is active (`plugins.enabled == true` and status in `Enabled | Unhealthy`), a background watcher task awaits `child.wait()`. Unexpected exits restart after exponential backoff (`1s`, `2s`, `4s`, `8s`, `16s`, capped at `30s`; backoff resets after `60s` of continuous uptime).
- **Stopping a child:** Cancels the plugin's supervision token first (preventing a restart), sends `SIGTERM` on Unix (`kill` on non-Unix), waits up to `3s` for exit, and sends `SIGKILL` if still alive.
- **Lifecycle hooks:**
  - **Approve (`Pending -> Enabled/Unhealthy`):** Persists `granted_permissions`; for `Managed` plugins, spawns and runs the readiness check.
  - **Disable / Enable:** Stops the child on `disable`; spawns and runs the readiness check on `enable`.
  - **Global `plugins.enabled` toggle (`PUT /api/v1/admin/plugins/settings`):** Turning plugins `false` stops all running managed children; turning plugins `true` spawns all `Managed` plugins in `Enabled` or `Unhealthy` status.
  - **Server startup:** Right after binding the Axum `TcpListener` (and only when `plugins.enabled == true`), a background task calls `start_managed_plugins()` to spawn all `Managed` plugins in `Enabled` or `Unhealthy` status so the HTTP server is already accepting `/api/v1/plugin/*` publish callbacks as plugins boot.
  - **Update (`POST /api/v1/admin/plugins/{id}/update`):**
    1. Resolve the target release (explicit `tag` if supplied, otherwise latest release from `repo_url` + `forge_kind`).
    2. If `tag` was omitted and `release.tag_name == installed_tag`, return `{ updated: false, previous_version, requires_approval: false, plugin }` without touching the running process.
    3. Download and validate the new bundle in staging (`manifest.id` must match `id`).
    4. Stop the running child, atomically replace `bin/plugin` and `manifest.json`, and update `name`, `manifest`, and `installed_tag` in SQLite.
    5. Check permissions: if every permission in the new `manifest.permissions` is already in `granted_permissions` (and the plugin was not `Pending`), preserve status (`Disabled` stays `Disabled`; `Enabled`/`Unhealthy` respawns immediately) and return `requires_approval: false`. If the new manifest requests any ungranted permission (or the plugin was already `Pending`), set `status = Pending` without spawning and return `requires_approval: true`.
  - **Remove (`DELETE /api/v1/admin/plugins/{id}?purge=...`):** Stops the child, removes DB records (and virtual libraries/items when `purge = true`), deletes `bin/` and `manifest.json`, and deletes `<data_dir>/plugins/<id>/` completely when `purge = true` (preserving `data/` when `purge = false`).
  - **Graceful server shutdown:** After `axum::serve` exits on `SIGINT`/`SIGTERM`, `main.rs` awaits `plugin_service.shutdown_managed_plugins()` to terminate all child processes cleanly before exiting.

---

## 6. Admin API & Web UI

### 6.1 Admin Routes (`crates/kadr-server/src/plugins/admin_routes.rs`)
All routes require `RequireAdmin` and `plugins.enabled == true` (returning `404 PLUGINS_DISABLED` when disabled).

| Route | Request Body | Response |
|---|---|---|
| `POST /api/v1/admin/plugins/install` | `{ "repo_url": String, "tag"?: String, "forge"?: ForgeKind }` | `201 { "plugin": PluginRecord }` (`status: "pending"`, `install_kind: "managed"`) |
| `POST /api/v1/admin/plugins/{id}/update` | `{ "tag"?: String }` (body optional) | `200 { "updated": bool, "previous_version": String, "requires_approval": bool, "plugin": PluginRecord }` (`400 INVALID_REQUEST` if `install_kind != "managed"`) |

Existing routes (`GET/POST /api/v1/admin/plugins`, `approve`, `connection`, `token`, `enable`, `disable`, `DELETE`) preserve their existing request/response shapes, with `PluginRecord` serialized with the new metadata fields.

### 6.2 Error Mapping
| Condition | HTTP Status | Error Code |
|---|---|---|
| Malformed `repo_url`, unsupported URL scheme/IP, missing release/tag, no matching `<os>-<arch>` asset, oversized/malformed tar, symlink/path traversal, invalid manifest, or `update` called on an `external` plugin | `400 Bad Request` | `INVALID_REQUEST` |
| Plugin `id` already registered | `409 Conflict` | `CONFLICT` |
| Checksum mismatch or missing archive entry in checksum asset | `502 Bad Gateway` | `CHECKSUM_MISMATCH` |
| Upstream forge 5xx or invalid forge API response | `502 Bad Gateway` | `FORGE_ERROR` |
| Forge metadata or download request timed out | `504 Gateway Timeout` | `FORGE_TIMEOUT` |

### 6.3 Web UI (`web/src/components/admin/`)
- **`RegisterPluginForm.tsx`:** Adds a two-mode segmented toggle:
  1. **Install from Git** *(default)*: Repository URL input (`https://github.com/owner/repo`, GitLab, or Gitea URL), optional release tag input, and **Install** button (`Installing…` loading state). On `201`, `PluginsPanel` refreshes the list and immediately opens `ApprovePluginDialog` for the newly installed plugin.
  2. **External URL**: Existing `base_url` registration input and **Register** button.
- **`PluginList.tsx`:**
  - Managed plugins display a `Managed · <installed_tag>` badge and link to `repo_url` instead of showing `http://127.0.0.1:<port>`.
  - Managed plugins render an **Update** button (calling `POST /api/v1/admin/plugins/{id}/update`):
    - `updated == false`: info/success banner `"<name> is already up to date (<version>)."`.
    - `updated == true && !requires_approval`: success banner `"Updated <name> from <previous_version> to <version>."`.
    - `updated == true && requires_approval`: warning banner + automatically opens `ApprovePluginDialog` so the admin can review the newly requested permissions.
  - Managed plugins omit the **Connection details** button since `KADR_*` credentials are injected automatically by `PluginSupervisor`.
- **`RemovePluginDialog.tsx`:** When `plugin.install_kind === 'managed'`, updates the purge checkbox label to `"Also delete this plugin's local data directory, virtual libraries, and items"`.

---

## 7. Testing & Documentation

### 7.1 Backend Tests
- **Unit (`forge.rs`, `installer.rs`, `plugin.rs`):**
  - GitHub, GitLab (2-segment and nested `group/sub/repo`), Codeberg/Gitea, and self-hosted Gitea→GitLab fallback URL resolution.
  - Canonical and alias `<os>-<arch>.tar.gz` asset selection; clear errors on missing or ambiguous assets.
  - SHA-256 verification pass/fail across `.sha256`, `checksums.txt`, and `SHA256SUMS`.
  - Tar extraction guards: root vs. single-top-dir layouts, rejection of `..`, absolute paths, symlinks/hardlinks, compressed/uncompressed size caps, and staging directory cleanup on failure.
  - Reserved `"install"` and `"settings"` IDs in `PluginManifest::validate()`.
- **Storage (`plugin_repo_test.rs`, `migration_test.rs`):**
  - Migration `010_managed_plugins.sql` upgrades existing `external` rows cleanly and persists `install_kind`, `repo_url`, `forge_kind`, and `installed_tag`.
- **Integration (`plugin_install_test.rs`):**
  - In-process Axum mock forge serving GitHub, GitLab, and Gitea release JSON + dynamically built `.tar.gz` bundles containing a real lightweight test plugin executable (`allow_private_proxy = true` for loopback forge).
  - Verifies: `install` keeps plugin `pending` without executing binary → `approve` spawns child with injected `KADR_*` env vars (and stripped parent env), publishes virtual library items, and resolves playback → `disable` terminates child → `enable` respawns on a fresh port → `update` with unchanged permissions hot-restarts cleanly → `update` adding `playback:resolve` stops child and moves to `pending` (`requires_approval: true`) → `remove` with `purge=false` keeps `data/` while `purge=true` deletes `<data_dir>/plugins/<id>/` → `shutdown_managed_plugins()` reaps all children.

### 7.2 Frontend Tests (`PluginsPanel.test.tsx`, `PluginList.test.tsx`, `PluginDialogs.test.tsx`)
- Switching between **Install from Git** and **External URL** modes; submitting repo URL + optional tag; auto-opening `ApprovePluginDialog` on install.
- Rendering `Managed` vs `External` badges, hiding **Connection details** on managed plugins, and handling all three **Update** outcomes (`updated: false`, seamless update, and `requires_approval: true`).
- Updated purge label in `RemovePluginDialog` for managed plugins.

### 7.3 Documentation & Example
- Document the `.tar.gz` bundle layout, supported Git hosts, target naming table, and a ready-to-copy GitHub Actions / CI packaging snippet in `docs/PLUGIN_GUIDE.md`.
- Add packaging instructions to `examples/plugin-hello/README.md` showing how to bundle `manifest.json` + the compiled `plugin-hello` binary (renamed to `plugin`) into `plugin-hello-<os>-<arch>.tar.gz`.
