# Video Thumbnail Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement automated video thumbnail extraction using `ffmpeg` when no folder artwork exists, storing cached thumbnails in `<data_dir>/thumbnails/<hash>.jpg`, and supporting on-demand extraction for existing media and folder browser files.

**Architecture:** Pure-Rust process wrapper around `ffmpeg` (`ThumbnailExtractor`) in `kadr-ingest` calculating dynamic seek offsets (10% or 15s) and writing deterministic sha256-hashed JPEGs to the cache directory. Integrated into `IngestPipeline` during scanning, with on-demand fallback in `stream_artwork` and `GET /api/v1/libraries/{id}/thumbnail?path=...` for unindexed files in `FolderBrowser`.

**Tech Stack:** Rust (tokio, process::Command, sha2, axum), ffmpeg, React 19, TypeScript, Vitest.

## Global Constraints
- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible; ffmpeg called via CLI process).
- Library media folders remain completely untouched; all generated thumbnails write to `<data_dir>/thumbnails/`.
- Dynamic offset: 10% into video duration (`(duration * 0.1).clamp(5.0, 30.0)`), fallback to 15.0s or 0.0s.
- Graceful degradation: if `ffmpeg` is missing or fails, return `Ok(None)` and fall back to existing placeholder icon.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.

---

### Task 1: ThumbnailExtractor Engine & Ingest Pipeline Integration (`crates/kadr-ingest`)

**Files:**
* Create: `crates/kadr-ingest/src/thumbnail/mod.rs`
* Modify: `crates/kadr-ingest/src/lib.rs`
* Modify: `crates/kadr-ingest/src/watcher/pipeline.rs:10-125`
* Test: `crates/kadr-ingest/tests/thumbnail_test.rs`

**Interfaces:**
* Produces:
  ```rust
  pub struct ThumbnailExtractor {
      output_dir: PathBuf,
  }

  impl ThumbnailExtractor {
      pub fn new(output_dir: PathBuf) -> Self;
      pub fn calculate_seek_seconds(duration_seconds: i64) -> f64;
      pub fn cache_path(&self, media_path: &Path) -> PathBuf;
      pub async fn extract_thumbnail(&self, media_path: &Path, duration_seconds: i64) -> Result<Option<PathBuf>>;
  }

  impl IngestPipeline {
      pub fn new(enable_ffprobe: bool, thumbnails_dir: Option<PathBuf>) -> Self;
  }
  ```

- [ ] **Step 1: Write failing unit tests in `crates/kadr-ingest/tests/thumbnail_test.rs`**

Test scenarios:
1. `calculate_seek_seconds`:
   - Duration 0 or negative -> 15.0s
   - Duration 30s -> 5.0s (min clamp)
   - Duration 200s -> 20.0s (10%)
   - Duration 600s -> 30.0s (max clamp)
2. `cache_path`: produces deterministic path inside `output_dir` with `.jpg` extension.
3. `extract_thumbnail`:
   - Extracts frame to cache file using ffmpeg when available.
   - Re-extracting returns existing file immediately without running ffmpeg.
   - Non-existent video file returns `Ok(None)`.
