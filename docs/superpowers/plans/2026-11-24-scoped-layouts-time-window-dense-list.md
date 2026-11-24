# Scoped Layouts, `time_window` Shelves & `dense_list` Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver per-user and per-device (`all`, `tv`, `web`, `mobile`) scoped screen layouts with 4-tier fallback precedence and personal Layout Studio RBAC, client-clock `time_window` shelf scheduling (same-day and overnight), and the compact `<DenseListWidget />` (`dense_list`) presentation across `kadr-core`, `kadr-storage`, `kadr-server`, `web/`, and docs.

**Architecture:** Extend `kadr-core::ast` with `TargetDevice`, `TimeWindow`, `WidgetNode::DenseList`, `CardViewModel.duration_seconds`, and `ResolvedLayoutScope`. Persist Global+`All` screens on disk (`data/screens/*.json`) for backwards compatibility while storing all other `(user_id, target_device)` overrides in SQLite (`016_scoped_layouts.sql`, `ON DELETE CASCADE` on `users(id)`) cached in memory inside `LayoutRegistry` for zero-DB-query fallback resolution. Hydrate active `time_window` shelves against `?local_time=HH:MM` / `X-Kadr-Local-Time` (or server local wall-clock time), open personal Layout Studio + live widget preview to `Standard` users (`scope_user_id=me`) while giving `Admin` users full multi-user/global scope + time simulation, and render `<DenseListWidget />` across Browse and Studio views.

**Tech Stack:** Rust (`rustc 1.100.0`, `axum 0.8`, `rusqlite 0.32`, `rusqlite_migration`, `serde`, `libc`), SQLite WAL, React 19, TypeScript, Tailwind CSS v4, Vitest + React Testing Library.

## Global Constraints

- **Zero new Cargo or npm dependencies**: Do not modify `[dependencies]` in any `Cargo.toml` or `"dependencies"` / `"devDependencies"` in `web/package.json`.
- **Static `musl` & zero-warning gate**: Preserve `cargo check --workspace --target x86_64-unknown-linux-musl` compatibility and `cargo clippy --workspace --all-targets -- -D warnings` with zero warnings.
- **4-tier layout fallback precedence**: `(Some(user_id), device)` $\to$ `(Some(user_id), All)` $\to$ `(None, device)` $\to$ `(None, All)` across both `GET /api/v1/screens/{id}` and `GET /api/v1/widgets/{widget_id}/data`.
- **RBAC & fail-closed privacy**: `Standard` users can inspect (`?unhydrated=true`), preview (`POST /api/v1/widgets/preview`), save (`PUT`), and reset (`DELETE`) **only** their own layouts (`scope_user_id` = `"me"` or `auth_user.id`), receiving `403 Forbidden` on `"global"` or another user's ID; `POST /api/v1/widgets/preview` MUST enforce `UnlockedLibraries`.
- **Time-window semantics**: `TimeWindow { start: "HH:MM", end: "HH:MM" }` validates `00:00..=23:59` and rejects `start == end`; same-day (`start < end`) is active when `m >= start && m < end`, overnight (`start > end`) is active when `m >= start || m < end`; inactive widgets are skipped during hydrated `GET /api/v1/screens/{id}` and preserved on `?unhydrated=true`.
- **Web accessibility (`modern-web-guidance`)**: Use native `<button type="button">`, explicit `<label htmlFor={...}>` on `<select>` and `<input type="time">`, `aria-pressed` on toggle controls, `aria-current="page"` on top-bar navigation buttons, `aria-hidden="true"` on decorative `lucide-react` SVGs, and `>= 24x24px` (`min-w-7 min-h-7`) interactive target sizes.
- **Git rules**: Commit directly on `main`; never run `git push` or create git tags unless explicitly instructed.

---

### Task 1: Core AST (`TargetDevice`, `TimeWindow`, `DenseList`, `duration_seconds`, `ResolvedLayoutScope`) & Workspace Exhaustive Match Compatibility

**Files:**
- Modify: `crates/kadr-core/src/ast.rs`
- Modify: `crates/kadr-core/src/events.rs`
- Modify: `crates/kadr-core/tests/ast_test.rs`
- Modify: `crates/kadr-core/tests/events_test.rs`
- Modify: `crates/kadr-core/tests/filter_ast_test.rs`
- Modify: `crates/kadr-core/tests/playlists_test.rs`
- Modify: `crates/kadr-server/src/resolver/card.rs`
- Modify: `crates/kadr-server/src/resolver/resolver.rs`
- Modify: `crates/kadr-server/src/layout/registry.rs`
- Modify: `crates/kadr-server/src/api/screen_routes.rs`
- Modify: `crates/kadr-server/src/playback/handler.rs`
- Modify: `crates/kadr-server/tests/layout_registry_test.rs`
- Modify: `crates/kadr-server/tests/widget_resolver_test.rs`
- Modify: `crates/kadr-server/tests/e2e_filter_ast_test.rs`
- Modify: `crates/kadr-server/tests/library_screens_sync_test.rs`
- Modify: `crates/kadr-server/tests/private_streaming_auth_test.rs`
- Modify: `crates/kadr-server/tests/e2e_playlists_test.rs`
- Modify: `crates/kadr-server/tests/screen_routes_test.rs`
- Modify: `crates/kadr-server/tests/event_bus_test.rs`
- Modify: `crates/kadr-server/tests/events_routes_test.rs`

