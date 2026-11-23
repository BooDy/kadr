# Per-User Favorites / Watchlist (`v0.4` Part 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver Kadr's per-profile isolated `user_favorites` system across SQLite storage (`015_user_favorites.sql`), the Declarative Widget & Filter AST engine (`QueryMacro::Favorites` + `FilterCondition::IsFavorite`), REST/SSE APIs (`/api/v1/favorites`, `favorite:updated`), and the React Web UI (`FavoriteButton`, `FavoritesScreen` with queue playback, default `Home` rail, and Layout Studio).

**Architecture:** Store `(user_id, media_item_id, favorited_at)` in `user_favorites` with cascading foreign keys and a composite `(user_id, favorited_at DESC, media_item_id DESC)` index. Batch-hydrate `is_favorite` onto `CardViewModel` in `WidgetResolver` alongside `PlaybackState` (zero per-card queries), compile `FilterCondition::IsFavorite` into parameterized `EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?)` subqueries, enforce fail-closed private-library isolation (`403 LIBRARY_LOCKED` & `locked_item_count`), and synchronize optimistic heart toggles across all mounted React views.

**Tech Stack:** Rust 1.80+ (`1.100.0`), Axum, `rusqlite` / `deadpool-sqlite` (WAL), React 19, TypeScript 5.7, Vite 6, Vitest 3 + React Testing Library, Tailwind CSS 4, `lucide-react`.

## Global Constraints

- **Pure-Rust & Zero New Dependencies:** Do not add any external Cargo or npm packages (`<= 30 MB` RSS budget, clean `x86_64-unknown-linux-musl` compilation, zero Clippy warnings under `-D warnings`).
- **Fail-Closed Private Library Isolation:** Items in PIN-locked private libraries (`libraries.is_private = 1` not present in signed `UnlockedLibraries` tokens) MUST never appear in `GET /api/v1/favorites` items, `QueryMacro::Favorites`, or `FilterCondition::IsFavorite` results, MUST return `403 Forbidden` (`LIBRARY_LOCKED`) on `PUT`/`DELETE /api/v1/favorites/{item_id}`, and `favorite:updated` SSE payloads MUST carry IDs only (`user_id`, `item_id`, `is_favorite`, `timestamp`).
- **`InProgress`-Only Resume Hydration:** `FavoriteItemView.resume_position_seconds` MUST only populate `Some(pos)` when `watch_state == WatchState::InProgress && pos > 0` so completed items start at `0s` when launched via `Play All` / `Shuffle`.
- **Backwards-Compatible Compact JSON:** `CardViewModel.is_favorite` and `SearchResultItem.is_favorite` use `#[serde(default, skip_serializing_if = "std::ops::Not::not")]` so `"is_favorite": false` is omitted from serialized JSON payloads while `"is_favorite": true` is emitted.
- **Accessibility & Card Cluster Coexistence:** `<FavoriteButton />` uses disambiguated `aria-label` (`Add ${title} to favorites` / `Remove ${title} from favorites`), `aria-pressed={isFavorite}`, `aria-hidden="true"` on decorative `Heart` SVGs, `min-w-7 min-h-7` (`>= 24×24px`), `e.stopPropagation()` on click and `Enter`/`Space` keydown, and sits cleanly side-by-side with `<WatchStateButton variant="icon" />` inside `absolute top-2 right-2 z-20 flex items-center gap-1.5` on poster cards.
- **Working Branch & Git Safety:** Commit directly on `main` in `/home/boody/work/personal/kadr`; never run `git push` or create git tags unless explicitly requested.

---

## File Map

- **Create:** `crates/kadr-core/src/favorites.rs` — `FavoriteItemView`, `FavoritesListResponse`, `FavoriteStatusResponse`.
- **Modify:** `crates/kadr-core/src/lib.rs`, `ast.rs`, `search.rs`, `events.rs`, `filter_ast.rs` — `CardViewModel.is_favorite`, `SearchResultItem.is_favorite`, `QueryMacro::Favorites`, `SystemEvent::FavoriteUpdated`, `BoolOp::Eq`, `FilterCondition::IsFavorite`, `FilterNode::needs_favorite_state`, `FilterNode::matches_with_context`.
- **Modify:** `crates/kadr-core/tests/filter_ast_test.rs` — serde & evaluation unit tests for Favorites AST and wire types.
- **Create:** `crates/kadr-storage/src/migrations/015_user_favorites.sql` — `user_favorites` table & composite index.
- **Modify:** `crates/kadr-storage/src/migrations.rs` — register `015_user_favorites.sql`.
- **Create:** `crates/kadr-storage/src/repos/favorites_repo.rs` — `FavoritesRepository` (`add_favorite`, `remove_favorite`, `is_favorite`, `get_favorited_ids`, `list_favorites`).
- **Modify:** `crates/kadr-storage/src/repos/mod.rs`, `filter_compiler.rs`, `widget_queries.rs`, `search_repo.rs` — `compile_filter_node_for_user`, `build_filter_envelope_for_user`, and `SearchRepository` `is_favorite` hydration.
- **Create:** `crates/kadr-storage/tests/favorites_repo_test.rs` — storage, privacy, cascade, and SQL-vs-Rust filter parity tests.
- **Create:** `crates/kadr-server/src/api/favorite_routes.rs` — `GET /api/v1/favorites`, `PUT /api/v1/favorites/{item_id}`, `DELETE /api/v1/favorites/{item_id}`.
- **Modify:** `crates/kadr-server/src/api/mod.rs`, `playlist_routes.rs`, `library_routes.rs`, `resolver/card.rs`, `resolver/mod.rs`, `resolver/resolver.rs`, `layout/registry.rs` — route registration, batch `is_favorite` hydration across all card surfaces, `QueryMacro::Favorites` resolution, and default `Home` `"my_favorites"` rail.
- **Modify:** `crates/kadr-server/tests/layout_registry_test.rs`, `widget_resolver_test.rs`, `screen_routes_test.rs` — update default `Home` widget count assertions (`6` $\to$ `7`).
- **Create:** `crates/kadr-server/tests/e2e_favorites_test.rs` — end-to-end HTTP, privacy, hydration, Studio preview, and SSE tests.
- **Modify:** `web/src/types/index.ts`, `web/src/types/search.ts`, `web/src/types/filterAst.ts`, `web/src/api/client.ts`, `web/src/api/client.test.ts` — TypeScript types & `ApiClient` favorite methods.
- **Create:** `web/src/utils/favoriteEvents.ts` — cross-view custom event sync (`emitFavoriteChanged`, `onFavoriteChanged`, `applyFavoriteToCard`).
- **Create:** `web/src/components/favorites/FavoriteButton.tsx` & `FavoriteButton.test.tsx` — reusable `icon` and `pill` heart toggle component.
- **Modify:** `web/src/components/browse/CarouselWidget.tsx`, `GridWidget.tsx`, `SpotlightWidget.tsx`, `ItemDetailsModal.tsx`, `BrowseScreen.tsx`, `ItemDetailsModal.test.tsx`, `BrowseScreen.test.tsx` — wire `<FavoriteButton />` and `onFavoriteChanged`.
- **Create:** `web/src/components/favorites/FavoritesScreen.tsx` & `FavoritesScreen.test.tsx` — dedicated top-bar `Favorites` screen with filter pills, sorting, locked-library count notice, and `Play All` / `Shuffle` queue playback.
- **Modify:** `web/src/App.tsx`, `web/src/App.test.tsx`, `web/src/components/studio/WidgetConfigModal.tsx`, `LayoutStudio.tsx`, `RuleBuilderEditor.tsx`, `WidgetConfigModal.test.tsx`, `RuleBuilderEditor.test.tsx`, `README.md`, `docs/CLIENT_GUIDE.md` — top navigation, Studio `favorites` macro & `is_favorite` rule, and documentation parity.

