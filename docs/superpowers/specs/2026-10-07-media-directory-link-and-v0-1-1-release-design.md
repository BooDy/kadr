# Design Specification: Media Directory Link & v0.1.1-alpha Release Suite

**Date:** 2026-10-07  
**Target Version:** `v0.1.1-alpha`  
**Status:** Approved  

---

## 1. Overview & Goals

This specification covers two coordinated efforts for the `v0.1.1-alpha` release of **Kadr**:
1. **Direct Directory Navigation**: Adding a direct "Browse Folder" action in the Media Details modal (`ItemDetailsModal`) that immediately navigates to that media item's containing folder inside the Folder Browser (`FolderBrowser`), supporting both local library switching and cross-library navigation with private library PIN unlock handling.
2. **v0.1.1-alpha Release Documentation Suite**:
   - Creating a comprehensive, end-to-end **Client Developer Guide** (`docs/CLIENT_GUIDE.md`) to guide developers building third-party TV, mobile, and desktop clients for Kadr.
   - Creating a standard **`CHANGELOG.md`** capturing all 100+ commits and features since `v0.1.0-alpha.2`.
   - Updating **`README.md`** with the new architecture, key features, and documentation links.

---

## 2. Direct Directory Navigation Architecture

### 2.1 Backend Changes (`crates/kadr-core` & `crates/kadr-server`)

#### AST Extension (`crates/kadr-core/src/ast.rs`)
`ItemDetailsPayload` is extended with optional `library_id` and `folder_path` fields:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDetailsPayload {
    pub card: CardViewModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical: Option<TechnicalInfo>,
    pub stream_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_position_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes: Option<Vec<CardViewModel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
}
```
*Note:* The `#[serde(default)]` and `skip_serializing_if = "Option::is_none"` attributes ensure 100% backward compatibility for existing callers, tests, and mock structures.

#### Resolver Implementation (`crates/kadr-server/src/resolver/resolver.rs`)
In `resolve_item_details_with_unlocked`:
1. Query the library record: `self.lib_repo.get_by_id(&item.library_id).await`.
2. Determine library roots: If `library.paths` is non-empty, use `library.paths`, otherwise `vec![library.path]`.
3. Canonicalize roots and examine `item.file_path.parent()`:
   - For each canonical root in the library, check if `parent_dir.starts_with(root)`.
   - If matched, compute the relative path `let rel = parent_dir.strip_prefix(root)`.
   - If `rel` is empty, `folder_path = Some("".to_string())`.
   - If `rel` is non-empty, format with forward slashes: `Some(rel.to_string_lossy().replace('\\', "/"))`.
4. If no root matches or library is not found, `folder_path` defaults to `None`.
5. Populate `library_id: Some(item.library_id.clone())` and `folder_path`.

---

### 2.2 Frontend Architecture (`web/`)

#### TypeScript Types (`web/src/types/index.ts`)
Extend `ItemDetailsPayload`:
```typescript
export interface ItemDetailsPayload {
  card: CardViewModel;
  overview?: string;
  genres: string[];
  duration_seconds?: number;
  technical?: TechnicalInfo;
  stream_url: string;
  resume_position_seconds?: number;
  episodes?: CardViewModel[];
  library_id?: string;
  folder_path?: string;
}
```

#### ItemDetailsModal (`web/src/components/browse/ItemDetailsModal.tsx`)
1. Prop update: Add `onNavigateToFolder?: (libraryId: string, folderPath: string) => void`.
2. UI rendering:
   In the main action button row (alongside "Play / Resume" and "Subtitles"):
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

#### FolderBrowser (`web/src/components/browse/FolderBrowser.tsx`)
1. Prop update: Add `initialPath?: string` to `FolderBrowserProps`.
2. State & effect:
   - When mounted or when `initialPath` changes, call `navigateTo(initialPath ?? '')`.
   - When child `ItemDetailsModal` fires `onNavigateToFolder(libraryId, folderPath)`:
     - If `libraryId === currentLibraryId`, call `navigateTo(folderPath)`.
     - Otherwise, bubble up to parent `onNavigateToFolder`.

#### BrowseScreen (`web/src/components/browse/BrowseScreen.tsx`)
1. Props update:
   - `initialViewMode?: 'catalog' | 'folders'`
   - `initialFolder?: string`
   - `onNavigateToFolder?: (libraryId: string, folderPath: string) => void`
2. Behavior:
   - When `onNavigateToFolder(libraryId, folderPath)` is received:
     - If `libraryId === screenId`:
       - `setViewMode('folders')`
       - Pass `folderPath` to `<FolderBrowser initialPath={targetFolder} ... />`.
     - If `libraryId !== screenId`:
       - Delegate to parent `onNavigateToFolder(libraryId, folderPath)`.

#### App Shell (`web/src/App.tsx`)
1. State management:
   - `folderNavigationTarget: { libraryId: string; folderPath: string } | null`.
2. Navigation Handler:
   - Define `handleNavigateToFolder(libraryId: string, folderPath: string)`:
     - Find library in `libraries`.
     - If library is private and locked:
       - Set `libraryToUnlock = lib` and stage `folderNavigationTarget`.
       - Upon successful unlock, navigate to `library-${lib.id}` in `folders` mode at `folderPath`.
     - If unlocked:
       - Set `activeLibrary = lib`.
       - Set `currentView = library-${lib.id}`.
       - Pass `initialViewMode="folders"` and `initialFolder={folderPath}` to `BrowseScreen`.

