# Recursive Declarative Filter AST & Visual Rule Builder Design (`v0.4` Part 1)

**Date:** 2026-11-21  
**Status:** Approved

---

## 1. Overview & Goals

Deliver Kadr's signature PRD §3.1.1 differentiator — a **Recursive Boolean Filter Abstract Syntax Tree (AST)** compiled into parameterized SQLite prepared statements — along with catalog facet discovery, stateless live query previews, and a modular visual Rule Builder in Layout Studio.

This is **Part 1 of 3** in the `v0.4` Declarative Layout & Personalization milestone:
1. **Part 1 (this spec):** Recursive Filter AST (`AND` / `OR` / `NOT`), Parameterized SQL Compiler, Facets & Live Preview APIs, and Studio Rule Builder.
2. **Part 2 (next spec):** Per-User Favorites / Watchlist (`user_favorites`) & `Favorites` query source.
3. **Part 3 (third spec):** Per-User / Per-Device Scoped Layouts, Time-Window Shelves (`time_window`), and `dense_list` Widget Presentation.

### 1.1 Primary Goals
- **Expressive Cinephile & Archivist Shelves**: Allow users and external clients to express arbitrarily combined filter rules across metadata (`director`, `country`, `language`, `studio`, `genres`, `tags`, `actors`, `writers`), technical attributes (`resolution`, `video_codec`, `audio_codec`, `audio_channels`, `container`), numeric/date bounds (`release_year`, `duration_seconds`, `rating`, `added_at` sliding windows), and per-user playback states (`watch_state`, `progress_between`, `last_watched_at`, `play_count`) using nested `AND`, `OR`, and `NOT` operators.
- **100% Backwards Compatibility**: Add `QueryMacro::CustomQuery` for pure rule-based shelves while attaching `rule_tree: Option<FilterNode>` onto `WidgetFilterConfig` so all existing saved `data/screens/*.json` layouts and preset macros (`RecentlyAdded`, `TopRated`, `ContinueWatching`, `LibraryItems`, `GenreShelf`, `SpotlightItem`, `PlaylistItems`) continue working unchanged and can optionally be refined with rule trees.
- **Parameterized SQL & Fail-Closed Privacy**: Compile `FilterNode` trees strictly into SQLite `?` bind parameters (`rusqlite::types::Value`) with zero raw SQL string interpolation, wrapped inside an outer fail-closed private-library boundary so no `OR` or `NOT` tree can ever leak locked private libraries.
- **Interactive Studio Experience**: Provide `GET /api/v1/catalog/facets` for instant dropdown/chip value suggestions and `POST /api/v1/widgets/preview` for live match counts, execution latency (`ms`), and card previews inside `WidgetConfigModal` and on the unsaved `LayoutStudio` canvas.

### 1.2 Architectural Invariants Preserved
- **Memory & Portability Budget**: Preserve Kadr's `<= 30 MB` idle RSS target and pure-Rust static `musl` portability (zero new C/native dependencies; no duplicate relational facet tables).
- **Latency KPI**: Maintain single-digit millisecond query execution (`<= 10 ms` per compiled widget query) via targeted SQLite column and JSON expression indexes (`014_filter_ast_indexes.sql`).
- **Fail-Closed Private Library Isolation**: Locked private libraries (`libraries.is_private = 1` not present in signed `UnlockedLibraries` tokens) never leak items, counts, or facet values across screen hydration, `/widgets/{id}/data`, `/widgets/preview`, or `/catalog/facets`.

### 1.3 Non-Goals (Part 1)
- Per-user or per-device (`tv`/`mobile`/`web`) layout overrides and `time_window` active hours (covered in `v0.4` Part 3).
- `user_favorites` table and heart/watchlist actions (covered in `v0.4` Part 2).
- Arbitrary raw SQL or regex evaluation in user-supplied widget configs.

---

## 2. Core AST Data Model & Validation (`crates/kadr-core`)

### 2.1 `QueryMacro` & `WidgetFilterConfig` Extensions (`crates/kadr-core/src/ast.rs`)