---

### Task 1: Core Domain & Declarative Filter AST Extensions (`crates/kadr-core`)

**Files:**
- Create: `crates/kadr-core/src/favorites.rs`
- Modify: `crates/kadr-core/src/lib.rs`
- Modify: `crates/kadr-core/src/ast.rs:110-150`
- Modify: `crates/kadr-core/src/search.rs:53-75`
- Modify: `crates/kadr-core/src/events.rs:12-67`
- Modify: `crates/kadr-core/src/filter_ast.rs`
- Modify: `crates/kadr-core/tests/filter_ast_test.rs`

**Interfaces:**
- Consumes: `CardViewModel`, `MediaItem`, `PlaybackState`, `FilterNode`, `FilterCondition`, `SystemEvent`.
- Produces:
  - `kadr_core::favorites::{FavoriteItemView, FavoritesListResponse, FavoriteStatusResponse}` (re-exported at `kadr_core::*`).
  - `CardViewModel.is_favorite: bool` & `SearchResultItem.is_favorite: bool` (`#[serde(default, skip_serializing_if = "std::ops::Not::not")]`).
  - `QueryMacro::Favorites` (`"favorites"`).
  - `SystemEvent::FavoriteUpdated { user_id: String, item_id: i64, is_favorite: bool, timestamp: i64 }` (`event_type() -> "favorite:updated"`).
  - `BoolOp::Eq` (`#[derive(Default)]`) and `FilterCondition::IsFavorite { #[serde(default)] op: BoolOp, value: bool }`.
  - `FilterNode::needs_favorite_state(&self) -> bool` and `FilterNode::matches_with_context(&self, item: &MediaItem, playback: Option<&PlaybackState>, is_favorite: bool, now_epoch: i64) -> bool` (with `FilterNode::matches(item, playback, now_epoch)` delegating to `matches_with_context(..., false, now_epoch)`).

- [ ] **Step 1: Write failing unit tests in `crates/kadr-core/tests/filter_ast_test.rs`**

Append tests to `crates/kadr-core/tests/filter_ast_test.rs` covering:
```rust
#[test]
fn favorites_ast_and_wire_types_serde_and_eval() {
    use kadr_core::{
        BoolOp, CardViewModel, FavoriteStatusResponse, FilterCondition, FilterNode, LogicalOp,
        QueryMacro, SystemEvent,
    };

    // QueryMacro::Favorites
    let macro_json = serde_json::to_string(&QueryMacro::Favorites).unwrap();
    assert_eq!(macro_json, "\"favorites\"");
    let parsed_macro: QueryMacro = serde_json::from_str(&macro_json).unwrap();
    assert_eq!(parsed_macro, QueryMacro::Favorites);

    // CardViewModel omits is_favorite when false, includes when true
    let default_card = CardViewModel {
        id: 1,
        title: "Film".to_string(),
        media_type: "movie".to_string(),
        ..Default::default()
    };
    let card_false_json = serde_json::to_string(&default_card).unwrap();
    assert!(!card_false_json.contains("is_favorite"));

    let fav_card = CardViewModel {
        is_favorite: true,
        ..default_card
    };
    let card_true_json = serde_json::to_string(&fav_card).unwrap();
    assert!(card_true_json.contains("\"is_favorite\":true"));

    // FilterCondition::IsFavorite accepts both explicit op="eq" and omitted op
    let with_op: FilterCondition =
        serde_json::from_str(r#"{"field":"is_favorite","op":"eq","value":true}"#).unwrap();
    let without_op: FilterCondition =
        serde_json::from_str(r#"{"field":"is_favorite","value":true}"#).unwrap();
    assert_eq!(
        with_op,
        FilterCondition::IsFavorite {
            op: BoolOp::Eq,
            value: true
        }
    );
    assert_eq!(with_op, without_op);

    let node = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![FilterNode::Rule {
            negated: false,
            condition: with_op,
        }],
    };
    node.validate().unwrap();
    assert!(node.needs_favorite_state());

    let item = sample_movie();
    assert!(!node.matches(&item, None, 1_700_000_000));
    assert!(node.matches_with_context(&item, None, true, 1_700_000_000));
    assert!(!node.matches_with_context(&item, None, false, 1_700_000_000));

    // SystemEvent::FavoriteUpdated
    let evt = SystemEvent::FavoriteUpdated {
        user_id: "u1".to_string(),
        item_id: 42,
        is_favorite: true,
        timestamp: 12345,
    };
    assert_eq!(evt.event_type(), "favorite:updated");
    let status = FavoriteStatusResponse {
        item_id: 42,
        is_favorite: true,
    };
    assert!(status.is_favorite);
}
```

