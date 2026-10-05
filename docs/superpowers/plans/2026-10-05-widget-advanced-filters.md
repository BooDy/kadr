# Widget Advanced Filters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement advanced filtering capabilities for screen layout widgets in Kadr, enabling users to exclude private libraries, exclude specific libraries, exclude specific genres, and filter items by date added duration.

**Architecture:** Extend the AST in `kadr-core` with `WidgetFilterConfig` on `WidgetQueryBinding`. Push filter clauses directly down into SQLite queries in `kadr-storage` for maximum performance and exact pagination. Wire filter parameters through `WidgetResolver` in `kadr-server`. Provide a rich, accessible "Advanced Filters" configuration panel in `WidgetConfigModal.tsx` in `web/` with TV UI focus rings.

**Tech Stack:** Rust (axum, rusqlite, deadpool-sqlite, serde), React 19, TypeScript, Tailwind CSS, Lucide icons, Vitest.

## Global Constraints
- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- 100% backward compatibility for existing screen ASTs; `filters` must be optional with default fallback.
- Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `bg-cta`, `text-text-main`, `text-muted`, `border-border-subtle`).
- 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all interactive inputs, toggles, chips, and buttons.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.

---

### Task 1: Core AST Schema & Serde Handling (`crates/kadr-core`)

**Files:**
* Modify: `crates/kadr-core/src/ast.rs:144-180`
* Test: `crates/kadr-core/tests/ast_test.rs`

**Interfaces:**
* Produces:
  ```rust
  #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
  pub struct WidgetFilterConfig {
      #[serde(default)]
      pub exclude_private: bool,
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub exclude_library_ids: Vec<String>,
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub exclude_genres: Vec<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub max_age_days: Option<u32>,
  }

  pub struct WidgetQueryBinding {
      pub macro_type: QueryMacro,
      pub limit: u32,
      pub sort: Option<String>,
      pub filters: Option<WidgetFilterConfig>,
  }

  impl WidgetQueryBinding {
      pub fn with_filters(mut self, filters: WidgetFilterConfig) -> Self;
  }
  ```

- [ ] **Step 1: Write failing tests in `crates/kadr-core/tests/ast_test.rs`**

Test scenarios:
1. `WidgetFilterConfig` default values and serde roundtrip with all fields populated.
2. `WidgetQueryBinding` backward compatibility: deserializing JSON without `filters` produces `filters: None`.
3. `WidgetQueryBinding` with `filters` serializes and deserializes cleanly.
4. `with_filters` builder method correctly sets `filters`.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_core --test ast_test`
Expected: FAIL (types / fields do not exist).

- [ ] **Step 3: Implement `WidgetFilterConfig` in `crates/kadr-core/src/ast.rs`**

1. Define `WidgetFilterConfig` with serde attributes (`default`, `skip_serializing_if`).
2. Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub filters: Option<WidgetFilterConfig>` to `WidgetQueryBinding`.
3. Implement `with_filters(mut self, filters: WidgetFilterConfig) -> Self` on `WidgetQueryBinding`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test -p kadr_core --test ast_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-core/src/ast.rs crates/kadr-core/tests/ast_test.rs
git commit -m "feat(core): add WidgetFilterConfig to WidgetQueryBinding AST"
```

---

### Task 2: Storage Query Push-Down & Filter Builder (`crates/kadr-storage`)

**Files:**
* Modify: `crates/kadr-storage/src/repos/widget_queries.rs:10-277`
* Test: `crates/kadr-storage/tests/widget_queries_test.rs`

**Interfaces:**
* Produces:
  ```rust
  pub fn build_filter_clauses(
      filters: Option<&WidgetFilterConfig>,
      unlocked_ids: &[String],
      current_time_epoch_secs: i64,
  ) -> (String, Vec<rusqlite::types::Value>);
  ```
* Updates methods on `WidgetQueries` to accept `filters: Option<&WidgetFilterConfig>`:
  - `find_recently_added_paginated(library_id, limit, offset, unlocked_ids, filters)`
  - `find_top_rated_paginated(limit, offset, unlocked_ids, filters)`
  - `find_by_genre_paginated(genre, limit, offset, unlocked_ids, filters)`
  - `find_by_library_paginated(library_id, limit, offset, sort, unlocked_ids, filters)`
  - `find_spotlight_candidate(unlocked_ids, filters)`

- [ ] **Step 1: Write failing integration tests in `crates/kadr-storage/tests/widget_queries_test.rs`**

Test scenarios:
1. `exclude_private: true` excludes private library media even when library ID is passed in `unlocked_ids`.
2. `exclude_library_ids` excludes items belonging to the specified library IDs.
3. `exclude_genres` excludes items whose metadata `genres` array contains any of the excluded genres (case-insensitive).
4. `max_age_days` excludes items where `added_at` is older than `now - (days * 86400)`.
5. Combining multiple filters (e.g. `exclude_private` + `exclude_genres` + `max_age_days`) properly compounds in SQL.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_storage --test widget_queries_test`
Expected: FAIL.

- [ ] **Step 3: Implement `build_filter_clauses` and update queries in `widget_queries.rs`**

