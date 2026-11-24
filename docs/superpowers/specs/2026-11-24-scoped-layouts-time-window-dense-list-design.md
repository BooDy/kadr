# Scoped Layouts, `time_window` Shelves & `dense_list` Widget Design (`v0.4` Part 3)

**Date:** 2026-11-24  
**Status:** Approved

---

## 1. Overview & Goals

Complete the **`v0.4` Declarative Layout & Personalization** milestone by delivering PRD §3.1.2, §3.1.3, §3.2.1, and §3.2.3 across Kadr's Rust core, SQLite storage, Axum server, and Web UI:

1. **Part 1 (shipped):** Recursive Filter AST (`AND` / `OR` / `NOT`), Parameterized SQL Compiler, Facets & Live Preview APIs, and Studio Rule Builder.
2. **Part 2 (shipped):** Per-User Favorites / Watchlist (`user_favorites`), Heart Toggles, Dedicated `Favorites` View, Default `Home` Rail, and `QueryMacro::Favorites` + `FilterCondition::IsFavorite`.
3. **Part 3 (this spec):**
   - **Per-User & Per-Device Scoped Layouts (`(user_id, target_device)`)** with a deterministic 4-tier fallback chain and personal Layout Studio access for both `Standard` and `Admin` profiles.
   - **Time-Window Shelf Visibility (`time_window: { start: "HH:MM", end: "HH:MM" }`)** supporting both same-day and overnight (`23:00` $\to$ `04:00`) schedules evaluated against client local time without timezone database bloat.
   - **`DenseList` (`dense_list`) Widget Presentation** rendering compact cinema rows with artwork thumbnail, title/subtitle, formatted runtime (`CardViewModel.duration_seconds`), watch progress bar, and inline quick actions.

### 1.1 Primary Goals

- **Independent Household & Device Customization**: Allow layouts to target `(user_id: Option<String>, target_device: TargetDevice)` across four device tiers (`all`, `tv`, `web`, `mobile`) with fallback precedence:
  1. `(Some(user_id), device)` *(when `device != All`)*
  2. `(Some(user_id), All)`
  3. `(None /* Global */, device)` *(when `device != All`)*
  4. `(None /* Global */, All)` *(existing `data/screens/*.json` / `config/screens/*.toml` override or factory default)*
- **PRD §2.3 RBAC Parity**:
  - `Standard` users can inspect, preview (`POST /api/v1/widgets/preview`), save, and reset **their own** personal layouts (`scope_user_id = self`) across any `target_device` via a top-bar **`Studio`** view, while Global layouts and other users' layouts remain strictly `403 Forbidden`.
  - `Admin` users can inspect, author, and reset **Global** layouts as well as **any user profile's** layouts across all device targets.
- **Zero-Bloat Time-Aware Shelves**: Skip SQL hydration and omit inactive `time_window` widgets during normal `GET /api/v1/screens/{id}` hydration (`?local_time=HH:MM` or `X-Kadr-Local-Time`, falling back to server local wall-clock `HH:MM`), while keeping all widgets intact on `?unhydrated=true` and providing a **Simulate Time** clock control in Layout Studio.
- **Zero-Extra-Roundtrip `DenseList` Runtime**: Hydrate `CardViewModel.duration_seconds` (`Option<u64>`, omitted when `None`/`0`) during standard batch card hydration so `<DenseListWidget />` displays formatted runtimes (`1h 54m`, `42m`) alongside progress bars and quick controls without additional requests.

### 1.2 Architectural Invariants Preserved

- **Memory, Latency & Portability Budget**: Zero new Cargo or npm dependencies; static `x86_64-unknown-linux-musl` portability preserved; `LayoutRegistry` caches scoped overrides in memory so `GET /api/v1/screens/{id}` performs **zero extra SQLite queries** to resolve the 4-tier fallback chain.
- **Backwards Compatibility**: Existing `config/screens/*.toml` and `data/screens/*.json` files remain the canonical persistence layer for Global + `All` (`(None, TargetDevice::All)`) screens; existing clients calling `GET /api/v1/screens/home` without `target_device` or `local_time` parameters continue working unchanged.
- **Referential Integrity & Fail-Closed Privacy**: `layout_overrides.user_id` references `users(id) ON DELETE CASCADE` so deleting a user profile automatically purges their personal layout overrides; `POST /api/v1/widgets/preview` enforces `UnlockedLibraries` so `Standard` users never preview items from locked private libraries without a valid unlock token.

