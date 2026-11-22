# Per-User Favorites / Watchlist Design (`v0.4` Part 2)

**Date:** 2026-11-22  
**Status:** Approved

---

## 1. Overview & Goals

Deliver PRD §3.2.1's **per-profile isolated `user_favorites` system** across Kadr's storage engine, Declarative Layout & Filter AST engine, REST/SSE APIs, and Cinema Web UI (`ItemDetailsModal`, `SpotlightWidget`, poster cards, dedicated `Favorites` view, and Layout Studio).

This is **Part 2 of 3** in the `v0.4` Declarative Layout & Personalization milestone:
1. **Part 1 (shipped):** Recursive Filter AST (`AND` / `OR` / `NOT`), Parameterized SQL Compiler, Facets & Live Preview APIs, and Studio Rule Builder.
2. **Part 2 (this spec):** Per-User Favorites / Watchlist (`user_favorites`), Heart Toggles, Dedicated `Favorites` View, Default `Home` Rail, and `QueryMacro::Favorites` + `FilterCondition::IsFavorite`.
3. **Part 3 (next spec):** Per-User / Per-Device Scoped Layouts, Time-Window Shelves (`time_window`), and `dense_list` Widget Presentation.

### 1.1 Primary Goals
- **Strict Per-Profile Isolation**: Partition favorites strictly by `user_id` (`PRIMARY KEY (user_id, media_item_id)` with `ON DELETE CASCADE` on both `users` and `media_items`) so household profiles maintain completely independent watchlists/favorites.
- **Any Catalog Item Type**: Support favoriting any `MediaItem` (`movie`, `show`, `episode`, `anime`, `music`, `home_videos`, `audiobook`, `live_channel`) without expanding TV `Show` containers into child episodes when bookmarking a series.
- **Zero-Extra-Roundtrip Card Hydration**: Batch-hydrate `is_favorite: bool` onto `CardViewModel` (automatically carried on `ItemDetailsPayload.card` and `PlaylistItemView.card`) and `SearchResultItem` so every UI surface renders accurate heart states immediately.
- **Declarative Engine & Default `Home` Integration**: Add `QueryMacro::Favorites` (`"favorites"`, placed right below `"Continue Watching"` on the factory-default `Home` layout and auto-hidden when empty) and `FilterCondition::IsFavorite` (`{ "field": "is_favorite", "op": "eq", "value": true }`) inside Part 1's SQL compiler and Studio Rule Builder.
- **Full Web UI & Queue Playback**: Provide instant optimistic heart toggles across `ItemDetailsModal`, `SpotlightWidget`, `CarouselWidget`, and `GridWidget`, plus a top-bar `Favorites` view with media-type filter pills, sort controls, `Play All` / `Shuffle` queue playback, and locked private-library indicators.

### 1.2 Architectural Invariants Preserved
- **Memory & Portability Budget**: Preserve `<= 30 MB` idle RSS and pure-Rust static `musl` portability (`015_user_favorites.sql`, zero new dependencies).
- **Fail-Closed Private Library Isolation**: Items in PIN-locked private libraries (`libraries.is_private = 1` not present in signed `UnlockedLibraries` tokens) never appear in `GET /api/v1/favorites` items, `QueryMacro::Favorites`, or `FilterCondition::IsFavorite` results, cannot be favorited/unfavorited while locked (`403 LIBRARY_LOCKED`), and never leak titles over SSE (`favorite:updated` carries IDs only).

### 1.3 Non-Goals (Part 2)
- Public/shared favorites across profiles (playlists with `is_public = true` already serve shared curated collections; favorites are strictly personal to `user_id`).
- Per-user custom layout overrides (covered in `v0.4` Part 3).

---

## 2. Storage & Domain Model (`crates/kadr-core`, `crates/kadr-storage`)

### 2.1 Migration `015_user_favorites.sql`

Register `015_user_favorites.sql` in `crates/kadr-storage/src/migrations.rs` immediately after `014_filter_ast_indexes.sql`:

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

