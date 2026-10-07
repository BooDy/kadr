# Image Viewing & Slideshow in Folder Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable users to view individual images and run auto-advancing slideshows for images discovered in media library directories.

**Architecture:** Extend backend directory scanning in `browse_library_folders` to discover image files, serve images via `GET /api/v1/libraries/:id/image?path=...`, and provide an Images gallery grid in `FolderBrowser.tsx` alongside an accessible, auto-hiding fullscreen `ImageViewerModal.tsx` component with 10-foot TV UI focus rings and a 4-second auto-advance timer.

**Tech Stack:** Rust (axum, tokio, mime_guess), TypeScript, React 19, Tailwind CSS, Lucide icons, Vitest, Testing Library.

## Global Constraints

- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- Image routes must validate sandboxing: reject path traversal (`..`), null bytes, and paths outside library roots with 400 Bad Request.
- Private library permissions must be enforced (`403 Forbidden` if locked).
- `LibraryFolderResponse` must use `#[serde(default)]` for `images` to preserve 100% backward compatibility for all existing callers and tests.
- Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-cta`, `ring-highlight`, `border-border-subtle`).
- 10-foot TV UI focus rings: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all interactive buttons, thumbnails, and controls.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.
- Clippy (`cargo clippy --workspace --all-targets -- -D warnings`) must remain clean with 0 warnings.

---

### Task 1: Backend Image Discovery & Streaming API (`crates/kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/api/library_routes.rs:40-65, 620-635, 870-900, 1060-1120`
- Modify: `crates/kadr-server/src/api/mod.rs:168-175`
- Test: `crates/kadr-server/tests/library_folder_routes_test.rs:780-801`

**Interfaces:**
- Consumes: `LibraryRepository`, `UnlockedLibraries`, `tokio::fs::File`, `tokio_util::io::ReaderStream`
- Produces: `FolderImageEntry`, `GET /api/v1/libraries/:id/image?path=...`, `images` in `LibraryFolderResponse`

- [ ] **Step 1: Write failing integration tests for image discovery and image streaming**

In `crates/kadr-server/tests/library_folder_routes_test.rs`, add `test_browse_and_stream_library_images`:

```rust
#[tokio::test]
async fn test_browse_and_stream_library_images() {
    let ctx = setup_test_context().await;

    // 1. Create a subfolder with images and a movie
    let gallery_dir = ctx.public_lib_root.join("vacation_photos");
    std::fs::create_dir_all(&gallery_dir).unwrap();
    let img1_path = gallery_dir.join("beach.jpg");
    std::fs::write(&img1_path, b"fake jpeg image data").unwrap();
    let img2_path = gallery_dir.join("sunset.png");
    std::fs::write(&img2_path, b"fake png image data").unwrap();
    let vid_path = gallery_dir.join("clip.mp4");
    std::fs::write(&vid_path, b"fake video content").unwrap();

    // 2. Request folder contents via GET /api/v1/libraries/:id/folders?path=vacation_photos
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/libraries/{}/folders?path=vacation_photos",
            ctx.public_lib_id
        ))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.standard_token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let folder_res: LibraryFolderResponse = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(folder_res.images.len(), 2);
    assert_eq!(folder_res.images[0].name, "beach.jpg");
    assert_eq!(folder_res.images[0].path, "vacation_photos/beach.jpg");
    assert!(folder_res.images[0].url.contains("/image?path="));
    assert_eq!(folder_res.images[0].size_bytes, b"fake jpeg image data".len() as u64);

    assert_eq!(folder_res.images[1].name, "sunset.png");
    assert_eq!(folder_res.images[1].path, "vacation_photos/sunset.png");

    // 3. Stream image via GET /api/v1/libraries/:id/image?path=vacation_photos/beach.jpg
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/libraries/{}/image?path=vacation_photos/beach.jpg",
            ctx.public_lib_id
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg"
    );
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=86400"
    );
    let img_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&img_bytes[..], b"fake jpeg image data");

    // 4. Stream png image
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/libraries/{}/image?path=vacation_photos/sunset.png",
            ctx.public_lib_id
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/png"
    );

    // 5. Path traversal rejection (400 Bad Request)
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/libraries/{}/image?path=../secret.jpg",
            ctx.public_lib_id
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 6. Missing image file (404 Not Found)
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/libraries/{}/image?path=vacation_photos/notfound.jpg",
            ctx.public_lib_id
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test library_folder_routes_test test_browse_and_stream_library_images`
Expected: FAIL with "no field `images` on type `LibraryFolderResponse`" or 404 for `/image` route

- [ ] **Step 3: Implement `FolderImageEntry`, `is_image_file`, and update `browse_library_folders`**

In `crates/kadr-server/src/api/library_routes.rs`:
1. Add `FolderImageEntry`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderImageEntry {
    pub name: String,
    pub path: String,
    pub url: String,
    pub size_bytes: u64,
}
```
2. Update `LibraryFolderResponse`:
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
3. Add `is_image_file`:
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
4. In `browse_library_folders`:
Add `let mut image_entries = Vec::new();`
Inside `while let Ok(Some(entry)) = reader.next_entry().await`:
If `is_dir`: count image files towards `sub_count` as well.
`else if is_video_file(&entry_path) { video_paths.push(entry_path); }`
`else if is_image_file(&entry_path) {`
  collect `(file_name, entry_path)`