### 1.3 Non-Goals

- Arbitrary per-hardware-instance IDs (e.g., MAC-address-keyed layouts); device scoping uses the 4 semantic PRD tiers (`all`, `tv`, `web`, `mobile`).
- Per-user custom screen creation (`POST /api/v1/screens` creating brand-new top-level custom screen slugs remains `Admin`-only; `Standard` users customize/override any registered screen such as `home`, `movies`, `shows`, or library screens for their profile).

---

## 2. Core AST & Event Model (`crates/kadr-core`)

### 2.1 `TargetDevice`, `TimeWindow`, and `ResolvedLayoutScope` (`crates/kadr-core/src/ast.rs`)

1. **`TargetDevice`:**
   ```rust
   #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
   #[serde(rename_all = "snake_case")]
   pub enum TargetDevice {
       #[default]
       All,
       Tv,
       Web,
       Mobile,
   }
   ```
   Implement `Display` (`"all"`, `"tv"`, `"web"`, `"mobile"`) and `FromStr` (case-insensitive, returning `Result<TargetDevice, String>` so invalid query strings return `400 Bad Request`).

2. **`TimeWindow`:**
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub struct TimeWindow {
       /// 24-hour "HH:MM" start time (inclusive), 00:00..=23:59.
       pub start: String,
       /// 24-hour "HH:MM" end time (exclusive, except when start == end which is rejected), 00:00..=23:59.
       pub end: String,
   }
   ```
   Validation & evaluation helpers on `TimeWindow` (and standalone `parse_hhmm_minutes(s: &str) -> Result<u16, String>`):
   - Strictly require 5-byte ASCII `"HH:MM"` where `HH` $\in 0..=23$ and `MM` $\in 0..=59$ (`0..=1439` minutes from midnight).
   - `validate(&self) -> Result<(), String>`: validates both `start` and `end` and rejects `start == end` (`"Time window start and end must differ"`).
   - `is_active_at_minutes(&self, current_minutes: u16) -> bool`:
     - Let `s = start_mins`, `e = end_mins`, `m = current_minutes % 1440`.
     - **Same-day window (`s < e`, e.g. `08:00`–`12:00`):** active when `m >= s && m < e`.
     - **Overnight wraparound window (`s > e`, e.g. `23:00`–`04:00`):** active when `m >= s || m < e`.

3. **`CardViewModel.duration_seconds`:**
   Add to `CardViewModel`:
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub duration_seconds: Option<u64>,
   ```
   Populated in `crates/kadr-server/src/resolver/card.rs` (`to_card_view_model_with_favorite`) as `(item.technical.duration_seconds > 0).then_some(item.technical.duration_seconds)`.

