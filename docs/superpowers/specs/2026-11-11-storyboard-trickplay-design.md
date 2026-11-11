# Seek Preview Thumbnails (Trickplay / Storyboard) Design

**Date:** 2026-11-11  
**Status:** Approved

---

## 1. Goal & Overview

Enable fast, responsive seek-preview thumbnails (trickplay) while scrubbing the progress bar on TV and web clients (~10s jumps every ~300ms). Running `ffmpeg` per scrub press is too slow on HEVC and 10-bit files; instead, Kadr generates tiled 10×10 JPEG sprite sheets once per media item alongside a JSON manifest, plus a single-frame fallback endpoint for clients that do not use sprite sheets.

---

## 2. Configuration

Add `StoryboardSettings` under `[storyboard]` in [`crates/kadr-server/src/config.rs`](file:///home/boody/work/personal/kadr/crates/kadr-server/src/config.rs) and [`kadr.toml`](file:///home/boody/work/personal/kadr/kadr.toml):

```toml
[storyboard]
enabled = true
max_concurrent = 1
interval_secs = 10
tile_width = 320
generate_on_scan = false
```

| Field | Type | Default | Constraints / Notes |
|---|---|---|---|
| `enabled` | `bool` | `true` | Master switch for `/storyboard`, `/storyboard/{index}.jpg`, and `/frame`. |
| `max_concurrent` | `usize` | `1` | Server-wide concurrency limit for background storyboard sheet jobs (`>= 1`). |
| `interval_secs` | `u64` | `10` | Base interval in seconds (`>= 1`). Items under 10 minutes use `min(interval_secs, 5)` (5s by default), widened as needed to cap total tiles at `<= 1000`. |
| `tile_width` | `u32` | `320` | Width of each tile in pixels (clamped to `160..=640`, rounded to an even integer). |
| `generate_on_scan` | `bool` | `false` | When `true`, newly ingested local video items enqueue background storyboard generation after database upsert. |

---

## 3. Interval, Grid, and Tile Math

Given a video item with `duration_seconds > 0` and configured `base_interval_secs` (clamped `>= 1`):

1. **Base interval selection:**
   - If `duration_seconds < 600` (under 10 minutes): `initial_secs = base_interval_secs.min(5).max(1)`.
   - If `duration_seconds >= 600`: `initial_secs = base_interval_secs.max(1)`.
2. **1000-tile cap:**
   - Compute minimum whole-second interval required so `ceil(duration_seconds / interval_s) <= 1000`:
     `min_secs_for_cap = (duration_seconds as u64).div_ceil(1000).max(1)`.
   - `interval_secs = initial_secs.max(min_secs_for_cap)`.
   - `interval_ms = interval_secs * 1000`.
3. **Tile and sheet counts:**
   - `columns = 10`, `rows = 10`, `tiles_per_sheet = 100`.
   - `total_tiles = ((duration_seconds as u64) * 1000).div_ceil(interval_ms).max(1) as u32`.
   - `sheet_count = total_tiles.div_ceil(100)`.
   - For each sheet `index` in `0..sheet_count`:
     - `first_tile = index * 100`
     - `tile_count = (total_tiles - first_tile).min(100)`
     - `url = format!("/api/v1/media/{item_id}/storyboard/{index}.jpg")`
   - Tile `n` (`0 <= n < total_tiles`) covers timestamp `n * interval_ms` on sheet `n / 100` at column `n % 10` and row `(n % 100) / 10`.
4. **Tile dimensions:**
   - `tile_width` is the configured width (clamped to `160..=640`, even).
   - `tile_height` follows the source video aspect ratio with an even height (`scale=<tile_width>:-2`), read directly from the generated JPEG sheet header (`sheet_height / rows`) so the manifest matches the actual sprite sheet pixels even for ultrawide (`2.39:1`, e.g. `320×134`) or vertical videos.

---

## 4. Storage, Cache Layout & Invalidation

All cached artifacts live under `<server.data_dir>/storyboards/`:

```text
<data_dir>/storyboards/
  items/
    <path_hash>/
      <source_hash>/
        manifest.json
        0.jpg
        1.jpg
        ...
  frames/
    <path_hash>/
      <frame_hash>.jpg
```

- **`path_hash`:** `sha256(canonical_file_path_bytes)` (hex). Grouping by `path_hash` allows both HTTP handlers (which know `item.file_path`) and `IngestWorker` (which receives `IngestMessage::Delete(PathBuf)`) to delete all cached storyboard sheets and single frames for a file without extra database lookups.
- **`source_hash`:** `sha256(canonical_file_path | mtime_secs | mtime_nanos | file_size | tile_width | interval_ms)` (hex).
  - When a file's `mtime` or `file_size` (or config `tile_width` / `interval_secs`) changes, `source_hash` changes automatically.
  - When a new `<source_hash>` directory is atomically promoted inside `items/<path_hash>/`, any sibling directories under `items/<path_hash>/` with old hashes (and any stale `frames/<path_hash>/` entries) are deleted.
- **`frame_hash`:** `sha256(canonical_file_path | mtime_secs | mtime_nanos | file_size | snapped_t_secs | clamped_width)` (hex).
- **Atomic writes:**
  - Storyboard generation writes to `<data_dir>/storyboards/items/<path_hash>/.tmp.<pid>.<counter>/` (`sheet_000.jpg`, `sheet_001.jpg`, …), renames sheets to `0.jpg`, `1.jpg`, …, writes `manifest.json`, and atomically renames the temporary directory to `<source_hash>/`.
  - Single-frame extraction writes to `<data_dir>/storyboards/frames/<path_hash>/.tmp.<pid>.<counter>.jpg` and atomically renames to `<frame_hash>.jpg`.

---

## 5. FFmpeg Execution & Concurrency Control

### 5.1 `StoryboardService`
Lives in `crates/kadr-server/src/storyboard/` and is shared via Axum `Extension<Arc<StoryboardService>>`:
- `storyboard_semaphore: Arc<tokio::sync::Semaphore>` initialized with `config.storyboard.max_concurrent.max(1)` permits.
- `frame_semaphore: Arc<tokio::sync::Semaphore>` initialized with `2` permits.
- `in_flight: Arc<tokio::sync::Mutex<HashSet<String>>>` tracking active/queued `source_hash` keys so concurrent polls to `/storyboard` or `/storyboard/{index}.jpg` do not enqueue duplicate jobs.
- `ffmpeg_available: bool` probed once at service initialization (or using `config.transcoding.ffmpeg_path`) so missing `ffmpeg` returns `503` immediately.

### 5.2 Storyboard FFmpeg Command
1. Acquire a permit from `storyboard_semaphore`.
2. Re-check if `<source_hash>/manifest.json` already exists on disk.
3. Run low-priority `ffmpeg` (on Unix, wrap with `nice -n 10` if `nice` is available; otherwise invoke `ffmpeg` directly):
   ```bash
   ffmpeg -hide_banner -loglevel error \
     -skip_frame nokey \
     -i <file> \
     -an -sn \
     -vf "fps=1/<interval_secs>,scale=<tile_width>:-2,tile=10x10" \
     -frames:v <sheet_count> \
     -q:v 6 \
     <tmp_dir>/sheet_%03d.jpg
   ```
4. **Short-clip / sparse-keyframe fallback:** If the `-skip_frame nokey` pass exits non-zero or produces no `sheet_000.jpg` (common on short synthetic test clips with a single keyframe), retry once without `-skip_frame nokey`.
5. **Dimension inspection & manifest persistence:**
   - Parse JPEG SOF0/SOF1/SOF2 dimensions from `sheet_000.jpg`: `tile_height = (jpeg_height / 10).max(2)`.
   - Rename `sheet_000.jpg` → `0.jpg`, `sheet_001.jpg` → `1.jpg`, etc.
   - If `ffmpeg` produced fewer sheets than the duration estimate (e.g., container duration slightly exceeded video stream timestamps), clamp `sheet_count` and `total_tiles` to match the generated sheets so every sheet URL in the manifest is guaranteed to exist on disk.
   - Write `manifest.json` and atomically rename `<tmp_dir>` → `<source_hash>`.

### 5.3 Single-Frame FFmpeg Command (`/frame`)
1. Check if `<frame_hash>.jpg` exists on disk; if so, serve immediately without acquiring a semaphore permit.
2. Acquire a permit from `frame_semaphore` (max 2 concurrent runs).
3. Re-check cache after acquiring the permit.
4. Run:
   ```bash
   ffmpeg -hide_banner -loglevel error \
     -ss <snapped_t> \
     -i <file> \
     -frames:v 1 \
     -vf "scale=<clamped_w>:-2" \
     -q:v 5 \
     -y <tmp_file>
   ```
   If seeking to `<snapped_t>` at the very end of the file yields an empty output and `snapped_t > 0`, retry at `0` seconds.
5. Atomically rename `<tmp_file>` → `<frame_hash>.jpg` and stream the response.

---

## 6. HTTP API Endpoints

### 6.1 Common Auth, Access Control, and Validation Order
All three endpoints apply checks in this exact order:
1. **Authentication (`AuthUser`):** Accepts `Authorization: Bearer <jwt>` or `?token=<jwt>`. Missing or invalid token returns `401 Unauthorized`.
2. **Item lookup:** Fetch `MediaItem` by `item_id`. If not found → `404 Not Found` `{"error": "Media item not found"}`.
3. **Private library check (`UnlockedLibraries`):** Fetch the item's `Library`. If `library.is_private && !unlocked.is_unlocked(&library.id)` (checking `X-Kadr-Unlocked`, `X-Library-Unlock-Token`, `?unlocked=`, and `?unlock_token=`) → `403 Forbidden` `{"error": "LIBRARY_LOCKED"}`.
4. **Feature & FFmpeg availability:** If `!storyboard.enabled` or `!ffmpeg_available` → `503 Service Unavailable` `{"error": "Storyboard generation unavailable"}`.
5. **Video eligibility:**
   - Reject plugin items (`item.plugin_ref().is_some()`), non-video media types (`Music`, `Audiobook`, `Show`, `Season`, `LiveChannel`), items with `duration_seconds <= 0`, or items where `video_codec` is absent and container/extension is audio-only → `404 Not Found` `{"error": "Media item has no video stream"}`.
6. **File existence & metadata:** `tokio::fs::metadata(&item.file_path)`. If missing or not a file → `404 Not Found` `{"error": "Media file missing from disk"}`.
   - Also verify via `ffprobe` (or cached negative marker) if `ffmpeg` finds no video stream during generation, recording a `.novideo` marker under `<source_hash>` so subsequent requests return `404 {"error": "Media item has no video stream"}` instead of looping on `202`.

### 6.2 `GET /api/v1/media/{item_id}/storyboard`
- **Cache hit (`<source_hash>/manifest.json` exists):**
  - Returns `200 OK` (`application/json`) with:
    ```json
    {
      "interval_ms": 10000,
      "tile_width": 320,
      "tile_height": 134,
      "columns": 10,
      "rows": 10,
      "total_tiles": 680,
      "sheets": [
        {
          "index": 0,
          "url": "/api/v1/media/42/storyboard/0.jpg",
          "first_tile": 0,
          "tile_count": 100
        }
      ]
    }
    ```
  - Sheet URLs never embed `?token=` or `?unlocked=`.
- **Cache miss:**
  - Enqueues background storyboard generation for `(item.file_path, source_hash)` if not already in flight.
  - Returns `202 Accepted` with header `Retry-After: 5` and JSON body `{"status": "generating"}`.

### 6.3 `GET /api/v1/media/{item_id}/storyboard/{sheet}`
- `{sheet}` must match `<index>.jpg` where `<index>` parses as `u32` (otherwise `404 Not Found`).
- **If `<source_hash>/manifest.json` exists:**
  - If `index >= manifest.sheets.len()` or `<index>.jpg` does not exist → `404 Not Found` `{"error": "Storyboard sheet not found"}`.
  - Otherwise computes `ETag: "\"<source_hash>-<index>\""`.
  - If request header `If-None-Match` matches the ETag → `304 Not Modified`.
  - Streams `<index>.jpg` with:
    - `Status: 200 OK`
    - `Content-Type: image/jpeg`
    - `Cache-Control: public, max-age=604800`
    - `ETag: "<source_hash>-<index>"`
    - `Content-Length: <bytes>`
- **If `<source_hash>/manifest.json` does not exist yet:**
  - Computes expected `sheet_count` from `item.technical.duration_seconds`.
  - If `index >= expected_sheet_count` → `404 Not Found` `{"error": "Storyboard sheet not found"}`.
  - Otherwise enqueues generation (if not already in flight) and returns `202 Accepted` with `Retry-After: 5` and `{"status": "generating"}`.

### 6.4 `GET /api/v1/media/{item_id}/frame?t=<seconds>&w=<width>`
- Query parameters:
  - `t`: `f64` (default `0.0`). Rejects negative or non-finite values with `400 Bad Request`. Snapped to nearest whole second `t.round() as u64`, clamped to `0..=(duration_seconds as u64)`.
  - `w`: `Option<u32>` (default `320`). Clamped to `160..=640` and rounded to an even integer (`w & !1`).
- Computes `ETag: "\"frame-<frame_hash>\""` and honors `If-None-Match` (`304 Not Modified`).
- Serves cached `<frame_hash>.jpg` or extracts on demand under the 2-permit `frame_semaphore`, returning `200 OK` with `Content-Type: image/jpeg`, `Cache-Control: public, max-age=604800`, and `ETag`.

---

## 7. Scan & Deletion Integration

- **Cache cleanup on item deletion:**
  - Add an optional cleanup callback or cache directory hook on [`IngestWorker`](file:///home/boody/work/personal/kadr/crates/kadr-ingest/src/watcher/worker.rs) so `IngestMessage::Delete(path)` removes `<data_dir>/storyboards/items/<path_hash>` and `<data_dir>/storyboards/frames/<path_hash>`.
- **Optional post-scan pre-generation (`storyboard.generate_on_scan`):**
  - When `storyboard.enabled && storyboard.generate_on_scan`, after `IngestWorker` flushes a batch of upserted items, local video items with `duration_seconds > 0` are submitted to `StoryboardService::enqueue_for_item` in the background (still respecting the single-concurrency `storyboard_semaphore` and `nice` priority).

---

## 8. Testing Strategy

1. **Unit tests (`storyboard::math` & `jpeg`):**
   - Interval and tile math across short (`< 10m` → `5s`), standard (`10m..10000s` → `10s`), and long (`> 10000s`, e.g. 4-hour movie → widened interval so `total_tiles <= 1000`) durations.
   - Sheet partitioning (`first_tile`, `tile_count`, URL formatting without query tokens).
   - JPEG SOF header dimension parser for `tile_height` calculation (including `2.39:1` → `320×134` and `16:9` → `320×180`).
2. **Integration tests (`crates/kadr-server/tests/storyboard_routes_test.rs`):**
   - `202 Accepted` (with `Retry-After: 5` and `{"status":"generating"}`) on first request, followed by `200 OK` manifest and `200 OK` sheet JPEG (`Cache-Control: public, max-age=604800`, `ETag`, `304 Not Modified`) using a synthetic `ffmpeg` fixture video.
   - Cache reuse: second request serves from disk without spawning `ffmpeg` (verified via invocation counter / mtime).
   - Cache invalidation when file size/mtime changes, and cache deletion when item is removed.
   - Auth via both `Authorization: Bearer <token>` and `?token=<token>`, plus `401` when missing/invalid.
   - Private library protection: `403 {"error":"LIBRARY_LOCKED"}` when locked, `200`/`202` when unlocked via header or `?unlocked=`.
   - Error cases: `404` missing item, `404` missing file on disk, `404` audio-only / no video stream, `404` out-of-range sheet index, `503` when `storyboard.enabled = false`.
   - Fallback `/api/v1/media/{item_id}/frame?t=<seconds>&w=<width>` extraction, width clamping, time snapping, and disk cache reuse.