- [ ] **Step 2: Run `cargo test -p kadr-core --test filter_ast_test` to confirm RED failure**

Run:
```bash
cargo test -p kadr-core --test filter_ast_test
```
Expected: FAIL (`QueryMacro::Favorites`, `BoolOp`, `FavoriteStatusResponse`, `is_favorite`, `matches_with_context` not found).

- [ ] **Step 3: Implement `favorites.rs` and extensions in `ast.rs`, `search.rs`, `events.rs`, `filter_ast.rs`, and `lib.rs`**

Create `crates/kadr-core/src/favorites.rs`:
```rust
use serde::{Deserialize, Serialize};

use crate::ast::CardViewModel;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FavoriteItemView {
    pub favorited_at: i64,
    pub duration_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_position_seconds: Option<i64>,
    pub card: CardViewModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FavoritesListResponse {
    pub items: Vec<FavoriteItemView>,
    pub total_count: u64,
    pub locked_item_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteStatusResponse {
    pub item_id: i64,
    pub is_favorite: bool,
}
```

Update `crates/kadr-core/src/lib.rs` to export `pub mod favorites; pub use favorites::*;`.
Update `CardViewModel` in `crates/kadr-core/src/ast.rs` and `SearchResultItem` in `crates/kadr-core/src/search.rs` to add:
```rust
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_favorite: bool,
```
Add `Favorites,` to `QueryMacro` in `crates/kadr-core/src/ast.rs`.
Add `FavoriteUpdated` to `SystemEvent` and `SystemEvent::event_type` in `crates/kadr-core/src/events.rs`.
In `crates/kadr-core/src/filter_ast.rs`:
- Add `BoolOp` (`#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")] pub enum BoolOp { #[default] Eq }`).
- Add `IsFavorite { #[serde(default)] op: BoolOp, value: bool }` to `FilterCondition` (`validate()` returns `Ok(())`).
- Add `FilterNode::needs_favorite_state(&self) -> bool` and `FilterNode::matches_with_context(&self, item: &MediaItem, playback: Option<&PlaybackState>, is_favorite: bool, now_epoch: i64) -> bool` (evaluating `FilterCondition::IsFavorite { value, .. } => is_favorite == *value`, while `FilterNode::matches` delegates with `is_favorite = false`).

- [ ] **Step 4: Run `cargo test -p kadr-core` to confirm GREEN**

Run:
```bash
cargo test -p kadr-core
```
Expected: PASS (all unit and integration tests in `kadr-core` green).

- [ ] **Step 5: Commit Task 1**

```bash
git add crates/kadr-core
git commit -m "feat(core): add user favorites models, Favorites macro, and IsFavorite filter AST condition"
```

---

### Task 2: SQLite Migration `015_user_favorites.sql`, `FavoritesRepository`, SQL Compiler & `SearchRepository` (`crates/kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/migrations/015_user_favorites.sql`
- Modify: `crates/kadr-storage/src/migrations.rs`
- Create: `crates/kadr-storage/src/repos/favorites_repo.rs`
- Modify: `crates/kadr-storage/src/repos/mod.rs`
- Modify: `crates/kadr-storage/src/repos/filter_compiler.rs`
- Modify: `crates/kadr-storage/src/repos/widget_queries.rs`
- Modify: `crates/kadr-storage/src/repos/search_repo.rs`
- Create: `crates/kadr-storage/tests/favorites_repo_test.rs`

**Interfaces:**
- Consumes: `kadr_core::filter_ast::{BoolOp, FilterCondition, FilterNode}`, `kadr_core::models::{MediaItem, MediaType}`, `kadr_core::search::SearchResultItem`.
- Produces:
  - `kadr_storage::repos::FavoritesRepository` (and alias `FavoritesRepo`) with:
    - `new(pool: Pool) -> Self`
    - `add_favorite(&self, user_id: &str, media_item_id: i64, now_epoch: i64) -> Result<()>`
    - `remove_favorite(&self, user_id: &str, media_item_id: i64) -> Result<bool>`
    - `is_favorite(&self, user_id: &str, media_item_id: i64) -> Result<bool>`
    - `get_favorited_ids(&self, user_id: &str, item_ids: &[i64]) -> Result<HashSet<i64>>`
    - `list_favorites(&self, user_id: &str, library_id: Option<&str>, item_type: Option<MediaType>, limit: u32, offset: u32, sort: Option<&str>, unlocked_ids: &[String], filters: Option<&WidgetFilterConfig>) -> Result<(Vec<(i64, MediaItem)>, u64, u64)>`
  - `compile_filter_node_for_user(node: &FilterNode, user_id: Option<&str>, now_epoch: i64) -> CompiledFilter` (and `build_filter_envelope_for_user(filters: Option<&WidgetFilterConfig>, user_id: Option<&str>, unlocked_ids: &[String], current_time_epoch_secs: i64) -> CompiledFilter`), keeping `compile_filter_node` and `build_filter_envelope` delegating with `user_id = None`.
  - `SearchRepository::search` populating `SearchResultItem.is_favorite`.

- [ ] **Step 1: Write failing storage & parity tests in `crates/kadr-storage/tests/favorites_repo_test.rs`**

Create `crates/kadr-storage/tests/favorites_repo_test.rs` testing:
1. Per-user isolation (`u1` vs `u2`), idempotent `add_favorite` preserving original `favorited_at`, `get_favorited_ids` batch lookup, and `remove_favorite`.
2. `ON DELETE CASCADE` when a `User` or `MediaItem` row is deleted.
3. `list_favorites` with `favorited_at:desc` (default), `favorited_at:asc`, `rating:desc`, `item_type` filtering (including `MediaType::Episode` and `MediaType::Show`), and fail-closed private library isolation (`locked_item_count` incremented when locked, zero items leaked, and revealed once `unlocked_ids` contains the private library ID).
4. `FilterCondition::IsFavorite { op: BoolOp::Eq, value: true | false }` (and negated `Rule { negated: true, ... }`) SQL-vs-Rust parity via `media_repo.find_custom_paginated` vs `tree.matches_with_context(...)`.
5. `SearchRepository::search` returning `is_favorite: true` for `u1` and `is_favorite: false` for `u2` on the same indexed item.