1. Add `CustomQuery` to `QueryMacro`:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
   #[serde(rename_all = "snake_case")]
   pub enum QueryMacro {
       ContinueWatching,
       RecentlyAdded,
       TopRated,
       LibraryItems { library_id: String },
       GenreShelf { genre: String },
       SpotlightItem { item_id: Option<i64> },
       ItemDetails { item_id: i64 },
       PlaylistItems { playlist_id: String },
       CustomQuery,
   }
   ```
   *Note:* Because `FilterCondition` includes `f32` bounds (`rating`, `progress_between`), `FilterNode` implements `PartialEq` (comparing normalized finite `f32` values cleanly) and `WidgetFilterConfig` / `WidgetQueryBinding` / `WidgetNode` derive `PartialEq` (dropping `Eq` on `WidgetFilterConfig` and `WidgetQueryBinding`, which nothing keys a `HashMap` on — `ScreenId` retains `Eq + Hash`).

2. Extend `WidgetFilterConfig` with `rule_tree`:
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub rule_tree: Option<FilterNode>,
   ```
   When both legacy `WidgetFilterConfig` fields (`exclude_private`, `exclude_library_ids`, `exclude_genres`, `max_age_days`, `library_id`, `all_libraries`) and `rule_tree` are present, both apply (`legacy_clauses AND compiled_rule_tree`).