**Schema invariants:**
- `favorited_at`: Unix epoch seconds (`i64`). Re-favoriting an already favorited item (`INSERT OR IGNORE`) is idempotent and preserves the original `favorited_at` timestamp.
- `ON DELETE CASCADE`: Deleting a user profile or removing/purging a media item automatically cleans up `user_favorites`.

### 2.2 Domain & Wire Models (`crates/kadr-core`)

1. **`CardViewModel` (`crates/kadr-core/src/ast.rs`) & `SearchResultItem` (`crates/kadr-core/src/search.rs`):**
   Add:
   ```rust
   #[serde(default, skip_serializing_if = "std::ops::Not::not")]
   pub is_favorite: bool,
   ```
   *(Because `ItemDetailsPayload` and `PlaylistItemView` both embed `pub card: CardViewModel`, both automatically include `card.is_favorite` when `true` while omitting `"is_favorite": false` to keep existing JSON payloads compact and backwards-compatible.)*

2. **Favorites Wire Types (`crates/kadr-core/src/favorites.rs`, re-exported from `lib.rs`):**
   ```rust
   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
   pub struct FavoriteItemView {
       pub favorited_at: i64,
       pub duration_seconds: i64,
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub resume_position_seconds: Option<i64>,
       pub card: CardViewModel,
   }

   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

3. **SSE Event (`crates/kadr-core/src/events.rs`):**
   Add variant to `SystemEvent`:
   ```rust
   #[serde(rename = "favorite:updated")]
   FavoriteUpdated {
       user_id: String,
       item_id: i64,
       is_favorite: bool,
       timestamp: i64,
   },
   ```
   with `event_type() -> "favorite:updated"`.

4. **Declarative AST Extensions (`crates/kadr-core/src/ast.rs` & `filter_ast.rs`):**
   - Add `QueryMacro::Favorites` (`"favorites"`).
   - Add `BoolOp::Eq` (`#[derive(Default)]`, `#[serde(rename_all = "snake_case")]`) and `FilterCondition::IsFavorite`:
     ```rust
     #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
     #[serde(rename_all = "snake_case")]
     pub enum BoolOp {
         #[default]
         Eq,
     }

     // Inside FilterCondition:
     IsFavorite {
         #[serde(default)]
         op: BoolOp,
         value: bool,
     },
     ```
     This serializes as `{ "field": "is_favorite", "op": "eq", "value": true }` (matching the uniform `{ field, op, ... }` convention across all 26 `FilterCondition` variants in Rust and TypeScript, while `#[serde(default)]` also accepts `{ "field": "is_favorite", "value": true }` from external API callers!).
   - Add `FilterNode::needs_favorite_state(&self) -> bool` and `FilterNode::matches_with_context(&self, item: &MediaItem, playback: Option<&PlaybackState>, is_favorite: bool, now_epoch: i64) -> bool` (keeping `FilterNode::matches(item, playback, now_epoch)` delegating with `is_favorite = false` so all existing tests remain 100% source-compatible).

### 2.3 `FavoritesRepository` & SQL Compiler Wiring (`crates/kadr-storage`)

