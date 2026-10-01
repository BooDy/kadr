# Milestone 3: Declarative Widget AST & Home Layout Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Declarative Widget AST & Home Layout Engine, providing server-driven UI layouts (Hero banner, Continue Watching shelf, Recently Added carousels, Top Rated carousels, Genre shelves, Paginated grids), concurrent data hydration, item detail inspection, and artwork streaming.

**Architecture:** Strongly-typed AST enums in `kadr-core`, indexed JSON metadata queries in `kadr-storage`, an in-memory `LayoutRegistry` and concurrent async `WidgetResolver` in `kadr-server`, exposed over Axum REST endpoints (`/api/v1/screens`, `/api/v1/widgets`, `/api/v1/items`, `/api/v1/artwork`).

**Tech Stack:** Rust (2021 edition), Axum 0.8, SQLite with WAL mode (`deadpool-sqlite`, `rusqlite`), `serde` / `serde_json`, `tokio`, `futures::future::join_all`, `tracing`.

## Global Constraints
- RSS memory usage must remain <= 30 MB under idle and standard layout query workloads.
- Latency / response budget: < 50 ms for fully hydrated screen layout responses.
- Zero-external runtime dependencies baseline (compilable against `x86_64-unknown-linux-musl`).
- All database operations in SQLite must run with WAL mode, `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.
- All screen, widget, and item inspection endpoints must require `AuthUser` authentication.
- Artwork streaming must support both Bearer tokens and unauthenticated/query-token requests with `Cache-Control: public, max-age=86400`.

---

### Task 1: Declarative Widget AST Schema & Domain Models (`kadr-core`)

**Files:**
- Create: `crates/kadr-core/src/ast.rs`
- Modify: `crates/kadr-core/src/lib.rs`
- Test: `crates/kadr-core/tests/ast_test.rs`

**Interfaces:**
- Consumes: `TechnicalInfo` from `crates/kadr-core/src/models.rs`
- Produces: `ScreenId`, `CardViewModel`, `QueryMacro`, `WidgetQueryBinding`, `WidgetNode`, `ItemDetailsPayload`, `ScreenLayout`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-core/tests/ast_test.rs` testing JSON serialization and deserialization of `ScreenLayout`, `WidgetNode::HeroBanner`, `WidgetNode::Carousel`, `WidgetNode::Grid`, `WidgetNode::ItemDetails`, and `CardViewModel`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-core --test ast_test`
Expected: Compilation failure because `ast` module does not exist.

- [ ] **Step 3: Implement AST models**
Create `crates/kadr-core/src/ast.rs` defining `ScreenId`, `CardViewModel`, `QueryMacro`, `WidgetQueryBinding`, `WidgetNode`, `ItemDetailsPayload`, and `ScreenLayout`. Expose in `crates/kadr-core/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-core --test ast_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(core): implement Declarative Widget AST models and view models`

---

### Task 2: Storage Migration 003 & Extended `MediaItemRepository` Queries (`kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/migrations/003_widget_query_indexes.sql`
- Modify: `crates/kadr-storage/src/migrations.rs`
- Modify: `crates/kadr-storage/src/repos/media_repo.rs`
- Test: `crates/kadr-storage/tests/widget_queries_test.rs`

**Interfaces:**
- Consumes: `MediaItem` from `kadr-core`
- Produces: `find_recently_added`, `find_top_rated`, `find_by_genre`, `find_by_library_paginated`, `find_spotlight_candidate`, `find_episodes_by_series`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-storage/tests/widget_queries_test.rs` asserting migration 003 applies and validating queries for recently added, top rated, by genre, paginated library items, and spotlight candidates.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-storage --test widget_queries_test`
Expected: Compilation failure due to missing repository methods.

- [ ] **Step 3: Implement migration and repository query methods**
- Create `crates/kadr-storage/src/migrations/003_widget_query_indexes.sql` adding `CREATE INDEX IF NOT EXISTS idx_media_rating ON media_items(CAST(json_extract(metadata, '$.rating') AS REAL) DESC);`.
- Register in `crates/kadr-storage/src/migrations.rs`.
- Implement `find_recently_added`, `find_top_rated`, `find_by_genre`, `find_by_library_paginated`, `find_spotlight_candidate`, and `find_episodes_by_series` in `crates/kadr-storage/src/repos/media_repo.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-storage --test widget_queries_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(storage): add migration 003 and extended media item queries for widgets`

---

### Task 3: In-Memory `LayoutRegistry` & Default Screen Layouts (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/layout/mod.rs`
- Create: `crates/kadr-server/src/layout/registry.rs`
- Modify: `crates/kadr-server/src/lib.rs`
- Test: `crates/kadr-server/tests/layout_registry_test.rs`