`}`
After processing, resolve image size and relative paths:
```rust
    for (img_name, img_path) in discovered_images {
        let rel_path = if current_path.is_empty() {
            img_name.clone()
        } else {
            format!("{}/{}", current_path, img_name)
        };
        let size_bytes = tokio::fs::metadata(&img_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let url = format!(
            "/api/v1/libraries/{}/image?path={}",
            library.id,
            urlencoding::encode(&rel_path)
        );
        image_entries.push(FolderImageEntry {
            name: img_name,
            path: rel_path,
            url,
            size_bytes,
        });
    }
    image_entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
```
Pass `images: image_entries` in `LibraryFolderResponse`.

- [ ] **Step 4: Implement `get_library_image` handler and MIME resolution**

In `crates/kadr-server/src/api/library_routes.rs`:
```rust
fn resolve_image_mime(path: &std::path::Path) -> &'static str {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return "application/octet-stream";
    };
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageQuery {
    pub path: String,
}

/// Handler for `GET /api/v1/libraries/{id}/image?path=...`.
///
/// Streams raw image files from within library folders with security sandboxing and caching.
pub async fn get_library_image(
    unlocked: UnlockedLibraries,
    Path(id): Path<String>,
    Query(query): Query<ImageQuery>,
    Extension(lib_repo): Extension<LibraryRepository>,
) -> Response {
    let library = match lib_repo.get_by_id(&id).await {
        Ok(Some(lib)) => lib,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Library not found" })),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, library_id = %id, "Failed to load library");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response();
        }
    };

    if library.is_private && !unlocked.is_unlocked(&library.id) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
        )
            .into_response();
    }

    let trimmed = query.path.trim();
    if trimmed.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path cannot be empty" })),
        )
            .into_response();
    }

    if trimmed.contains('\0') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid path containing null bytes" })),
        )
            .into_response();
    }

    if trimmed.contains("..") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Path traversal not allowed" })),
        )
            .into_response();
    }

    let rel_check = std::path::Path::new(trimmed);
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || rel_check.is_absolute()
        || rel_check.has_root()
        || trimmed.contains(':')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Absolute path not allowed" })),
        )
            .into_response();
    }

    let clean_path = trimmed.trim_matches(|c| c == '/' || c == '\\');
    let raw_roots = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    let mut found_path: Option<PathBuf> = None;
    for root in &raw_roots {
        if let Ok(canon_root) = std::fs::canonicalize(root) {
            let candidate = canon_root.join(clean_path);
            if let Ok(canon_file) = std::fs::canonicalize(&candidate) {
                if canon_file.starts_with(&canon_root) && canon_file.is_file() {
                    found_path = Some(canon_file);
                    break;
                }
            }
        }
    }

    let Some(target_file) = found_path else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Image file not found" })),
        )
            .into_response();
    };

    let meta = match tokio::fs::metadata(&target_file).await {
        Ok(m) => m,
        Err(e) => {
            error!(error = %e, path = ?target_file, "Failed to read image metadata");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read image file" })),
            )
                .into_response();
        }
    };

    let file = match tokio::fs::File::open(&target_file).await {
        Ok(f) => f,
        Err(e) => {
            error!(error = %e, path = ?target_file, "Failed to open image file");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to open image file" })),
            )
                .into_response();
        }
    };

    let mime = resolve_image_mime(&target_file);
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, meta.len().to_string())
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(body)
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR).into_response())
}
```

- [ ] **Step 5: Register route in `crates/kadr-server/src/api/mod.rs`**

