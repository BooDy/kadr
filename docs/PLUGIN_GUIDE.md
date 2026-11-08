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

## Example

See [`examples/plugin-hello`](../examples/plugin-hello).