- [ ] **Step 2: Run `cargo test -p kadr-storage --test favorites_repo_test` to confirm RED failure**

Run:
```bash
cargo test -p kadr-storage --test favorites_repo_test
```
Expected: FAIL (`FavoritesRepository` does not exist).

- [ ] **Step 3: Implement `015_user_favorites.sql`, `FavoritesRepository`, `filter_compiler.rs`, `widget_queries.rs`, and `search_repo.rs`**

1. Create `crates/kadr-storage/src/migrations/015_user_favorites.sql`:
```sql
CREATE TABLE IF NOT EXISTS user_favorites (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    favorited_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_user_favorites_user_recent
    ON user_favorites(user_id, favorited_at DESC, media_item_id DESC);
```
Register `M::up(include_str!("migrations/015_user_favorites.sql"))` in `crates/kadr-storage/src/migrations.rs`.

2. In `crates/kadr-storage/src/repos/filter_compiler.rs`:
   - Add `pub fn compile_filter_node_for_user(node: &FilterNode, user_id: Option<&str>, now_epoch: i64) -> CompiledFilter` (threading `user_id` into `compile_inner` and `compile_condition`), and keep `pub fn compile_filter_node(node: &FilterNode, now_epoch: i64) -> CompiledFilter` calling `compile_filter_node_for_user(node, None, now_epoch)`.
   - Handle `FilterCondition::IsFavorite { value, .. }`:
     ```rust
     FilterCondition::IsFavorite { value, .. } => {
         state
             .params
             .push(Value::Text(user_id.unwrap_or("").to_string()));
         if *value {
             "(EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?))"
                 .to_string()
         } else {
             "(NOT EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?))"
                 .to_string()
         }
     }
     ```

3. In `crates/kadr-storage/src/repos/widget_queries.rs`:
   - Add `pub fn build_filter_envelope_for_user(filters: Option<&WidgetFilterConfig>, user_id: Option<&str>, unlocked_ids: &[String], current_time_epoch_secs: i64) -> CompiledFilter` (using `compile_filter_node_for_user(tree, user_id, current_time_epoch_secs)`), keeping `build_filter_envelope` delegating with `None`.
   - Update all `*_for_user` and `find_custom_paginated` queries in `widget_queries.rs` to call `build_filter_envelope_for_user(filters_owned.as_ref(), Some(&uid), &unlocked, now)`.

4. Create `crates/kadr-storage/src/repos/favorites_repo.rs`:
   - Implement `FavoritesRepository` (`Clone`, wrapping `Pool`) with `add_favorite` (`INSERT OR IGNORE`), `remove_favorite`, `is_favorite`, `get_favorited_ids`, and `list_favorites`.
   - In `list_favorites`:
     - Compute `locked_count`: `SELECT COUNT(*) FROM user_favorites uf JOIN media_items m ON m.id = uf.media_item_id JOIN libraries l ON l.id = m.library_id WHERE uf.user_id = ? AND l.is_private = 1 [AND l.id NOT IN (...unlocked...)] [AND m.library_id = ?] [AND m.item_type = ?]`.
     - Resolve sort: `"favorited_at:asc" | "favorited_at_asc"` $\to$ `("uf.favorited_at ASC, m.id ASC", false)`, `"favorited_at:desc" | "favorited_at_desc" | "" | None` $\to$ `("uf.favorited_at DESC, m.id DESC", false)`, else delegate to `resolve_sort_clause(sort, "uf.favorited_at DESC, m.id DESC")`.
     - Build `envelope = build_filter_envelope_for_user(filters, Some(&uid), &unlocked, now)`, run `SELECT COUNT(*)` for `visible_total`, then `SELECT {SELECT_JOINED_COLUMNS}, uf.favorited_at FROM user_favorites uf JOIN media_items m ON m.id = uf.media_item_id JOIN libraries l ON l.id = m.library_id {playback_join} WHERE uf.user_id = ? {lib_cond} {type_cond} AND {envelope.sql} ORDER BY {order_clause} LIMIT ? OFFSET ?` (do NOT exclude `m.item_type = 'episode'` by default so favorited episodes appear!).
   - Re-export `FavoritesRepository`, `FavoritesRepo`, `compile_filter_node_for_user`, and `build_filter_envelope_for_user` in `crates/kadr-storage/src/repos/mod.rs`.

5. In `crates/kadr-storage/src/repos/search_repo.rs`:
   - Add `EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?)` as column 13 in `select_sql` (binding a second copy of `uid` right before the `LEFT JOIN user_playback_states` `uid` param) and set `is_favorite: row.get::<_, i64>(13).unwrap_or(0) != 0` on `SearchResultItem`.

- [ ] **Step 4: Run `cargo test -p kadr-storage` to confirm GREEN**

Run:
```bash
cargo test -p kadr-storage
```
Expected: PASS (all storage unit and integration tests pass, zero warnings).

- [ ] **Step 5: Commit Task 2**

```bash
git add crates/kadr-storage
git commit -m "feat(storage): add 015_user_favorites migration, FavoritesRepository, and IsFavorite SQL compiler"
```

---

### Task 3: Server REST Routes (`favorite_routes.rs`), Batch Card Hydration & Default `Home` Rail (`crates/kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/favorite_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Modify: `crates/kadr-server/src/resolver/card.rs`
- Modify: `crates/kadr-server/src/resolver/mod.rs`
- Modify: `crates/kadr-server/src/resolver/resolver.rs`
- Modify: `crates/kadr-server/src/api/playlist_routes.rs`
- Modify: `crates/kadr-server/src/api/library_routes.rs`
- Modify: `crates/kadr-server/src/layout/registry.rs`
- Modify: `crates/kadr-server/tests/layout_registry_test.rs`
- Modify: `crates/kadr-server/tests/widget_resolver_test.rs`
- Modify: `crates/kadr-server/tests/screen_routes_test.rs`
- Create: `crates/kadr-server/tests/e2e_favorites_test.rs`