**Interfaces:**
- Consumes: `ScreenId`, `ScreenLayout`, `WidgetNode`, `QueryMacro` from `kadr-core::ast`
- Produces: `LayoutRegistry` with `get_screen(&self, id: &ScreenId) -> Option<ScreenLayout>`, `list_screens(&self) -> Vec<(ScreenId, String)>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/layout_registry_test.rs` testing `LayoutRegistry` initialization with built-in `Home`, `Movies`, and `Shows` layouts.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test layout_registry_test`
Expected: Compilation failure because `layout` module does not exist.

- [ ] **Step 3: Implement LayoutRegistry**
Create `crates/kadr-server/src/layout/registry.rs` defining the default `Home` (Hero, Continue Watching, Recently Added, Top Rated, Genre Shelves), `Movies` (Grid), and `Shows` (Grid) layouts. Expose in `src/layout/mod.rs` and `src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test layout_registry_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement in-memory LayoutRegistry with default screen layouts`

---

### Task 4: Card Normalization & Concurrent `WidgetResolver` (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/resolver/mod.rs`
- Create: `crates/kadr-server/src/resolver/card.rs`
- Create: `crates/kadr-server/src/resolver/resolver.rs`
- Modify: `crates/kadr-server/src/lib.rs`
- Test: `crates/kadr-server/tests/widget_resolver_test.rs`

**Interfaces:**
- Consumes: `MediaItemRepository`, `PlaybackRepository`, `ScreenLayout`, `WidgetNode`
- Produces: `WidgetResolver::new(...)`, `resolve_screen(&self, screen: ScreenLayout, user_id: &str) -> ScreenLayout`, `resolve_widget_data(&self, binding: &WidgetQueryBinding, user_id: &str, offset: u32) -> Result<...>`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/widget_resolver_test.rs` testing concurrent hydration of `HeroBanner`, `Carousel`, and `Grid` widgets, ensuring `playback_progress` and badges (`4K`, `NEW`, `RESUME`) are properly calculated.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test widget_resolver_test`
Expected: Compilation failure because `resolver` module does not exist.