### 2.2 Recursive `FilterNode` Schema (`crates/kadr-core/src/filter_ast.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterNode {
    Group {
        op: LogicalOp,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
        children: Vec<FilterNode>,
    },
    Rule {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
        condition: FilterCondition,
    },
}
```

### 2.3 Typed `FilterCondition` Variants

`FilterCondition` uses struct variants with `#[serde(tag = "field", rename_all = "snake_case")]` and `#[serde(flatten)]` on the operator enum (avoiding serde's restriction on internally-tagged newtype enum variants) so JSON payloads serialize flat as `{ "field": "country", "op": "eq", "value": "Egypt" }` and invalid field/operator combinations fail deserialization cleanly:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FilterCondition {
    // Scope fields
    LibraryId { op: SetOp, values: Vec<String> },
    ItemType { op: SetOp, values: Vec<MediaType> },

    // Scalar text fields (case-insensitive)
    Title { #[serde(flatten)] matcher: TextMatch },
    OriginalTitle { #[serde(flatten)] matcher: TextMatch },
    SeriesTitle { #[serde(flatten)] matcher: TextMatch },
    Director { #[serde(flatten)] matcher: TextMatch },
    Studio { #[serde(flatten)] matcher: TextMatch },
    Country { #[serde(flatten)] matcher: TextMatch },
    Language { #[serde(flatten)] matcher: TextMatch },
    Resolution { #[serde(flatten)] matcher: TextMatch },
    VideoCodec { #[serde(flatten)] matcher: TextMatch },
    AudioCodec { #[serde(flatten)] matcher: TextMatch },
    Container { #[serde(flatten)] matcher: TextMatch },

    // JSON array text fields (case-insensitive element matching)
    Genres { #[serde(flatten)] matcher: ArrayMatch },
    Tags { #[serde(flatten)] matcher: ArrayMatch },
    Actors { #[serde(flatten)] matcher: ArrayMatch },
    Writers { #[serde(flatten)] matcher: ArrayMatch },

    // Numeric fields
    ReleaseYear { #[serde(flatten)] cmp: IntComparison },
    DurationSeconds { #[serde(flatten)] cmp: IntComparison },
    AudioChannels { #[serde(flatten)] cmp: IntComparison },
    PlayCount { #[serde(flatten)] cmp: IntComparison },
    Rating { #[serde(flatten)] cmp: FloatComparison },

    // Timestamp / sliding-window fields (epoch seconds)
    AddedAt { #[serde(flatten)] cmp: TimeComparison },
    LastWatchedAt { #[serde(flatten)] cmp: TimeComparison },

    // User playback state & progress ratio (0.0..=1.0)
    WatchState { #[serde(flatten)] matcher: WatchStateMatch },
}
```

Supporting operator structs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOp {
    In,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TextMatch {
    Eq { value: String },
    In { values: Vec<String> },
    Contains { value: String },
    StartsWith { value: String },
    IsEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ArrayMatch {
    ContainsAny { values: Vec<String> },
    ContainsAll { values: Vec<String> },
    IsEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum IntComparison {
    Eq { value: i64 },
    Gte { value: i64 },
    Lte { value: i64 },
    Between { min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FloatComparison {
    Gte { value: f32 },
    Lte { value: f32 },
    Between { min: f32, max: f32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TimeComparison {
    WithinLastDays { days: u32 },
    OlderThanDays { days: u32 },
    AfterEpoch { timestamp: i64 },
    BeforeEpoch { timestamp: i64 },
    BetweenEpochs { min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WatchStateMatch {
    StateIn { values: Vec<WatchState> },
    ProgressBetween { min: f32, max: f32 },
}
```

*(Remember that negation is already modeled orthogonally via `negated: bool` on `FilterNode::Rule` and `FilterNode::Group`, so `NOT (Country = Egypt)`, `NOT (Genres contains_any ["Musical"])`, and `NOT (Title is_empty)` work uniformly across every field without duplicating `NotEq` / `NotIn` / `IsNotEmpty` operator variants!)*

### 2.4 Validation Rules (`FilterNode::validate()`)

`FilterNode::validate(&self) -> std::result::Result<(), FilterValidationError>` enforces:
- **Tree bounds:** Maximum nesting depth `<= 5` (`Group` inside `Group` count; a single root `Group` of `Rule`s is depth `1`, and a 2-level Studio builder tree is depth `2`). Total `Rule` leaf nodes across the tree must be in `1..=32`. Every `Group` must have non-empty `children`.
- **String & list bounds:** Every `value` in `TextMatch` (`Eq`, `Contains`, `StartsWith`) must be non-empty after `.trim()` and `<= 200` chars. Every `values` list (`In`, `ContainsAny`, `ContainsAll`, `StateIn`) must contain `1..=32` non-empty trimmed items.
- **Numeric & time bounds:**
  - `Between` / `BetweenEpochs` / `ProgressBetween`: `min <= max`.
  - `FloatComparison`: finite (`!is_nan() && !is_infinite()`), `0.0..=10.0` for `Rating`.
  - `ProgressBetween`: finite, `0.0 <= min <= max <= 1.0`.
  - `WithinLastDays` / `OlderThanDays`: `1..=36_500` days (up to 100 years).
  - `DurationSeconds`: `>= 0`.

### 2.5 Pure-Rust Evaluator (`FilterNode::matches`)

`FilterNode::matches(&self, item: &MediaItem, playback: Option<&PlaybackState>, now_epoch: i64) -> bool` implements the exact same semantics in pure Rust for:
1. Post-filtering non-SQL macro streams (`PlaylistItems`), and
2. Property/parity testing verifying that the Rust evaluator and SQLite compiler agree 100% on every operator and `NULL`/empty edge case.

---

## 3. Parameterized SQL Compiler & Storage Layer (`crates/kadr-storage`)

### 3.1 Migration `014_filter_ast_indexes.sql`

Register `014_filter_ast_indexes.sql` in `crates/kadr-storage/src/migrations.rs` immediately after `013_audio_tracks.sql`:

```sql
-- Column & JSON expression indexes for fast Declarative Filter AST evaluation
CREATE INDEX IF NOT EXISTS idx_media_release_year
    ON media_items(release_year DESC);

CREATE INDEX IF NOT EXISTS idx_media_duration
    ON media_items(duration_seconds);

CREATE INDEX IF NOT EXISTS idx_media_resolution_lower
    ON media_items(LOWER(resolution));

CREATE INDEX IF NOT EXISTS idx_media_video_codec_lower
    ON media_items(LOWER(video_codec));

CREATE INDEX IF NOT EXISTS idx_media_country_lower
    ON media_items(LOWER(json_extract(metadata, '$.country')));

CREATE INDEX IF NOT EXISTS idx_media_language_lower
    ON media_items(LOWER(json_extract(metadata, '$.language')));

CREATE INDEX IF NOT EXISTS idx_media_director_lower
    ON media_items(LOWER(json_extract(metadata, '$.director')));

CREATE INDEX IF NOT EXISTS idx_media_studio_lower
    ON media_items(LOWER(json_extract(metadata, '$.studio')));
```

### 3.2 SQL Compilation Semantics (`crates/kadr-storage/src/repos/filter_compiler.rs`)

`compile_filter_node(node: &FilterNode, now_epoch: i64) -> CompiledFilter` produces:
```rust
pub struct CompiledFilter {
    pub sql: String,
    pub params: Vec<rusqlite::types::Value>,
    pub needs_playback_join: bool,
    pub references_item_type: bool,
}
```

Exact SQL mapping rules (all values bound via `?` placeholders; `LIKE` patterns escape `\`, `%`, and `_` with `ESCAPE '\'`):
- **`Group { op, negated, children }`:** Joins child SQL fragments with ` AND ` or ` OR `, parenthesizes `(child_1 OP child_2 ...)`, and prefixes `NOT (...)` when `negated == true`.
- **`Rule { negated, condition }`:** Wraps the condition SQL in `NOT (...)` when `negated == true`. **NULL-safety rule:** When `negated == true` on nullable columns/JSON fields (e.g., `NOT (Country = 'Egypt')`), a row whose `country` is `NULL` should match *"is not Egypt"*, whereas SQL's raw `NOT (LOWER(NULL) = 'egypt')` evaluates to `NULL` (falsy in `WHERE`)! Therefore, condition predicates use `COALESCE(..., '')` on text comparisons or explicit `(... IS NULL OR NOT (...))` / `CASE WHEN ... THEN 1 ELSE 0 END = 0` so `FilterNode::matches` (Rust) and SQLite agree 100% under arbitrary nested `NOT`s!
- **Field expressions:**
  - Direct columns: `m.library_id`, `m.item_type`, `m.title`, `m.original_title`, `m.release_year`, `m.duration_seconds`, `m.added_at`, `m.resolution`, `m.video_codec`, `m.audio_codec`, `m.audio_channels`, `m.container`.
  - JSON scalar text fields: `json_extract(m.metadata, '$.series_title')`, `'$.director'`, `'$.studio'`, `'$.country'`, `'$.language'`.
    - `Eq`: `COALESCE((LOWER(<expr>) = ?), 0)` (bound lowercase trimmed string; matches `LOWER(<expr>)` expression indexes directly in SQLite's query planner).
    - `In`: `COALESCE(LOWER(<expr>) IN (?, ...), 0)`.
    - `Contains`: `COALESCE(LOWER(<expr>) LIKE ? ESCAPE '\', 0)` (`%escaped%`).
    - `StartsWith`: `COALESCE(LOWER(<expr>) LIKE ? ESCAPE '\', 0)` (`escaped%`).
    - `IsEmpty`: `(<expr> IS NULL OR TRIM(<expr>) = '')`.
  - JSON array fields (`$.genres`, `$.tags`, `$.actors`, `$.writers`):
    - `ContainsAny`: `EXISTS (SELECT 1 FROM json_each(m.metadata, '$.genres') WHERE LOWER(TRIM(value)) IN (?, ...))`.
    - `ContainsAll`: one `EXISTS (SELECT 1 FROM json_each(m.metadata, '$.genres') WHERE LOWER(TRIM(value)) = ?)` per required value joined by `AND`.
    - `IsEmpty`: `(json_extract(m.metadata, '$.genres') IS NULL OR json_array_length(m.metadata, '$.genres') = 0)`.
  - Numeric fields:
    - `m.release_year`, `m.duration_seconds`, `m.audio_channels`, `CAST(json_extract(m.metadata, '$.rating') AS REAL)`, and `COALESCE(ups.play_count, 0)` (sets `needs_playback_join = true`).
    - Nullable numeric fields wrap comparisons in `COALESCE((<expr> >= ?), 0)` so `NOT` behaves deterministically on `NULL` rows (`rating IS NULL` is not `>= 8.0`).
  - Timestamp fields (`m.added_at`, `ups.last_watched_at`):
    - `WithinLastDays { days }`: `COALESCE((<expr> >= ?), 0)` where cutoff is `now_epoch - (days as i64 * 86_400)`.
    - `OlderThanDays { days }`: `COALESCE((<expr> < ?), 0)` where cutoff is `now_epoch - (days as i64 * 86_400)`.
    - `AfterEpoch` / `BeforeEpoch` / `BetweenEpochs`: `>=`, `<=`, `BETWEEN ? AND ?` wrapped in `COALESCE(..., 0)`. (`LastWatchedAt` sets `needs_playback_join = true`.)
  - `WatchState` (sets `needs_playback_join = true`):
    - Effective state expression: `COALESCE(ups.watch_state, 'unwatched')`.
    - `StateIn { values }`: `COALESCE(ups.watch_state, 'unwatched') IN (?, ...)`.
    - `ProgressBetween { min, max }`: `COALESCE((m.duration_seconds > 0 AND (CAST(COALESCE(ups.playback_position_seconds, 0) AS REAL) / CAST(m.duration_seconds AS REAL)) BETWEEN ? AND ?), 0)`.

### 3.3 `WidgetQueries` Integration & Default Episode Filtering

1. **Outer Privacy & Scope Envelope (`build_filter_clauses_with_user`):**
   - Always emits:
     `WHERE <library_scope_if_any> AND <episode_guard_if_any> AND <privacy_clause> AND <legacy_filter_clauses> AND (<compiled_rule_tree>)`
   - When `needs_playback_join` (or `sort == "last_watched_at:..."`) is true, joins:
     `LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?`
     (placing `user_id` first in the bind parameter vector right after `JOIN`).
2. **Episode Guard Behavior on `CustomQuery`:**
   - Preset macros (`RecentlyAdded`, `TopRated`, `GenreShelf`) keep `m.item_type != 'episode'` as today.
   - `QueryMacro::CustomQuery` applies `m.item_type != 'episode'` **by default**, UNLESS `compiled.references_item_type == true` (meaning the user explicitly added an `ItemType` rule such as `item_type IN ["episode"]`), in which case `CustomQuery` respects the user's explicit `ItemType` rule!
3. **Supported `sort` Values Across `CustomQuery` & `LibraryItems` (and optional override on preset macros):**
   - `added_at:desc` (default for `CustomQuery`), `added_at:asc`
   - `title:asc`, `title:desc`
   - `rating:desc`, `rating:asc`
   - `release_year:desc`, `release_year:asc`
   - `duration_seconds:asc`, `duration_seconds:desc`
   - `last_watched_at:desc`, `last_watched_at:asc` (uses `COALESCE(ups.last_watched_at, 0)` with `LEFT JOIN user_playback_states`)

### 3.4 Catalog Facets Query (`CatalogFacetsRepository` / `WidgetQueries::get_catalog_facets`)

Aggregates distinct values across `media_items m JOIN libraries l ON m.library_id = l.id WHERE (l.is_private = 0 OR l.id IN (...unlocked_ids...))` (with optional `library_id` filter):
- Scalar facets (`countries`, `languages`, `studios`, `directors`, `resolutions`, `video_codecs`, `audio_codecs`, `containers`): grouped case-insensitively (`GROUP BY LOWER(TRIM(val))`), ordered by item count descending then alphabetically, capped at `100` entries each.
- Array facets (`genres`, `tags`): aggregated via `json_each`, ordered by frequency descending then alphabetically, capped at `100` entries each.
- Year bounds: `year_min: Option<i32>`, `year_max: Option<i32>`.

---

## 4. Server API & Resolver Wiring (`crates/kadr-server`)

### 4.1 Screen Save Validation (`PUT /api/v1/screens/{screen_id}`)
- Before persisting a `ScreenLayout` in `save_screen_handler`, iterate `screen.widgets` and call `rule_tree.validate()` whenever `widget.binding().and_then(|b| b.filters.as_ref()).and_then(|f| f.rule_tree.as_ref())` is `Some`.
- If validation fails, return `400 Bad Request`:
  `{ "error": "Invalid filter rule tree in widget '<id>': <reason>" }`.

### 4.2 `GET /api/v1/catalog/facets`
- **Auth:** `AuthUser` + `UnlockedLibraries`.
- **Query params:** Optional `?library_id=<id>`. If `library_id` refers to a locked private library, return empty facets (`200 OK` with empty arrays and `null` year bounds — zero leak).
- **Response (`200 OK`):**
  ```json
  {
    "genres": ["Drama", "Crime", "Thriller"],
    "countries": ["Egypt", "France", "United States"],
    "languages": ["ar", "en", "fr"],
    "studios": ["Studio Misr", "A24"],
    "directors": ["Youssef Chahine", "Denis Villeneuve"],
    "tags": ["criterion", "restored"],
    "resolutions": ["4k", "1080p", "720p"],
    "video_codecs": ["hevc", "h264", "av1"],
    "audio_codecs": ["aac", "eac3", "flac"],
    "containers": ["mkv", "mp4"],
    "year_min": 1954,
    "year_max": 2026
  }
  ```

### 4.3 `POST /api/v1/widgets/preview`
- **Auth:** `RequireAdmin` + `AuthUser` + `UnlockedLibraries`.
- **Request body:**
  ```json
  {
    "binding": {
      "macro_type": "custom_query",
      "limit": 12,
      "sort": "release_year:asc",
      "filters": {
        "rule_tree": {
          "kind": "group",
          "op": "and",
          "children": [
            { "kind": "rule", "condition": { "field": "country", "op": "eq", "value": "Egypt" } },
            { "kind": "rule", "condition": { "field": "duration_seconds", "op": "lte", "value": 5400 } }
          ]
        }
      }
    },
    "screen_id": "home",
    "limit": 10
  }
  ```
- **Validation & Execution:**
  - Validates `binding.filters.rule_tree` (`400 Bad Request` if invalid).
  - Clamps preview `limit` (`payload.limit.unwrap_or(10).clamp(1, 24)`).
  - Resolves `default_library_id` from optional `screen_id` via `resolver.resolve_library_id_for_screen`.
  - Runs `resolver.resolve_widget_data_with_library(...)` timed with `Instant::now()`.
- **Response (`200 OK`):**
  ```json
  {
    "items": [ /* CardViewModel[] */ ],
    "total_count": 14,
    "execution_ms": 2
  }
  ```

---

## 5. Web Client & Studio Rule Builder (`web/`)

### 5.1 TypeScript Types & API Client (`web/src/types/filterAst.ts`, `web/src/api/client.ts`)
- Mirror `FilterNode`, `LogicalOp`, `FilterCondition`, `CatalogFacets`, `WidgetPreviewRequest`, and `WidgetPreviewResponse` in `web/src/types/filterAst.ts` (re-exported from `web/src/types/index.ts`).
- Add `'custom_query'` to `QueryMacro` and `rule_tree?: FilterNode` to `WidgetFilterConfig`.
- Add `api.getCatalogFacets(libraryId?: string)` and `api.previewWidget(req: WidgetPreviewRequest)` to `web/src/api/client.ts`.

### 5.2 Modular Studio Components (`web/src/components/studio/`)

1. **`RuleBuilderEditor.tsx`:**
   - **Visual Mode (2-level Group Builder):**
     - Header bar with root combinator toggle (`Match ALL (AND)` / `Match ANY (OR)`), root `NOT` inversion pill, `Clear Rules`, and a **Quick Templates** dropdown:
       - *Short Films (< 90 min)* (`duration_seconds <= 5400`, `item_type = movie`)
       - *4K Remux / Modern Codecs* (`resolution = 4k`, `video_codec in [hevc, av1]`)
       - *Unwatched Gems (Rating >= 7.5)* (`watch_state = unwatched`, `rating >= 7.5`)
       - *Added in Last 30 Days* (`added_at within_last_days 30`)
     - **Condition Row:** `[NOT toggle]` + `[Field select (grouped: Metadata, Technical, Runtime & Year, Watch State, Scope)]` + `[Operator select]` + `[Typed Value control]` + `[Remove button]`.
       - Value controls adapt to the field: searchable suggestion input / multi-chip picker backed by `CatalogFacets` for `genres`, `tags`, `country`, `language`, `studio`, `director`, `resolution`, `video_codec`, `audio_codec`, `container`; checkboxes/pills for `item_type`, `library_id`, `watch_state`; minute inputs for `DurationSeconds` (converting `minutes <-> seconds` cleanly in the UI); `%` inputs (`0–100%`) for `ProgressBetween`.
     - **Nested Sub-Groups (Level 2):** `+ Add Rule Group` adds a visually indented sub-card with its own `ALL (AND)` / `ANY (OR)` + `NOT` toggle and `+ Add Rule` button.
   - **Advanced JSON Mode:**
     - Toggle button (`Visual` | `JSON AST`) allowing power users to paste/edit up to depth-5 `FilterNode` JSON with inline parse & schema validation errors. If a loaded widget has depth `> 2`, `RuleBuilderEditor` opens cleanly in `JSON AST` mode with an informational banner explaining that flattening to Visual mode requires `<= 2` group levels.

2. **`WidgetLivePreview.tsx`:**
   - Debounces (`250ms`, cancelling stale requests via `AbortController`) calls to `api.previewWidget({ binding, screen_id, limit: 8 })`.
   - Displays a header pill (`14 matches · 2 ms`, or `0 matches` empty hint, or inline `400` validation message) and a compact horizontal mini-poster strip inside `WidgetConfigModal`.

3. **`WidgetConfigModal.tsx` & `LayoutStudio.tsx` Updates:**
   - Add **`Custom Filter Query`** (`custom_query`) to `MacroTypeOption` and `SORT_OPTIONS` entries for `Duration (Shortest)` (`duration_seconds:asc`), `Duration (Longest)` (`duration_seconds:desc`), and `Last Watched (Recent)` (`last_watched_at:desc`).
   - Embed `RuleBuilderEditor` inside the Filters section (auto-expanded when `macroType === 'custom_query'` or when `initialWidget` has a `rule_tree`) and `WidgetLivePreview` at the bottom of the modal.
   - Update `HydratedSpotlight`, `StudioCarousel`, and `StudioGrid` in `LayoutStudio.tsx` so newly added or edited unsaved widgets (whose `id` is not yet persisted on disk or while `isDirty` is true) fall back to `api.previewWidget({ binding: widget.binding, screen_id: selectedScreenId, limit })` when `getWidgetData` returns 404 or when `widget.items` was cleared on edit — giving instant, accurate live previews on the TV/Tablet/Mobile Studio canvas before hitting Save!

### 5.3 Client Documentation (`docs/CLIENT_GUIDE.md`)
- Document `QueryMacro::CustomQuery`, the full `FilterNode` / `FilterCondition` JSON grammar, `GET /api/v1/catalog/facets`, and `POST /api/v1/widgets/preview` with cinephile shelf examples.

---

## 6. Testing & Verification Strategy

1. **`kadr-core` Unit Tests (`crates/kadr-core/tests/filter_ast_test.rs`):**
   - JSON serialization/deserialization roundtrips for every `FilterCondition` variant and backwards compatibility with legacy `WidgetFilterConfig` payloads lacking `rule_tree`.
   - `FilterNode::validate()` rejection of depth `6`, `> 32` rules, empty groups, empty strings, `min > max`, `NaN`/`Infinity`, and out-of-range progress/rating values.
   - `FilterNode::matches()` truth-table tests across `AND`, `OR`, `NOT`, `NULL`/empty fields, array `contains_any`/`contains_all`/`is_empty`, and watch progress boundaries.
2. **`kadr-storage` Compiler & Privacy Tests (`crates/kadr-storage/tests/filter_compiler_test.rs`):**
   - Exact parity test running identical complex `FilterNode` trees through both SQLite (`WidgetQueries::find_custom_paginated`) and `FilterNode::matches()` across a diverse fixture catalog (movies, shows, episodes, NULL metadata, Unicode/Arabic strings, `LIKE` wildcard characters `%`/`_`, and partial watch states) — asserting identical matched ID sets!
   - **Security tests:** Verify a locked private library item matching an `OR` branch or `NOT` branch at the root of `rule_tree` NEVER appears in results or `total_count` when locked, and DOES appear when unlocked; verify `get_catalog_facets` never leaks private library genres/tags/directors/countries when locked.
3. **`kadr-server` API & Performance E2E Tests (`crates/kadr-server/tests/e2e_filter_ast_test.rs`):**
   - End-to-end flow: create custom shelf with `CustomQuery` + nested `AND`/`OR`/`NOT` rules -> `POST /api/v1/widgets/preview` -> `PUT /api/v1/screens/home` (including `400` rejection of invalid AST) -> `GET /api/v1/screens/home` single-roundtrip hydration -> `GET /api/v1/widgets/{id}/data` pagination -> `GET /api/v1/catalog/facets`.
   - Seed 1,000+ synthetic items and assert `execution_ms < 25` in debug test builds (`< 5 ms` in release).
4. **Frontend Vitest Suite (`web/src/components/studio/*.test.tsx`):**
   - `RuleBuilderEditor.test.tsx`: adding/removing rules & nested groups, toggling `AND`/`OR`/`NOT`, quick templates, minute-to-second duration conversion, facet chip selection, and Visual <-> JSON AST mode switching (including depth `> 2` guard).
   - `WidgetLivePreview.test.tsx` & `WidgetConfigModal.test.tsx`: debounced `previewWidget` calls, `custom_query` selection, and `rule_tree` payload emission on save.
