# Design Specification: Image Viewing & Slideshow in Folder Browser

**Author:** Antigravity  
**Date:** 2026-10-07  
**Status:** Approved  
**Topic:** Allow users to view individual images and start auto-advancing slideshows for images within library directories

---

## 1. Overview & Goals

Kadr currently supports directory browsing within media libraries via `FolderBrowser`, but `browse_library_folders` filters exclusively for video files (`mkv`, `mp4`, `webm`, etc.). Directories containing photos, artwork collections, home photography, or mixed media show either an empty state or only video clips.

This specification introduces:
1. Image file discovery in `browse_library_folders` across all standard formats (`.jpg`, `.jpeg`, `.png`, `.webp`, `.gif`, `.avif`, `.bmp`).
2. An authenticated, sandboxed image serving endpoint: `GET /api/v1/libraries/:id/image?path=...`.
3. Extended `LibraryFolderResponse` containing `images: Vec<FolderImageEntry>` (with `#[serde(default)]` for 100% backward compatibility).
4. An **Images** gallery grid in `FolderBrowser.tsx` with a prominent **"Start Slideshow"** action.
5. A dedicated, TV-accessible fullscreen `ImageViewerModal.tsx` component with:
   - High-resolution centered image viewing.
   - Next / Previous navigation via buttons and <kbd>Left</kbd>/<kbd>Right</kbd> arrow keys.
   - 4-second auto-advancing slideshow with Play/Pause controls (<kbd>Space</kbd> toggle) and a visual progress bar.
   - Auto-hiding HUD controls (3-second inactivity timer matching `CinemaPlayer`).
   - 10-foot TV UI focus rings and remote navigation.

---

## 2. Architecture & Backend Design

### 2.1 Image File Detection & Response Model (`crates/kadr-server`)

- **File**: `crates/kadr-server/src/api/library_routes.rs`
- **Helper Function**:
  ```rust
  fn is_image_file(path: &std::path::Path) -> bool {
      let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
          return false;
      };
      matches!(
          ext.to_ascii_lowercase().as_str(),
          "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "bmp"
      )
  }
  ```
- **Image Entry Model**:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
  pub struct FolderImageEntry {
      pub name: String,
      pub path: String,       // relative path within library, e.g. "photos/2026/beach.jpg"
      pub url: String,        // e.g. "/api/v1/libraries/{id}/image?path=photos%2F2026%2Fbeach.jpg"
      pub size_bytes: u64,
  }
  ```
- **Extended Folder Response**:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct LibraryFolderResponse {
      pub library_id: String,
      pub library_name: String,
      pub current_path: String,
      pub parent_path: Option<String>,
      pub breadcrumbs: Vec<BreadcrumbItem>,
      pub directories: Vec<FolderEntry>,
      pub items: Vec<CardViewModel>,
      #[serde(default)]
      pub images: Vec<FolderImageEntry>,
  }
  ```
  `#[serde(default)]` guarantees zero regression on existing AST models and serde tests.

### 2.2 Image Streaming Endpoint

- **File**: `crates/kadr-server/src/api/library_routes.rs` & `crates/kadr-server/src/api/mod.rs`
- **Route**: `GET /api/v1/libraries/:id/image`
- **Query**: `?path=<relative_path>`
- **Handler**: `pub async fn get_library_image(...) -> Response`
- **Security & Validation Rules**:
  1. Resolves `Library` via `lib_repo.get_by_id(&id)`. Returns `404 Not Found` if missing.
  2. If `library.is_private`, verifies `unlocked.is_unlocked(&library.id)`. Returns `403 Forbidden` (`{"error": "LIBRARY_LOCKED"}`) if locked.
  3. Validates path: rejects empty strings, null bytes (`\0`), traversal sequences (`..`), leading slashes, and drive prefixes with `400 Bad Request`.
  4. Resolves candidate path against library root paths. Validates canonical path begins with root (`canon.starts_with(root)`). Returns `400 Bad Request` if path escapes sandbox.
  5. Validates candidate exists on disk and is a regular file. Returns `404 Not Found` if file does not exist.
  6. MIME mapping:
     - `.jpg`, `.jpeg` -> `image/jpeg`
     - `.png` -> `image/png`
     - `.webp` -> `image/webp`
     - `.gif` -> `image/gif`
     - `.avif` -> `image/avif`
     - `.bmp` -> `image/bmp`
     - other -> `application/octet-stream`
  7. Streams file asynchronously with `ReaderStream` and headers:
     - `Content-Type: <mime>`
     - `Content-Length: <file_size>`
     - `Cache-Control: public, max-age=86400`

---

## 3. Frontend Client & UI Design

### 3.1 Types (`web/src/types/index.ts`)

