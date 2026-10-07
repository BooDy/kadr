# Media Directory Link & v0.1.1-alpha Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement direct folder navigation from item details to the folder browser across web and backend, create the complete Client Developer Guide (`docs/CLIENT_GUIDE.md`), create `CHANGELOG.md` for `v0.1.1-alpha`, and update `README.md`.

**Architecture:** Extend `ItemDetailsPayload` with optional `library_id` and `folder_path` in `kadr-core` and compute relative folder paths in `kadr-server`'s `WidgetResolver`. In the web client, wire `ItemDetailsModal`'s "Browse Folder" action to switch libraries and activate `FolderBrowser` at the target folder. Finally, write the comprehensive client specification and release docs for `v0.1.1-alpha`.

**Tech Stack:** Rust (Axum, SQLite, tokio, serde), TypeScript, React 19, Vite 6, Tailwind CSS v4, Lucide React, Markdown.

## Global Constraints

- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- `ItemDetailsPayload` must use `#[serde(default)]` and `skip_serializing_if = "Option::is_none"` to preserve 100% backward compatibility for all existing tests and callers.
- Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `ring-highlight`, `border-border-subtle`).
- 10-foot TV UI focus rings: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all new buttons.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`cd web && npm test -- --run`) must remain 100% passing.
- Frontend production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- Clippy (`cargo clippy --workspace --all-targets -- -D warnings`) must remain clean with 0 warnings.

---

### Task 1: Backend AST Model Extension & Resolver Folder Path Computation

**Files:**
- Modify: `crates/kadr-core/src/ast.rs`
- Modify: `crates/kadr-server/src/resolver/resolver.rs`
- Test: `crates/kadr-server/tests/screen_routes_test.rs`

**Interfaces:**
- Consumes: `MediaItem.file_path`, `MediaItem.library_id`, `LibraryRepository.get_by_id`.
- Produces: `ItemDetailsPayload.library_id: Option<String>`, `ItemDetailsPayload.folder_path: Option<String>`.

- [ ] **Step 1: Write failing integration test for item details with library_id and folder_path**

In `crates/kadr-server/tests/screen_routes_test.rs`, add a test asserting that querying item details returns `library_id` and `folder_path`:

```rust
#[tokio::test]
async fn test_get_item_details_includes_library_and_folder_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let pool = kadr_storage::init_db_file(&db_path).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let event_bus = EventBus::new(32);

    let lib_dir = temp_dir.path().join("movies");
    let sub_dir = lib_dir.join("Sci-Fi").join("Inception (2010)");
    std::fs::create_dir_all(&sub_dir).unwrap();
    let video_file = sub_dir.join("Inception.mkv");
    std::fs::write(&video_file, b"dummy video bytes").unwrap();

    let library = lib_repo
        .create_with_paths(
            "lib-movies-1",
            "Movies",
            &[lib_dir.clone()],
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    let user = user_repo.create("testuser", None).await.unwrap();

    let mut item = MediaItem::new(
        library.id.clone(),
        MediaType::Movie,
        "Inception".to_string(),
        video_file.clone(),
        1000,
    );
    item.metadata.overview = Some("Mind-bending thriller".to_string());
    let created_item = media_repo.create(&item).await.unwrap();
    let item_id = created_item.id.unwrap();

    let resolver = WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
        lib_repo.clone(),
    );

    let details = resolver
        .resolve_item_details(item_id, &user.id)
        .await
        .unwrap()
        .expect("Item details should exist");

    assert_eq!(details.library_id.as_deref(), Some("lib-movies-1"));
    assert_eq!(details.folder_path.as_deref(), Some("Sci-Fi/Inception (2010)"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test screen_routes_test test_get_item_details_includes_library_and_folder_path`  
Expected: FAIL (fields `library_id` and `folder_path` not found on `ItemDetailsPayload`).

- [ ] **Step 3: Update `ItemDetailsPayload` in `crates/kadr-core/src/ast.rs`**

Add fields to `ItemDetailsPayload`:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
```

- [ ] **Step 4: Implement relative folder path calculation in `crates/kadr-server/src/resolver/resolver.rs`**

In `resolve_item_details_with_unlocked`:
```rust
        let (library_id_opt, folder_path_opt) = match self.lib_repo.get_by_id(&item.library_id).await {
            Ok(Some(lib)) => {
                let roots = if lib.paths.is_empty() {
                    vec![lib.path.clone()]
                } else {
                    lib.paths.clone()
                };

                let mut rel_folder = None;
                if let Some(parent) = item.file_path.parent() {
                    let parent_canon = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
                    for root in &roots {
                        let root_canon = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
                        if let Ok(rel) = parent_canon.strip_prefix(&root_canon) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            rel_folder = Some(rel_str);
                            break;
                        }
                    }
                }
                (Some(item.library_id.clone()), rel_folder)
            }
            _ => (Some(item.library_id.clone()), None),
        };