**Interfaces:**
- Consumes: `kadr_core::filter_ast::FilterNode`, `kadr_core::models::{MediaItem, PlaybackState, TechnicalInfo}`.
- Produces:
  - `kadr_core::ast::TargetDevice` (`All`, `Tv`, `Web`, `Mobile`; `Default`, `Display`, `FromStr` returning `Result<TargetDevice, String>`, `Serialize`, `Deserialize`).
  - `kadr_core::ast::parse_hhmm_minutes(s: &str) -> Result<u16, String>` and `kadr_core::ast::TimeWindow { pub start: String, pub end: String }` with `TimeWindow::validate(&self) -> Result<(), String>` and `TimeWindow::is_active_at_minutes(&self, current_minutes: u16) -> bool`.
  - `CardViewModel.duration_seconds: Option<u64>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`), populated in `to_card_view_model_with_favorite` when `item.technical.duration_seconds > 0`.
  - `WidgetNode::DenseList { id, title, binding, time_window, items, next_cursor, total_count }` and `time_window: Option<TimeWindow>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) on `HeroBanner`, `Carousel`, `Grid`, and `DenseList`, plus `WidgetNode::time_window(&self) -> Option<&TimeWindow>` and `WidgetNode::strip_hydrated_data(&mut self)`.
  - `ResolvedLayoutScope { pub user_id: Option<String>, pub target_device: TargetDevice, pub is_exact_override: bool }` and `ScreenLayout.resolved_scope: Option<ResolvedLayoutScope>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`), plus `ScreenLayout::strip_runtime_fields(&mut self)`.
  - `SystemEvent::LayoutChanged { screen_id: String, user_id: Option<String>, target_device: Option<TargetDevice>, timestamp: i64 }` (`#[serde(default, skip_serializing_if = "Option::is_none")]` on `user_id` and `target_device`).

- [ ] **Step 1: Write failing unit tests in `crates/kadr-core/tests/ast_test.rs` and `crates/kadr-core/tests/events_test.rs`**

Append unit tests to `crates/kadr-core/tests/ast_test.rs` covering:
```rust
#[test]
fn test_target_device_serde_and_from_str() {
    use kadr_core::ast::TargetDevice;

    assert_eq!(TargetDevice::default(), TargetDevice::All);
    for (raw, expected, canonical) in [
        ("all", TargetDevice::All, "all"),
        ("TV", TargetDevice::Tv, "tv"),
        ("web", TargetDevice::Web, "web"),
        ("Mobile", TargetDevice::Mobile, "mobile"),
    ] {
        let parsed: TargetDevice = raw.parse().expect("valid target device");
        assert_eq!(parsed, expected);
        assert_eq!(parsed.to_string(), canonical);
        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, format!("\"{canonical}\""));
        let back: TargetDevice = serde_json::from_str(&json).unwrap();
        assert_eq!(back, expected);
    }
    assert!("fridge".parse::<TargetDevice>().is_err());
}

#[test]
fn test_time_window_validation_and_same_day_overnight_evaluation() {
    use kadr_core::ast::{parse_hhmm_minutes, TimeWindow};

    assert_eq!(parse_hhmm_minutes("00:00").unwrap(), 0);
    assert_eq!(parse_hhmm_minutes("23:59").unwrap(), 1439);
    for bad in ["", "9:00", "24:00", "12:60", "ab:cd", "1200", "12:00:00"] {
        assert!(parse_hhmm_minutes(bad).is_err(), "expected {bad} to fail");
    }

    let same_day = TimeWindow {
        start: "08:00".to_string(),
        end: "12:00".to_string(),
    };
    same_day.validate().unwrap();
    assert!(!same_day.is_active_at_minutes(7 * 60 + 59));
    assert!(same_day.is_active_at_minutes(8 * 60));
    assert!(same_day.is_active_at_minutes(11 * 60 + 59));
    assert!(!same_day.is_active_at_minutes(12 * 60));

    let overnight = TimeWindow {
        start: "23:00".to_string(),
        end: "04:00".to_string(),
    };
    overnight.validate().unwrap();
    assert!(!overnight.is_active_at_minutes(22 * 60 + 59));
    assert!(overnight.is_active_at_minutes(23 * 60));
    assert!(overnight.is_active_at_minutes(0));
    assert!(overnight.is_active_at_minutes(3 * 60 + 59));
    assert!(!overnight.is_active_at_minutes(4 * 60));

    let equal_bounds = TimeWindow {
        start: "10:00".to_string(),
        end: "10:00".to_string(),
    };
    assert!(equal_bounds.validate().is_err());
}

#[test]
fn test_widget_node_dense_list_duration_and_resolved_scope_roundtrip() {
    use kadr_core::ast::{
        CardViewModel, QueryMacro, ResolvedLayoutScope, ScreenId, ScreenLayout, TargetDevice,
        TimeWindow, WidgetNode, WidgetQueryBinding,
    };

    let card = CardViewModel {
        id: 42,
        title: "Cairo Station".to_string(),
        media_type: "movie".to_string(),
        duration_seconds: Some(4620),
        ..Default::default()
    };
    let dense = WidgetNode::DenseList {
        id: "late_night_shorts".to_string(),
        title: "Late Night Shorts".to_string(),
        binding: WidgetQueryBinding::new(QueryMacro::TopRated).with_limit(10),
        time_window: Some(TimeWindow {
            start: "23:00".to_string(),
            end: "04:00".to_string(),
        }),
        items: Some(vec![card]),
        next_cursor: Some("/api/v1/widgets/late_night_shorts/data?offset=10".to_string()),
        total_count: Some(25),
    };
    assert_eq!(dense.id(), "late_night_shorts");
    assert!(dense.is_hydrated());
    assert_eq!(dense.time_window().unwrap().start, "23:00");

    let mut screen = ScreenLayout::new(ScreenId::Home, "Home", vec![dense]);
    screen.resolved_scope = Some(ResolvedLayoutScope {
        user_id: Some("u-1".to_string()),
        target_device: TargetDevice::Tv,
        is_exact_override: true,
    });

    let json = serde_json::to_string(&screen).unwrap();
    assert!(json.contains("\"type\":\"dense_list\""));
    assert!(json.contains("\"duration_seconds\":4620"));
    assert!(json.contains("\"resolved_scope\""));

    let mut parsed: ScreenLayout = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, screen);

    parsed.strip_runtime_fields();
    assert!(parsed.resolved_scope.is_none());
    assert!(!parsed.widgets[0].is_hydrated());
    assert_eq!(parsed.widgets[0].time_window().unwrap().end, "04:00");
}
```
And in `crates/kadr-core/tests/events_test.rs`, add a test verifying `SystemEvent::LayoutChanged` serializes without `user_id`/`target_device` when both are `None` (preserving legacy JSON compatibility) and includes `"user_id"` + `"target_device":"tv"` when populated.