In `crates/kadr-server/src/api/mod.rs`:
```rust
        .route(
            "/api/v1/libraries/:id/image",
            get(library_routes::get_library_image),
        )
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p kadr-server --test library_folder_routes_test test_browse_and_stream_library_images`
Expected: PASS

- [ ] **Step 7: Run full workspace test suite and clippy**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS with 0 warnings

- [ ] **Step 8: Commit changes**

```bash
git add crates/kadr-server/src/api/library_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/tests/library_folder_routes_test.rs
git commit -m "feat(server): add image discovery in library folders and GET /api/v1/libraries/:id/image route"
```

---

### Task 2: Web Fullscreen ImageViewerModal Component (`web/`)

**Files:**
- Modify: `web/src/types/index.ts:285-305`
- Create: `web/src/components/player/ImageViewerModal.tsx`
- Test: `web/src/components/player/ImageViewerModal.test.tsx`

**Interfaces:**
- Consumes: `FolderImageEntry`
- Produces: `<ImageViewerModal images={images} initialIndex={index} isOpen={isOpen} autoPlay={bool} onClose={fn} />`

- [ ] **Step 1: Update TypeScript types in `web/src/types/index.ts`**

In `web/src/types/index.ts`:
```typescript
export interface FolderImageEntry {
  name: string;
  path: string;
  url: string;
  size_bytes: number;
}
```
In `LibraryFolderResponse`:
```typescript
  images?: FolderImageEntry[];
```

- [ ] **Step 2: Write failing component tests for `ImageViewerModal`**

Create `web/src/components/player/ImageViewerModal.test.tsx`:
```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ImageViewerModal } from './ImageViewerModal';
import type { FolderImageEntry } from '../../types';

describe('ImageViewerModal Component', () => {
  const mockImages: FolderImageEntry[] = [
    { name: 'photo1.jpg', path: 'photos/photo1.jpg', url: '/api/v1/libraries/lib-1/image?path=photo1.jpg', size_bytes: 1024 },
    { name: 'photo2.png', path: 'photos/photo2.png', url: '/api/v1/libraries/lib-1/image?path=photo2.png', size_bytes: 2048 },
    { name: 'photo3.webp', path: 'photos/photo3.webp', url: '/api/v1/libraries/lib-1/image?path=photo3.webp', size_bytes: 4096 },
  ];

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders current image, filename, and index counter', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();
    expect(screen.getByText('1 / 3')).toBeInTheDocument();
    const img = screen.getByRole('img');
    expect(img).toHaveAttribute('src', mockImages[0].url);
  });

  it('navigates next and previous on button click and wraps around', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    const nextBtn = screen.getByRole('button', { name: /next image/i });
    fireEvent.click(nextBtn);
    expect(screen.getByText('photo2.png')).toBeInTheDocument();
    expect(screen.getByText('2 / 3')).toBeInTheDocument();

    const prevBtn = screen.getByRole('button', { name: /previous image/i });
    fireEvent.click(prevBtn);
    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();

    // Wrap around to last
    fireEvent.click(prevBtn);
    expect(screen.getByText('photo3.webp')).toBeInTheDocument();
  });

  it('handles keyboard navigation: ArrowRight, ArrowLeft, Space to pause/play, Escape to close', () => {
    const onClose = vi.fn();
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={onClose}
      />
    );

    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(screen.getByText('photo2.png')).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'ArrowLeft' });
    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();

    // Escape closes
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('auto-advances every 4 seconds when in slideshow mode and pauses with space', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        autoPlay={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();

    // Advance 4 seconds
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(screen.getByText('photo2.png')).toBeInTheDocument();

    // Toggle pause with Space
    fireEvent.keyDown(window, { key: ' ' });

    // Advance 4 seconds: should NOT advance
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(screen.getByText('photo2.png')).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd web && npm test -- --run ImageViewerModal.test.tsx`
Expected: FAIL with "Cannot find module './ImageViewerModal'"

- [ ] **Step 4: Implement `ImageViewerModal.tsx`**

