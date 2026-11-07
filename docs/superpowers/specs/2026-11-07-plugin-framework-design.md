# Plugin Framework (Sub-project 1) — Design

Date: 2026-11-07
Status: Draft, pending user review

## 1. Goal and scope

Let Kadr host out-of-process plugins that publish **virtual media items** (items with no file on disk) and **resolve playback** for them. This is the foundation for later IPTV and music-streaming plugins, which are separate specs.

**In scope**
- Plugin registry, manifest, health checks, admin-approved permissions.
- Virtual libraries and virtual items (items with a `plugin://` path).
- Playback resolution (`direct`, `proxy`, `transcode`) reusing the HLS pipeline.
- Plugin→server scoped tokens and server→plugin shared secret.
- A minimal example plugin used as a test fixture and template.

**Out of scope**
- Admin UI screens (registration is through the admin API in v1).
- Event webhooks, UI widgets, metadata/subtitle provider traits, secrets store.
- The IPTV and music plugins themselves.
- Server-supervised plugin processes (the server never starts or stops plugins).

## 2. Transport and trust model

- Plugins are separate processes reachable over HTTP. The server calls them; they call back into the Kadr API.
- An **admin registers a plugin by URL** via `POST /api/v1/admin/plugins`. The server fetches `GET <base_url>/manifest.json`, validates it, and stores the plugin as `pending`. The admin then approves the requested permissions (`POST /api/v1/admin/plugins/:id/approve`), moving it to `enabled`.
- Global plugin settings live in `kadr.toml` under `[plugins]`: `enabled` (default `false`), `resolve_timeout_secs` (default 5), `allow_private_proxy` (default `false`). Individual plugins are not listed in `kadr.toml`.
- **Server→plugin auth:** `Authorization: Bearer <shared secret>`. The secret is derived, not stored: `base64url(HMAC-SHA256(server_secret, "kadr-plugin-secret-v1:" + plugin_id))`. It is returned to the admin in the registration response to configure in the plugin. The server needs the raw value for every outbound call, which is why it is derived instead of hashed and stored.
- **Plugin→server auth:** scoped JWT with `sub = plugin:<id>` and a distinct claim type, issued on approval. It is rejected on all non-plugin routes, and user tokens are rejected on plugin routes.

## 3. Components

New `plugins/` module in `kadr-server`; shared types in `kadr-core`.

- **`PluginRegistry`**: CRUD over the `plugins` table (id, name, base_url, manifest JSON, granted permissions, status `pending|enabled|disabled|unhealthy`, created_at).
- **`PluginClient`**: `reqwest` wrapper with timeouts, bearer auth, and a circuit breaker (opens after N consecutive failures, half-opens after a cool-down).
- **`PluginHealth`**: background task polling `GET /health`; marks plugins `unhealthy` / back to `enabled`. Items of an unhealthy plugin stay listed but are flagged unavailable.

### Manifest (`GET /manifest.json`)
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
Registration fails if `api_version` is unsupported or `id` collides with an existing plugin.

### v1 API
Plugin → server (plugin token required):
- `POST /api/v1/plugin/libraries` — create a virtual library owned by the calling plugin.
- `PUT /api/v1/plugin/libraries/:id/items` — batch upsert items keyed by `remote_id`.
- `DELETE /api/v1/plugin/libraries/:id/items/:remote_id` — remove an item.

Server → plugin (shared secret):
- `GET /health`
- `POST /playback/resolve` — body: `{ remote_id, user_id (opaque), client: { codecs, containers, max_height } }`; response: playback descriptor (see §5).

## 4. Data model

Migration `007_plugins.sql`:
- New `plugins` table (as above).
- `libraries`: add `plugin_id TEXT NULL` (no foreign key, so removing a plugin without purge leaves its libraries in place). Non-null marks a virtual library. Virtual libraries get a synthetic `path` of `plugin://<plugin_id>/library/<library_id>`. The watcher skips them because the path does not exist on disk.
- `media_items` is **not** altered. A plugin item is identified by its synthetic `file_path` of `plugin://<plugin_id>/<remote_id>`, which keeps the existing `UNIQUE NOT NULL` constraint and the existing `ON CONFLICT(file_path)` upsert working unchanged. `remote_id` is therefore unique per plugin, not per library.
- Existing rows are unchanged.

`kadr-core` model changes:
- `MediaType`: add `LiveChannel`.
- **`MediaItem` and `Library` get no new fields.** Both are built with exhaustive struct literals in roughly 47 places (mostly tests), so new fields would force edits across the codebase for no benefit. Instead, `MediaItem` gains derived accessors: `plugin_ref()` parses the `plugin://` path, and `is_live()` / `is_seekable()` read the `live` / `seekable` booleans from `metadata.extra`.