1. **`FavoritesRepository` (`crates/kadr-storage/src/repos/favorites_repo.rs`):**
   - `add_favorite(&self, user_id: &str, media_item_id: i64, now_epoch: i64) -> Result<()>` (`INSERT OR IGNORE INTO user_favorites (user_id, media_item_id, favorited_at) VALUES (?1, ?2, ?3)`)
   - `remove_favorite(&self, user_id: &str, media_item_id: i64) -> Result<bool>` (`DELETE FROM user_favorites WHERE user_id = ?1 AND media_item_id = ?2`)
   - `is_favorite(&self, user_id: &str, media_item_id: i64) -> Result<bool>`
   - `get_favorited_ids(&self, user_id: &str, item_ids: &[i64]) -> Result<HashSet<i64>>` (single `SELECT media_item_id FROM user_favorites WHERE user_id = ? AND media_item_id IN (...)` query for batch card hydration)
   - `list_favorites(&self, user_id: &str, library_id: Option<&str>, item_type: Option<MediaType>, limit: u32, offset: u32, sort: Option<&str>, unlocked_ids: &[String], filters: Option<&WidgetFilterConfig>) -> Result<(Vec<(i64 /* favorited_at */, MediaItem)>, u64 /* visible_total */, u64 /* locked_count */)>`:
     - Computes `locked_count` as the count of `user_favorites` rows for `user_id` joining `media_items m JOIN libraries l ON m.library_id = l.id` where `l.is_private = 1 AND l.id NOT IN (...unlocked_ids...)` (scoped to `library_id` / `item_type` if provided).
     - Queries visible favorites joining `user_favorites uf ON uf.media_item_id = m.id` with `uf.user_id = ?` inside the standard fail-closed `build_filter_envelope` (`(l.is_private = 0 OR l.id IN (...))`).
     - Does **not** exclude `episode` items by default (if a user explicitly favorited an episode, it appears in their Favorites!).
     - Supports sorts: `favorited_at:desc` (default: `uf.favorited_at DESC, m.id DESC`), `favorited_at:asc` (`uf.favorited_at ASC, m.id ASC`), plus all existing sorts (`title`, `release_year`, `rating`, `added_at`, `duration_seconds`, `last_watched_at`).

2. **SQL Filter Compiler (`crates/kadr-storage/src/repos/filter_compiler.rs` & `widget_queries.rs`):**
   - Extend `compile_filter_node` (or pass a placeholder / `needs_user_id` binding) for `FilterCondition::IsFavorite { value, .. }`:
     - Add `user_id_param_positions` or compile `FilterCondition::IsFavorite { value: true, .. }` to:
       `EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?)`
       and `value: false` to:
       `NOT EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?)`
       binding `user_id` cleanly from `compile_filter_node_for_user(node, user_id, now_epoch)`!
     - Because `user_favorites` has `PRIMARY KEY (user_id, media_item_id)`, `EXISTS (...)` is an O(log N) primary-key lookup per candidate row with zero row multiplication on `COUNT(*)`.
   - Also update `SearchRepository::search` (`crates/kadr-storage/src/repos/search_repo.rs`) to populate `is_favorite` via `EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?)` in the existing search query!

---

## 3. Server Routes, Hydration & Default `Home` Layout (`crates/kadr-server`)

### 3.1 Batch Hydration Across `WidgetResolver`, `item_routes`, and `playlist_routes`
- Update `to_card_view_model` (or add `to_card_view_model_with_favorite(item, playback, is_favorite)`) in `crates/kadr-server/src/resolver/card.rs`.
- In `WidgetResolver::hydrate_items_to_cards` (`resolver.rs`), batch-fetch `favorited_ids = self.favorites_repo.get_favorited_ids(user_id, &item_ids).await` right alongside `self.playback_repo.get_states_for_items(user_id, &item_ids).await` — 1 batched query per widget regardless of item count!
- Also populate `card.is_favorite` on single-item paths (`resolve_hero_banner_data`, `ContinueWatching`, `SpotlightItem`, `ItemDetails` / `get_item_details`, and `playlist_routes::get_playlist`).
- Wire `QueryMacro::Favorites` in `WidgetResolver::resolve_widget_data_with_library` calling `favorites_repo.list_favorites(...)` (note: like `PlaylistItems`, `determine_effective_library_id` returns `None` by default for `QueryMacro::Favorites` unless `filters.library_id` is explicitly set, so placing a `Favorites` widget on a screen shows favorites across all accessible libraries unless scoped!).