- [ ] **Step 2: Run `cargo test -p kadr-core` to confirm RED failure**

Run:
```bash
cargo test -p kadr-core
```
Expected: FAIL (`TargetDevice`, `TimeWindow`, `parse_hhmm_minutes`, `WidgetNode::DenseList`, `ResolvedLayoutScope`, `duration_seconds` missing).

- [ ] **Step 3: Implement `kadr-core` AST & event extensions and update exhaustive workspace struct/match sites**

1. In `crates/kadr-core/src/ast.rs`:
   - Add `TargetDevice`, `parse_hhmm_minutes`, `TimeWindow` (`validate`, `is_active_at_minutes`), and `ResolvedLayoutScope`.
   - Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub duration_seconds: Option<u64>` to `CardViewModel`.
   - Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub time_window: Option<TimeWindow>` to `HeroBanner`, `Carousel`, `Grid`, and the new `DenseList` variant on `WidgetNode`.
   - Implement `WidgetNode::time_window(&self) -> Option<&TimeWindow>` and `WidgetNode::strip_hydrated_data(&mut self)`.
   - Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub resolved_scope: Option<ResolvedLayoutScope>` to `ScreenLayout` (initialized to `None` in `ScreenLayout::new`), and implement `ScreenLayout::strip_runtime_fields(&mut self)` (sets `self.resolved_scope = None` and calls `w.strip_hydrated_data()` on every widget).
2. In `crates/kadr-core/src/events.rs`:
   - Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub user_id: Option<String>` and `#[serde(default, skip_serializing_if = "Option::is_none")] pub target_device: Option<crate::ast::TargetDevice>` to `SystemEvent::LayoutChanged`.
3. Update `crates/kadr-server/src/resolver/card.rs`:
   - Populate `duration_seconds: (item.technical.duration_seconds > 0).then_some(item.technical.duration_seconds)` in `to_card_view_model_with_favorite`.
4. Update workspace struct literals & exhaustive matches so the entire workspace compiles cleanly:
   - Add `..Default::default()` or `duration_seconds: None` to direct `CardViewModel { ... }` literals in `crates/kadr-core/tests/{ast_test.rs, filter_ast_test.rs, playlists_test.rs}`.
   - Add `time_window: None` (and `resolved_scope: None` where `ScreenLayout { ... }` is constructed directly) and `WidgetNode::DenseList` match arms in `crates/kadr-server/src/layout/registry.rs`, `crates/kadr-server/src/resolver/resolver.rs` (including basic `DenseList` hydration arm delegating to `resolve_widget_data_with_library`), `crates/kadr-server/src/api/screen_routes.rs` (using `screen.strip_runtime_fields()`), `crates/kadr-server/src/playback/handler.rs`, and existing test literals in `crates/kadr-core/tests/ast_test.rs` and `crates/kadr-server/tests/{layout_registry_test.rs, widget_resolver_test.rs, e2e_filter_ast_test.rs, library_screens_sync_test.rs, private_streaming_auth_test.rs, e2e_playlists_test.rs, screen_routes_test.rs, event_bus_test.rs, events_routes_test.rs}`.

- [ ] **Step 4: Run workspace tests & Clippy to verify GREEN**

Run:
```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
```
Expected: All workspace tests PASS and Clippy exits `0` with zero warnings.

- [ ] **Step 5: Commit Task 1**

```bash
git add crates
git commit -m "feat(core): add TargetDevice, TimeWindow, DenseList AST node, and CardViewModel duration"
```

---

### Task 2: SQLite Migration `016_scoped_layouts.sql` & `LayoutOverrideRepository` (`crates/kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/migrations/016_scoped_layouts.sql`
- Modify: `crates/kadr-storage/src/migrations.rs`
- Create: `crates/kadr-storage/src/repos/layout_override_repo.rs`
- Modify: `crates/kadr-storage/src/repos/mod.rs`
- Create: `crates/kadr-storage/tests/layout_override_repo_test.rs`

**Interfaces:**
- Consumes: `kadr_core::ast::{ScreenLayout, TargetDevice}`, `deadpool_sqlite::Pool`.
- Produces:
  - Migration `016_scoped_layouts.sql` (`layout_overrides` table with `user_id TEXT REFERENCES users(id) ON DELETE CASCADE`, `target_device TEXT NOT NULL CHECK (target_device IN ('all', 'tv', 'web', 'mobile'))`, `idx_layout_overrides_scope` unique index on `(screen_id, IFNULL(user_id, ''), target_device)`, and `idx_layout_overrides_user`).
  - `StoredLayoutOverride { pub screen_id: String, pub user_id: Option<String>, pub target_device: TargetDevice, pub layout: ScreenLayout, pub updated_at: i64 }`.
  - `LayoutOverrideRepository::new(pool: Pool)` with async methods:
    - `list_all(&self) -> Result<Vec<StoredLayoutOverride>>`
    - `get(&self, screen_id: &str, user_id: Option<&str>, target_device: TargetDevice) -> Result<Option<StoredLayoutOverride>>`
    - `upsert(&self, screen_id: &str, user_id: Option<&str>, target_device: TargetDevice, layout: &ScreenLayout, now_epoch: i64) -> Result<()>`
    - `delete(&self, screen_id: &str, user_id: Option<&str>, target_device: TargetDevice) -> Result<bool>`
    - `delete_all_for_screen(&self, screen_id: &str) -> Result<u64>`

- [ ] **Step 1: Write failing storage tests in `crates/kadr-storage/tests/layout_override_repo_test.rs`**

Create `crates/kadr-storage/tests/layout_override_repo_test.rs` testing:
1. Upserting and retrieving a Global device override (`user_id = None`, `TargetDevice::Tv`) and Per-User overrides (`Some("u1")`, `TargetDevice::All` & `TargetDevice::Mobile`; `Some("u2")`, `TargetDevice::Tv`), verifying idempotent `upsert` updates `layout_json` and `updated_at` in place without duplicate rows.
2. Ensuring runtime fields (`resolved_scope`, hydrated `items`) are stripped before persisting `layout_json`.
3. Deleting a single scoped slot (`delete("home", Some("u1"), TargetDevice::Mobile)`) leaves `("home", Some("u1"), TargetDevice::All)` and `("home", None, TargetDevice::Tv)` untouched.
4. `delete_all_for_screen("custom_scifi")` removes all global and per-user overrides for `"custom_scifi"` while preserving `"home"` overrides.
5. Deleting user `"u1"` from `users` (`DELETE FROM users WHERE id = 'u1'`) cascades via `ON DELETE CASCADE` and removes all `"u1"` rows from `layout_overrides` while keeping `"u2"` and Global rows intact.