- [ ] **Step 3: Implement Card Normalizer and WidgetResolver**
- Create `card.rs` to map `MediaItem` and optional `PlaybackState` into `CardViewModel`.
- Create `resolver.rs` using `futures::future::join_all` to concurrently query `MediaItemRepository` and `PlaybackRepository`, populating `data` on `HeroBanner`, `items` on `Carousel` and `Grid`, and omitting/empty handling for `ContinueWatching`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test widget_resolver_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement card view model normalizer and concurrent WidgetResolver`

---

### Task 5: Screen, Widget & Item Detail API Routes (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/screen_routes.rs`
- Create: `crates/kadr-server/src/api/widget_routes.rs`
- Create: `crates/kadr-server/src/api/item_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Test: `crates/kadr-server/tests/screen_routes_test.rs`

**Interfaces:**
- Consumes: `LayoutRegistry`, `WidgetResolver`, `AuthUser`
- Produces: Axum handlers for `GET /api/v1/screens`, `GET /api/v1/screens/:id`, `GET /api/v1/widgets/:id/data`, `GET /api/v1/items/:id/details`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/screen_routes_test.rs` testing authenticated calls to `/api/v1/screens`, `/api/v1/screens/home`, `/api/v1/widgets/recently_added/data`, and `/api/v1/items/:id/details`.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test screen_routes_test`
Expected: Compilation failure due to missing routes.

- [ ] **Step 3: Implement screen, widget, and item detail routes**
Implement `screen_routes.rs`, `widget_routes.rs`, and `item_routes.rs`. Mount in `crates/kadr-server/src/api/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test screen_routes_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement screen layouts, widget data, and item detail REST routes`

---

### Task 6: High-Performance Artwork Streaming Route (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/artwork_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Test: `crates/kadr-server/tests/artwork_routes_test.rs`

**Interfaces:**
- Consumes: `MediaItemRepository`
- Produces: `GET /api/v1/artwork/:id/poster`, `GET /api/v1/artwork/:id/backdrop`

- [ ] **Step 1: Write the failing test**
Create `crates/kadr-server/tests/artwork_routes_test.rs` testing image streaming with proper `Content-Type` (`image/jpeg`, `image/png`, `image/webp`), `Cache-Control: public, max-age=86400`, and `404 Not Found` when artwork is missing.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p kadr-server --test artwork_routes_test`
Expected: Compilation failure due to missing artwork handlers.

- [ ] **Step 3: Implement artwork streaming routes**
Create `artwork_routes.rs` reading `poster_path` / `backdrop_path` from item metadata, verifying path exists, and streaming via `tokio::fs::File`. Mount in `crates/kadr-server/src/api/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p kadr-server --test artwork_routes_test`
Expected: PASS

- [ ] **Step 5: Run workspace tests and clippy**
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): implement cached artwork streaming endpoints for posters and backdrops`

---

### Task 7: Server Bootstrap Assembly & End-to-End Milestone 3 Integration Test

**Files:**
- Modify: `crates/kadr-server/src/main.rs`
- Modify: `crates/kadr-server/Cargo.toml`
- Create: `tests/e2e_widget_ast_test.rs`

**Interfaces:**
- Consumes: All Milestone 3 components across all crates
- Produces: Fully assembled server with layout engine and E2E test verifying end-to-end user journey

- [ ] **Step 1: Write the end-to-end integration test**
Create `tests/e2e_widget_ast_test.rs` exercising the complete Milestone 3 flow:
1. Ingest media items with metadata and artwork.
2. Authenticate admin user via PIN `1234`.
3. Fetch screen list `/api/v1/screens`.
4. Fetch `/api/v1/screens/home` and assert HeroBanner, Recently Added, and Top Rated are hydrated.
5. Post playback progress for an item (>60s) to transition to `InProgress`.
6. Re-fetch `/api/v1/screens/home` and assert Continue Watching carousel appears with progress badge.
7. Fetch item details `/api/v1/items/:id/details`.
8. Fetch `/api/v1/artwork/:id/poster` and verify HTTP 200 with cache headers.

- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test --test e2e_widget_ast_test`
Expected: Compilation/routing failure.

- [ ] **Step 3: Wire LayoutRegistry and WidgetResolver in `main.rs`**
Update `main.rs` and `create_router(...)` to instantiate `LayoutRegistry` and `WidgetResolver` and register all new routes. Update `crates/kadr-server/Cargo.toml` with `[[test]]` targeting `../../tests/e2e_widget_ast_test.rs`.

- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test e2e_widget_ast_test`
Expected: PASS

- [ ] **Step 5: Run full workspace verification**
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: All tests pass, 0 clippy warnings.

- [ ] **Step 6: Commit**
Commit: `feat(server): wire LayoutRegistry, WidgetResolver into main router and add Milestone 3 E2E test`
