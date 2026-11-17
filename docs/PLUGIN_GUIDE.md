# Kadr Plugin Guide

Kadr plugins are separate processes that talk to the server over HTTP. A plugin can:

- publish **virtual items** (items with no file on disk) into libraries it owns, and
- **resolve playback** for those items (`direct`, `proxy` or `transcode`).

Kadr supports two ways to run plugins (API version: `1`):

1. **Managed plugins (recommended):** Installed directly from a public **GitHub**, **GitLab**, or **Gitea / Forgejo** release archive. Kadr downloads the platform binary, waits for admin permission approval before executing it, allocates a loopback port, injects credentials automatically, restarts crashed processes with exponential backoff, and supports one-click updates.
2. **External plugins:** Started independently (e.g. local dev process, Docker container, or separate host) and registered with Kadr by `http://` or `https://` URL.

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
2. **install from Git** by pasting a GitHub, GitLab, or Gitea repository URL (plus optional release tag), or **register** an external plugin by HTTP URL;
3. **review and approve** the permissions the plugin requested (you can approve a subset — managed binaries are never executed before approval);
4. **update** a managed plugin with one click (if a newer release requests additional permissions, Kadr pauses the plugin in `pending` until re-approved);
5. **enable / disable** an approved plugin, or **remove** it (optionally deleting its local data directory, virtual libraries, and items);
6. for external plugins, open **Connection details** to copy the plugin's secret, generate a token, and copy ready-made `KADR_URL`, `KADR_PLUGIN_SECRET` and `KADR_PLUGIN_TOKEN` lines.
   *(Note: `KADR_URL` in Connection details is built from `window.location.origin`; managed plugins automatically receive Kadr's local loopback URL.)*

Plugin statuses refresh every 15 seconds while the tab is open, and the server-side health monitor probes every 30 seconds.

## Packaging for Git Installation

To make your plugin installable via **Install from Git** (`POST /api/v1/admin/plugins/install`), publish a release on a public **GitHub** (`github.com/<owner>/<repo>`), **GitLab** (`gitlab.com/<group>/.../<repo>` or self-hosted), or **Gitea / Forgejo** (`codeberg.org/<owner>/<repo>` or self-hosted) repository.

### 1. Release asset naming

Attach one `.tar.gz` archive per supported target named `<name>-<os>-<arch>.tar.gz`:

| Target | Canonical suffix | Accepted alias suffixes |
|---|---|---|
| Linux x86_64 | `-linux-x86_64.tar.gz` | `-linux-amd64.tar.gz` |
| Linux ARM64 | `-linux-aarch64.tar.gz` | `-linux-arm64.tar.gz` |
| macOS Apple Silicon | `-macos-aarch64.tar.gz` | `-darwin-arm64.tar.gz`, `-macos-arm64.tar.gz`, `-darwin-aarch64.tar.gz` |
| macOS Intel | `-macos-x86_64.tar.gz` | `-darwin-x86_64.tar.gz`, `-darwin-amd64.tar.gz`, `-macos-amd64.tar.gz` |

Archive size limits: **100 MiB** compressed, **250 MiB** uncompressed. Symlinks, hardlinks, FIFOs, device nodes, and `..` / absolute paths are rejected.

### 2. Archive layout & optional SHA-256 checksums

Place `manifest.json` and the compiled executable named `plugin` (`plugin.exe` on Windows) either at the archive root or inside a single top-level directory. Any optional static asset subdirectories (such as `web/dist/...`) are preserved alongside the plugin so both `./web/dist` and `<exe_dir>/web/dist` resolve automatically:

```text
manifest.json
plugin
web/dist/...        # optional static UI assets
```

Optionally attach `<archive>.sha256`, `checksums.txt`, or `SHA256SUMS` (`sha256sum` format, max 64 KiB). When present, Kadr verifies the archive's SHA-256 digest before unpacking.

### 3. Managed process environment

When Kadr starts an approved managed plugin, it clears inherited server environment variables (`env_clear()`), passes only safe OS variables (`PATH`, `LANG`, `LC_ALL`, `TZ`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, `SYSTEMROOT`), sets the working directory to `<data_dir>/plugins/<id>`, and injects:

| Variable | Value |
|---|---|
| `PLUGIN_BIND` | `127.0.0.1:<ephemeral_port>` — address your HTTP server must bind to |
| `KADR_URL` | Local Kadr base URL (e.g. `http://127.0.0.1:8492`) |
| `KADR_PLUGIN_SECRET` | Derived bearer secret Kadr sends on `/health` and `/playback/resolve` |
| `KADR_PLUGIN_TOKEN` | Scoped bearer JWT your plugin sends to `/api/v1/plugin/*` |
| `PLUGIN_DATA_DIR` | Persistent writable directory (`<data_dir>/plugins/<id>/data`), preserved across updates |

### 4. Example GitHub Actions release job

```yaml
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - run: |
          cp target/release/my-plugin plugin
          tar -czf my-plugin-linux-x86_64.tar.gz manifest.json plugin
          sha256sum my-plugin-linux-x86_64.tar.gz > checksums.txt
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            my-plugin-linux-x86_64.tar.gz
            checksums.txt
```

## Manifest

Serve `GET <base_url>/manifest.json` (for external plugins) and include `manifest.json` in release archives (for managed plugins):

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

`id` must match `[a-z0-9-]{1,32}` and be unique. The IDs `settings` and `install` are reserved.

## Registration, installation, and approval (admin)

| Call | Result |
|---|---|
| `GET /api/v1/admin/plugins/settings` | `{ enabled, resolve_timeout_secs, allow_private_proxy }`; works while plugins are disabled |
| `PUT /api/v1/admin/plugins/settings` | partial body; validates `resolve_timeout_secs` (1-60); persists and applies immediately; works while plugins are disabled |
| `POST /api/v1/admin/plugins/install` `{"repo_url": "https://...", "tag"?: "...", "forge"?: "github\|gitlab\|gitea"}` | `201 { plugin }`; downloads & unpacks release archive as `pending` (`install_kind: "managed"`) without executing the binary |
| `POST /api/v1/admin/plugins/{id}/update` `{"tag"?: "..."}` (body optional) | `200 { updated, previous_version, requires_approval, plugin }`; updates a managed plugin in place |
| `POST /api/v1/admin/plugins` `{"base_url": "http://host:port"}` | `201 { plugin, secret }`; registers an external plugin as `pending` |
| `POST /api/v1/admin/plugins/{id}/approve` `{"permissions": [...]}` (body optional) | `{ plugin, token }`; grants requested permissions (or a subset), enables the plugin, and spawns managed plugins |
| `GET /api/v1/admin/plugins/{id}/connection` | `{ base_url, secret }` for configuring an external plugin process |
| `POST /api/v1/admin/plugins/{id}/token` | `{ token }`: issues a fresh plugin-to-server token (approved plugins only) |
| `POST /api/v1/admin/plugins/{id}/enable` / `disable` | toggles a previously approved plugin (spawning or stopping managed plugins) |
| `DELETE /api/v1/admin/plugins/{id}?purge=true` | stops & removes the plugin; `purge` also deletes its local `data/` directory, virtual libraries, and items |
| `GET /api/v1/admin/plugins` | lists plugins |

- `secret` is what **Kadr sends to the plugin**: `Authorization: Bearer <secret>` on `/health` and `/playback/resolve`. The plugin must reject calls without it. `secret` is derived from the server secret, never stored, and can be viewed again at any time by an admin (web UI: Connection details, or `GET .../connection`).
- `token` is what **the plugin sends to Kadr** on `/api/v1/plugin/*` calls. It is not a user token and cannot be used on any other route.

## Publishing (plugin to Kadr)

All calls need `Authorization: Bearer <token>` and the `items:publish` permission.

- `POST /api/v1/plugin/libraries` `{"id"?: "...", "name": "...", "media_type"?: "live_channel"}` returns `201`.
- `PUT /api/v1/plugin/libraries/{id}/items` `{"items": [...]}` upserts up to 1000 items per call.
- `DELETE /api/v1/plugin/libraries/{id}/items/{remote_id}` returns `204`. Percent-encode `/` inside `remote_id`.

Item fields: `remote_id`, `title` (required); `item_type`, `original_title`, `release_year`, `duration_seconds`, `live`, `seekable`, `resolution`, `video_codec`, `audio_codec`, `container`, `poster_url`, `backdrop_url`, `overview`, `genres`.

A plugin can only touch libraries it created. Live items are detected via an in-memory session flag set at session creation: live items never resume and never persist watch progress regardless of duration.

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

Three consecutive failures open a per-plugin circuit for 30 seconds; in the circuit breaker's half-open state, a single probe is admitted to test recovery. A background health monitor is started from `main.rs` (not from the router builder) and probes every 30 seconds; a failing plugin becomes `unhealthy` and a `plugin:error` event is published on `/api/v1/events`.

## Security notes

- Managed plugin binaries are **never executed** during `install`; they run only after an admin explicitly approves permissions.
- Forge release discovery and archive downloads require `https://` and reject private/loopback IP addresses at every redirect hop (max 5 hops) unless `allow_private_proxy = true` (which permits self-hosted LAN Gitea/GitLab instances).
- Descriptor URLs must be `http` or `https`. For `proxy` and `transcode`, hosts that resolve to loopback, private, link-local, CGNAT addresses, all of `0.0.0.0/8`, or IPv4-compatible/mapped IPv6 representations of private ranges are rejected unless `allow_private_proxy = true`.
- Known limitations: DNS is resolved again when Kadr or ffmpeg connects, and ffmpeg follows redirects internally. Only install or register plugins you trust, and keep `allow_private_proxy = false` unless a plugin or self-hosted Git forge lives on your LAN.
- Private (PIN-locked) libraries are checked before any resolve call, so a plugin is never asked about an item the user has not unlocked.
- Plugins never receive user credentials, only an opaque user id.
- The plugin secret is derived from Kadr's server secret. If it leaks, rotate the server secret: stop Kadr, delete the `jwt.secret` file in the data directory (`server.data_dir`, default `./data`; find the exact path in the startup log line "Generated and saved new JWT secret"), and start Kadr again. This invalidates every user session, plugin token and derived plugin secret.
- The plugin IDs `settings` and `install` are reserved.

## Example

See [`examples/plugin-hello`](../examples/plugin-hello).