4. **`WidgetNode` Extensions:**
   - Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub time_window: Option<TimeWindow>` to `HeroBanner`, `Carousel`, `Grid`, and the new `DenseList` variant:
     ```rust
     DenseList {
         id: String,
         title: String,
         binding: WidgetQueryBinding,
         #[serde(default, skip_serializing_if = "Option::is_none")]
         time_window: Option<TimeWindow>,
         #[serde(default, skip_serializing_if = "Option::is_none")]
         items: Option<Vec<CardViewModel>>,
         #[serde(default, skip_serializing_if = "Option::is_none")]
         next_cursor: Option<String>,
         #[serde(default, skip_serializing_if = "Option::is_none")]
         total_count: Option<u64>,
     },
     ```
   - Extend `WidgetNode::id(&self)`, `WidgetNode::binding(&self)`, `WidgetNode::is_hydrated(&self)`, and add `WidgetNode::time_window(&self) -> Option<&TimeWindow>` and `WidgetNode::strip_hydrated_data(&mut self)`.

5. **`ResolvedLayoutScope` on `ScreenLayout`:**
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub struct ResolvedLayoutScope {
       /// Effective owner of the resolved layout (`None` = global, `Some(uid)` = per-user).
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub user_id: Option<String>,
       /// Effective device tier that matched during resolution.
       pub target_device: TargetDevice,
       /// True when an explicit override exists at the requested `(scope_user_id, target_device)` slot
       /// (or when a non-factory override matched during normal browsing).
       pub is_exact_override: bool,
   }
   ```
   Attach to `ScreenLayout` as:
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub resolved_scope: Option<ResolvedLayoutScope>,
   ```
   Cleared (`None`) inside `strip_hydrated_data` before persisting `ScreenLayout` JSON to disk or SQLite so stored layout JSON remains pure and portable.

### 2.2 `SystemEvent::LayoutChanged` (`crates/kadr-core/src/events.rs`)

Extend `SystemEvent::LayoutChanged` with optional scope metadata (defaulting to `None` and skipped when `None` for 100% wire compatibility with existing consumers and tests):

```rust
#[serde(rename = "layout:changed")]
LayoutChanged {
    screen_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_device: Option<TargetDevice>,
    timestamp: i64,
},
```

---

## 3. SQLite Storage & `LayoutOverrideRepository` (`crates/kadr-storage`)

### 3.1 Migration `016_scoped_layouts.sql`

Register `016_scoped_layouts.sql` in `crates/kadr-storage/src/migrations.rs` immediately after `015_user_favorites.sql`:

```sql
CREATE TABLE IF NOT EXISTS layout_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    screen_id TEXT NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    target_device TEXT NOT NULL CHECK (target_device IN ('all', 'tv', 'web', 'mobile')),
    layout_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_layout_overrides_scope
    ON layout_overrides(screen_id, IFNULL(user_id, ''), target_device);

CREATE INDEX IF NOT EXISTS idx_layout_overrides_user
    ON layout_overrides(user_id)
    WHERE user_id IS NOT NULL;