```
And populate `library_id: library_id_opt` and `folder_path: folder_path_opt` in `ItemDetailsPayload`.

- [ ] **Step 5: Run tests to verify passing**

Run: `cargo test -p kadr-server --test screen_routes_test test_get_item_details_includes_library_and_folder_path`  
Expected: PASS.  
Run: `cargo test --workspace` to verify all workspace tests pass.  
Run: `cargo clippy --workspace --all-targets -- -D warnings` to verify clean.

- [ ] **Step 6: Commit**

```bash
git add crates/kadr-core/src/ast.rs crates/kadr-server/src/resolver/resolver.rs crates/kadr-server/tests/screen_routes_test.rs
git commit -m "feat(server): include library_id and relative folder_path in ItemDetailsPayload"
```

---

### Task 2: Frontend Direct Directory Navigation in `ItemDetailsModal`, `FolderBrowser`, and `App`

**Files:**
- Modify: `web/src/types/index.ts`
- Modify: `web/src/components/browse/ItemDetailsModal.tsx`
- Modify: `web/src/components/browse/FolderBrowser.tsx`
- Modify: `web/src/components/browse/BrowseScreen.tsx`
- Modify: `web/src/App.tsx`
- Test: `web/src/components/browse/ItemDetailsModal.test.tsx`
- Test: `web/src/components/browse/FolderBrowser.test.tsx`
- Test: `web/src/components/browse/BrowseScreen.test.tsx`

**Interfaces:**
- Consumes: `ItemDetailsPayload.library_id`, `ItemDetailsPayload.folder_path`.
- Produces: `onNavigateToFolder(libraryId, folderPath)` handler in `ItemDetailsModal`, `initialPath` in `FolderBrowser`, `handleNavigateToFolder` in `App.tsx`.

- [ ] **Step 1: Write failing component tests for "Browse Folder" button and navigation**

In `web/src/components/browse/ItemDetailsModal.test.tsx`, add tests:
```tsx
it('renders "Browse Folder" button when library_id is provided and calls onNavigateToFolder', () => {
  const onNavigateToFolder = vi.fn();
  const onClose = vi.fn();
  const detailsWithFolder: ItemDetailsPayload = {
    ...mockMovieDetails,
    library_id: 'lib-movies-1',
    folder_path: 'Sci-Fi/Inception (2010)',
  };

  render(
    <ItemDetailsModal
      isOpen={true}
      onClose={onClose}
      itemId={101}
      initialDetails={detailsWithFolder}
      onPlay={vi.fn()}
      onNavigateToFolder={onNavigateToFolder}
    />
  );

  const browseBtn = screen.getByRole('button', { name: /browse folder/i });
  expect(browseBtn).toBeInTheDocument();
  fireEvent.click(browseBtn);

  expect(onClose).toHaveBeenCalled();
  expect(onNavigateToFolder).toHaveBeenCalledWith('lib-movies-1', 'Sci-Fi/Inception (2010)');
});