**Interfaces:**
- Consumes: `FavoritesRepository`, `FavoriteItemView`, `FavoritesListResponse`, `FavoriteStatusResponse`, `QueryMacro::Favorites`, `SystemEvent::FavoriteUpdated`.
- Produces:
  - `to_card_view_model_with_favorite(item: &MediaItem, playback: Option<&PlaybackState>, is_favorite: bool) -> CardViewModel`
  - `GET /api/v1/favorites?item_type=&library_id=&sort=&limit=&offset=` $\to$ `200 OK` `Json<FavoritesListResponse>`
  - `PUT /api/v1/favorites/{item_id}` $\to$ `200 OK` `Json<FavoriteStatusResponse>` (`404`, `403 LIBRARY_LOCKED`)
  - `DELETE /api/v1/favorites/{item_id}` $\to$ `200 OK` `Json<FavoriteStatusResponse>` (`404`, `403 LIBRARY_LOCKED`)
  - Default `Home` widget `"my_favorites"` (`QueryMacro::Favorites`, `"My Favorites"`) immediately after `"continue_watching"`.

- [ ] **Step 1: Write failing E2E tests in `crates/kadr-server/tests/e2e_favorites_test.rs`**

Create `crates/kadr-server/tests/e2e_favorites_test.rs` testing:
1. `PUT /api/v1/favorites/{id}` & `DELETE /api/v1/favorites/{id}`: `401` unauthenticated, `404` unknown item, `403 LIBRARY_LOCKED` on locked private-library item, `200 OK` `{ "item_id": id, "is_favorite": true | false }`, per-user isolation (`admin` vs `standard` user), and `favorite:updated` SSE broadcast on `EventBus`.
2. `GET /api/v1/favorites`: returns `FavoritesListResponse` with `is_favorite: true` on cards, `InProgress`-only `resume_position_seconds` (`None` when item is marked completed via `PUT /api/v1/playback/states/{id}`), `item_type` filtering, and `locked_item_count` accuracy before/after unlocking a private library.
3. Full surface hydration: `GET /api/v1/screens/home` includes `"my_favorites"` carousel hydrated with the user's favorited items AND sets `is_favorite: true` on matching cards in `recently_added` / `home_spotlight`, `GET /api/v1/items/{id}` (`card.is_favorite`), `GET /api/v1/playlists/{id}` (`items[0].card.is_favorite`), `GET /api/v1/search?q=...` (`items[0].is_favorite`), and `POST /api/v1/widgets/preview` with `FilterCondition::IsFavorite`.

- [ ] **Step 2: Run `cargo test -p kadr-server --test e2e_favorites_test` to confirm RED failure**

Run:
```bash
cargo test -p kadr-server --test e2e_favorites_test
```
Expected: FAIL (`404 Not Found` on `/api/v1/favorites/...`).

- [ ] **Step 3: Implement card hydration, `WidgetResolver`, `default_home_layout`, and `favorite_routes.rs`**

1. **`crates/kadr-server/src/resolver/card.rs` & `resolver/mod.rs`:**
   - Add `pub fn to_card_view_model_with_favorite(item: &MediaItem, playback: Option<&PlaybackState>, is_favorite: bool) -> CardViewModel` and keep `to_card_view_model(item, playback)` delegating with `false`. Re-export both in `resolver/mod.rs`.

2. **`crates/kadr-server/src/resolver/resolver.rs`:**
   - Add `favorites_repo: FavoritesRepository` field to `WidgetResolver`, initialized inside `WidgetResolver::new` via `FavoritesRepository::new(media_repo.pool().clone())` (preserving the 2-arg `WidgetResolver::new(media_repo, playback_repo)` signature across all 20+ callers!).
   - In `determine_effective_library_id`: treat `QueryMacro::Favorites` identically to `QueryMacro::PlaylistItems { .. }` (returning `None` unless `filters.library_id` is explicitly set, so `Favorites` shelves span all accessible libraries by default).
   - In `is_item_matching_filters`: compute `let is_fav = if tree.needs_favorite_state() { item.id.is_some_and(|id| /* await self.favorites_repo.is_favorite(...) */) } else { false };` and call `tree.matches_with_context(item, playback_ref, is_fav, now)`.
   - Hydrate `is_favorite` across all 6 card paths in `WidgetResolver`:
     - `hydrate_items_to_cards`: batch-fetch `favorited_ids = self.favorites_repo.get_favorited_ids(user_id, &item_ids).await.unwrap_or_default()`.
     - `ContinueWatching`: batch-fetch `favorited_ids` for `item_ids` alongside `items_map`.
     - `resolve_hero_banner_data`, `SpotlightItem`, `ItemDetails`, and `resolve_item_details_with_unlocked`: check `self.favorites_repo.is_favorite(user_id, id).await.unwrap_or(false)`.
   - Add `QueryMacro::Favorites` match arm in `resolve_widget_data_with_library`:
     ```rust
     QueryMacro::Favorites => {
         let (fav_pairs, total, _locked) = self
             .favorites_repo
             .list_favorites(
                 user_id,
                 effective_library_id,
                 None,
                 binding.limit,
                 offset,
                 binding.sort.as_deref(),
                 unlocked_ids,
                 binding.filters.as_ref(),
             )
             .await?;
         let items: Vec<MediaItem> = fav_pairs.into_iter().map(|(_, item)| item).collect();
         let count = items.len() as u32;
         let has_more = (offset as u64 + count as u64) < total;
         let cards = self.hydrate_items_to_cards(items, user_id).await;
         let next_cursor = if has_more {
             Some((offset + count).to_string())
         } else {
             None
         };
         Ok((cards, next_cursor, Some(total)))
     }
     ```

3. **`playlist_routes.rs` & `library_routes.rs`:**
   - In `hydrate_playlist_detail` (`playlist_routes.rs`), construct `FavoritesRepository::new(media_repo_or_playlist_pool)` (using `playlist_repo.pool().clone()` — add `pub fn pool(&self) -> &Pool` on `PlaylistRepository` if not present) to batch-fetch `favorited_ids` for `visible_ids` and build cards with `to_card_view_model_with_favorite(&item, state, is_fav)`.
   - In `list_library_folders` (`library_routes.rs:1063`), batch-fetch `favorited_ids` via `FavoritesRepository::new(media_repo.pool().clone())` and call `to_card_view_model_with_favorite`.