### 3.2 Factory-Default `Home` Screen (`crates/kadr-server/src/layout/registry.rs`)
- Insert `"My Favorites"` (`id: "my_favorites"`, `title: "My Favorites"`, `QueryMacro::Favorites`) into `default_home_layout()` immediately after `"continue_watching"`:
  1. `home_spotlight` (`SpotlightItem`)
  2. `continue_watching` (`ContinueWatching`)
  3. `my_favorites` (`Favorites`)
  4. `recently_added` (`RecentlyAdded`)
  5. `top_rated` (`TopRated`)
  6. `genre_action` (`GenreShelf: Action`)
  7. `genre_scifi` (`GenreShelf: Sci-Fi`)
  *(Because `CarouselWidget` returns `null` when `items.length === 0`, `My Favorites` stays completely invisible until the user favorites at least one item, and automatically appears right below `Continue Watching` as soon as they do! Existing custom `data/screens/home.json` overrides on disk remain untouched unless the admin resets `Home` to defaults or adds a `My Favorites` widget in Studio.)*

### 3.3 REST Endpoints (`crates/kadr-server/src/api/favorite_routes.rs`)
Register `favorite_routes()` in `crates/kadr-server/src/api/mod.rs`:
1. **`GET /api/v1/favorites`**
   - **Auth:** `AuthUser` + `UnlockedLibraries`.
   - **Query params:** `item_type?: MediaType`, `library_id?: String`, `sort?: String` (default `favorited_at:desc`), `limit?: u32` (`1..=200`, default `60`), `offset?: u32` (default `0`).
   - **Response (`200 OK`):** `FavoritesListResponse { items: Vec<FavoriteItemView>, total_count, locked_item_count }`, where each `FavoriteItemView` hydrates `card` (with `is_favorite: true` and `playback_progress`) plus `duration_seconds` and `resume_position_seconds` for direct queue playback!
2. **`PUT /api/v1/favorites/{item_id}`**
   - **Auth:** `AuthUser` + `UnlockedLibraries`.
   - **Checks:** Look up `media_repo.get_by_id(item_id)` -> `404 Not Found` if absent; look up `lib_repo.get_by_id(&item.library_id)` -> if `lib.is_private && !unlocked.contains(&lib.id)`, return `403 Forbidden` `{ "error": "Library is locked", "code": "LIBRARY_LOCKED", "library_id": lib.id }`.
   - **Action:** `favorites_repo.add_favorite(&auth_user.id, item_id, now)` -> emit `SystemEvent::FavoriteUpdated { user_id, item_id, is_favorite: true, timestamp: now }` -> return `200 OK` `{ "item_id": item_id, "is_favorite": true }`.
3. **`DELETE /api/v1/favorites/{item_id}`**
   - **Auth:** `AuthUser` + `UnlockedLibraries`.
   - **Checks:** Same `404 Not Found` and `403 LIBRARY_LOCKED` checks as `PUT`.
   - **Action:** `favorites_repo.remove_favorite(&auth_user.id, item_id)` -> emit `SystemEvent::FavoriteUpdated { user_id, item_id, is_favorite: false, timestamp: now }` -> return `200 OK` `{ "item_id": item_id, "is_favorite": false }`.

---

## 4. Web Client, Optimistic Sync & Studio Integration (`web/`)

### 4.1 Types, Client & Cross-Component Favorite Sync (`web/src/types/`, `web/src/api/client.ts`, `web/src/utils/favoriteEvents.ts`)
- Add `is_favorite?: boolean` to `CardViewModel` and `SearchResultItem`; add `'favorites'` to `QueryMacro`; add `{ field: 'is_favorite'; op: 'eq'; value: boolean }` to `FilterCondition`; export `FavoriteItemView`, `FavoritesListResponse`, and `FavoriteStatusResponse`.
- Add `api.getFavorites(params?)`, `api.addFavorite(itemId)`, and `api.removeFavorite(itemId)` to `web/src/api/client.ts`.
- Create tiny event helper `web/src/utils/favoriteEvents.ts` (`emitFavoriteChanged(itemId, isFavorite)` + `onFavoriteChanged(handler)` using a `CustomEvent` on `window`) so toggling a heart in `ItemDetailsModal`, `SpotlightWidget`, `CarouselWidget`, `GridWidget`, or `FavoritesScreen` updates `is_favorite` across mounted cards immediately and triggers `BrowseScreen` (`screenId === 'home'`) to refresh when an item is added/removed from `My Favorites`!