it('does not render "Browse Folder" button when library_id is omitted or onNavigateToFolder not passed', () => {
  const detailsWithoutFolder: ItemDetailsPayload = {
    ...mockMovieDetails,
    library_id: undefined,
    folder_path: undefined,
  };

  render(
    <ItemDetailsModal
      isOpen={true}
      onClose={vi.fn()}
      itemId={101}
      initialDetails={detailsWithoutFolder}
      onPlay={vi.fn()}
    />
  );

  expect(screen.queryByRole('button', { name: /browse folder/i })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npm test -- --run ItemDetailsModal.test.tsx`  
Expected: FAIL (`onNavigateToFolder` not supported or "Browse Folder" button not found).

- [ ] **Step 3: Update `web/src/types/index.ts`**

Add `library_id?: string;` and `folder_path?: string;` to `ItemDetailsPayload`.

- [ ] **Step 4: Update `ItemDetailsModal.tsx`**

1. Import `Folder` from `lucide-react`.
2. Add `onNavigateToFolder?: (libraryId: string, folderPath: string) => void;` to `ItemDetailsModalProps`.
3. In the action button group (next to Subtitles button):
```tsx
{details.library_id && onNavigateToFolder && (
  <button
    type="button"
    onClick={() => {
      onClose();
      onNavigateToFolder(details.library_id!, details.folder_path ?? '');
    }}
    className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium bg-panel hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
  >
    <Folder className="w-4 h-4 text-accent" />
    <span>Browse Folder</span>
  </button>
)}
```

- [ ] **Step 5: Update `FolderBrowser.tsx`**

1. Add `initialPath?: string;` and `onNavigateToFolder?: (libraryId: string, folderPath: string) => void;` to `FolderBrowserProps`.
2. When mounted or `initialPath` changes:
```tsx
useEffect(() => {
  const target = initialPath ?? '';
  setCurrentPath(target);
  navigateTo(target);
}, [libraryId, initialPath, navigateTo]);
```
3. When child `ItemDetailsModal` triggers `onNavigateToFolder(libId, fPath)`:
```tsx
const handleItemFolderNavigate = (libId: string, fPath: string) => {
  if (libId === libraryId) {
    navigateTo(fPath);
  } else {
    onNavigateToFolder?.(libId, fPath);
  }
};
```
Pass `onNavigateToFolder={handleItemFolderNavigate}` to `<ItemDetailsModal>`.

- [ ] **Step 6: Update `BrowseScreen.tsx`**

1. Add `initialViewMode?: 'catalog' | 'folders';`, `initialFolder?: string;`, and `onNavigateToFolder?: (libraryId: string, folderPath: string) => void;` to `BrowseScreenProps`.
2. Initialize `viewMode` with `initialViewMode ?? 'catalog'`.
3. Track `folderPath` state, initialized with `initialFolder ?? ''`.
4. When `onNavigateToFolder(targetLibId, targetFolder)` is called:
   - If `targetLibId === screenId`:
     - `setViewMode('folders')`
     - `setFolderPath(targetFolder)`
   - If `targetLibId !== screenId`:
     - `onNavigateToFolder?.(targetLibId, targetFolder)`
5. Pass `initialPath={folderPath}` to `<FolderBrowser>`.
6. Pass `onNavigateToFolder={handleFolderNavigate}` to `<ItemDetailsModal>`.

- [ ] **Step 7: Update `App.tsx`**

1. Track `pendingFolderNav: { libraryId: string; folderPath: string } | null`.
2. Add `handleNavigateToFolder = (libraryId: string, folderPath: string) => { ... }`:
   - Find library in `libraries`.
   - If library is private and locked:
     - Set `setPendingFolderNav({ libraryId, folderPath })` and `setLibraryToUnlock(lib)`.
   - If library is unlocked:
     - Set `setActiveLibrary(lib)`.
     - Set `setCurrentView('library-' + lib.id)`.
     - Pass `initialViewMode="folders"` and `initialFolder={folderPath}` to `<BrowseScreen>`.
3. In `handleUnlockSuccess`:
   - If `pendingFolderNav` matches the unlocked library, set `setActiveLibrary(lib)`, set view, and clear `pendingFolderNav`.

- [ ] **Step 8: Run web tests and verify passing**

Run: `cd web && npm test -- --run`  
Run: `npm run build`  
Expected: All 15+ test suites pass, build succeeds with 0 errors.

- [ ] **Step 9: Commit**

```bash
git add web/src/types/index.ts web/src/components/browse/ItemDetailsModal.tsx web/src/components/browse/ItemDetailsModal.test.tsx web/src/components/browse/FolderBrowser.tsx web/src/components/browse/FolderBrowser.test.tsx web/src/components/browse/BrowseScreen.tsx web/src/components/browse/BrowseScreen.test.tsx web/src/App.tsx
git commit -m "feat(web): add Browse Folder action in ItemDetailsModal with direct folder navigation"
```

---

### Task 3: Complete Client Developer Guide (`docs/CLIENT_GUIDE.md`)

**Files:**
- Create: `docs/CLIENT_GUIDE.md`

**Interfaces:**
- Documents: All public REST endpoints, headers, AST schemas, playback lifecycle, streaming, subtitles, folder browser, and SSE event streaming.

- [ ] **Step 1: Write `docs/CLIENT_GUIDE.md`**

Create `docs/CLIENT_GUIDE.md` with:
1. **Overview & Principles**:
   - HTTP Base URL, port 8492, REST JSON API conventions.
   - Client authentication headers (`Authorization: Bearer <token>`, `X-Library-Unlock-Token: <token>`).
   - Pure HTTP 206 range-request direct play (no transcode required, client-side AV decoding).
2. **Network Autodiscovery (`GET /api/v1/discovery`)**:
   - Schema and response example (`server_id`, `name`, `version`, `protocol_version`, `setup_completed`, `status`).
   - UDP broadcast and subnet scanning recommendations.
3. **Authentication, Profiles & Private Libraries**:
   - Profiles list (`GET /api/v1/auth/profiles`).
   - Login & PIN token generation (`POST /api/v1/auth/login`).
   - Private Library PIN unlock (`POST /api/v1/libraries/:id/unlock`) and token usage.
4. **Declarative Layout Engine (Screen AST)**:
   - Summary of screens (`GET /api/v1/screens`).
   - Hydrated layouts (`GET /api/v1/screens/:id?hydrated=true`).
   - Complete AST JSON structure (`HeroSpotlight`, `HorizontalCarousel`, `CatalogGrid`, `ContinueWatching`).
   - Badges (`4K`, `NEW`, `RESUME` progress bar).
   - Pagination (`GET /api/v1/screens/:id/widgets/:widget_id/items?cursor=...&limit=...`).
5. **Item Details & Direct Folder Navigation**:
   - Query endpoint (`GET /api/v1/screens/items/:id`).
   - Movie payload vs Show payload (with hydrated season tabs and episode lists).
   - Using `library_id` and `folder_path` for direct jumping to folder browser.
6. **Video Streaming & 10s Playback Scrobbling**:
   - Stream endpoint (`GET /api/v1/streaming/:id/stream`) with byte ranges.
   - Playback session creation (`POST /api/v1/playback/sessions`).
   - Periodic heartbeat (`POST /api/v1/playback/sessions/heartbeat`).
   - Stop session (`DELETE /api/v1/playback/sessions/:id`).
7. **Subtitles & Media Assets**:
   - Listing tracks (`GET /api/v1/subtitles/items/:id`).
   - WebVTT streaming (`GET /api/v1/subtitles/stream/:id`).
   - Live OpenSubtitles search & download (`POST /api/v1/subtitles/search`, `POST /api/v1/subtitles/download`).
   - Video thumbnails (`GET /api/v1/thumbnails/videos/:id`) and library image stream (`GET /api/v1/libraries/:id/image`).
8. **Hierarchical Folder Browsing & Real-Time SSE Stream**:
   - Directory navigation (`GET /api/v1/libraries/:id/folders?path=...`).
   - Server-Sent Events stream (`GET /api/v1/events`).
   - Event types (`library:updated`, `session:synced`, `subtitle:downloaded`, `system:telemetry`).

- [ ] **Step 2: Self-review document for accuracy against codebase implementation**

Verify every route path, request body, and response payload matches the actual Rust router definitions in `crates/kadr-server/src/api/`.

- [ ] **Step 3: Commit**

```bash
git add docs/CLIENT_GUIDE.md
git commit -m "docs: add comprehensive Client Developer Guide"
```

---

### Task 4: Release Documentation Suite: `CHANGELOG.md` & `README.md` (`v0.1.1-alpha`)

**Files:**
- Create: `CHANGELOG.md`
- Modify: `README.md`

**Interfaces:**
- Documents: All additions, changes, and fixes in `v0.1.1-alpha` since `v0.1.0-alpha.2`.

- [ ] **Step 1: Create `CHANGELOG.md`**

Write standard Keep a Changelog document detailing `[v0.1.1-alpha] - 2026-10-07`:
- **Added**:
  - Direct folder navigation link from item details to folder browser.
  - Image viewing and fullscreen auto-advancing slideshow in folder browser.
  - Series and seasons grouping with episode cue parsing and 1-click playback.
  - Video thumbnail extraction engine and on-demand fallback routes.
  - Network autodiscovery endpoint (`/discovery`) and configurable server identity.
  - Hierarchical folder browser with breadcrumbs and view switcher.
  - In-player live subtitle search and download via OpenSubtitles.
  - Advanced widget filters (exclude private, exclude libraries, exclude genres, max age days, rating filters).
  - Interactive Layout Studio with declarative AST editor.
  - Multi-path library management with server filesystem browser modal.
  - Private libraries with 4-digit PIN protection, database isolation, and HMAC unlock tokens.
  - Inline library renaming in Admin Dashboard.
  - Comprehensive Client Developer Guide (`docs/CLIENT_GUIDE.md`).
- **Changed & Improved**:
  - Theme design tokens overhaul with 10-foot TV UI focus states.
  - SQLite WAL query pushdown and symlink traversal cycle deduplication.
  - Unified admin dashboard tabs embedding Studio and Telemetry.
- **Fixed**:
  - Path traversal checks and file type validation on folder and image endpoints.
  - Video player HUD auto-hide resets during keyboard interactions.
  - DOM fullscreen event synchronization.

- [ ] **Step 2: Update `README.md`**

1. Update version badges to `v0.1.1-alpha`.
2. Update **Key Features** to include Series/Seasons grouping, Photo viewing/slideshow, Direct directory navigation, Autodiscovery, and In-player subtitles.
3. Update the **Architecture Diagram** to include the thumbnail pipeline, folder browser, and autodiscovery services.
4. Add direct links to `CHANGELOG.md` and `docs/CLIENT_GUIDE.md`.

- [ ] **Step 3: Verify workspace build and tests**

Run: `cargo test --workspace`  
Run: `cd web && npm test -- --run`  
Run: `npm run build`  
Run: `cargo clippy --workspace --all-targets -- -D warnings`

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md README.md
git commit -m "docs: create CHANGELOG for v0.1.1-alpha and update README"
```