4. **`crates/kadr-server/src/layout/registry.rs` & existing Home layout tests:**
   - Insert `WidgetNode::Carousel { id: "my_favorites".to_string(), title: "My Favorites".to_string(), binding: WidgetQueryBinding::new(QueryMacro::Favorites), items: None, next_cursor: None }` into `default_home_layout()` immediately after `"continue_watching"`.
   - Update the default Home widget count (`6` $\to$ `7`) and index assertions in `crates/kadr-server/tests/layout_registry_test.rs`, `crates/kadr-server/tests/widget_resolver_test.rs`, and `crates/kadr-server/tests/screen_routes_test.rs`.

5. **`crates/kadr-server/src/api/favorite_routes.rs` & `api/mod.rs`:**
   - Implement `favorite_routes()` with:
     - `GET /api/v1/favorites`: clamps `limit` to `1..=200` (default `60`), `offset` default `0`, calls `favorites_repo.list_favorites(...)`, batch-fetches `playback_repo.get_states_for_items(&auth_user.id, &visible_ids)`, maps each `(favorited_at, item)` to `FavoriteItemView { favorited_at, duration_seconds: item.technical.duration_seconds.max(0), resume_position_seconds: state.and_then(|s| if s.watch_state == WatchState::InProgress && s.playback_position_seconds > 0 { Some(s.playback_position_seconds) } else { None }), card: to_card_view_model_with_favorite(&item, state, true) }`, and returns `Json(FavoritesListResponse { items, total_count, locked_item_count })`.
     - `PUT /api/v1/favorites/{item_id}` & `DELETE /api/v1/favorites/{item_id}`: verify item exists (`404`) and library is unlocked (`403` `{ "error": "LIBRARY_LOCKED", "code": "LIBRARY_LOCKED", "library_id": lib.id }`), mutate `favorites_repo`, publish `SystemEvent::FavoriteUpdated`, and return `200 OK` `Json(FavoriteStatusResponse { item_id, is_favorite })`.
   - Register `pub mod favorite_routes;`, `.merge(favorite_routes::favorite_routes())`, and `.layer(Extension(favorites_repo))` in `crates/kadr-server/src/api/mod.rs`.

- [ ] **Step 4: Run all Rust workspace tests & Clippy**

Run:
```bash
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
```
Expected: PASS (`0` failures, `0` warnings).

- [ ] **Step 5: Commit Task 3**

```bash
git add crates/kadr-storage crates/kadr-server
git commit -m "feat(server): add /api/v1/favorites endpoints, batch card hydration, and default Home favorites rail"
```

---

### Task 4: Frontend Types, `ApiClient`, `<FavoriteButton />` & Browse/Modal Surfaces (`web/`)

**Files:**
- Modify: `web/src/types/index.ts`
- Modify: `web/src/types/search.ts`
- Modify: `web/src/types/filterAst.ts`
- Modify: `web/src/api/client.ts`
- Modify: `web/src/api/client.test.ts`
- Create: `web/src/utils/favoriteEvents.ts`
- Create: `web/src/components/favorites/FavoriteButton.tsx`
- Create: `web/src/components/favorites/FavoriteButton.test.tsx`
- Modify: `web/src/components/browse/CarouselWidget.tsx`
- Modify: `web/src/components/browse/GridWidget.tsx`
- Modify: `web/src/components/browse/SpotlightWidget.tsx`
- Modify: `web/src/components/browse/ItemDetailsModal.tsx`
- Modify: `web/src/components/browse/BrowseScreen.tsx`
- Modify: `web/src/components/browse/ItemDetailsModal.test.tsx`
- Modify: `web/src/components/browse/BrowseScreen.test.tsx`

**Interfaces:**
- Consumes: `GET /api/v1/favorites`, `PUT /api/v1/favorites/{id}`, `DELETE /api/v1/favorites/{id}`, `<WatchStateButton />`.
- Produces:
  - `CardViewModel.is_favorite?: boolean`, `SearchResultItem.is_favorite?: boolean`, `QueryMacro` union `'favorites'`, `SystemEvent` union `'favorite:updated'`, `FavoriteItemView`, `FavoritesListResponse`, `FavoriteStatusResponse`, `FavoritesQueryParams`, and `IsFavoriteFilterCondition` (`{ field: 'is_favorite'; op: 'eq'; value: boolean }`) in `web/src/types/`.
  - `api.getFavorites(params?: FavoritesQueryParams): Promise<FavoritesListResponse>`
  - `api.addFavorite(itemId: number): Promise<FavoriteStatusResponse>`
  - `api.removeFavorite(itemId: number): Promise<FavoriteStatusResponse>`
  - `applyFavoriteToCard(card: CardViewModel, isFavorite: boolean): CardViewModel`, `emitFavoriteChanged(itemId: number, isFavorite: boolean): void`, `onFavoriteChanged(handler): () => void` in `web/src/utils/favoriteEvents.ts`.
  - `<FavoriteButton />` (`web/src/components/favorites/FavoriteButton.tsx`).

- [ ] **Step 1: Write failing tests in `client.test.ts`, `FavoriteButton.test.tsx`, `ItemDetailsModal.test.tsx`, and `BrowseScreen.test.tsx`**

1. In `web/src/api/client.test.ts`, add tests for `api.getFavorites({ item_type: 'movie', sort: 'favorited_at:desc', limit: 40, offset: 0 })`, `api.addFavorite(42)` (`PUT /api/v1/favorites/42`), and `api.removeFavorite(42)` (`DELETE /api/v1/favorites/42`).
2. Create `web/src/components/favorites/FavoriteButton.test.tsx` testing:
   - Disambiguated `aria-label` (`Add Inception to favorites` $\leftrightarrow$ `Remove Inception from favorites`) and `aria-pressed` (`false` $\leftrightarrow$ `true`).
   - `stopPropagation` on click and `Enter`/`Space` keydown inside a clickable card wrapper.
   - Optimistic toggle + rollback when `api.addFavorite` / `api.removeFavorite` rejects.
   - Works on all `mediaType`s including `'show'` and `'live_channel'`.