- [ ] **Step 2: Run `cargo test -p kadr-storage --test layout_override_repo_test` to confirm RED failure**

Run:
```bash
cargo test -p kadr-storage --test layout_override_repo_test
```
Expected: FAIL (`LayoutOverrideRepository` does not exist).

- [ ] **Step 3: Implement `016_scoped_layouts.sql` and `LayoutOverrideRepository`**

1. Create `crates/kadr-storage/src/migrations/016_scoped_layouts.sql`:
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
   and register it in `crates/kadr-storage/src/migrations.rs`.
2. Implement `crates/kadr-storage/src/repos/layout_override_repo.rs` and re-export `LayoutOverrideRepository` + `StoredLayoutOverride` in `crates/kadr-storage/src/repos/mod.rs`:
   - In `upsert`, clone `layout`, call `clean.strip_runtime_fields()`, serialize to JSON, and execute:
     ```sql
     INSERT INTO layout_overrides (screen_id, user_id, target_device, layout_json, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5)
     ON CONFLICT(screen_id, IFNULL(user_id, ''), target_device)
     DO UPDATE SET layout_json = excluded.layout_json, updated_at = excluded.updated_at
     ```
   - In `list_all`, deserialize rows and skip any corrupt JSON row with `eprintln!` / non-fatal handling so one bad manual edit never bricks startup.
   - In `get` and `delete`, match `screen_id = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND target_device = ?3`.

- [ ] **Step 4: Run `kadr-storage` tests & Clippy to verify GREEN**

Run:
```bash
cargo test -p kadr-storage && cargo clippy -p kadr-storage --all-targets -- -D warnings
```
Expected: All `kadr-storage` tests PASS (`0` warnings).

- [ ] **Step 5: Commit Task 2**

```bash
git add crates/kadr-storage
git commit -m "feat(storage): add 016_scoped_layouts migration and LayoutOverrideRepository"
```

---

### Task 3: Scoped `LayoutRegistry`, `WidgetResolver` Time-Window Hydration & Scope-Aware RBAC Routes (`crates/kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/layout/registry.rs`
- Modify: `crates/kadr-server/src/resolver/resolver.rs`
- Modify: `crates/kadr-server/src/api/screen_routes.rs`
- Modify: `crates/kadr-server/src/api/widget_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Modify: `crates/kadr-server/src/main.rs`
- Create: `crates/kadr-server/tests/e2e_scoped_layouts_test.rs`

**Interfaces:**
- Consumes: `TargetDevice`, `TimeWindow`, `parse_hhmm_minutes`, `ResolvedLayoutScope`, `WidgetNode::DenseList`, `LayoutOverrideRepository`, `StoredLayoutOverride`.
- Produces:
  - `LayoutRegistry` methods: `load_scoped_overrides`, `set_scoped_override`, `remove_scoped_override`, `remove_all_overrides_for_screen`, and `resolve_scoped_screen(&self, canonical_id: &ScreenId, requested_user_id: Option<&str>, requested_device: TargetDevice) -> Option<ScreenLayout>` (4-tier fallback + accurate `ResolvedLayoutScope` including `disk_overridden: HashSet<ScreenId>` tracking for `(None, TargetDevice::All)`).
  - `WidgetResolver::resolve_screen_with_library_at(&self, screen: ScreenLayout, user_id: &str, unlocked_ids: &[String], default_library_id: Option<&str>, active_minutes: u16) -> ScreenLayout` + `server_local_minutes() -> u16` (via `libc::localtime_r` on `#[cfg(unix)]`).
  - Updated REST endpoints:
    - `GET /api/v1/screens/{screen_id}?unhydrated=&target_device=&local_time=&scope_user_id=` (also reading `X-Kadr-Device` and `X-Kadr-Local-Time` headers)
    - `PUT /api/v1/screens/{screen_id}?scope_user_id=&target_device=` (RBAC for `Standard` self-scope vs `Admin` any-scope, `TimeWindow::validate()`, `FilterNode::validate()`, `LayoutChanged` SSE broadcast)
    - `DELETE /api/v1/screens/{screen_id}?scope_user_id=&target_device=` (scoped reset vs Global reset/delete + `delete_all_for_screen` cleanup)
    - `GET /api/v1/widgets/{widget_id}/data?screen_id=&offset=&limit=&sort=&target_device=` (scoped widget lookup)
    - `POST /api/v1/widgets/preview` (accessible to `AuthUser` + `UnlockedLibraries`).

- [ ] **Step 1: Write failing E2E tests in `crates/kadr-server/tests/e2e_scoped_layouts_test.rs`**

Create `crates/kadr-server/tests/e2e_scoped_layouts_test.rs` verifying:
1. **4-Tier Fallback Precedence**:
   - Start with factory default `Home`.
   - Admin saves Global `tv` override (`PUT /api/v1/screens/home?scope_user_id=global&target_device=tv`), User A (`Standard`) saves `all` override (`PUT /api/v1/screens/home?scope_user_id=me&target_device=all`), and User A saves `mobile` override (`PUT /api/v1/screens/home?scope_user_id=me&target_device=mobile`).
   - Verify:
     - User A on `mobile` $\to$ gets `(uA, mobile)` (`is_exact_override: true`).
     - User A on `tv` $\to$ gets `(uA, all)` (`resolved_scope.user_id = Some(uA)`, `target_device = All`, `is_exact_override: false` — user `all` beats global `tv`!).
     - User B (no personal overrides) on `tv` $\to$ gets `(None, tv)` (`is_exact_override: true`).
     - User B on `web` $\to$ gets `(None, all)` (`is_exact_override: false`).
   - Deleting User A's `mobile` override (`DELETE /api/v1/screens/home?scope_user_id=me&target_device=mobile`) falls back cleanly to `(uA, all)`.
   - Also verify canonical library UUID resolution: saving an override on `movies` applies when fetching `GET /api/v1/screens/{movies_library_uuid}?target_device=tv`.