```typescript
export interface FolderImageEntry {
  name: string;
  path: string;
  url: string;
  size_bytes: number;
}

export interface LibraryFolderResponse {
  library_id: string;
  library_name: string;
  current_path: string;
  parent_path: string | null;
  breadcrumbs: BreadcrumbItem[];
  directories: FolderEntry[];
  items: CardViewModel[];
  images?: FolderImageEntry[];
}
```

### 3.2 FolderBrowser Enhancement (`web/src/components/browse/FolderBrowser.tsx`)

- Displays an **Images ({data.images.length})** section when `data.images && data.images.length > 0`.
- **Header**:
  - Title: `Images ({data.images.length})` with `ImageIcon` from `lucide-react`.
  - **"Start Slideshow"** CTA button:
    ```tsx
    <button
      type="button"
      onClick={() => handleOpenSlideshow(0, true)}
      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-cta hover:bg-cta-hover text-white shadow-sm transition-all focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none cursor-pointer"
    >
      <Play className="w-3.5 h-3.5 fill-white text-white" />
      <span>Start Slideshow</span>
    </button>
    ```
- **Grid Layout**:
  - Responsive: `grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4`.
  - Image card item:
    - Thumbnail container with `aspect-square`, rounded corners, and subtle zoom effect on hover/focus.
    - Lazy-loaded `<img>` referencing `image.url`.
    - Truncated filename at the bottom with image size indicator.
    - Keyboard accessible (<kbd>Enter</kbd> or <kbd>Space</kbd> to open at index).

### 3.3 Fullscreen Lightbox & Slideshow Modal (`web/src/components/player/ImageViewerModal.tsx`)

- **State Management**:
  - `currentIndex: number` (active image index)
  - `isPlaying: boolean` (slideshow running state)
  - `progress: number` (0 to 100 for auto-advance visual timer)
  - `isControlsVisible: boolean` (HUD auto-hide after 3s inactivity)
- **Keyboard Shortcuts**:
  - <kbd>ArrowLeft</kbd>: Previous image (loops to end if at beginning)
  - <kbd>ArrowRight</kbd>: Next image (loops to start if at end)
  - <kbd>Space</kbd>: Toggle Play / Pause slideshow
  - <kbd>Escape</kbd>: Close viewer
- **Slideshow Timer**:
  - 4-second auto-advance timer.
  - Smooth bottom progress bar indicating time until next slide.
  - Pauses automatically when user manually changes slide or presses Space.
- **Visual Design**:
  - Dark transparent overlay: `fixed inset-0 z-50 bg-canvas/98 flex flex-col justify-between`.
  - Top Bar:
    - Back / Close button (<kbd>Esc</kbd>).
    - Image title and resolution / index indicator (`3 / 24`).
    - Slideshow badge (`Auto-Play: 4s` when active).
  - Center View:
    - `<img>` with `max-h-[85vh] max-w-[90vw] object-contain select-none shadow-2xl rounded-lg transition-opacity duration-300`.
    - Floating Previous (<kbd>ArrowLeft</kbd>) and Next (<kbd>ArrowRight</kbd>) navigation arrows.
  - Bottom Bar:
    - Slideshow Play / Pause button.
    - Previous / Next buttons.
    - Timer progress bar.
  - 10-foot TV UI focus rings: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.

---

## 4. Verification & Testing Plan

1. **Backend Integration Tests (`crates/kadr-server/tests/library_folder_routes_test.rs`)**:
   - `test_browse_library_folders_discovers_images`: Creates dummy folder with `.jpg`, `.png`, and `.mp4`; asserts `res.images` contains images with correct URLs, sizes, and names.
   - `test_get_library_image_streaming`: Verifies `GET /api/v1/libraries/:id/image?path=...` returns image content bytes and correct `Content-Type: image/png` or `image/jpeg`.
   - `test_get_library_image_security`: Verifies rejection of path traversal (`..`), non-existent files (`404`), and private locked library access (`403`).
2. **Frontend Unit Tests (`web/src/components/browse/FolderBrowser.test.tsx` & `ImageViewerModal.test.tsx`)**:
   - `FolderBrowser.test.tsx`:
     - Renders Images section and thumbnail cards when images are present in `LibraryFolderResponse`.
     - Clicking a thumbnail opens `ImageViewerModal` with `initialIndex`.
     - Clicking "Start Slideshow" opens `ImageViewerModal` with `autoPlay: true`.
   - `ImageViewerModal.test.tsx`:
     - Renders active image name and counter.
     - Navigates Next and Previous via click and Arrow keys.
     - Advances every 4 seconds in slideshow mode using Vitest fake timers.
     - Toggles play/pause with Space key.
     - Closes modal on Escape key.
3. **Workspace Quality Gate**:
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `npm test -- --run`
   - `npm run build`