3. Add tests in `ItemDetailsModal.test.tsx` and `BrowseScreen.test.tsx` verifying `FavoriteButton` in `ItemDetailsModal` (including TV `Show` items and `EpisodeCard` rows), `SpotlightWidget`, and `CarouselWidget`/`GridWidget` cards (coexisting alongside `WatchStateButton` and triggering silent `home` screen refresh on `onFavoriteChanged`).

- [ ] **Step 2: Run Vitest on the targeted test files to confirm RED failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/api/client.test.ts src/components/favorites/FavoriteButton.test.tsx src/components/browse/ItemDetailsModal.test.tsx src/components/browse/BrowseScreen.test.tsx
```
Expected: FAIL (`FavoriteButton` module missing; `getFavorites`/`addFavorite`/`removeFavorite` missing).

- [ ] **Step 3: Implement types, `ApiClient` methods, `favoriteEvents.ts`, `FavoriteButton.tsx`, and browse/modal integrations**

1. **Types (`web/src/types/index.ts`, `search.ts`, `filterAst.ts`):**
   - Add `is_favorite?: boolean;` to `CardViewModel` and `SearchResultItem`.
   - Add `'favorites'` to `QueryMacro`, `'favorite:updated'` to `SystemEvent`, and export `FavoriteItemView`, `FavoritesListResponse`, `FavoriteStatusResponse`, `FavoritesQueryParams`.
   - In `web/src/types/filterAst.ts`: add `'is_favorite'` to `FilterField`, export `export interface IsFavoriteFilterCondition { field: 'is_favorite'; op: 'eq'; value: boolean; }`, include it in `FilterCondition`, and validate it in `parseFilterNodeJson` (accepting omitted `op` and normalizing to `op: 'eq'`, `typeof value === 'boolean'`).

2. **`ApiClient` (`web/src/api/client.ts`):**
   - Implement `getFavorites(params?: FavoritesQueryParams)`, `addFavorite(itemId: number)`, and `removeFavorite(itemId: number)`.

3. **`web/src/utils/favoriteEvents.ts` & `web/src/components/favorites/FavoriteButton.tsx`:**
   - Implement `applyFavoriteToCard`, `emitFavoriteChanged`, and `onFavoriteChanged`.
   - Implement `FavoriteButton` (`itemId`, `title`, `isFavorite`, `variant?: 'icon' | 'pill'`, `alwaysVisible?: boolean`, `onToggled?: (isFavorite: boolean) => void`) with `Heart` icon (`aria-hidden="true"`, `fill-cta text-cta` when active), `[itemId, isFavorite]` effect sync, defensive `typeof api.addFavorite === 'function'` guard, optimistic state + rollback, and `emitFavoriteChanged`.

4. **Browse & Modal Surfaces:**
   - In `CarouselWidget.tsx` and `GridWidget.tsx`: wrap the top-right poster overlay in `<div className="absolute top-2 right-2 z-20 flex items-center gap-1.5">`, rendering `<FavoriteButton itemId={item.id} title={item.title} isFavorite={Boolean(item.is_favorite)} variant="icon" />` for ALL cards (including `show` and `live_channel`) alongside `<WatchStateButton ... />` (for non-`show`, non-`live_channel` cards), and subscribe to `onFavoriteChanged` to update `items`.
   - In `SpotlightWidget.tsx`: render `<FavoriteButton itemId={item.id} title={item.title} isFavorite={Boolean(item.is_favorite)} variant="pill" />` in the hero action bar.
   - In `BrowseScreen.tsx`: subscribe to `onFavoriteChanged` in `HydratedSpotlight` (updating `item`) and in `BrowseScreen` (silently refreshing `api.getScreen('home')` when `screenId === 'home'`).
   - In `ItemDetailsModal.tsx`: render `<FavoriteButton itemId={itemId} title={card?.title || 'Item'} isFavorite={Boolean(card?.is_favorite)} variant="pill" onToggled={...} />` in the primary action bar AND `<FavoriteButton ... variant="icon" alwaysVisible />` in `EpisodeCard` next to `WatchStateButton`, `+ Playlist`, and `Play`.

- [ ] **Step 4: Run Vitest & `tsc --noEmit` to confirm GREEN**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/api/client.test.ts src/components/favorites/FavoriteButton.test.tsx src/components/browse/ItemDetailsModal.test.tsx src/components/browse/BrowseScreen.test.tsx && npx tsc --noEmit
```
Expected: PASS (`0` failures, `0` TS errors).

- [ ] **Step 5: Commit Task 4**

```bash
git add web/src/types web/src/api web/src/utils/favoriteEvents.ts web/src/components/favorites/FavoriteButton.tsx web/src/components/favorites/FavoriteButton.test.tsx web/src/components/browse
git commit -m "feat(web): add FavoriteButton and wire heart toggles across cards, spotlight, and item details"
```

---

### Task 5: `FavoritesScreen.tsx`, Top Navigation, Layout Studio (`favorites` + `is_favorite`) & Documentation

**Files:**
- Create: `web/src/components/favorites/FavoritesScreen.tsx`
- Create: `web/src/components/favorites/FavoritesScreen.test.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.test.tsx`
- Modify: `web/src/components/studio/WidgetConfigModal.tsx`
- Modify: `web/src/components/studio/LayoutStudio.tsx`
- Modify: `web/src/components/studio/RuleBuilderEditor.tsx`
- Modify: `web/src/components/studio/WidgetConfigModal.test.tsx`
- Modify: `web/src/components/studio/RuleBuilderEditor.test.tsx`
- Modify: `README.md`
- Modify: `docs/CLIENT_GUIDE.md`