Create `web/src/components/player/ImageViewerModal.tsx`:
```tsx
import { useState, useEffect, useCallback, type FC } from 'react';
import {
  X,
  ChevronLeft,
  ChevronRight,
  Play,
  Pause,
  Maximize,
  Minimize,
} from 'lucide-react';
import type { FolderImageEntry } from '../../types';

export interface ImageViewerModalProps {
  images: FolderImageEntry[];
  initialIndex?: number;
  isOpen: boolean;
  autoPlay?: boolean;
  onClose: () => void;
}

export const ImageViewerModal: FC<ImageViewerModalProps> = ({
  images,
  initialIndex = 0,
  isOpen,
  autoPlay = false,
  onClose,
}) => {
  const [currentIndex, setCurrentIndex] = useState<number>(initialIndex);
  const [isPlaying, setIsPlaying] = useState<boolean>(autoPlay);
  const [isControlsVisible, setIsControlsVisible] = useState<boolean>(true);
  const [isFullscreen, setIsFullscreen] = useState<boolean>(false);

  useEffect(() => {
    if (isOpen) {
      setCurrentIndex(initialIndex);
      setIsPlaying(autoPlay);
      setIsControlsVisible(true);
    }
  }, [isOpen, initialIndex, autoPlay]);

  const handleNext = useCallback(() => {
    if (images.length === 0) return;
    setCurrentIndex((prev) => (prev + 1) % images.length);
  }, [images.length]);

  const handlePrev = useCallback(() => {
    if (images.length === 0) return;
    setCurrentIndex((prev) => (prev - 1 + images.length) % images.length);
  }, [images.length]);

  const togglePlay = useCallback(() => {
    setIsPlaying((prev) => !prev);
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen?.().catch(() => {});
      setIsFullscreen(true);
    } else {
      document.exitFullscreen?.().catch(() => {});
      setIsFullscreen(false);
    }
  }, []);

  // Slideshow auto-advance interval (4 seconds)
  useEffect(() => {
    if (!isOpen || !isPlaying || images.length <= 1) return;

    const timer = setInterval(() => {
      handleNext();
    }, 4000);

    return () => clearInterval(timer);
  }, [isOpen, isPlaying, images.length, handleNext]);

  // Controls auto-hide after 3 seconds of inactivity
  useEffect(() => {
    if (!isOpen) return;

    let timeoutId: NodeJS.Timeout;
    const resetTimer = () => {
      setIsControlsVisible(true);
      clearTimeout(timeoutId);
      timeoutId = setTimeout(() => {
        setIsControlsVisible(false);
      }, 3000);
    };

    resetTimer();
    window.addEventListener('mousemove', resetTimer);
    return () => {
      clearTimeout(timeoutId);
      window.removeEventListener('mousemove', resetTimer);
    };
  }, [isOpen]);

  // Keyboard navigation
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      setIsControlsVisible(true);
      switch (e.key) {
        case 'ArrowRight':
          e.preventDefault();
          handleNext();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          handlePrev();
          break;
        case ' ':
          e.preventDefault();
          togglePlay();
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
        case 'f':
        case 'F':
          e.preventDefault();
          toggleFullscreen();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, handleNext, handlePrev, togglePlay, onClose, toggleFullscreen]);

  if (!isOpen || images.length === 0) return null;

  const currentImage = images[currentIndex] || images[0];

  return (
    <div
      role="dialog"
      aria-label="Image viewer"
      className="fixed inset-0 z-50 bg-canvas/98 flex flex-col justify-between overflow-hidden select-none"
    >
      {/* Top Header Bar */}
      <div
        className={`w-full p-4 sm:p-6 bg-gradient-to-b from-canvas/90 via-canvas/40 to-transparent flex items-center justify-between gap-4 transition-opacity duration-300 z-10 ${
          isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="flex items-center gap-3 min-w-0">
          <span className="font-semibold text-base sm:text-lg text-text-main truncate">
            {currentImage.name}
          </span>
          <span className="text-xs px-2.5 py-0.5 rounded-full font-mono font-medium bg-panel border border-border-subtle text-muted shrink-0">
            {currentIndex + 1} / {images.length}
          </span>
          {isPlaying && (
            <span className="text-xs px-2.5 py-0.5 rounded-full font-semibold bg-accent/20 text-accent border border-accent/30 shrink-0">
              Slideshow (4s)
            </span>
          )}
        </div>

        <button
          type="button"
          onClick={onClose}
          aria-label="Close image viewer"
          className="p-2 rounded-xl bg-panel/80 hover:bg-panel border border-border-subtle text-muted hover:text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shrink-0"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      {/* Main Image Display Area */}
      <div className="relative flex-1 flex items-center justify-center p-4">
        {/* Previous Button Overlay */}
        <button
          type="button"
          onClick={handlePrev}
          aria-label="Previous image"
          className={`absolute left-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-panel/80 hover:bg-panel border border-border-subtle text-text-main shadow-lg transition-all duration-300 z-10 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
            isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
          }`}
        >
          <ChevronLeft className="w-6 h-6" />
        </button>

        {/* The Image */}
        <img
          key={currentImage.url}
          src={currentImage.url}
          alt={currentImage.name}
          className="max-h-[82vh] max-w-[92vw] object-contain shadow-2xl rounded-lg transition-opacity duration-200"
        />

        {/* Next Button Overlay */}
        <button
          type="button"
          onClick={handleNext}
          aria-label="Next image"
          className={`absolute right-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-panel/80 hover:bg-panel border border-border-subtle text-text-main shadow-lg transition-all duration-300 z-10 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
            isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
          }`}
        >
          <ChevronRight className="w-6 h-6" />
        </button>
      </div>

      {/* Bottom Controls Bar */}
      <div
        className={`w-full p-4 sm:p-6 bg-gradient-to-t from-canvas/95 via-canvas/50 to-transparent flex items-center justify-between gap-4 transition-opacity duration-300 z-10 ${
          isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={togglePlay}
            aria-label={isPlaying ? 'Pause slideshow' : 'Play slideshow'}
            className="flex items-center gap-2 px-4 py-2 rounded-xl bg-cta hover:bg-cta-hover text-white text-xs font-semibold shadow-md transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            {isPlaying ? (
              <>
                <Pause className="w-4 h-4 fill-white text-white" />
                <span>Pause</span>
              </>
            ) : (
              <>
                <Play className="w-4 h-4 fill-white text-white" />
                <span>Play Slideshow</span>
              </>
            )}
          </button>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={toggleFullscreen}
            aria-label={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
            className="p-2 rounded-xl bg-panel/80 hover:bg-panel border border-border-subtle text-muted hover:text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            {isFullscreen ? <Minimize className="w-4 h-4" /> : <Maximize className="w-4 h-4" />}
          </button>
        </div>
      </div>
    </div>
  );
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd web && npm test -- --run ImageViewerModal.test.tsx`
Expected: PASS

- [ ] **Step 6: Commit changes**

```bash
git add web/src/types/index.ts web/src/components/player/ImageViewerModal.tsx web/src/components/player/ImageViewerModal.test.tsx
git commit -m "feat(web): add ImageViewerModal with manual navigation and auto-advancing slideshow"
```

---

### Task 3: FolderBrowser Images Gallery & Slideshow Integration (`web/`)

**Files:**
- Modify: `web/src/components/browse/FolderBrowser.tsx:1-15, 120-140, 260-308`
- Test: `web/src/components/browse/FolderBrowser.test.tsx`

**Interfaces:**
- Consumes: `<ImageViewerModal />`, `LibraryFolderResponse.images`
- Produces: Images gallery grid and "Start Slideshow" button in `FolderBrowser`

- [ ] **Step 1: Write failing component tests in `web/src/components/browse/FolderBrowser.test.tsx`**

In `web/src/components/browse/FolderBrowser.test.tsx`, add a test verifying images display and slideshow invocation:

```typescript
  it('renders Images section and launches ImageViewerModal when clicking image or Start Slideshow', async () => {
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValue({
      library_id: 'lib-1',
      library_name: 'Mixed Library',
      current_path: 'vacation',
      parent_path: null,
      breadcrumbs: [{ name: 'vacation', path: 'vacation' }],
      directories: [],
      items: [],
      images: [
        { name: 'beach.jpg', path: 'vacation/beach.jpg', url: '/api/v1/libraries/lib-1/image?path=beach.jpg', size_bytes: 1024 },
        { name: 'sunset.png', path: 'vacation/sunset.png', url: '/api/v1/libraries/lib-1/image?path=sunset.png', size_bytes: 2048 },
      ],
    });

    render(<FolderBrowser libraryId="lib-1" onPlayItem={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText('Images (2)')).toBeInTheDocument();
      expect(screen.getByText('beach.jpg')).toBeInTheDocument();
      expect(screen.getByText('sunset.png')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /start slideshow/i })).toBeInTheDocument();
    });

    // Click "Start Slideshow" opens viewer
    fireEvent.click(screen.getByRole('button', { name: /start slideshow/i }));
    expect(screen.getByRole('dialog', { name: /image viewer/i })).toBeInTheDocument();
    expect(screen.getByText('Slideshow (4s)')).toBeInTheDocument();

    // Close viewer
    fireEvent.click(screen.getByRole('button', { name: /close image viewer/i }));
    expect(screen.queryByRole('dialog', { name: /image viewer/i })).not.toBeInTheDocument();

    // Click individual image opens viewer at that image
    fireEvent.click(screen.getByText('sunset.png'));
    expect(screen.getByRole('dialog', { name: /image viewer/i })).toBeInTheDocument();
    expect(screen.getByText('2 / 2')).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npm test -- --run FolderBrowser.test.tsx`
Expected: FAIL with "Unable to find an element with the text: Images (2)"

- [ ] **Step 3: Update `web/src/components/browse/FolderBrowser.tsx` to render Images section and ImageViewerModal**

In `web/src/components/browse/FolderBrowser.tsx`:
1. Import `Image as ImageIcon` from `lucide-react`.
2. Import `ImageViewerModal` from `../player/ImageViewerModal`.
3. Add state:
```typescript
  const [selectedImageIndex, setSelectedImageIndex] = useState<number | null>(null);
  const [isSlideshowAutoPlay, setIsSlideshowAutoPlay] = useState<boolean>(false);
```
4. Render Images section before/after Media section:
```tsx
          {/* Images Gallery Grid */}
          {data.images && data.images.length > 0 && (
            <section className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <ImageIcon className="w-4 h-4 text-accent" />
                  <h2 className="text-xs font-semibold tracking-wider uppercase text-muted">
                    Images ({data.images.length})
                  </h2>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setSelectedImageIndex(0);
                    setIsSlideshowAutoPlay(true);
                  }}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-cta hover:bg-cta-hover text-white shadow-sm transition-all focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none cursor-pointer"
                >
                  <Play className="w-3.5 h-3.5 fill-white text-white" />
                  <span>Start Slideshow</span>
                </button>
              </div>

              <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
                {data.images.map((img, idx) => (
                  <div
                    key={img.path}
                    role="button"
                    tabIndex={0}
                    onClick={() => {
                      setSelectedImageIndex(idx);
                      setIsSlideshowAutoPlay(false);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        setSelectedImageIndex(idx);
                        setIsSlideshowAutoPlay(false);
                      }
                    }}
                    className="group relative flex flex-col cursor-pointer rounded-xl overflow-hidden border border-border-subtle bg-panel hover:bg-panel-hover transition-all duration-200 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none p-2"
                  >
                    <div className="relative aspect-square w-full rounded-lg overflow-hidden bg-canvas">
                      <img
                        src={img.url}
                        alt={img.name}
                        loading="lazy"
                        className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 group-hover:scale-105 transition-all duration-200"
                      />
                    </div>
                    <div className="mt-2 px-0.5 truncate">
                      <span className="text-xs font-medium text-text-main truncate group-hover:text-white transition-colors" title={img.name}>
                        {img.name}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}
```
5. Mount `ImageViewerModal`:
```tsx
      {data?.images && data.images.length > 0 && (
        <ImageViewerModal
          images={data.images}
          initialIndex={selectedImageIndex ?? 0}
          isOpen={selectedImageIndex !== null}
          autoPlay={isSlideshowAutoPlay}
          onClose={() => setSelectedImageIndex(null)}
        />
      )}
```

- [ ] **Step 4: Run component tests to verify they pass**

Run: `cd web && npm test -- --run FolderBrowser.test.tsx`
Expected: PASS

- [ ] **Step 5: Run full frontend test suite and build**

Run: `cd web && npm test -- --run && npm run build`
Expected: PASS and clean build

- [ ] **Step 6: Commit changes**

```bash
git add web/src/components/browse/FolderBrowser.tsx web/src/components/browse/FolderBrowser.test.tsx
git commit -m "feat(web): integrate images gallery and slideshow launch into FolderBrowser"
```

---

### Task 4: Full Workspace Verification & Quality Gate

**Files:**
- N/A (Workspace verification)

**Interfaces:**
- Workspace quality gate verification

- [ ] **Step 1: Run full Rust workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Run Rust clippy across all targets**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 errors, 0 warnings

- [ ] **Step 3: Run full web test suite**

Run: `cd web && npm test -- --run`
Expected: All tests pass

- [ ] **Step 4: Run production web build**

Run: `cd web && npm run build`
Expected: Clean build into `web/dist`

- [ ] **Step 5: Rebuild and restart server binary**

Run: `cargo build` and restart daemon to serve new endpoints.
