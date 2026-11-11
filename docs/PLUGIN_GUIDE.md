# Kadr Plugin Guide

Kadr plugins are separate processes that talk to the server over HTTP. A plugin can:

- publish **virtual items** (items with no file on disk) into libraries it owns, and
- **resolve playback** for those items (`direct`, `proxy` or `transcode`).

The server never starts or stops plugins. API version: `1`.

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
   *(Note: `KADR_URL` is built from the address you opened Kadr at in your browser (`window.location.origin`); plugins running elsewhere may need a different URL.)*

Plugin statuses refresh every 15 seconds while the tab is open, and the server-side health monitor probes every 30 seconds.

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
| `GET /api/v1/admin/plugins/settings` | `{ enabled, resolve_timeout_secs, allow_private_proxy }`; works while plugins are disabled |
| `PUT /api/v1/admin/plugins/settings` | partial body; validates `resolve_timeout_secs` (1-60); persists and applies immediately; works while plugins are disabled |
| `POST /api/v1/admin/plugins` `{"base_url": "http://host:port"}` | `201 { plugin, secret }`; plugin is `pending` |
| `POST /api/v1/admin/plugins/{id}/approve` `{"permissions": [...]}` (body optional) | `{ plugin, token }`; grants the requested permissions (or a subset) and enables it |
| `GET /api/v1/admin/plugins/{id}/connection` | `{ base_url, secret }` for configuring the plugin process |
| `POST /api/v1/admin/plugins/{id}/token` | `{ token }`: issues a fresh plugin-to-server token (approved plugins only) |
| `POST /api/v1/admin/plugins/{id}/enable` / `disable` | toggles a previously approved plugin |
| `DELETE /api/v1/admin/plugins/{id}?purge=true` | removes the plugin; `purge` also deletes its libraries and items |
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

- Descriptor URLs must be `http` or `https`. For `proxy` and `transcode`, hosts that resolve to loopback, private, link-local, CGNAT addresses, all of `0.0.0.0/8`, or IPv4-compatible/mapped IPv6 representations of private ranges are rejected unless `allow_private_proxy = true`.
- Known limitations: DNS is resolved again when Kadr or ffmpeg connects, and ffmpeg follows redirects internally. Only register plugins you trust, and keep `allow_private_proxy = false` unless a plugin must reach your LAN.
- Private (PIN-locked) libraries are checked before any resolve call, so a plugin is never asked about an item the user has not unlocked.
- Plugins never receive user credentials, only an opaque user id.
- The plugin secret is derived from Kadr's server secret. If it leaks, rotate the server secret: stop Kadr, delete the `jwt.secret` file in the data directory (`server.data_dir`, default `./data`; find the exact path in the startup log line "Generated and saved new JWT secret"), and start Kadr again. This invalidates every user session, plugin token and derived plugin secret, so users must sign in again and plugins must be given their new secret and a fresh token from **Connection details**.
- The plugin id `settings` is reserved.

## Example

See [`examples/plugin-hello`](../examples/plugin-hello).