```

**Storage invariants:**
- **Global + `All` (`user_id IS NULL AND target_device = 'all'`)** continues to persist on disk in `data/screens/{screen_id}.json` (and load from `config/screens/*.toml|json` + `data/screens/*.json`), preserving existing homelab file workflows.
- **All other 7 scope combinations** — Global device overrides (`(None, Tv | Web | Mobile)`) and Per-User overrides (`(Some(uid), All | Tv | Web | Mobile)`) — persist in `layout_overrides`.
- When a `users` row is deleted, `ON DELETE CASCADE` automatically deletes all `layout_overrides` rows for that `user_id`.

### 3.2 `LayoutOverrideRepository` (`crates/kadr-storage/src/repos/layout_override_repo.rs`)

- `list_all(&self) -> Result<Vec<StoredLayoutOverride>>`: loads all rows at server startup to hydrate `LayoutRegistry`'s in-memory scoped map (logging and skipping any malformed JSON row without aborting startup).
- `upsert(&self, screen_id: &str, user_id: Option<&str>, target_device: TargetDevice, layout: &ScreenLayout, now_epoch: i64) -> Result<()>`: uses `INSERT INTO layout_overrides (...) VALUES (...) ON CONFLICT(screen_id, IFNULL(user_id, ''), target_device) DO UPDATE SET layout_json = excluded.layout_json, updated_at = excluded.updated_at`.
- `delete(&self, screen_id: &str, user_id: Option<&str>, target_device: TargetDevice) -> Result<bool>`: deletes matching row (`IS ?` / `IFNULL(user_id, '') = IFNULL(?2, '')`).
- `delete_all_for_screen(&self, screen_id: &str) -> Result<u64>`: called when an admin deletes a `ScreenId::Custom` screen altogether so no orphaned device/user overrides remain for a deleted custom screen.

---

## 4. `LayoutRegistry`, Fallback Resolution, Time-Window Hydration & API (`crates/kadr-server`)

### 4.1 Scoped `LayoutRegistry` (`crates/kadr-server/src/layout/registry.rs`)

Extend `LayoutRegistryInner`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutScopeKey {
    pub screen_id: ScreenId,
    pub user_id: Option<String>,
    pub target_device: TargetDevice,
}
```

- `screens: HashMap<ScreenId, ScreenLayout>` remains the `(None, TargetDevice::All)` baseline map (preserving every existing `get_screen`, `register_screen`, `save_screen`, `reset_screen`, and `load_overrides_from_dir` method signature!).
- Add `scoped_overrides: HashMap<LayoutScopeKey, ScreenLayout>` to `LayoutRegistryInner`.
- Add methods on `LayoutRegistry`:
  - `load_scoped_overrides(&self, entries: Vec<StoredLayoutOverride>)`
  - `set_scoped_override(&self, screen_id: ScreenId, user_id: Option<String>, target_device: TargetDevice, screen: ScreenLayout)`
  - `remove_scoped_override(&self, screen_id: &ScreenId, user_id: Option<&str>, target_device: TargetDevice) -> bool`
  - `remove_all_overrides_for_screen(&self, screen_id: &ScreenId)`
  - `resolve_scoped_screen(&self, canonical_id: &ScreenId, requested_user_id: Option<&str>, requested_device: TargetDevice) -> Option<ScreenLayout>`:
    Evaluates the 4-tier fallback chain in order:
    1. If `requested_user_id == Some(uid)` and `requested_device != TargetDevice::All`: check `(canonical_id, Some(uid), requested_device)`.
    2. If `requested_user_id == Some(uid)`: check `(canonical_id, Some(uid), TargetDevice::All)`.
    3. If `requested_device != TargetDevice::All`: check `(canonical_id, None, requested_device)`.
    4. Fallback to `self.get_screen(canonical_id)` (`(canonical_id, None, TargetDevice::All)`).
    Populates `screen.resolved_scope = Some(ResolvedLayoutScope { user_id: matched_user_id, target_device: matched_device, is_exact_override })`, where `is_exact_override` is `true` when the matched `(matched_user_id, matched_device)` equals `(requested_user_id, requested_device)` (and for `(None, All)`, `true` if overridden on disk or always `false` if factory default — track `disk_overridden: HashSet<ScreenId>` in `LayoutRegistryInner` so Studio knows whether Global+All itself has a saved disk override!).

### 4.2 Canonical Library Screen Lookup Before Scoped Resolution

In both `screen_routes::get_screen` and `widget_routes::get_widget_data`:
1. Resolve the requested `screen_id` to its canonical baseline `ScreenId` first (e.g., if `screen_id` is a library UUID whose library name is `"Movies"`, canonical `ScreenId` is `ScreenId::Movies`; if a new library has no baseline yet, register `default_library_layout`).
2. Call `registry.resolve_scoped_screen(&canonical_id, effective_user_id, target_device)` so scoped overrides saved under `"movies"` automatically apply whether the client requests `/api/v1/screens/movies` or `/api/v1/screens/{movies_library_uuid}`!

### 4.3 `TimeWindow` Filtering & Clock Resolution (`crates/kadr-server/src/resolver/resolver.rs`)

1. **Clock Resolution (`resolve_active_minutes`):**
   - Check query parameter `local_time` first, then HTTP header `X-Kadr-Local-Time`.
   - If present, parse with `parse_hhmm_minutes(val)` — if malformed (e.g., `"25:99"` or `"abc"`), return `400 Bad Request` `{ "error": "Invalid local_time '...': expected HH:MM (00:00..23:59)" }`.
   - If omitted, compute server local wall-clock minutes (`0..1439`) using `libc::localtime_r` on `#[cfg(unix)]` (with UTC `(epoch_secs / 60) % 1440` fallback on non-Unix).
2. **`WidgetResolver::resolve_screen_with_library_at`:**
   - Keep existing `resolve_screen_with_library(layout, user_id, unlocked_ids, default_library_id)` delegating to `resolve_screen_with_library_at(..., server_local_minutes())` so all existing tests compile untouched.
   - Before spawning concurrent widget queries (`join_all`), retain only widgets where `widget.time_window().map_or(true, |tw| tw.is_active_at_minutes(active_minutes))`. Inactive widgets are neither queried in SQLite nor emitted in the hydrated `ScreenLayout.widgets` vector!
   - Handle `WidgetNode::DenseList` identically to `WidgetNode::Grid` during hydration (`resolve_widget_data_with_library`, populating `items`, `next_cursor`, and `total_count`) and in `resolve_library_id_for_layout` / `LayoutRegistry::find_screen_for_library`.

### 4.4 REST API Contract (`crates/kadr-server/src/api/screen_routes.rs` & `widget_routes.rs`)

1. **`GET /api/v1/screens/{screen_id}`**
   - **Auth:** `AuthUser` + `UnlockedLibraries`.
   - **Query params (`ScreenQuery`):**
     - `unhydrated?: bool` (default `false`)
     - `target_device?: String` (optional; falls back to `X-Kadr-Device` header, then `"all"`; validated via `TargetDevice::from_str` $\to$ `400 Bad Request` on unknown value)
     - `local_time?: String` (optional; falls back to `X-Kadr-Local-Time` header, then server local clock; validated via `parse_hhmm_minutes` $\to$ `400 Bad Request` on invalid format)
     - `scope_user_id?: String` (optional Studio parameter: `"global"` means `None`, `"me"` means `Some(auth_user.id)`, or an explicit `<user_id>`):
       - When `scope_user_id` is omitted and `unhydrated == false` (normal client browsing), `requested_user_id` is `Some(auth_user.id.as_str())` so every caller resolves their personalized 4-tier fallback chain.
       - When `scope_user_id` is omitted and `unhydrated == true`, `requested_user_id` defaults to `None` (`"global"`) for `Admin` callers (preserving 100% backwards compatibility with existing `GET /api/v1/screens/{id}?unhydrated=true` callers/tests) and `Some(auth_user.id.as_str())` (`"me"`) for `Standard` callers.
       - When `scope_user_id` is explicitly provided and is neither `"me"` nor `auth_user.id`, require `auth_user.role == UserRole::Admin` (`403 Forbidden` if a `Standard` user attempts to inspect raw `"global"` admin scope or another user's layout).
   - **Behavior:**
     - When `unhydrated == true`: returns all widgets (no `time_window` filtering) plus `resolved_scope` so Layout Studio knows whether `(scope_user_id, target_device)` has an `is_exact_override` or is currently inheriting from a fallback tier.
     - When `unhydrated == false`: filters widgets by `time_window` at `active_minutes` and hydrates active widgets concurrently.

2. **`PUT /api/v1/screens/{screen_id}`**
   - **Auth:** `AuthUser` (replaces `RequireAdmin` extractor with explicit scope-aware RBAC inside the handler).
   - **Query params (`ScopeMutationQuery`):**
     - `scope_user_id?: String` (default `"global"` for backwards compatibility with existing Admin Studio & tests! Accepts `"global"`, `"me"`, or `<user_id>`).
     - `target_device?: TargetDevice` (default `TargetDevice::All`).
   - **RBAC Enforcement:**
     - Resolve `effective_user_id: Option<String>`:
       - `"global"` (or omitted) $\to$ `None` (**requires `auth_user.role == UserRole::Admin`**, else `403 Forbidden`).
       - `"me"` $\to$ `Some(auth_user.id.clone())` (allowed for both `Standard` and `Admin`).
       - `<other_uid>` $\to$ if `<other_uid> == auth_user.id`, `Some(auth_user.id.clone())`; else **requires `auth_user.role == UserRole::Admin`** AND verifies `user_repo.get_by_id(&other_uid)` exists (`404 Not Found` if user does not exist).
   - **Validation:**
     - Validate `screen_id`.
     - Validate every widget's `rule_tree` (`FilterNode::validate()`) AND every widget's `time_window` (`TimeWindow::validate()`) $\to$ `400 Bad Request` on invalid AST or invalid `HH:MM` / `start == end`.
     - Strip hydrated data (`widget.strip_hydrated_data()`) and clear `screen.resolved_scope = None`.
   - **Persistence & Broadcast:**
     - If `(effective_user_id, target_device) == (None, TargetDevice::All)`: save to `data/screens/{canonical_id}.json` via `registry.save_screen(...)`.
     - Otherwise: persist to SQLite via `layout_override_repo.upsert(...)` AND update `registry.set_scoped_override(...)`.
     - Publish `SystemEvent::LayoutChanged { screen_id: canonical_id.to_string(), user_id: effective_user_id.clone(), target_device: Some(target_device), timestamp: now }` on `EventBus`.
     - Return `ScreenLayout` with `resolved_scope = Some(ResolvedLayoutScope { user_id: effective_user_id, target_device, is_exact_override: true })`.

3. **`DELETE /api/v1/screens/{screen_id}`**
   - **Auth:** `AuthUser` with the exact same `ScopeMutationQuery` (`scope_user_id`, `target_device`) and RBAC rules as `PUT`:
     - If `(effective_user_id, target_device) == (None, TargetDevice::All)`:
       - Requires `Admin`. Resets built-in screens (`Home`, `Movies`, `Shows`) or library screens to factory defaults, or deletes a `ScreenId::Custom` screen (also calling `layout_override_repo.delete_all_for_screen` + `registry.remove_all_overrides_for_screen`).
     - Otherwise (scoped override slot):
       - Allowed for `Standard` when `effective_user_id == Some(auth_user.id)`, and for `Admin` on any scope.
       - Deletes the scoped override in `layout_override_repo` and `registry.remove_scoped_override`, publishes `SystemEvent::LayoutChanged`, and returns `200 OK` `{ "message": "Scoped layout override reset" }`.

4. **`GET /api/v1/widgets/{widget_id}/data` & `POST /api/v1/widgets/preview`**
   - `GET /api/v1/widgets/{widget_id}/data`: accepts optional `target_device?: String` (or `X-Kadr-Device`) and resolves the screen via `registry.resolve_scoped_screen(&canonical_id, Some(&auth_user.id), target_device)` (and when `screen_id` is omitted, searches the caller's scoped screens first before global screens) so `"Load More"` pagination works seamlessly on personal/device-overridden widgets!
   - `POST /api/v1/widgets/preview`: change extractor from `RequireAdmin` to `AuthUser` + `UnlockedLibraries` so `Standard` users can see live previews in personal Layout Studio while remaining strictly isolated from locked private libraries.

---

## 5. Web Client, `<DenseListWidget />` & Scoped Layout Studio (`web/`)

### 5.1 TypeScript Types & `ApiClient` (`web/src/types/index.ts`, `web/src/api/client.ts`)

- Add `TargetDevice = 'all' | 'tv' | 'web' | 'mobile'`, `TimeWindow = { start: string; end: string }`, and `ResolvedLayoutScope = { user_id?: string; target_device: TargetDevice; is_exact_override: boolean }`.
- Add `duration_seconds?: number` to `CardViewModel`, `time_window?: TimeWindow` to `HeroBanner` / `Carousel` / `Grid` / `DenseList`, the `{ type: 'dense_list'; id: string; title: string; binding: WidgetQueryBinding; time_window?: TimeWindow; items?: CardViewModel[]; next_cursor?: string; total_count?: number }` union member to `WidgetNode`, and `resolved_scope?: ResolvedLayoutScope` to `ScreenLayout`.
- Update `api.getScreen(screenId, unhydrated?, options?: { targetDevice?: TargetDevice; localTime?: string; scopeUserId?: string })`, `api.saveScreen(screenId, layout, options?: { scopeUserId?: string; targetDevice?: TargetDevice })`, `api.deleteScreen(screenId, options?: { scopeUserId?: string; targetDevice?: TargetDevice })`, and `api.getWidgetData(widgetId, offset, limit, screenId?, sort?, targetDevice?)`.
- Add helper `detectClientTargetDevice(): 'mobile' | 'web'` (`window.matchMedia?.('(max-width: 767px)').matches ? 'mobile' : 'web'`) and `formatLocalTimeHHMM(date = new Date()): string`.

### 5.2 `<DenseListWidget />` (`web/src/components/browse/DenseListWidget.tsx`)

- Renders a titled section with compact, keyboard-navigable cinema rows (`<ul>` / `<li>` semantic list):
  - **Left:** Compact 16:9 backdrop (falling back to poster or cinema placeholder icon) with play-hover overlay and subtle watched checkmark badge when `isCardWatched(item)`.
  - **Center:** Primary `item.title` button (opens `ItemDetailsModal`), `item.subtitle` (`2024 • 1080p` or `S01E02 - Series`), metadata chips (`badge` e.g. `RESUME` / `NEW` / `4K`, star `rating`), and a thin horizontal resume progress bar when `playback_progress > 0 && !isCardWatched(item)`.
  - **Right:** Formatted runtime (`formatDurationCompact(item.duration_seconds)` $\to$ `1h 54m` / `45m`, omitted for live channels or `0`), plus inline quick actions (`<FavoriteButton variant="icon" alwaysVisible />`, `<WatchStateButton variant="icon" alwaysVisible />` for playable VOD items, and a compact `Play` `<button type="button">`).
  - **Pagination & Live Sync:** Subscribes to `onWatchStateChanged` and `onFavoriteChanged`, and supports `"Load More"` via `next_cursor` (`api.getWidgetData`) matching `GridWidget.tsx`.
- Wire `widget.type === 'dense_list'` into `BrowseScreen.tsx` (passing `targetDevice` and `localTime` to `api.getScreen`) and `LayoutStudio.tsx` (`StudioDenseList` preview wrapper).

### 5.3 Personal & Admin Layout Studio (`web/src/App.tsx`, `LayoutStudio.tsx`, `WidgetConfigModal.tsx`)

1. **Top-Bar `Studio` Navigation (`web/src/App.tsx`):**
   - Add `'studio'` to `NavView` and render a top-bar **`Studio`** button (`Layers` icon, `aria-current="page"` when active) for every authenticated user (`currentUser != null`).
   - Pass `currentUser` into `<LayoutStudio currentUser={currentUser} onPlayItem={handlePlayItem} />` (and also pass `currentUser` from `AdminDashboard.tsx`).
2. **Scope & Time-Simulation Controls in `LayoutStudio.tsx`:**
   - **Layout Owner Selector (`defaultScope?: 'global' | 'personal'`):**
     - If `currentUser?.role === 'admin'`: `<select>` offering `Global (All Users)` (`scopeUserId = 'global'`, default when mounted inside `AdminDashboard` with `defaultScope="global"`), `My Profile (${currentUser.username})` (`scopeUserId = currentUser.id`, default when opened from top-bar `Studio` with `defaultScope="personal"`), plus all household profiles loaded via `api.getProfiles()`.
     - If `currentUser?.role !== 'admin'` (`Standard` user): displays a clear **`Personal Layout · ${currentUser.username}`** badge locked to `scopeUserId = currentUser.id` (no global/other-user selector, and hides the Admin-only `"New Custom Screen"` button).
   - **Target Device Selector (`All Devices`, `Web`, `TV`, `Mobile`):**
     - Selects `targetDevice` (`'all' | 'web' | 'tv' | 'mobile'`), automatically syncing the preview viewport frame (`tv` $\to$ `'tv'`, `mobile` $\to$ `'mobile'`, `web` $\to$ `'tv'`/wide) while still letting the user toggle preview aspect ratios freely.
     - Displays a scope status pill from `layout.resolved_scope`: **`Custom Override Active`** (with **`Reset Override`** button calling `api.deleteScreen(selectedScreenId, { scopeUserId, targetDevice })` to fall back to the parent tier) vs **`Inherited Default`** (saving creates an override at that exact `(scopeUserId, targetDevice)` slot).
   - **Simulate Time Control (`Simulate Time: Now | Custom HH:MM`):**
     - Optional `<input type="time">` in the Studio toolbar; widgets in the Widget Tree Inspector and canvas with a `time_window` display a clock badge (`23:00–04:00`) and an **`Inactive at HH:MM`** indicator (dimmed in Studio preview rather than hidden so authors can still click/edit them, with a clear label explaining they will be hidden on live screens outside `start–end`).
3. **`WidgetConfigModal.tsx`:**
   - Add **`Dense List`** (`dense_list`) to the Widget Presentation Type selector (`Hero Banner`, `Carousel`, `Grid`, `Dense List`).
   - Add an **Active Time Window (Optional)** section with an enable toggle + `Start Time` (`<input type="time">`) and `End Time` (`<input type="time">`) paired with `<label htmlFor={...}>`, inline validation rejecting `start === end`, and a helper badge indicating **Same-day window** vs **Overnight window (crosses midnight)** when `start > end`.

---

## 6. Testing & Verification Strategy

1. **`kadr-core` Unit Tests (`crates/kadr-core/tests/ast_test.rs` & `events_test.rs`):**
   - `TargetDevice` serde & `FromStr`; `TimeWindow::validate` (valid `00:00`–`23:59`, invalid `24:00`, `9:00`, `start == end`) and `TimeWindow::is_active_at_minutes` across boundary minutes for both same-day (`08:00`–`12:00`) and overnight (`23:00`–`04:00`) windows; `WidgetNode::DenseList` & `CardViewModel.duration_seconds` & `ResolvedLayoutScope` serde roundtrips; backwards-compatible `SystemEvent::LayoutChanged` serde.
2. **`kadr-storage` Repository Tests (`crates/kadr-storage/tests/layout_override_repo_test.rs`):**
   - `016_scoped_layouts.sql` migration, `upsert` idempotency across `(None, Tv)` and `(Some(u1), All | Tv | Mobile)`, multi-user isolation, `delete`, `delete_all_for_screen`, and `ON DELETE CASCADE` when a `users` row is deleted.
3. **`kadr-server` E2E Tests (`crates/kadr-server/tests/e2e_scoped_layouts_test.rs`):**
   - Full 4-tier fallback resolution (`user+device` $\to$ `user+all` $\to$ `global+device` $\to$ `global+all`) on `GET /api/v1/screens/home` and canonical library screens (`/api/v1/screens/{movies_lib_uuid}`).
   - RBAC enforcement: `Standard` user succeeds on `PUT`/`DELETE /api/v1/screens/home?scope_user_id=me&target_device=tv` and `POST /api/v1/widgets/preview` (with private library fail-closed isolation), and receives `403 Forbidden` on `scope_user_id=global` or another user's ID; `Admin` succeeds across all scopes.
   - `time_window` hydration: `23:00`–`04:00` widget included and hydrated at `?local_time=23:30` and `?local_time=02:15`, omitted at `?local_time=14:00`, and preserved on `?unhydrated=true`; `400 Bad Request` on malformed `local_time` or `start == end` on `PUT`.
   - `DenseList` hydration + `CardViewModel.duration_seconds` + `GET /api/v1/widgets/{id}/data` pagination on a personal `DenseList` widget.
4. **Frontend Vitest Suite (`DenseListWidget.test.tsx`, `LayoutStudio.test.tsx`, `WidgetConfigModal.test.tsx`, `BrowseScreen.test.tsx`, `App.test.tsx`):**
   - `DenseListWidget`: runtime formatting, progress bar, watched badge, `<FavoriteButton />` & `<WatchStateButton />` quick actions, `"Load More"` pagination.
   - `WidgetConfigModal`: creating/editing a `dense_list` widget and configuring/validating same-day & overnight `time_window` schedules.
   - `LayoutStudio` & `App`: top-bar `Studio` access for a `Standard` user (personal scope locked, saving & resetting override), `Admin` scope switcher (`Global` vs user profiles + `target_device`), and `Simulate Time` active/inactive shelf indicators.
   - `BrowseScreen`: forwarding `targetDevice` + `localTime` and rendering `dense_list` widgets.