2. **RBAC & Privacy Isolation**:
   - `Standard` user receives `403 Forbidden` on `PUT /api/v1/screens/home` (default `global`), `PUT /api/v1/screens/home?scope_user_id=global&target_device=tv`, `PUT /api/v1/screens/home?scope_user_id={other_uid}`, `DELETE /api/v1/screens/home?scope_user_id=global`, and `GET /api/v1/screens/home?unhydrated=true&scope_user_id=global` (or `{other_uid}`).
   - `Standard` user succeeds on `GET /api/v1/screens/home?unhydrated=true` (defaults to `me`), `PUT /api/v1/screens/home?scope_user_id=me&target_device=web`, and `POST /api/v1/widgets/preview` (returning `0` items from a locked private library until an unlock token is supplied).
   - `Admin` succeeds when saving/inspecting/resetting overrides on behalf of `{standard_user_id}` (`404 Not Found` if `{nonexistent_user_id}` does not exist).
3. **`TimeWindow` Hydration & Validation**:
   - Save a screen containing a day shelf (`08:00`–`20:00`) and a late-night `DenseList` shelf (`23:00`–`04:00`).
   - `GET /api/v1/screens/home?local_time=14:30` returns only the day shelf; `GET /api/v1/screens/home?local_time=23:45` and `X-Kadr-Local-Time: 02:15` return only the late-night `DenseList` shelf (with hydrated `CardViewModel.duration_seconds`); `GET /api/v1/screens/home?unhydrated=true` returns **both** widgets with `time_window` intact.
   - Invalid `?local_time=99:00`, invalid `?target_device=toaster`, or `PUT` with `time_window: { start: "12:00", end: "12:00" }` returns `400 Bad Request`.
4. **Scoped Widget Pagination & SSE**:
   - `GET /api/v1/widgets/{personal_dense_list_id}/data?screen_id=home&target_device=mobile&offset=1&limit=1` resolves User A's personal `DenseList` widget and returns paginated items with `&target_device=mobile` preserved in `next_cursor`.
   - `PUT /api/v1/screens/home?scope_user_id=me&target_device=tv` emits `SystemEvent::LayoutChanged` carrying `user_id` and `target_device: Some(TargetDevice::Tv)`.

- [ ] **Step 2: Run `cargo test -p kadr-server --test e2e_scoped_layouts_test` to confirm RED failure**

Run:
```bash
cargo test -p kadr-server --test e2e_scoped_layouts_test
```
Expected: FAIL (`resolve_scoped_screen`, query params, and RBAC handlers not implemented yet).

- [ ] **Step 3: Implement `LayoutRegistry` fallback, `WidgetResolver` time-window hydration, startup loading, and API routes**

1. **`crates/kadr-server/src/layout/registry.rs`:**
   - Add `LayoutScopeKey { pub screen_id: ScreenId, pub user_id: Option<String>, pub target_device: TargetDevice }`, `scoped_overrides: HashMap<LayoutScopeKey, ScreenLayout>`, and `disk_overridden: HashSet<ScreenId>` inside `LayoutRegistryInner` (mark `disk_overridden` in `save_screen` and `load_overrides_from_dir`, and clear in `reset_screen` / `delete_custom_screen`).
   - Implement `load_scoped_overrides`, `set_scoped_override`, `remove_scoped_override`, `remove_all_overrides_for_screen`, and `resolve_scoped_screen` (4-tier fallback populating `screen.resolved_scope`).
   - Include `WidgetNode::DenseList { binding, .. }` alongside `Grid` and `Carousel` in `find_screen_for_library`.
2. **`crates/kadr-server/src/resolver/resolver.rs`:**
   - Implement `pub fn server_local_minutes() -> u16` using `libc::localtime_r` under `#[cfg(unix)]` (and UTC fallback under `#[cfg(not(unix))]`).
   - Add `pub async fn resolve_screen_with_library_at(&self, screen: ScreenLayout, user_id: &str, unlocked_ids: &[String], default_library_id: Option<&str>, active_minutes: u16) -> ScreenLayout`:
     - Preserve `screen.resolved_scope`.
     - Retain only widgets where `widget.time_window().map_or(true, |tw| tw.is_active_at_minutes(active_minutes))` before `join_all`.
     - Hydrate `WidgetNode::DenseList` with `resolve_widget_data_with_library` (identical to `Grid`, preserving `time_window`).
     - Keep `resolve_screen_with_library` delegating to `resolve_screen_with_library_at(..., server_local_minutes())`.
   - Include `WidgetNode::DenseList { binding, .. }` in `resolve_library_id_for_layout`.