4. `IngestPipeline` integration:
   - When folder artwork is present (`poster.jpg`), uses folder artwork.
   - When folder artwork is absent and `thumbnails_dir` is configured, sets `meta.poster_path` to extracted thumbnail path.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_ingest --test thumbnail_test`
Expected: FAIL.

- [ ] **Step 3: Implement `ThumbnailExtractor` and update `IngestPipeline`**

1. Create `crates/kadr-ingest/src/thumbnail/mod.rs` implementing `ThumbnailExtractor`.
2. Add `sha2` crate if needed or compute deterministic hex hash from path string.
3. Export `thumbnail` module in `crates/kadr-ingest/src/lib.rs`.
4. Update `IngestPipeline`:
   - Add `thumbnail_extractor: Option<ThumbnailExtractor>` field.
   - Update `new(enable_ffprobe: bool, thumbnails_dir: Option<PathBuf>)`.
   - Update `Default::default()` to pass `None`.
   - In `process_file`: if `artwork.poster.is_none()`, extract thumbnail and assign to `meta.poster_path`.
5. Update server and test call sites to pass `thumbnails_dir` or `None`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test -p kadr_ingest --test thumbnail_test`
Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest/src/thumbnail/mod.rs crates/kadr-ingest/src/lib.rs crates/kadr-ingest/src/watcher/pipeline.rs crates/kadr-ingest/tests/thumbnail_test.rs
git commit -m "feat(ingest): add ThumbnailExtractor and integrate into IngestPipeline"
```

---

### Task 2: Server Routes & On-Demand Fallback (`crates/kadr-server`)

**Files:**
* Modify: `crates/kadr-server/src/api/artwork_routes.rs:38-120`
* Modify: `crates/kadr-server/src/api/library_routes.rs:1-120, 850-950`
* Modify: `crates/kadr-server/src/api/mod.rs`
* Modify: `crates/kadr-server/src/main.rs:165-175`
* Test: `crates/kadr-server/tests/artwork_routes_test.rs`
* Test: `crates/kadr-server/tests/library_folder_routes_test.rs`

**Interfaces:**
* Produces:
  - `GET /api/v1/artwork/{id}/poster` generates and streams thumbnail if `metadata.poster_path` is missing.
  - `GET /api/v1/libraries/{id}/thumbnail?path=...` sandboxed on-demand thumbnail route for unindexed video files in `FolderBrowser`.
  - In `browse_library_folders`: sets `poster_url` for synthetic cards pointing to `/api/v1/libraries/{id}/thumbnail?path={encoded}`.

- [ ] **Step 1: Write failing tests in `artwork_routes_test.rs` and `library_folder_routes_test.rs`**

Test scenarios:
1. `GET /api/v1/artwork/{id}/poster` when `poster_path` is None:
   - If video file exists on disk, extracts thumbnail on the fly, saves to cache, updates item in DB, and returns 200 with `image/jpeg`.
2. `GET /api/v1/libraries/{id}/thumbnail?path=...`:
   - Valid video path extracts and returns 200 `image/jpeg`.
   - Traversal attempt (`?path=../secret.mp4`) returns 400 Bad Request.
   - Non-existent path returns 404 Not Found.
   - Locked private library without token returns 403 Forbidden.
3. In `browse_library_folders`: synthetic video files return `poster_url` pointing to the library thumbnail endpoint.

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p kadr_server --test artwork_routes_test --test library_folder_routes_test`
Expected: FAIL.

- [ ] **Step 3: Implement route updates and on-demand fallback**

1. In `crates/kadr-server/src/api/artwork_routes.rs`:
   - Inject `thumbnails_dir: PathBuf` or `ThumbnailExtractor` into router state / extensions.
   - In `stream_artwork`: if `poster_path` is missing or missing on disk and item video file exists, call `extractor.extract_thumbnail()`, update `item.metadata.poster_path` in `media_repo`, and stream the generated thumbnail.
2. In `crates/kadr-server/src/api/library_routes.rs`:
   - Implement `get_library_thumbnail` handler with directory sandboxing and `ThumbnailExtractor`.
   - Register route in `crates/kadr-server/src/api/mod.rs`: `GET /api/v1/libraries/{id}/thumbnail`.
   - In `browse_library_folders`: populate `poster_url` for synthetic cards.
3. In `crates/kadr-server/src/main.rs`: pass `<data_dir>/thumbnails` to `IngestPipeline::new`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cargo test -p kadr_server --test artwork_routes_test --test library_folder_routes_test`
Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/api/artwork_routes.rs crates/kadr-server/src/api/library_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/src/main.rs crates/kadr-server/tests/artwork_routes_test.rs crates/kadr-server/tests/library_folder_routes_test.rs
git commit -m "feat(server): add on-demand video thumbnail fallback and folder browser thumbnail route"
```

---

### Task 3: Web Client Verification & Full Workspace Quality Gate (`web/`)

**Files:**
* Modify: `web/src/components/browse/FolderBrowser.test.tsx`
* Test: `web/src/components/browse/FolderBrowser.test.tsx`
* Test: `web/src/components/browse/GridWidget.test.tsx`

**Interfaces:**
* Verifies frontend cards (`FolderBrowser`, `GridWidget`, `CarouselWidget`) load extracted thumbnails cleanly and fallback to placeholder icon if loading fails.

- [ ] **Step 1: Write/update tests in `web/src/components/browse/FolderBrowser.test.tsx`**

Test scenarios:
1. Verify synthetic video card renders `poster_url` when returned from API.
2. Verify image loading error triggers fallback `Film` icon without breaking card interactions or 1-click play.

- [ ] **Step 2: Run web tests and production build**

Run: `cd web && npm test -- --run`
Run: `npm run build`
Expected: 100% PASS with 0 build errors.

- [ ] **Step 3: Run full Rust workspace verification**

Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 100% PASS with 0 warnings.

- [ ] **Step 4: Commit**

```bash
git add web/src/components/browse/FolderBrowser.test.tsx
git commit -m "test(web): verify video thumbnail rendering and fallback in FolderBrowser"
```
