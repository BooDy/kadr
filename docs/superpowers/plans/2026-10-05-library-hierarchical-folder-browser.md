# Library Hierarchical Folder Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a filesystem-like folder browser for media libraries in Kadr, allowing users to navigate actual disk directory trees and play media files enriched with posters, ratings, and playback progress.

**Architecture:**
* Backend: Add `GET /api/v1/libraries/{id}/folders?path=...` in `crates/kadr-server/src/api/library_routes.rs` with strict canonical sandboxing within library root paths, private library unlock verification, asynchronous `tokio::fs::read_dir` traversal, and SQLite batch lookup (`media_item_repo.find_by_paths_batch`) to return `LibraryFolderResponse`.
* Frontend: Add types and `api.getLibraryFolders` in `web/src/api/client.ts`; build `FolderBrowser.tsx` with clickable breadcrumbs, responsive directory cards, and rich media poster cards; integrate a `[ ▦ Catalog ]` / `[ 📁 Folders ]` view mode switcher in `BrowseScreen.tsx`.

**Tech Stack:** Rust (Axum, Tokio, Serde), React 19, TypeScript, Tailwind CSS v4, Lucide React, Vitest.

## Global Constraints
* Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
* Strict security sandboxing: any path with `..`, absolute prefix, null bytes, or resolving outside canonical library root(s) must return `400 Bad Request`.
* Private library protection: if `library.is_private`, verify unlock token via `UnlockedLibraries`, returning `403 Forbidden` if locked.
* Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `bg-cta`, `text-text-main`, `text-muted`, `border-border-subtle`).
* 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all buttons and cards.
* Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
* 100% test pass rate across Rust workspace (`cargo test --workspace`) and frontend tests (`npm test -- --run`).

---

### Task 1: Backend Sandboxed Directory Traversal API & Batch DB Enrichment

**Files:**
* Modify: `crates/kadr-server/src/api/library_routes.rs:25-80, 480-524`
* Modify: `crates/kadr-server/src/api/mod.rs:80-140`
* Create: `crates/kadr-server/tests/library_folder_routes_test.rs`

