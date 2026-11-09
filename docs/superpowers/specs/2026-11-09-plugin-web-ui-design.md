# Plugin Management from the Web UI — Design

Date: 2026-11-09
Status: Draft, pending user review
Builds on: `2026-11-07-plugin-framework-design.md` (merged)

## 1. Goal and scope

Let an admin enable and fully manage plugins from the web UI, so no config file needs editing.

**In scope**
- Persist plugin settings in the database; they override `kadr.toml`.
- Settings API that works while plugins are disabled.
- Connection-details API (secret, fresh token) for the plugin owner to configure the plugin process.
- A "Plugins" admin tab: enable switch, settings, register by URL, permission-review approval, enable/disable, remove (with purge option), connection details.

**Out of scope**
- Automatic push handshake from Kadr to the plugin (secret/token are copied by the admin).
- Plugin-defined UI widgets.
- Editing other `kadr.toml` settings from the UI.
- Removing or changing `kadr.toml` support: it remains the default for fresh installs.

## 2. Persistence

Migration `008_settings.sql`:

```sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

`SettingsRepository` (kadr-storage): `get(&str) -> Option<String>`, `set(&str, &str)`, `get_all() -> HashMap<String, String>`.

Keys: `plugins.enabled` (`"true"`/`"false"`), `plugins.resolve_timeout_secs` (integer string), `plugins.allow_private_proxy` (`"true"`/`"false"`).

Precedence at startup: a stored value overrides the `kadr.toml` value; keys that are not stored fall back to `kadr.toml`, then to the built-in defaults. Unparseable stored values are ignored with a warning (the fallback applies).

## 3. API

All routes require an admin token (`RequireAdmin`).

| Route | Behavior |
|---|---|
| `GET /api/v1/admin/plugins/settings` | `{ enabled, resolve_timeout_secs, allow_private_proxy }` from the live config. Works while plugins are disabled. |
| `PUT /api/v1/admin/plugins/settings` | Partial body. Validates `resolve_timeout_secs` in `1..=60` (otherwise `400 INVALID_REQUEST`). Persists every supplied key, updates the in-memory config, returns the full settings. Works while plugins are disabled. |
| `GET /api/v1/admin/plugins/{id}/connection` | `{ base_url, secret }`; the secret is the derived server-to-plugin secret. Requires plugins enabled and the plugin to exist. |
| `POST /api/v1/admin/plugins/{id}/token` | Issues a fresh plugin token `{ token }`. Requires an approved plugin (`enabled`, `disabled` or `unhealthy`, not `pending`). |

Route ordering: `/api/v1/admin/plugins/settings` must not be captured by the `{id}` routes.

All existing plugin routes are unchanged. The earlier rule "the secret is returned once" is replaced: the secret is derived, so the admin can view it again. It is never logged.

## 4. Applying changes without a restart

- `enabled` and `allow_private_proxy` are already read from the live config on every request.
- `resolve_timeout_secs` is currently fixed when `PluginService` is constructed. `PluginClient` must read the timeout from a shared, updatable value (for example an `Arc<AtomicU64>` held by the client and updated by the settings handler) so a change applies to the next call.
- The health monitor is spawned unconditionally from `main.rs` and does nothing while plugins are disabled, so no restart is needed to start probing.
- On startup `main.rs` loads stored settings into `AppConfig.plugins` before creating the router and the monitor's `PluginService`.

## 5. Web UI

New component `web/src/components/admin/PluginsPanel.tsx`; `AdminDashboard.tsx` only adds `'plugins'` to `AdminTab`, a tab button, and renders the panel. `api/client.ts` and `types/` gain the plugin types and methods.

Layout, top to bottom:

1. **Master switch** "Enable plugins" with a one-line explanation. Toggling calls `PUT .../settings`, then loads the plugin list. While off, the rest of the panel is dimmed and states that plugins are disabled.
2. **Settings:** resolve timeout (number field, 1–60 s) and "Allow plugins to stream from private/LAN addresses" (toggle with a warning). Saved immediately, applied without restart.
3. **Register plugin:** URL field and a Register button. Errors from the API are shown verbatim.
4. **Plugin list:** name, version, status badge (`pending`, `enabled`, `disabled`, `unhealthy`), granted permissions. Actions by status:
   - `pending`: **Review & approve** opens a dialog with one checkbox per permission requested in the manifest (all checked by default). Nothing is granted until the admin confirms; the selection is sent as `permissions` to the approve endpoint.
   - `enabled` / `unhealthy`: **Connection details**, **Disable**.
   - `disabled`: **Enable**, **Connection details**.
   - **Remove** always available; its confirm dialog has an unchecked "Also delete this plugin's libraries and items" box mapped to `?purge=true`.
5. **Connection details dialog:** base URL and secret with copy buttons; **Generate token** shows a fresh token with a copy button; ready-to-paste `KADR_URL=`, `KADR_PLUGIN_SECRET=` and `KADR_PLUGIN_TOKEN=` lines.

Behavior:
- While the tab is open, plugin statuses refresh every 15 seconds.
- Status messages, loading states and styling follow the existing admin conventions.
- Error bodies use the existing structured `{ error, message }` shape.

## 6. Error handling

- Settings validation failures return `400 INVALID_REQUEST` with a message the UI shows inline.
- A database write failure on `PUT .../settings` returns `500` and leaves the in-memory config unchanged (persist first, then apply).
- `connection` and `token` for an unknown plugin return `404`; `token` for a pending plugin returns `400`; both return `404 PLUGINS_DISABLED` when plugins are disabled.
- The UI shows the `message` field of API errors and never blocks on a failed refresh (it keeps the last known list and shows a warning).

## 7. Testing

Backend (existing harness in `crates/kadr-server/tests/plugin_support`):
- Settings round trip through the API, persisted, and visible after the router and config are rebuilt from the same database.
- Stored value overrides the `kadr.toml` default; missing keys fall back; unparseable stored values are ignored.
- Validation bounds on `resolve_timeout_secs`.
- Settings routes work while plugins are disabled; other plugin admin routes still return `404 PLUGINS_DISABLED`.
- `connection` returns the same secret the registration returned; `token` returns a token that works on `/api/v1/plugin/*`; pending plugin gets 400; non-admin and anonymous get 403/401.
- A changed `resolve_timeout_secs` applies to the next resolve call without rebuilding the service.
- Route `/api/v1/admin/plugins/settings` is not captured as a plugin id.

Storage: `SettingsRepository` get, set (upsert), get_all; migration applies cleanly on an existing database.

Web (Vitest and React Testing Library, following `AdminDashboard.test.tsx`):
- The switch calls the settings API and then loads the list.
- Register, approve with a permission subset, enable/disable, remove with and without purge.
- Copying the secret, generating a token, and the disabled state.
- Error messages from the API are rendered.

## 8. Risks

- `PluginClient` timeout becoming dynamic touches a shared component; covered by the dynamic-timeout test and the existing client tests.
- Showing the derived secret to admins widens its exposure from "once" to "any time"; it remains admin-only and is never logged. An admin who leaks it can be handled by rotating the server secret (documented in `PLUGIN_GUIDE.md`).
- `AdminDashboard.tsx` stays untouched except for the tab wiring, to avoid growing a 1100-line file.