### 4.2 `<FavoriteButton />` & Browse Surfaces
1. **`web/src/components/favorites/FavoriteButton.tsx`:**
   - Supports two visual variants:
     - `variant="icon"` (compact circular button for top-right of poster cards in `CarouselWidget` and `GridWidget`): when `isFavorite` is `true`, always visible with filled crimson/accent heart (`fill-cta text-cta`); when `false`, visible on card hover or keyboard focus (`opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus:opacity-100`). Calls `e.stopPropagation()` and `e.preventDefault()` on click/keydown so toggling a card's heart never opens `ItemDetailsModal`!
     - `variant="pill"` (labeled action button for `ItemDetailsModal.tsx` and `SpotlightWidget.tsx` alongside `Play` and `+ Playlist`): displays `"Favorited"` (filled heart) vs `"Favorite"` (outline heart).
   - Guards `typeof api.addFavorite === 'function'` / `removeFavorite` defensively, updates optimistically, rolls back on network error, and emits `emitFavoriteChanged(itemId, next)`.
2. **Wire `<FavoriteButton />` into:**
   - `CarouselWidget.tsx` (`MediaCard`) —note: convert outer `<div role="button">` wrapper's nested button interaction cleanly or ensure `onClick`/`onKeyDown` ignores events originating from `<FavoriteButton />` (`e.stopPropagation()`).
   - `GridWidget.tsx` (poster cards).
   - `SpotlightWidget.tsx` (hero action bar).
   - `ItemDetailsModal.tsx` (primary action bar + episode row heart action if desired, or main modal action bar).

### 4.3 Dedicated `FavoritesScreen.tsx` & Top Navigation (`web/src/App.tsx`)
- Add `'favorites'` to `NavView` in `web/src/App.tsx` with a `Heart` icon button (`Favorites`) in the top navigation bar right next to `Playlists`.
- Create `web/src/components/favorites/FavoritesScreen.tsx`:
  - **Header:** Title (`My Favorites`), item count badge, locked-vault notice badge when `locked_item_count > 0` (`"{locked_item_count} in locked libraries"`), and **`Play All`** + **`Shuffle`** buttons!
    - **Queue construction for `Play All` / `Shuffle`:** Playable items (`item_type !== 'show' && item_type !== 'season'`) map directly from `FavoriteItemView` to `PlaylistItemView` (`position`, `added_at: favorited_at`, `duration_seconds`, `resume_position_seconds`, `card`) and invoke `onPlayQueue(queue, 0)` in `App.tsx`! (If `items` contains TV `Show` cards, `Play All` plays all directly playable favorite items, or if only `Show`s are favorited, disables `Play All` / opens the show details modal cleanly.)
  - **Filter & Sort Bar:**
    - Type pills: `All`, `Movies` (`movie`), `TV Shows` (`show`), `Episodes` (`episode`), `Home Videos` (`home_videos`), `Live Channels` (`live_channel`).
    - Sort dropdown: `Recently Favorited` (`favorited_at:desc`), `Oldest Favorited` (`favorited_at:asc`), `Title (A–Z)` (`title:asc`), `Release Year` (`release_year:desc`), `Rating` (`rating:desc`), `Duration (Shortest)` (`duration_seconds:asc`).
  - **Grid & Actions:** Poster grid with `<FavoriteButton variant="icon" />` (unfavoriting removes the card smoothly from the grid and decrements `total_count`), clicking a card opens `ItemDetailsModal`.
  - **Empty State:** Cinema-themed empty state explaining how to heart movies, shows, episodes, or live channels from any screen.