**Interfaces:**
- Consumes: `api.getFavorites`, `<FavoriteButton />`, `<WatchStateButton />`, `<ItemDetailsModal />`, `onFavoriteChanged`, `onWatchStateChanged`, `IsFavoriteFilterCondition`.
- Produces:
  - `<FavoritesScreen onPlayItem={...} onPlayQueue={...} onNavigateToFolder={...} currentUser={...} />`
  - Top-bar `Favorites` navigation item (`NavView` includes `'favorites'`, contextual `returnLabel="Back to Favorites"` in `CinemaPlayer`).
  - Layout Studio `My Favorites` (`favorites` macro + `favorited_at:desc|asc` sort options) and Rule Builder `Is Favorite` (`is_favorite`) rule + `"Unwatched Favorites"` quick template.
  - Updated `README.md` and `docs/CLIENT_GUIDE.md`.

- [ ] **Step 1: Write failing tests in `FavoritesScreen.test.tsx`, `App.test.tsx`, `WidgetConfigModal.test.tsx`, and `RuleBuilderEditor.test.tsx`**

1. Create `web/src/components/favorites/FavoritesScreen.test.tsx` testing:
   - Empty state rendering when `items` is empty.
   - Type filter pills (`All`, `Movies`, `TV Shows`, `Episodes`, `Home Videos`, `Live Channels`) and sort `<select>` triggering `api.getFavorites` with matching `item_type` / `sort`.
   - `locked_item_count > 0` displaying the locked-libraries notice badge (`"2 in locked libraries"`).
   - Unfavoriting an item via its heart button removing the card and decrementing the count.
   - Clicking **`Play All`** and **`Shuffle`** building a `PlaylistItemView[]` queue from playable items (`media_type !== 'show' && media_type !== 'season'`) and calling `onPlayQueue(queue, 0)`.
2. Add a test in `web/src/App.test.tsx` verifying clicking the top-bar `Favorites` button navigates to `FavoritesScreen`.
3. Add tests in `WidgetConfigModal.test.tsx` (selecting `My Favorites` macro + `Favorited (Newest)` sort emits `macro_type: 'favorites'`) and `RuleBuilderEditor.test.tsx` (selecting `Is Favorite` field emits `{ field: 'is_favorite', op: 'eq', value: true }`, toggling to `false`, and applying the `"Unwatched Favorites"` quick template).

- [ ] **Step 2: Run Vitest on the targeted test files to confirm RED failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/components/favorites/FavoritesScreen.test.tsx src/App.test.tsx src/components/studio/WidgetConfigModal.test.tsx src/components/studio/RuleBuilderEditor.test.tsx
```
Expected: FAIL (`FavoritesScreen` missing; `My Favorites` macro / `Is Favorite` rule missing).

- [ ] **Step 3: Implement `FavoritesScreen.tsx`, `App.tsx` navigation, Studio updates, and documentation**

1. Create `web/src/components/favorites/FavoritesScreen.tsx`:
   - Header with `Heart` icon, `My Favorites` `<h1>`, total visible count badge, locked-libraries pill (`Lock` icon + `${lockedCount} in locked libraries` when `lockedCount > 0`), and `Play All` + `Shuffle` buttons (enabled whenever at least 1 item has `card.media_type !== 'show' && card.media_type !== 'season'`).
   - Filter pills (`All`, `Movies`, `TV Shows`, `Episodes`, `Home Videos`, `Live Channels`) with `aria-pressed` and accessible `<label htmlFor={sortId}>Sort</label>` `<select>` (`favorited_at:desc`, `favorited_at:asc`, `title:asc`, `release_year:desc`, `rating:desc`, `duration_seconds:asc`).
   - Responsive poster grid matching `GridWidget` card styling (with top-right `<FavoriteButton variant="icon" alwaysVisible />` + `<WatchStateButton variant="icon" />` for playable items, resume bar when `hasProgress && !isCardWatched(card)`, and `<ItemDetailsModal />` on card click).
   - Subscribe to `onFavoriteChanged` (removing unfavorited items immediately) and `onWatchStateChanged` (updating card progress).

2. Update `web/src/App.tsx`:
   - Extend `NavView` to include `'favorites'`.
   - Render top-bar `Favorites` button (`Heart` icon from `lucide-react`) right next to `Playlists`.
   - Render `<FavoritesScreen ... />` when `currentView === 'favorites'`, and return `'Back to Favorites'` in `returnLabel` when launching playback from `'favorites'`.

3. Update Layout Studio (`WidgetConfigModal.tsx`, `LayoutStudio.tsx`, `RuleBuilderEditor.tsx`):
   - Add `'favorites'` (`My Favorites`) to `MacroTypeOption`, `#widget-macro` `<select>`, binding builder, and `LayoutStudio.tsx` `formatWidgetBindingSummary`, plus `favorited_at:desc` (`Favorited (Newest)`) and `favorited_at:asc` (`Favorited (Oldest)`) sort options when `macroType === 'favorites'`.
   - In `RuleBuilderEditor.tsx`: add `{ value: 'is_favorite', label: 'Is Favorite', group: 'Watch State' }`, default condition `{ field: 'is_favorite', op: 'eq', value: true }`, boolean toggle UI (`Yes — Favorited` / `No — Not Favorited`), summary formatter, and `"Unwatched Favorites"` quick template.

4. Update `README.md` & `docs/CLIENT_GUIDE.md`:
   - Update migration range (`001–015`), Key Features (`Per-User Favorites & Watchlist`), REST API Reference (`GET /api/v1/favorites`, `PUT /api/v1/favorites/:item_id`, `DELETE /api/v1/favorites/:item_id`), `CardViewModel.is_favorite`, `QueryMacro` table (`"favorites"`), `FilterCondition` table (`is_favorite`), SSE event list (`favorite:updated`), and add Section `9.5 Per-User Favorites / Watchlist` in `docs/CLIENT_GUIDE.md`.

- [ ] **Step 4: Run full Web & Rust workspace verification**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npm test -- --run && npm run build
cd /home/boody/work/personal/kadr && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
```
Expected: All Vitest suites PASS, `npm run build` succeeds cleanly, all Rust workspace tests PASS, and Clippy exits `0` with zero warnings.

- [ ] **Step 5: Commit Task 5**

```bash
git add web README.md docs/CLIENT_GUIDE.md
git commit -m "feat(favorites): add FavoritesScreen, Layout Studio favorites integration, and API docs"
```