**Interfaces:**
* Produces:
  * Route: `GET /api/v1/libraries/{id}/folders?path={optional_subpath}`
  * DTOs:
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FolderEntry {
        pub name: String,
        pub path: String,
        pub item_count: usize,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BreadcrumbItem {
        pub name: String,
        pub path: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LibraryFolderResponse {
        pub library_id: String,
        pub library_name: String,
        pub current_path: String,
        pub parent_path: Option<String>,
        pub breadcrumbs: Vec<BreadcrumbItem>,
        pub directories: Vec<FolderEntry>,
        pub items: Vec<CardViewModel>,
    }
    ```

- [ ] **Step 1: Write failing integration test in `library_folder_routes_test.rs`**

Test scenarios:
1. `GET /api/v1/libraries/{id}/folders` returns root subdirectories and media items.
2. `GET /api/v1/libraries/{id}/folders?path=Action` returns subfolder contents with breadcrumbs.
3. Path traversal attempt `?path=../../etc` returns 400 Bad Request.
4. Non-existent path returns 404 Not Found.
5. Private library without token returns 403 Forbidden; with token returns 200 OK.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test library_folder_routes_test`
Expected: FAIL (route not registered, handler not found).

- [ ] **Step 3: Implement handler and route in `crates/kadr-server`**

1. In `crates/kadr-server/src/api/library_routes.rs`:
   - Implement `browse_library_folders` handler.
   - Validate library exists and check `unlocked` token if `is_private`.
   - Sandbox `path`: sanitize, ensure relative, resolve with canonical library root(s), verify `starts_with`.
   - Asynchronously read target dir using `tokio::fs::read_dir`.
   - Filter hidden files, collect `directories` with item counts.
   - Filter video extensions, collect paths, query `media_item_repo.find_by_paths_batch(&video_paths)`.
   - Query watch states and convert to `CardViewModel`s.
   - Build breadcrumbs.
2. In `crates/kadr-server/src/api/mod.rs`:
   - Register route `.route("/api/v1/libraries/{id}/folders", get(library_routes::browse_library_folders))`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test --test library_folder_routes_test`
Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/api/library_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/tests/library_folder_routes_test.rs
git commit -m "feat(server): add sandboxed library folder browsing API with batch metadata enrichment"
```

---

### Task 2: Web Client API & FolderBrowser Component

**Files:**
* Modify: `web/src/types/index.ts:250-320`
* Modify: `web/src/api/client.ts:380-420`
* Create: `web/src/components/browse/FolderBrowser.tsx`
* Create: `web/src/components/browse/FolderBrowser.test.tsx`

**Interfaces:**
* Produces:
  * Types: `FolderEntry`, `BreadcrumbItem`, `LibraryFolderResponse`.
  * `api.getLibraryFolders(libraryId: string, path?: string): Promise<LibraryFolderResponse>`.
  * Component `FolderBrowser`:
    ```typescript
    export interface FolderBrowserProps {
      libraryId: string;
      onPlayItem: (itemId: number) => void;
    }
    ```

- [ ] **Step 1: Write failing component tests in `FolderBrowser.test.tsx`**

Test scenarios:
1. Renders breadcrumbs and root directories.
2. Clicking a directory card calls `api.getLibraryFolders` with subpath and updates breadcrumbs.
3. Clicking "Up one level" navigates back to parent folder.
4. Renders media cards with titles, ratings, and artwork.
5. Clicking a media card opens `ItemDetailsModal`; clicking play calls `onPlayItem`.
6. Empty state notice when a directory has no subfolders and no media items.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run FolderBrowser.test.tsx`
Expected: FAIL (component and API methods not implemented).

- [ ] **Step 3: Implement types, client method, and `FolderBrowser.tsx`**

1. In `web/src/types/index.ts`: export `FolderEntry`, `BreadcrumbItem`, `LibraryFolderResponse`.
2. In `web/src/api/client.ts`: implement `getLibraryFolders(libraryId, path)`.
3. In `web/src/components/browse/FolderBrowser.tsx`:
   - State for `currentPath`, `data`, `loading`, `error`, `selectedItemId`.
   - Render clickable breadcrumbs with divider slashes and `← Up one level` button.
   - Render responsive directories grid (accent `#FF6B00` folder icon, name, count badge, hover elevation, focus ring).
   - Render responsive media cards grid with poster images, badges, progress bars, and hover play action.
   - Render `ItemDetailsModal` when `selectedItemId !== null`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cd web && npm test -- --run FolderBrowser.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/types/index.ts web/src/api/client.ts web/src/components/browse/FolderBrowser.tsx web/src/components/browse/FolderBrowser.test.tsx
git commit -m "feat(web): add FolderBrowser component with breadcrumb navigation and enriched media cards"
```

---

### Task 3: BrowseScreen View Mode Switcher (Catalog vs Folders)

**Files:**
* Modify: `web/src/components/browse/BrowseScreen.tsx:1-120`
* Modify: `web/src/components/browse/BrowseScreen.test.tsx`

**Interfaces:**
* Produces:
  * In `BrowseScreen.tsx`:
    * Segmented mode toggle: `[ ▦ Catalog ]` / `[ 📁 Folders ]` (visible when viewing a library).
    * `viewMode` state: `'catalog' | 'folders'`.
    * When `viewMode === 'folders'`, renders `<FolderBrowser libraryId={screenId} onPlayItem={onPlayItem} />`.

- [ ] **Step 1: Write failing tests in `BrowseScreen.test.tsx`**

Test scenarios:
1. When viewing a library, renders `Catalog` and `Folders` view mode buttons in header.
2. Clicking `Folders` button switches to `FolderBrowser`.
3. Clicking `Catalog` button switches back to declarative widget layout.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run BrowseScreen.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement mode switcher in `BrowseScreen.tsx`**

1. Import `FolderBrowser` and `Folder`, `LayoutGrid` icons from `lucide-react`.
2. Add `viewMode: 'catalog' | 'folders'` state (defaulting to `'catalog'`).
3. Add header bar with segmented toggle buttons styled with theme tokens (`bg-panel border border-border-subtle rounded-xl p-1`, focus rings).
4. Conditionally render `<FolderBrowser libraryId={screenId} onPlayItem={onPlayItem} />` when `viewMode === 'folders'`.

- [ ] **Step 4: Run tests and verify clean build**

Run: `cd web && npm test -- --run BrowseScreen.test.tsx && npm test -- --run`
Run: `npm run build` in `web/`
Run: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` from repo root
Expected: 100% PASS across all suites.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/browse/BrowseScreen.tsx web/src/components/browse/BrowseScreen.test.tsx
git commit -m "feat(web): integrate catalog vs folders view mode switcher in BrowseScreen"
```