### 4.4 Layout Studio & Rule Builder Integration (`web/src/components/studio/`)
- **`WidgetConfigModal.tsx` & `LayoutStudio.tsx`:**
  - Add **`My Favorites`** (`favorites`) to `MacroTypeOption`, `#widget-macro` `<select>`, and `formatWidgetBindingSummary` in `LayoutStudio.tsx`.
  - Add `Favorited (Newest)` (`favorited_at:desc`) and `Favorited (Oldest)` (`favorited_at:asc`) to `SORT_OPTIONS` when `macroType === 'favorites'`.
- **`RuleBuilderEditor.tsx`:**
  - Add **`Is Favorite`** (`is_favorite`) under the `Watch State` field group with boolean toggle (`Yes (Favorited)` / `No (Not Favorited)` -> `{ field: 'is_favorite', op: 'eq', value: true | false }`).
  - Add Quick Template: **"Unwatched Favorites"** (`is_favorite = true AND watch_state in ['unwatched']`).
- **Documentation (`docs/CLIENT_GUIDE.md` & `README.md`):**
  - Document `/api/v1/favorites` (`GET`, `PUT`, `DELETE`), `CardViewModel.is_favorite`, `QueryMacro::Favorites`, `FilterCondition::IsFavorite`, and SSE `favorite:updated`.

---

## 5. Testing & Verification Strategy

1. **`kadr-core` Unit Tests (`crates/kadr-core/tests/filter_ast_test.rs`):**
   - Serde roundtrips for `QueryMacro::Favorites`, `FilterCondition::IsFavorite` (with and without explicit `"op": "eq"`), `CardViewModel.is_favorite` omission when `false` and inclusion when `true`, `SystemEvent::FavoriteUpdated`, and `FilterNode::matches_with_context`.
2. **`kadr-storage` Repository & Compiler Tests (`crates/kadr-storage/tests/favorites_repo_test.rs`):**
   - Per-user isolation (`u1` vs `u2`), idempotent `add_favorite` preserving `favorited_at`, `ON DELETE CASCADE` when a `User` or `MediaItem` is deleted, `list_favorites` type filtering (`movie`, `show`, `episode`) & sorting (`favorited_at:desc|asc`, `rating:desc`), locked private library zero-leak + `locked_item_count` accuracy, `SearchRepository` `is_favorite` hydration, and `FilterCondition::IsFavorite` SQL-vs-Rust parity (including negated `NOT is_favorite`).
3. **`kadr-server` E2E Tests (`crates/kadr-server/tests/e2e_favorites_test.rs`):**
   - `PUT /api/v1/favorites/{id}` & `DELETE /api/v1/favorites/{id}` (`401` unauthenticated, `404` missing item, `403 LIBRARY_LOCKED` on locked private item, `200 OK` + SSE `favorite:updated` broadcast).
   - `GET /api/v1/favorites` pagination, type filter, `locked_item_count` when private vault is locked vs unlocked.
   - `GET /api/v1/screens/home` default `"my_favorites"` carousel hydration + `CardViewModel.is_favorite` hydration across `Home` widgets, `GET /api/v1/items/{id}`, `GET /api/v1/playlists/{id}`, `GET /api/v1/search`, and `POST /api/v1/widgets/preview` with `FilterCondition::IsFavorite`.
4. **Frontend Vitest Suite (`web/src/components/favorites/*.test.tsx`, `studio/*.test.tsx`, `App.test.tsx`):**
   - `FavoriteButton.test.tsx`: optimistic heart toggle on cards & `ItemDetailsModal`, `stopPropagation` preventing card modal open on heart click, rollback on API failure.
   - `FavoritesScreen.test.tsx`: empty state, type filter pills, sort changes, `locked_item_count` notice, unfavorite removal, and `Play All` / `Shuffle` queue launch.
   - Studio tests: `My Favorites` (`favorites`) widget creation and `is_favorite` rule in `RuleBuilderEditor`.