3. **`crates/kadr-server/src/api/mod.rs` & `crates/kadr-server/src/main.rs`:**
   - Construct `let layout_override_repo = LayoutOverrideRepository::new(media_repo.pool().clone());` in `create_router_with_ingest` (also calling `if let Ok(entries) = layout_override_repo.list_all().await` in `main.rs` on startup — and in `create_router_with_ingest` if `tokio::runtime::Handle::try_current()` or lazy sync isn't needed, load in `main.rs` right after `load_overrides_from_dir`, and layer `Extension(layout_override_repo)` on the Axum router).
4. **`crates/kadr-server/src/api/screen_routes.rs` & `crates/kadr-server/src/api/widget_routes.rs`:**
   - Extract shared helper `resolve_canonical_screen_id(requested_id: &str, registry: &LayoutRegistry, lib_repo: &LibraryRepository) -> Option<ScreenId>` so both `screen_routes` and `widget_routes` resolve library UUIDs / names to canonical `ScreenId`s consistently before calling `registry.resolve_scoped_screen(...)`.
   - Parse `target_device` from query param or `HeaderMap` (`x-kadr-device`), defaulting to `TargetDevice::All` (`400` on invalid value).
   - Parse `local_time` from query param or `HeaderMap` (`x-kadr-local-time`), defaulting to `server_local_minutes()` (`400` on invalid `HH:MM`).
   - Implement `GET`, `PUT`, and `DELETE /api/v1/screens/{screen_id}` with full RBAC (`UserRepository` lookup when Admin targets `<other_uid>`), `TimeWindow::validate()`, `FilterNode::validate()`, SQLite + `LayoutRegistry` persistence, and `SystemEvent::LayoutChanged` broadcast via `Extension<Arc<EventBus>>`.
   - Update `GET /api/v1/widgets/{widget_id}/data` to resolve scoped screens (`Some(&auth_user.id)`, `target_device`) and preserve `&target_device=...` in `next_cursor` when provided.
   - Update `POST /api/v1/widgets/preview` from `RequireAdmin(auth_user)` to `auth_user: AuthUser`.

- [ ] **Step 4: Run full Rust workspace tests, Clippy & `musl` check to verify GREEN**

Run:
```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo check --workspace --target x86_64-unknown-linux-musl
```
Expected: All Rust workspace tests PASS, zero Clippy warnings, and `musl` check exits `0`.

- [ ] **Step 5: Commit Task 3**

```bash
git add crates
git commit -m "feat(server): add scoped layout fallback, time_window hydration, and personal Studio RBAC"
```

---

### Task 4: Frontend Types, `ApiClient`, `<DenseListWidget />`, and `BrowseScreen` Device & Local-Time Wiring (`web/`)

**Files:**
- Modify: `web/src/types/index.ts`
- Modify: `web/src/api/client.ts`
- Modify: `web/src/api/client.test.ts`
- Create: `web/src/components/browse/DenseListWidget.tsx`
- Create: `web/src/components/browse/DenseListWidget.test.tsx`
- Modify: `web/src/components/browse/BrowseScreen.tsx`
- Modify: `web/src/components/browse/BrowseScreen.test.tsx`

**Interfaces:**
- Consumes: `GET /api/v1/screens/{id}?target_device=&local_time=`, `GET /api/v1/widgets/{id}/data`, `<FavoriteButton />`, `<WatchStateButton />`, `onFavoriteChanged`, `onWatchStateChanged`.
- Produces:
  - TypeScript types in `web/src/types/index.ts`: `TargetDevice`, `TimeWindow`, `ResolvedLayoutScope`, `CardViewModel.duration_seconds?: number`, `WidgetNode` `'dense_list'` variant + `time_window?: TimeWindow` on widget variants, `ScreenLayout.resolved_scope?: ResolvedLayoutScope`, and optional `user_id` / `target_device` on `SystemEvent` `'layout:changed'`.
  - `ApiClient` methods in `web/src/api/client.ts`:
    - `getScreen(screenId: string, unhydrated?: boolean, options?: ScreenRequestOptions): Promise<ScreenLayout>` (`ScreenRequestOptions = { targetDevice?: TargetDevice; localTime?: string; scopeUserId?: string }`)
    - `saveScreen(screenId: string, layout: ScreenLayout, options?: ScreenScopeOptions): Promise<ScreenLayout>` (`ScreenScopeOptions = { scopeUserId?: string; targetDevice?: TargetDevice }`)
    - `deleteScreen(screenId: string, options?: ScreenScopeOptions): Promise<{ message: string }>`
    - `getWidgetData(widgetId: string, offset?: number, limit?: number, screenId?: string, sort?: string, targetDevice?: TargetDevice): Promise<CardViewModel[]>`
  - Helpers exported from `web/src/utils/layoutScope.ts` (or `web/src/api/client.ts` / `DenseListWidget.tsx`): `detectClientTargetDevice(): 'mobile' | 'web'`, `formatLocalTimeHHMM(date?: Date): string`, `isTimeWindowActive(tw: TimeWindow, hhmm: string): boolean`, and `formatDurationCompact(seconds?: number): string`.
  - `<DenseListWidget />` (`web/src/components/browse/DenseListWidget.tsx`).

- [ ] **Step 1: Write failing tests in `client.test.ts`, `DenseListWidget.test.tsx`, and `BrowseScreen.test.tsx`**

1. In `web/src/api/client.test.ts`, add tests verifying:
   - `getScreen('home', false, { targetDevice: 'web', localTime: '23:15' })` requests `/api/v1/screens/home?target_device=web&local_time=23%3A15` (and `getScreen('home')` with no options still requests `/api/v1/screens/home` so existing tests stay 100% compatible!).
   - `getScreen('home', true, { scopeUserId: 'me', targetDevice: 'tv' })` requests `/api/v1/screens/home?unhydrated=true&scope_user_id=me&target_device=tv`.
   - `saveScreen('home', layout, { scopeUserId: 'me', targetDevice: 'mobile' })` and `deleteScreen('home', { scopeUserId: 'me', targetDevice: 'mobile' })` append `scope_user_id=me&target_device=mobile`.
2. Create `web/src/components/browse/DenseListWidget.test.tsx` testing:
   - Rendering compact rows inside a semantic `<ul role="list">` with title, subtitle, formatted runtime (`4620` $\to$ `1h 17m`, `2700` $\to$ `45m`, omitted for `live_channel` or `undefined`), progress bar when `in_progress`, watched badge when `playback_progress >= 1`, `<FavoriteButton variant="icon" alwaysVisible />` (on all items including `show` and `live_channel`), `<WatchStateButton variant="icon" alwaysVisible />` (on playable VOD items, omitted on `show`, `season`, and `live_channel`), and `Play` action button (`onPlayItem`) vs row title click (`onSelectItem`) without event bubbling collisions.
   - Live reactivity to `emitFavoriteChanged` and `emitWatchStateChanged`, plus `"Load More"` pagination via `api.getWidgetData`.
3. In `web/src/components/browse/BrowseScreen.test.tsx`, add a test verifying `BrowseScreen` renders `type: 'dense_list'` widgets via `<DenseListWidget />` and forwards `{ targetDevice, localTime }` to `api.getScreen` (note: ensure existing `expect(getScreenSpy).toHaveBeenCalledWith(...)` assertions in `BrowseScreen.test.tsx` remain green or are updated cleanly!).

- [ ] **Step 2: Run targeted Vitest files to confirm RED failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/api/client.test.ts src/components/browse/DenseListWidget.test.tsx src/components/browse/BrowseScreen.test.tsx
```
Expected: FAIL (`DenseListWidget` missing; `ScreenRequestOptions` / `dense_list` unhandled).

- [ ] **Step 3: Implement types, `ApiClient` options, `layoutScope.ts`, `<DenseListWidget />`, and `BrowseScreen` wiring**

1. Create `web/src/utils/layoutScope.ts` exporting `detectClientTargetDevice()`, `formatLocalTimeHHMM(date = new Date())`, `isTimeWindowActive(tw: TimeWindow, hhmm: string): boolean`, and `formatDurationCompact(seconds?: number): string`.
2. Update `web/src/types/index.ts` and `web/src/api/client.ts` (preserving 1-argument `api.getScreen(screenId)` behavior when `options` is omitted unless `BrowseScreen` explicitly passes `{ targetDevice, localTime }`; check existing `BrowseScreen.test.tsx` / `App.test.tsx` assertions on `api.getScreen`!).
3. Create `web/src/components/browse/DenseListWidget.tsx`:
   - Semantic `<ul role="list">` (`role="list"` preserves list semantics in Safari when CSS flex/grid removes bullets, per `modern-web-guidance` `accessibility`).
   - Compact horizontal row card (`bg-panel/80 hover:bg-panel-hover border border-border-subtle rounded-xl p-2.5 sm:p-3 flex items-center gap-3 sm:gap-4`):
     - 16:9 thumbnail (`w-24 sm:w-28 aspect-video rounded-lg overflow-hidden shrink-0`) using `backdrop_url || poster_url` with subtle play overlay and watched indicator.
     - Main info button (`<button type="button" onClick={() => onSelectItem(item)}>`): `title`, `subtitle`, `badge` (`RESUME` guarded by `!watched`), star `rating`, and thin bottom progress bar when `hasProgress && !watched`.
     - Right cluster: `duration_seconds` pill (`1h 17m` for non-`live_channel` items with `duration_seconds > 0`), `<FavoriteButton variant="icon" alwaysVisible ... />`, `<WatchStateButton variant="icon" alwaysVisible ... />` (when `media_type !== 'show' && media_type !== 'season' && media_type !== 'live_channel'`), and a compact `Play` `<button type="button">` (`min-w-7 min-h-7`).
   - Deduplicate initial & paginated items by `id`, subscribe to `onFavoriteChanged` and `onWatchStateChanged`, and render `"Load More"` when `nextCursor` is present.
4. Update `web/src/components/browse/BrowseScreen.tsx`:
   - Pass `{ targetDevice: detectClientTargetDevice(), localTime: formatLocalTimeHHMM() }` in `fetchScreen` and `refreshHome`, and forward `targetDevice` to `GridWidget` / `CarouselWidget` / `DenseListWidget` for `"Load More"` requests.
   - Render `<DenseListWidget />` when `widget.type === 'dense_list'`.

- [ ] **Step 4: Run Vitest & TypeScript check to verify GREEN**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npm test -- --run && npx tsc --noEmit
```
Expected: All Vitest suites PASS and `tsc --noEmit` exits `0`.

- [ ] **Step 5: Commit Task 4**

```bash
git add web
git commit -m "feat(web): add DenseListWidget and wire target_device and local_time into BrowseScreen"
```

---

### Task 5: Personal & Admin `LayoutStudio`, `WidgetConfigModal` (`dense_list` + `time_window`), Top-Bar `Studio` Navigation & Documentation

**Files:**
- Modify: `web/src/components/studio/WidgetConfigModal.tsx`
- Modify: `web/src/components/studio/WidgetConfigModal.test.tsx`
- Modify: `web/src/components/studio/LayoutStudio.tsx`
- Modify: `web/src/components/studio/LayoutStudio.test.tsx`
- Modify: `web/src/components/admin/AdminDashboard.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.test.tsx`
- Modify: `README.md`
- Modify: `docs/CLIENT_GUIDE.md`

**Interfaces:**
- Consumes: `TargetDevice`, `TimeWindow`, `ResolvedLayoutScope`, `<DenseListWidget />`, `api.getScreen`, `api.saveScreen`, `api.deleteScreen`, `api.getProfiles`, `isTimeWindowActive`, `formatLocalTimeHHMM`.
- Produces:
  - `WidgetConfigModal`: **`Dense List`** (`dense_list`) widget presentation option and **Active Time Window** (`time_window?: TimeWindow`) editor with `Start Time` / `End Time` `<input type="time">` controls, overnight indicator badge (`start > end`), and validation (`start !== end`).
  - `LayoutStudio` (`currentUser?: User | null`, `defaultScope?: 'global' | 'personal'`):
    - Scope Owner selector (`Global (All Users)` + household profiles for `Admin`; locked **`Personal Layout · {username}`** badge for `Standard` users with Admin-only `"New Custom Screen"` hidden).
    - Target Device selector (`All Devices`, `Web`, `TV`, `Mobile`) + **`Custom Override Active`** / **`Inherited Default`** badge + **`Reset Override`** action when `resolved_scope?.is_exact_override` is `true` on a non-factory scope.
    - **Simulate Time** (`Live` vs custom `HH:MM` `<input type="time">`) toolbar control showing `time_window` badges (`23:00–04:00`) and dimming/badging inactive shelves (`Inactive at HH:MM`) in both the Widget Tree Inspector and canvas preview, plus `StudioDenseList` preview rendering.
  - Top-bar **`Studio`** navigation item in `web/src/App.tsx` for all authenticated profiles (`defaultScope="personal"`).
  - Updated `README.md` (`001–016` migrations, scoped layouts, `time_window`, `dense_list`, test counts) and `docs/CLIENT_GUIDE.md` (`TargetDevice`, `TimeWindow`, `DenseList`, `CardViewModel.duration_seconds`, scoped `/api/v1/screens` query params & RBAC).

- [ ] **Step 1: Write failing tests in `WidgetConfigModal.test.tsx`, `LayoutStudio.test.tsx`, and `App.test.tsx`**

1. In `web/src/components/studio/WidgetConfigModal.test.tsx`, add tests verifying:
   - Selecting **`Dense List`** (`dense_list`) and enabling **Active Time Window** (`start = "23:00"`, `end = "04:00"`) displays the **Overnight window (crosses midnight)** badge and emits `{ type: 'dense_list', ..., time_window: { start: '23:00', end: '04:00' } }` on save.
   - Setting `start === end` (`"10:00"` and `"10:00"`) shows an accessible validation message and disables/blocks saving until corrected.
   - Editing an existing widget with `time_window` pre-populates the toggle and time inputs and allows clearing `time_window` back to `undefined`.
2. In `web/src/components/studio/LayoutStudio.test.tsx`, add tests verifying:
   - **Standard user (`role: 'standard'`)**: locks scope to `Personal Layout · {username}`, hides `"New Screen"`, fetches `getScreen('home', true, { scopeUserId: user.id, targetDevice: 'all' })`, saves with `{ scopeUserId: user.id, targetDevice: 'tv' }` when `TV` target is chosen, and resets a personal override via `deleteScreen('home', { scopeUserId: user.id, targetDevice: 'tv' })`.
   - **Admin user (`role: 'admin'`)**: renders the Scope Owner `<select>` (`Global (All Users)` + household profiles from `api.getProfiles()`), switches scope + `targetDevice`, renders `dense_list` widgets via `StudioDenseList`, and marks a `23:00–04:00` widget as `Inactive at 14:00` when **Simulate Time** is set to `14:00` vs active at `23:30`.
3. In `web/src/App.test.tsx`, add a test verifying a logged-in `Standard` user (who does **not** see the `Admin` tab) sees the top-bar **`Studio`** button (`aria-current="page"` when clicked) and opens personal `LayoutStudio` (`Personal Layout · ...`).

- [ ] **Step 2: Run targeted Vitest suites to confirm RED failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/components/studio/WidgetConfigModal.test.tsx src/components/studio/LayoutStudio.test.tsx src/App.test.tsx
```
Expected: FAIL (`Dense List` option, `time_window` controls, Scope/Simulate Time selectors, and top-bar `Studio` button missing).

- [ ] **Step 3: Implement `WidgetConfigModal`, `LayoutStudio`, `AdminDashboard`, `App.tsx`, and documentation**

1. **`web/src/components/studio/WidgetConfigModal.tsx`:**
   - Expand `widgetType` union to `'hero_banner' | 'carousel' | 'grid' | 'dense_list'` and add `<option value="dense_list">Dense List</option>`.
   - Add `timeWindowEnabled`, `timeWindowStart` (default `'23:00'`), and `timeWindowEnd` (default `'04:00'`) state initialized from `initialWidget.time_window` when editing.
   - Render accessible toggle `<button type="button" aria-pressed={timeWindowEnabled}>` + `<label htmlFor={startId}>Start Time</label>` `<input id={startId} type="time" ... />` and `<label htmlFor={endId}>End Time</label>` `<input id={endId} type="time" ... />`, showing **Overnight window (crosses midnight)** when `start > end`, **Same-day window** when `start < end`, and inline error when `start === end`.
   - Attach `time_window` (when enabled) to `hero_banner`, `carousel`, `grid`, and `dense_list` nodes on submit.
2. **`web/src/components/studio/LayoutStudio.tsx` & `AdminDashboard.tsx`:**
   - Accept `currentUser?: User | null` and `defaultScope?: 'global' | 'personal'` (when `currentUser` is omitted in legacy unit tests, preserve existing `'global'` admin behavior so all 19 existing `LayoutStudio.test.tsx` tests continue passing unchanged!).
   - Add Scope Owner `<select>` (for `Admin`) vs **`Personal Layout · ${currentUser.username}`** pill (for `Standard`), **Target Device** selector (`all`, `web`, `tv`, `mobile`), **`Custom Override Active`** / **`Inherited Default`** status badge + **`Reset Override`** button, **Simulate Time** control (`Live` vs custom `<input type="time">`), `time_window` shelf badges (`23:00–04:00` + `Inactive at HH:MM` dimmed state in preview), and `StudioDenseList` wrapper rendering `<DenseListWidget />`.
   - Pass `currentUser={currentUser} defaultScope="global"` in `AdminDashboard.tsx`.
3. **`web/src/App.tsx`:**
   - Add `'studio'` to `NavView`, render top-bar **`Studio`** button (`Layers` icon with `aria-hidden="true"` and `aria-current={currentView === 'studio' ? 'page' : undefined}`) for all authenticated users, and mount `<LayoutStudio currentUser={currentUser} defaultScope="personal" onPlayItem={handlePlayItem} />` when `currentView === 'studio'` (with `returnLabel="Back to Studio"` in `CinemaPlayer`).
4. **`README.md` & `docs/CLIENT_GUIDE.md`:**
   - Update migration range (`001–016`), Key Features (Per-User & Per-Device Scoped Layouts, Time-Window Shelves, `DenseList` presentation, Personal Studio for Standard profiles), REST API Reference (`GET`/`PUT`/`DELETE /api/v1/screens/:screen_id` query parameters & RBAC), `docs/CLIENT_GUIDE.md` AST reference (`TargetDevice`, `TimeWindow`, `WidgetNode::DenseList`, `CardViewModel.duration_seconds`, `ResolvedLayoutScope`, 4-tier fallback matrix, `X-Kadr-Device` / `X-Kadr-Local-Time` headers), and test counts.

- [ ] **Step 4: Run full Web & Rust workspace verification**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npm test -- --run && npm run build
cd /home/boody/work/personal/kadr && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check && cargo check --workspace --target x86_64-unknown-linux-musl
```
Expected: All Vitest suites PASS, `npm run build` succeeds cleanly, all Rust workspace tests PASS, Clippy & `rustfmt` exit `0`, and `musl` check succeeds cleanly.

- [ ] **Step 5: Commit Task 5**

```bash
git add web README.md docs/CLIENT_GUIDE.md
git commit -m "feat(studio): add personal & device-scoped Layout Studio, time_window editor, and docs"
```