1. Implement `build_filter_clauses` handling `exclude_private`, `exclude_library_ids`, `exclude_genres`, and `max_age_days`.
2. Update `WidgetQueries` paginated query methods to incorporate `build_filter_clauses`.
3. Retain backwards compatibility for non-paginated convenience methods by passing `None` for filters.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test -p kadr_storage --test widget_queries_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-storage/src/repos/widget_queries.rs crates/kadr-storage/tests/widget_queries_test.rs
git commit -m "feat(storage): push down advanced widget filters into SQLite query clauses"
```

---

### Task 3: Resolver Integration & Filter Evaluation (`crates/kadr-server`)

**Files:**
* Modify: `crates/kadr-server/src/resolver/resolver.rs:170-380`
* Test: `crates/kadr-server/tests/widget_resolver_test.rs`

**Interfaces:**
* Updates `WidgetResolver`:
  - Passes `binding.filters.as_ref()` to `find_recently_added_paginated`, `find_top_rated_paginated`, `find_by_genre_paginated`, `find_by_library_paginated`, and `find_spotlight_candidate`.
  - In `QueryMacro::ContinueWatching`: filters in-progress items via `is_item_matching_filters(item, filters, unlocked_ids)`.

- [ ] **Step 1: Write failing tests in `crates/kadr-server/tests/widget_resolver_test.rs`**

Test scenarios:
1. `resolve_widget_data` with `RecentlyAdded` macro and `filters.exclude_genres: ["Horror"]` excludes horror movies.
2. `resolve_widget_data` with `ContinueWatching` macro and `filters.exclude_library_ids` excludes continue-watching cards from that library.
3. `resolve_widget_data` with `filters.exclude_private: true` excludes private cards even when unlocked tokens are present.
4. Hero banner spotlight respects `binding.filters`.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_server --test widget_resolver_test`
Expected: FAIL.

- [ ] **Step 3: Implement filter wiring in `resolver.rs`**

1. Pass `binding.filters.as_ref()` into `media_repo` calls.
2. Implement `is_item_matching_filters` for in-memory checks in `ContinueWatching` and single-item candidates.
3. Update server tests and existing callers to compile cleanly.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/resolver/resolver.rs crates/kadr-server/tests/widget_resolver_test.rs
git commit -m "feat(server): wire advanced widget filters through WidgetResolver"
```

---

### Task 4: Web Client Types & WidgetConfigModal UI (`web/`)

**Files:**
* Modify: `web/src/types/index.ts:63-95`
* Modify: `web/src/components/studio/WidgetConfigModal.tsx:1-440`
* Create: `web/src/components/studio/WidgetConfigModal.test.tsx`
* Modify: `web/src/components/studio/LayoutStudio.test.tsx`

**Interfaces:**
* Produces:
  ```typescript
  export interface WidgetFilterConfig {
    exclude_private?: boolean;
    exclude_library_ids?: string[];
    exclude_genres?: string[];
    max_age_days?: number;
  }

  export interface WidgetQueryBinding {
    macro_type: QueryMacro | string | Record<string, unknown>;
    limit: number;
    sort?: string;
    filters?: WidgetFilterConfig;
  }
  ```

- [ ] **Step 1: Write failing component tests in `WidgetConfigModal.test.tsx`**

Test scenarios:
1. Renders collapsible "Advanced Filters" section with active filter count badge.
2. Expanding section reveals "Exclude Private Libraries", "Exclude Libraries", "Exclude Genres", and "Date Added" controls.
3. Toggling "Exclude Private Libraries" sets `filters.exclude_private = true`.
4. Checking library checkboxes adds them to `filters.exclude_library_ids`.
5. Clicking popular genre chips adds/removes them from `filters.exclude_genres`.
6. Clicking date added preset pills (e.g. "30 Days") sets `filters.max_age_days = 30`.
7. Submitting modal passes `filters` inside `binding` to `onSave`.
8. Opening modal with existing `initialWidget` containing `filters` pre-fills all filter controls.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run WidgetConfigModal.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement types and modal controls**

1. In `web/src/types/index.ts`: export `WidgetFilterConfig` and update `WidgetQueryBinding`.
2. In `web/src/components/studio/WidgetConfigModal.tsx`:
   - State for `isFiltersOpen`, `excludePrivate`, `excludeLibraryIds`, `excludeGenres`, `maxAgeDays`.
   - Collapsible header button with filter count indicator badge.
   - Private libraries toggle checkbox.
   - Excluded libraries multi-select checkboxes.
   - Excluded genres input with quick-toggle chips (`Horror`, `Romance`, `Kids`, `Animation`, `Documentary`, `Drama`).
   - Max age days input with presets (`7 Days`, `30 Days`, `90 Days`, `1 Year`, `All Time`).
   - Map state to `filters` in `binding` on form submission.
   - Pre-fill state from `initialWidget.binding.filters` when editing.
   - Apply 10-foot TV UI focus rings and design tokens.

- [ ] **Step 4: Run tests and verify clean production build**

Run: `cd web && npm test -- --run WidgetConfigModal.test.tsx`
Run: `cd web && npm test -- --run LayoutStudio.test.tsx`
Run: `cd web && npm test -- --run`
Run: `npm run build` in `web/`
Run: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` from repo root
Expected: 100% PASS across all suites.

- [ ] **Step 5: Commit**

```bash
git add web/src/types/index.ts web/src/components/studio/WidgetConfigModal.tsx web/src/components/studio/WidgetConfigModal.test.tsx web/src/components/studio/LayoutStudio.test.tsx
git commit -m "feat(web): add Advanced Filters configuration panel in WidgetConfigModal"
```