---

## 3. Client Developer Guide (`docs/CLIENT_GUIDE.md`)

The Client Developer Guide will provide everything required for third-party client implementations:

### Key Sections:
1. **Introduction & Protocol Conventions**:
   - HTTP REST API (`/api/v1`), UTF-8 JSON payloads, standard HTTP error responses.
   - Authentication headers: `Authorization: Bearer <token>`, `X-Library-Unlock-Token: <token>`.
2. **Network Autodiscovery (`GET /api/v1/discovery`)**:
   - Server identification, friendly name, version string, setup status, protocol version.
   - UDP / mDNS guidelines and broadcast scanning.
3. **User Profiles & Authentication**:
   - `GET /api/v1/auth/profiles`: List available profiles.
   - `POST /api/v1/auth/login`: PIN verification, returns JWT token.
   - `POST /api/v1/libraries/:id/unlock`: Private library unlock with PIN, returns HMAC unlock token.
4. **Declarative Screen Layout Engine (AST)**:
   - Fetching layouts: `GET /api/v1/screens` and `GET /api/v1/screens/:id?hydrated=true`.
   - Widget Nodes: `HeroSpotlight`, `HorizontalCarousel`, `CatalogGrid`, `ContinueWatching`.
   - Pagination: `GET /api/v1/screens/:id/widgets/:widget_id/items?cursor=...&limit=...`.
   - Badges: `4K`, `NEW`, `RESUME` progress bar rendering.
5. **Item Details & Navigation**:
   - `GET /api/v1/screens/items/:id`: Movies and Shows.
   - Series & Seasons hierarchy: season tabs, episode lists, thumbnail cards.
   - Directory navigation: `library_id` and `folder_path` attributes for direct file jumping.
6. **Zero-Transcode Media Streaming & Playback Scrobbling**:
   - Streaming: `GET /api/v1/streaming/:id/stream` (standard HTTP 206 Byte-Range streaming).
   - Session lifecycle:
     - `POST /api/v1/playback/sessions`: Register playback session.
     - `POST /api/v1/playback/sessions/heartbeat`: 10-second periodic scrobble (`position_seconds`, `duration_seconds`).
     - `DELETE /api/v1/playback/sessions/:id`: Terminate session.
7. **Subtitles & Media Assets**:
   - Discovery: `GET /api/v1/subtitles/items/:id`.
   - WebVTT delivery: `GET /api/v1/subtitles/stream/:id` (on-the-fly SRT $\to$ WebVTT conversion with caching).
   - In-player live subtitle search & download (`/subtitles/search`, `/subtitles/download`).
   - Video thumbnails (`/api/v1/thumbnails/videos/:id`) and high-res images (`/api/v1/libraries/:id/image`).
8. **Hierarchical Folder Browsing & Real-Time SSE**:
   - Folder browsing: `GET /api/v1/libraries/:id/folders?path=...`.
   - Event stream: `GET /api/v1/events` (`library:updated`, `session:synced`, `subtitle:downloaded`, `system:telemetry`).

---

## 4. CHANGELOG & README Updates

### 4.1 CHANGELOG (`CHANGELOG.md`)
Create a structured changelog starting with `[v0.1.1-alpha] - 2026-10-07`, grouping changes into:
- **Added**: Direct folder navigation, Image viewer & slideshow, Series & seasons grouping, Video thumbnail extractor, Network autodiscovery, Hierarchical folder browser, Live in-player subtitle search & download, Interactive Layout Studio, Multi-path library support, Private library PIN protection & unlock tokens, Inline library renaming, Client Developer Guide.
- **Changed & Improved**: Theme design tokens overhaul, 10-foot TV UI focus states, SQLite WAL optimizations, Symlink traversal deduplication, Unified Admin Dashboard.
- **Fixed**: Directory traversal and format validation on folder and image routes, Fullscreen DOM synchronization, HUD auto-hide resets during keyboard navigation.

### 4.2 README (`README.md`)
- Update version references to `v0.1.1-alpha`.
- Add feature entries for new subsystems.
- Update architecture diagram.
- Add links to `CHANGELOG.md` and `docs/CLIENT_GUIDE.md`.

---

## 5. Verification & Test Plan

1. **Backend Unit & Integration Tests**:
   - Test `resolve_item_details_with_unlocked` verifies `library_id` and `folder_path` are correctly populated for nested files, root files, and multi-root libraries.
   - Run `cargo test --workspace` to ensure all existing tests pass with the new optional fields.
2. **Frontend Component Tests**:
   - `ItemDetailsModal.test.tsx`: Verify the "Browse Folder" button renders when `library_id` is present and invokes `onNavigateToFolder` with correct arguments on click.
   - `FolderBrowser.test.tsx`: Verify `FolderBrowser` honors `initialPath` and navigates on mount.
   - `BrowseScreen.test.tsx`: Verify `BrowseScreen` transitions to folders view when `onNavigateToFolder` is triggered for current library.
   - Run `cd web && npm test -- --run` and `npm run build`.
3. **Linter & Cleanliness**:
   - `cargo clippy --workspace --all-targets -- -D warnings` must pass with 0 warnings.