## 5. Playback resolution

```rust
enum PlaybackSource {
    File(PathBuf),
    Url { url: String, headers: HashMap<String, String> },
}
```

The plugin's resolve response selects a mode:
- `direct`: server returns the URL and the client plays it. Nothing is proxied.
- `proxy`: server fetches the URL and streams it to the client.
- `transcode`: server feeds the URL to the existing HLS pipeline (`ffmpeg -i <url>`).

Descriptors carry `expires_at`, are cached in memory until then, and are never persisted.

### Changes to existing code
- `streaming/handler.rs`: branch on `item.plugin_ref()`; plugin items go through resolve, then redirect (`direct`), proxy (`proxy`), or a `409 PLAYBACK_REQUIRES_TRANSCODE` hint pointing at the HLS endpoint (`transcode`).
- `api/hls_routes.rs` and the transcode builder: add `build_ffmpeg_hls_args_for_source(&PlaybackSource, ...)` and `HlsSessionManager::get_or_create_session_for_source(...)`. The existing `Path`-based functions stay as thin wrappers, so existing callers and tests are untouched. Only `proxy` and `transcode` descriptors (which are SSRF-validated) are fed to ffmpeg. A `direct` descriptor gets `409 PLAYBACK_DIRECT_ONLY` with the URL.
- Playback sessions: when `capabilities.live` is true, skip resume positions and scrobble thresholds (`evaluate_scrobble`).
- Subtitle, artwork and sidecar lookups skip plugin items. Plugins may supply artwork URLs in item metadata.
- Private/locked library checks run **before** any resolve call.

## 6. Permissions

- `items:publish`: create and manage the plugin's own virtual libraries and items only. It can never read or modify file-based items or other plugins' items.
- `playback:resolve`: the server may call the plugin's resolve endpoint. Without it, the plugin is never asked to resolve.
- Plugins never receive user credentials; resolve requests carry only an opaque user ID and client capabilities.
- Removing a plugin revokes its token. With data purge it deletes its virtual libraries and items, cascading to playback state. Disabling keeps data and marks items unavailable.

## 7. Error handling

- Resolve timeout from `resolve_timeout_secs`; one retry for idempotent calls; circuit breaker after N consecutive failures.
- Error mapping: `503 PLUGIN_UNAVAILABLE`, `502 PLUGIN_BAD_RESPONSE`, `504 PLUGIN_TIMEOUT`.
- `direct` descriptors must not carry headers (the server cannot attach headers to a redirect). Header names and values must not contain CR, LF or NUL.
- Proxy mode does not follow upstream redirects (a redirect is a `502 PLUGIN_BAD_RESPONSE`), to avoid redirect-based SSRF bypass. ffmpeg follows redirects internally, and DNS is re-resolved at connect time. These are documented limitations of `transcode` mode.
- Descriptor URLs must be `http` or `https`. In `proxy` and `transcode` modes, loopback and private address ranges are blocked unless `allow_private_proxy = true` (SSRF protection).
- Plugin errors are logged with the plugin id and published on the SSE event bus as admin-visible events.

## 8. Testing

- Unit: manifest parsing, descriptor validation, circuit breaker, `PlaybackSource` conversions.
- Migration: existing databases migrate cleanly and file items behave identically (the `libraries.plugin_id` column defaults to NULL).
- Integration (style of existing `*_routes_test.rs`), using an in-process axum mock plugin: register and approve, publish items, list a virtual library, resolve in `direct`, `proxy` and `transcode` modes, and the 502/503/504 paths.
- Regression: existing streaming, HLS (including `tests/e2e_hls_transcoding_test.rs`) and library tests pass unchanged after the `PlaybackSource` refactor.
- Permissions: plugin tokens rejected on user routes and vice versa; plugin cannot touch other libraries; locked libraries never trigger a resolve call; SSRF blocking.

## 9. Example plugin

`examples/plugin-hello/`: a small standalone binary that serves a manifest, publishes three fake items, and resolves them to a public test video URL. It serves as a test fixture and as the template for the IPTV spec.

## 10. Risks

- Migration is the only change to stored data; it is additive with defaults.
- Any code path that assumes `file_path` exists on disk must be audited. Known sites: `streaming/handler.rs`, `hls_routes.rs`, subtitle routes, artwork routes, `resolver.rs`.
- The `Path` to `PlaybackSource` change touches the transcode builder signature; covered by existing HLS tests.
