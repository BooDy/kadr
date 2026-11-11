# Seek Preview Thumbnails (Trickplay / Storyboard) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add seek preview thumbnails (storyboard sprite sheets + JSON manifest + single-frame fallback endpoint) so TV and web clients can display instant preview frames while scrubbing.

**Architecture:** `StoryboardService` in `crates/kadr-server/src/storyboard/` manages content-addressed disk caches under `<data_dir>/storyboards/`, runs low-priority background `ffmpeg` jobs (`-skip_frame nokey` with automatic fallback) bounded by a configurable `tokio::sync::Semaphore` (default 1 concurrent storyboard job, 2 concurrent single-frame extractions), and persists `manifest.json` atomically alongside `0.jpg`, `1.jpg`, etc. Axum handlers under `/api/v1/media/{item_id}/storyboard`, `/api/v1/media/{item_id}/storyboard/{sheet}`, and `/api/v1/media/{item_id}/frame` enforce JWT auth (`Bearer` or `?token=`) and private-library unlock tokens before serving or enqueuing generation.

**Tech Stack:** Rust (Edition 2021), Axum 0.8, Tokio, Serde, SHA-256 (`sha2`), `ffmpeg` CLI.

## Global Constraints

- Storyboard endpoints require `AuthUser` (`Authorization: Bearer <token>` or `?token=<token>`); missing or invalid tokens return `401 Unauthorized`.
- Private libraries that are not unlocked via `X-Kadr-Unlocked`, `X-Library-Unlock-Token`, `?unlocked=`, or `?unlock_token=` return `403 Forbidden` with `{"error": "LIBRARY_LOCKED"}`.
- Sheet URLs in the manifest (`"/api/v1/media/{item_id}/storyboard/{index}.jpg"`) MUST NOT embed `?token=` or `?unlocked=`.
- When sheets are still generating, `GET /api/v1/media/{item_id}/storyboard` and in-range `GET /api/v1/media/{item_id}/storyboard/{index}.jpg` return `202 Accepted` with header `Retry-After: 5` and body `{"status": "generating"}`.
- Completed sheet JPEGs and `/frame` JPEGs return `200 OK` with `Content-Type: image/jpeg`, `Cache-Control: public, max-age=604800`, and `ETag` (plus `304 Not Modified` when `If-None-Match` matches).
- Missing items return `404 {"error": "Media item not found"}`; missing disk files return `404 {"error": "Media file missing from disk"}`; audio-only / no-video items return `404 {"error": "Media item has no video stream"}`; disabled feature or missing `ffmpeg` returns `503 {"error": "Storyboard generation unavailable"}`.
- Zero compiler or `cargo clippy --workspace --all-targets -- -D warnings` warnings.

---

### File Map

- **Modify:** `crates/kadr-server/src/config.rs` — add `StoryboardSettings` (`enabled`, `max_concurrent`, `interval_secs`, `tile_width`, `generate_on_scan`) and attach `pub storyboard: StoryboardSettings` to `AppConfig`.
- **Modify:** `kadr.toml` — document `[storyboard]` defaults.
- **Modify:** `crates/kadr-server/src/lib.rs` — export `pub mod storyboard;`.
- **Create:** `crates/kadr-server/src/storyboard/mod.rs` — module root re-exporting math and service types.
- **Create:** `crates/kadr-server/src/storyboard/math.rs` — interval/tile math, manifest construction, JPEG SOF header dimension parser, and `/frame` parameter clamping/snapping.
- **Create:** `crates/kadr-server/src/storyboard/service.rs` — `StoryboardService`, disk cache hashing, atomic temp-dir generation, `Semaphore` + in-flight deduplication, low-priority `ffmpeg` runner, single-frame extraction, and `purge_for_path`.
- **Create:** `crates/kadr-server/tests/storyboard_service_test.rs` — integration tests for `StoryboardService` generation, cache reuse, mtime/size invalidation, `/frame` caching, and `purge_for_path`.
- **Create:** `crates/kadr-server/src/api/storyboard_routes.rs` — Axum route handlers for `/api/v1/media/{item_id}/storyboard`, `/api/v1/media/{item_id}/storyboard/{sheet}`, and `/api/v1/media/{item_id}/frame`.
- **Modify:** `crates/kadr-server/src/api/mod.rs` — mount `storyboard_routes` and attach `Extension(Arc<StoryboardService>)`.
- **Create:** `crates/kadr-server/tests/storyboard_routes_test.rs` — HTTP integration tests for manifest, sheets, `202` -> `200` flow, `ETag`/`304`, auth, `LIBRARY_LOCKED`, error statuses, cache reuse, and `/frame`.
- **Modify:** `crates/kadr-ingest/src/watcher/worker.rs` — add optional delete and upsert callbacks on `IngestWorker`.
- **Modify:** `crates/kadr-server/src/main.rs` — wire `StoryboardService` into `IngestWorker` for cache deletion on `IngestMessage::Delete` and optional pre-generation when `storyboard.generate_on_scan` is `true`.
- **Modify:** `CHANGELOG.md` — record storyboard trickplay and frame preview endpoints under `[Unreleased]`.

---

### Task 1: Storyboard Configuration, Manifest Types, Tile Math & JPEG Dimension Parser

**Files:**
- Modify: `crates/kadr-server/src/config.rs`
- Modify: `kadr.toml`
- Modify: `crates/kadr-server/src/lib.rs`
- Create: `crates/kadr-server/src/storyboard/mod.rs`
- Create: `crates/kadr-server/src/storyboard/math.rs`

**Interfaces:**
- Produces:
  - `kadr_server::config::StoryboardSettings` (`enabled: bool`, `max_concurrent: usize`, `interval_secs: u64`, `tile_width: u32`, `generate_on_scan: bool`) on `AppConfig.storyboard`
  - `kadr_server::storyboard::math::{StoryboardManifest, StoryboardSheet, StoryboardPlan, STORYBOARD_COLUMNS, STORYBOARD_ROWS, TILES_PER_SHEET, clamp_tile_width, compute_storyboard_plan, build_manifest, parse_jpeg_dimensions, snap_frame_time}`

- [ ] **Step 1: Add `StoryboardSettings` to `crates/kadr-server/src/config.rs` and `kadr.toml`**

In `crates/kadr-server/src/config.rs`, add `StoryboardSettings` and include `#[serde(default)] pub storyboard: StoryboardSettings` in `AppConfig`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryboardSettings {
    #[serde(default = "default_storyboard_enabled")]
    pub enabled: bool,
    #[serde(default = "default_storyboard_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_storyboard_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_storyboard_tile_width")]
    pub tile_width: u32,
    #[serde(default)]
    pub generate_on_scan: bool,
}

fn default_storyboard_enabled() -> bool {
    true
}
fn default_storyboard_max_concurrent() -> usize {
    1
}
fn default_storyboard_interval_secs() -> u64 {
    10
}
fn default_storyboard_tile_width() -> u32 {
    320
}

impl Default for StoryboardSettings {
    fn default() -> Self {
        Self {
            enabled: default_storyboard_enabled(),
            max_concurrent: default_storyboard_max_concurrent(),
            interval_secs: default_storyboard_interval_secs(),
            tile_width: default_storyboard_tile_width(),
            generate_on_scan: false,
        }
    }
}
```

And in `AppConfig`:
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub storage: StorageSettings,
    #[serde(default)]
    pub scanner: ScannerSettings,
    #[serde(default)]
    pub libraries: Vec<LibraryConfig>,
    #[serde(default)]
    pub transcoding: TranscodingSettings,
    #[serde(default)]
    pub plugins: PluginSettings,
    #[serde(default)]
    pub storyboard: StoryboardSettings,
}
```

In `kadr.toml`, append:
```toml

[storyboard]
enabled = true
max_concurrent = 1
interval_secs = 10
tile_width = 320
generate_on_scan = false
```

- [ ] **Step 2: Create `crates/kadr-server/src/storyboard/mod.rs` and `crates/kadr-server/src/storyboard/math.rs` with unit tests**

In `crates/kadr-server/src/lib.rs`, add `pub mod storyboard;`.

Create `crates/kadr-server/src/storyboard/mod.rs`:
```rust
pub mod math;
```

Create `crates/kadr-server/src/storyboard/math.rs`:
```rust
use serde::{Deserialize, Serialize};

pub const STORYBOARD_COLUMNS: u32 = 10;
pub const STORYBOARD_ROWS: u32 = 10;
pub const TILES_PER_SHEET: u32 = STORYBOARD_COLUMNS * STORYBOARD_ROWS;
pub const MAX_TARGET_TILES: u64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryboardSheet {
    pub index: u32,
    pub url: String,
    pub first_tile: u32,
    pub tile_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryboardManifest {
    pub interval_ms: u64,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub total_tiles: u32,
    pub sheets: Vec<StoryboardSheet>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryboardPlan {
    pub interval_secs: u64,
    pub interval_ms: u64,
    pub tile_width: u32,
    pub columns: u32,
    pub rows: u32,
    pub total_tiles: u32,
    pub sheet_count: u32,
}

/// Clamps a requested tile/frame width to `160..=640` and rounds down to an even number.
pub fn clamp_tile_width(width: u32) -> u32 {
    let clamped = width.clamp(160, 640);
    clamped & !1
}

/// Computes the storyboard interval, tile count, and sheet count for an item.
/// Returns `None` if `duration_seconds <= 0`.
pub fn compute_storyboard_plan(
    duration_seconds: i64,
    base_interval_secs: u64,
    raw_tile_width: u32,
) -> Option<StoryboardPlan> {
    if duration_seconds <= 0 {
        return None;
    }
    let duration_u64 = duration_seconds as u64;
    let base_secs = base_interval_secs.max(1);

    let initial_secs = if duration_u64 < 600 {
        base_secs.min(5).max(1)
    } else {
        base_secs
    };

    let min_secs_for_cap = duration_u64.div_ceil(MAX_TARGET_TILES).max(1);
    let interval_secs = initial_secs.max(min_secs_for_cap);
    let interval_ms = interval_secs * 1000;

    let total_tiles = (duration_u64 * 1000)
        .div_ceil(interval_ms)
        .max(1) as u32;
    let sheet_count = total_tiles.div_ceil(TILES_PER_SHEET).max(1);
    let tile_width = clamp_tile_width(raw_tile_width);

    Some(StoryboardPlan {
        interval_secs,
        interval_ms,
        tile_width,
        columns: STORYBOARD_COLUMNS,
        rows: STORYBOARD_ROWS,
        total_tiles,
        sheet_count,
    })
}

/// Builds a `StoryboardManifest` for `item_id`, optionally clamping `total_tiles` to the
/// number of sheets actually emitted by `ffmpeg`.
pub fn build_manifest(
    item_id: i64,
    plan: &StoryboardPlan,
    tile_height: u32,
    actual_sheet_count: Option<u32>,
) -> StoryboardManifest {
    let max_tiles_from_sheets = actual_sheet_count
        .map(|s| s.saturating_mul(TILES_PER_SHEET))
        .unwrap_or(plan.total_tiles);
    let total_tiles = plan.total_tiles.min(max_tiles_from_sheets).max(1);
    let sheet_count = total_tiles.div_ceil(TILES_PER_SHEET).max(1);

    let mut sheets = Vec::with_capacity(sheet_count as usize);
    for index in 0..sheet_count {
        let first_tile = index * TILES_PER_SHEET;
        let remaining = total_tiles.saturating_sub(first_tile);
        let tile_count = remaining.min(TILES_PER_SHEET);
        sheets.push(StoryboardSheet {
            index,
            url: format!("/api/v1/media/{item_id}/storyboard/{index}.jpg"),
            first_tile,
            tile_count,
        });
    }

    let even_height = (tile_height.max(2)) & !1;

    StoryboardManifest {
        interval_ms: plan.interval_ms,
        tile_width: plan.tile_width,
        tile_height: even_height,
        columns: plan.columns,
        rows: plan.rows,
        total_tiles,
        sheets,
    }
}

/// Rewrites `sheets[].url` on a cached manifest so it matches the requested `item_id`.
pub fn rewrite_manifest_urls(mut manifest: StoryboardManifest, item_id: i64) -> StoryboardManifest {
    for sheet in &mut manifest.sheets {
        sheet.url = format!("/api/v1/media/{item_id}/storyboard/{}.jpg", sheet.index);
    }
    manifest
}

/// Snaps a floating-point timestamp `t` (in seconds) to the nearest whole second,
/// clamped to `0..=duration_seconds`. Returns `None` if `t` is negative or non-finite.
pub fn snap_frame_time(t: f64, duration_seconds: i64) -> Option<u64> {
    if !t.is_finite() || t < 0.0 || duration_seconds <= 0 {
        return None;
    }
    let snapped = t.round() as u64;
    Some(snapped.min(duration_seconds as u64))
}

/// Parses width and height from a JPEG byte slice by scanning for SOF0/SOF1/SOF2 markers.
pub fn parse_jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 1 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let marker = bytes[i];
        i += 1;

        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }

        if i + 1 >= bytes.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if seg_len < 2 || i + seg_len > bytes.len() {
            break;
        }

        // SOF0 (0xC0), SOF1 (0xC1), SOF2 (0xC2)
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) {
            if seg_len < 7 {
                return None;
            }
            let height = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            if width > 0 && height > 0 {
                return Some((width, height));
            }
            return None;
        }

        i += seg_len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_under_ten_minutes_use_5s_interval() {
        let plan = compute_storyboard_plan(300, 10, 320).unwrap();
        assert_eq!(plan.interval_secs, 5);
        assert_eq!(plan.interval_ms, 5000);
        assert_eq!(plan.total_tiles, 60);
        assert_eq!(plan.sheet_count, 1);
    }

    #[test]
    fn standard_feature_uses_default_10s_interval_and_multiple_sheets() {
        let plan = compute_storyboard_plan(6800, 10, 320).unwrap();
        assert_eq!(plan.interval_secs, 10);
        assert_eq!(plan.interval_ms, 10000);
        assert_eq!(plan.total_tiles, 680);
        assert_eq!(plan.sheet_count, 7);

        let manifest = build_manifest(42, &plan, 134, None);
        assert_eq!(manifest.interval_ms, 10000);
        assert_eq!(manifest.tile_width, 320);
        assert_eq!(manifest.tile_height, 134);
        assert_eq!(manifest.columns, 10);
        assert_eq!(manifest.rows, 10);
        assert_eq!(manifest.total_tiles, 680);
        assert_eq!(manifest.sheets.len(), 7);
        assert_eq!(
            manifest.sheets[0],
            StoryboardSheet {
                index: 0,
                url: "/api/v1/media/42/storyboard/0.jpg".to_string(),
                first_tile: 0,
                tile_count: 100,
            }
        );
        assert_eq!(
            manifest.sheets[6],
            StoryboardSheet {
                index: 6,
                url: "/api/v1/media/42/storyboard/6.jpg".to_string(),
                first_tile: 600,
                tile_count: 80,
            }
        );
    }

    #[test]
    fn long_items_widen_interval_to_cap_tiles_at_1000() {
        // 4 hours = 14400s -> 10s interval would be 1440 tiles (> 1000).
        // Widens to ceil(14400 / 1000) = 15s -> 960 tiles.
        let plan = compute_storyboard_plan(14400, 10, 320).unwrap();
        assert_eq!(plan.interval_secs, 15);
        assert_eq!(plan.interval_ms, 15000);
        assert!(plan.total_tiles <= 1000);
        assert_eq!(plan.total_tiles, 960);
        assert_eq!(plan.sheet_count, 10);
    }

    #[test]
    fn non_positive_duration_returns_none() {
        assert!(compute_storyboard_plan(0, 10, 320).is_none());
        assert!(compute_storyboard_plan(-5, 10, 320).is_none());
    }

    #[test]
    fn tile_width_clamping_and_even_rounding() {
        assert_eq!(clamp_tile_width(80), 160);
        assert_eq!(clamp_tile_width(320), 320);
        assert_eq!(clamp_tile_width(321), 320);
        assert_eq!(clamp_tile_width(999), 640);
    }

    #[test]
    fn frame_time_snaps_and_rejects_invalid_values() {
        assert_eq!(snap_frame_time(12.4, 100), Some(12));
        assert_eq!(snap_frame_time(12.6, 100), Some(13));
        assert_eq!(snap_frame_time(150.0, 100), Some(100));
        assert_eq!(snap_frame_time(-1.0, 100), None);
        assert_eq!(snap_frame_time(f64::NAN, 100), None);
    }

    #[test]
    fn jpeg_sof_dimension_parser_extracts_width_and_height() {
        // Minimal synthetic JPEG header with APP0 + SOF0 (width=3200, height=1340)
        let bytes: Vec<u8> = vec![
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, // APP0 length 4
            0xFF, 0xC0, 0x00, 0x0B, // SOF0 length 11
            0x08, // precision
            0x05, 0x3C, // height = 1340
            0x0C, 0x80, // width = 3200
            0x01, 0x01, 0x11, 0x00, // component info
            0xFF, 0xD9, // EOI
        ];
        assert_eq!(parse_jpeg_dimensions(&bytes), Some((3200, 1340)));
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p kadr-server storyboard::math`
Expected: PASS (7 tests pass).

- [ ] **Step 4: Commit**

```bash
git add crates/kadr-server/src/config.rs kadr.toml crates/kadr-server/src/lib.rs crates/kadr-server/src/storyboard/mod.rs crates/kadr-server/src/storyboard/math.rs
git commit -m "feat(storyboard): add storyboard settings, tile math, and JPEG parser"
```

---

### Task 2: `StoryboardService` — Disk Cache, FFmpeg Sprite Generation, Single-Frame Extraction, and Cache Cleanup

**Files:**
- Modify: `crates/kadr-server/src/storyboard/mod.rs`
- Create: `crates/kadr-server/src/storyboard/service.rs`
- Create: `crates/kadr-server/tests/storyboard_service_test.rs`

**Interfaces:**
- Consumes: `StoryboardSettings`, `StoryboardManifest`, `StoryboardPlan`, `compute_storyboard_plan`, `build_manifest`, `rewrite_manifest_urls`, `clamp_tile_width`, `snap_frame_time`, `parse_jpeg_dimensions`
- Produces:
  - `kadr_server::storyboard::service::{StoryboardService, StoryboardStatus, SheetStatus, FrameStatus}`
  - `StoryboardService::new(cache_root: PathBuf, settings: StoryboardSettings, ffmpeg_path: String) -> Self`
  - `StoryboardService::with_ availability_override(cache_root: PathBuf, settings: StoryboardSettings, ffmpeg_path: String, ffmpeg_available: bool) -> Self`
  - `StoryboardService::is_available(&self) -> bool`
  - `StoryboardService::get_or_enqueue_manifest(&self, item_id: i64, file_path: &Path, duration_seconds: i64) -> Result<StoryboardStatus, std::io::Error>`
  - `StoryboardService::get_or_enqueue_sheet(&self, item_id: i64, index: u32, file_path: &Path, duration_seconds: i64) -> Result<SheetStatus, std::io::Error>`
  - `StoryboardService::extract_or_get_frame(&self, file_path: &Path, duration_seconds: i64, snapped_t: u64, clamped_w: u32) -> Result<FrameStatus, std::io::Error>`
  - `StoryboardService::purge_for_path(&self, file_path: &Path) -> std::io::Result<()>`
  - `StoryboardService::storyboard_generation_count(&self) -> u64`
  - `StoryboardService::frame_generation_count(&self) -> u64`

- [ ] **Step 1: Write the failing integration tests in `crates/kadr-server/tests/storyboard_service_test.rs`**

Create `crates/kadr-server/tests/storyboard_service_test.rs`:

```rust
use kadr_server::config::StoryboardSettings;
use kadr_server::storyboard::service::{FrameStatus, SheetStatus, StoryboardService, StoryboardStatus};
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn generate_test_video(path: &Path, duration_secs: u32, width: u32, height: u32) -> bool {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size={width}x{height}:rate=10"),
            "-c:v",
            "libx264",
            "-g",
            "10",
            "-pix_fmt",
            "yuv420p",
            path.to_str().unwrap(),
        ])
        .output();

    matches!(output, Ok(out) if out.status.success())
}

#[tokio::test]
async fn storyboard_generates_lazily_and_reuses_cache() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("clip.mp4");
    if !generate_test_video(&video_path, 12, 640, 268) {
        eprintln!("ffmpeg not available; skipping test");
        return;
    }

    let settings = StoryboardSettings {
        enabled: true,
        max_concurrent: 1,
        interval_secs: 5,
        tile_width: 320,
        generate_on_scan: false,
    };
    let svc = StoryboardService::new(
        dir.path().join("storyboards"),
        settings,
        "ffmpeg".to_string(),
    );

    // First call enqueues background generation and returns Generating
    let first = svc
        .get_or_enqueue_manifest(42, &video_path, 12)
        .await
        .unwrap();
    assert!(matches!(first, StoryboardStatus::Generating));

    // Poll until Ready
    let mut manifest = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let StoryboardStatus::Ready(m) = svc
            .get_or_enqueue_manifest(42, &video_path, 12)
            .await
            .unwrap()
        {
            manifest = Some(m);
            break;
        }
    }

    let manifest = manifest.expect("storyboard manifest should become ready");
    assert_eq!(manifest.interval_ms, 5000);
    assert_eq!(manifest.tile_width, 320);
    assert_eq!(manifest.tile_height, 134);
    assert_eq!(manifest.columns, 10);
    assert_eq!(manifest.rows, 10);
    assert_eq!(manifest.sheets.len(), 1);
    assert_eq!(manifest.sheets[0].url, "/api/v1/media/42/storyboard/0.jpg");
    assert_eq!(svc.storyboard_generation_count(), 1);

    // Sheet 0 is Ready; Sheet 1 is OutOfRange
    match svc.get_or_enqueue_sheet(42, 0, &video_path, 12).await.unwrap() {
        SheetStatus::Ready { path, etag } => {
            assert!(path.exists());
            assert!(!etag.is_empty());
        }
        other => panic!("expected Ready sheet, got {other:?}"),
    }
    assert!(matches!(
        svc.get_or_enqueue_sheet(42, 1, &video_path, 12).await.unwrap(),
        SheetStatus::OutOfRange
    ));

    // Second manifest call reuses disk cache without spawning ffmpeg again
    let second = svc
        .get_or_enqueue_manifest(99, &video_path, 12)
        .await
        .unwrap();
    match second {
        StoryboardStatus::Ready(m) => {
            assert_eq!(m.sheets[0].url, "/api/v1/media/99/storyboard/0.jpg");
        }
        other => panic!("expected Ready on cache hit, got {other:?}"),
    }
    assert_eq!(svc.storyboard_generation_count(), 1);
}

#[tokio::test]
async fn frame_extraction_caches_and_purges_on_delete() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("frame_clip.mp4");
    if !generate_test_video(&video_path, 4, 640, 360) {
        eprintln!("ffmpeg not available; skipping test");
        return;
    }

    let svc = StoryboardService::new(
        dir.path().join("storyboards"),
        StoryboardSettings::default(),
        "ffmpeg".to_string(),
    );

    let first = svc
        .extract_or_get_frame(&video_path, 4, 1, 320)
        .await
        .unwrap();
    let cached_path = match first {
        FrameStatus::Ready { path, .. } => {
            assert!(path.exists());
            path
        }
        other => panic!("expected Ready frame, got {other:?}"),
    };
    assert_eq!(svc.frame_generation_count(), 1);

    // Second request for the same (t, w) reuses cache
    let _second = svc
        .extract_or_get_frame(&video_path, 4, 1, 320)
        .await
        .unwrap();
    assert_eq!(svc.frame_generation_count(), 1);

    // Purging removes the cached file
    svc.purge_for_path(&video_path).await.unwrap();
    assert!(!cached_path.exists());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p kadr-server --test storyboard_service_test`
Expected: FAIL (`could not find service in storyboard`).

- [ ] **Step 3: Implement `crates/kadr-server/src/storyboard/service.rs` and export from `mod.rs`**

Update `crates/kadr-server/src/storyboard/mod.rs`:
```rust
pub mod math;
pub mod service;

pub use math::{
    build_manifest, clamp_tile_width, compute_storyboard_plan, parse_jpeg_dimensions,
    rewrite_manifest_urls, snap_frame_time, StoryboardManifest, StoryboardPlan, StoryboardSheet,
    STORYBOARD_COLUMNS, STORYBOARD_ROWS, TILES_PER_SHEET,
};
pub use service::{FrameStatus, SheetStatus, StoryboardService, StoryboardStatus};
```

Create `crates/kadr-server/src/storyboard/service.rs`:
```rust
use crate::config::StoryboardSettings;
use crate::storyboard::math::{
    build_manifest, compute_storyboard_plan, parse_jpeg_dimensions, rewrite_manifest_urls,
    StoryboardManifest, StoryboardPlan, STORYBOARD_ROWS,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, warn};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryboardStatus {
    Ready(StoryboardManifest),
    Generating,
    NoVideoStream,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetStatus {
    Ready { path: PathBuf, etag: String },
    Generating,
    OutOfRange,
    NoVideoStream,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameStatus {
    Ready { path: PathBuf, etag: String },
    NoVideoStream,
    Unavailable,
}

#[derive(Clone)]
pub struct StoryboardService {
    inner: Arc<StoryboardInner>,
}

struct StoryboardInner {
    cache_root: PathBuf,
    settings: StoryboardSettings,
    ffmpeg_path: String,
    nice_available: bool,
    ffmpeg_available: bool,
    storyboard_semaphore: Arc<Semaphore>,
    frame_semaphore: Arc<Semaphore>,
    in_flight: Mutex<HashSet<String>>,
    storyboard_generations: AtomicU64,
    frame_generations: AtomicU64,
}

impl StoryboardService {
    pub fn new(cache_root: PathBuf, settings: StoryboardSettings, ffmpeg_path: String) -> Self {
        let ffmpeg_available = probe_binary(&ffmpeg_path, "-version");
        Self::with_availability_override(cache_root, settings, ffmpeg_path, ffmpeg_available)
    }

    pub fn with_availability_override(
        cache_root: PathBuf,
        settings: StoryboardSettings,
        ffmpeg_path: String,
        ffmpeg_available: bool,
    ) -> Self {
        let nice_available = cfg!(unix) && probe_binary("nice", "--version");
        let permits = settings.max_concurrent.max(1);
        Self {
            inner: Arc::new(StoryboardInner {
                cache_root,
                settings,
                ffmpeg_path,
                nice_available,
                ffmpeg_available,
                storyboard_semaphore: Arc::new(Semaphore::new(permits)),
                frame_semaphore: Arc::new(Semaphore::new(2)),
                in_flight: Mutex::new(HashSet::new()),
                storyboard_generations: AtomicU64::new(0),
                frame_generations: AtomicU64::new(0),
            }),
        }
    }

    pub fn is_available(&self) -> bool {
        self.inner.settings.enabled && self.inner.ffmpeg_available
    }

    pub fn settings(&self) -> &StoryboardSettings {
        &self.inner.settings
    }

    pub fn storyboard_generation_count(&self) -> u64 {
        self.inner.storyboard_generations.load(Ordering::SeqCst)
    }

    pub fn frame_generation_count(&self) -> u64 {
        self.inner.frame_generations.load(Ordering::SeqCst)
    }

    pub fn path_hash(file_path: &Path) -> String {
        let canonical = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
        let mut hasher = Sha256::new();
        hasher.update(canonical.to_string_lossy().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    async fn file_fingerprint(file_path: &Path) -> std::io::Result<(PathBuf, u64, u32, u64)> {
        let canonical = tokio::fs::canonicalize(file_path)
            .await
            .unwrap_or_else(|_| file_path.to_path_buf());
        let meta = tokio::fs::metadata(&canonical).await?;
        if !meta.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not a regular file",
            ));
        }
        let (mtime_secs, mtime_nanos) = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| (d.as_secs(), d.subsec_nanos()))
            .unwrap_or((0, 0));
        Ok((canonical, mtime_secs, mtime_nanos, meta.len()))
    }

    fn compute_source_hash(
        canonical_path: &Path,
        mtime_secs: u64,
        mtime_nanos: u32,
        file_size: u64,
        tile_width: u32,
        interval_ms: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(canonical_path.to_string_lossy().as_bytes());
        hasher.update(b"|");
        hasher.update(mtime_secs.to_le_bytes());
        hasher.update(mtime_nanos.to_le_bytes());
        hasher.update(file_size.to_le_bytes());
        hasher.update(tile_width.to_le_bytes());
        hasher.update(interval_ms.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn compute_frame_hash(
        canonical_path: &Path,
        mtime_secs: u64,
        mtime_nanos: u32,
        file_size: u64,
        snapped_t: u64,
        clamped_w: u32,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(canonical_path.to_string_lossy().as_bytes());
        hasher.update(b"|frame|");
        hasher.update(mtime_secs.to_le_bytes());
        hasher.update(mtime_nanos.to_le_bytes());
        hasher.update(file_size.to_le_bytes());
        hasher.update(snapped_t.to_le_bytes());
        hasher.update(clamped_w.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn item_dir(&self, path_hash: &str) -> PathBuf {
        self.inner.cache_root.join("items").join(path_hash)
    }

    fn frames_dir(&self, path_hash: &str) -> PathBuf {
        self.inner.cache_root.join("frames").join(path_hash)
    }

    pub async fn purge_for_path(&self, file_path: &Path) -> std::io::Result<()> {
        let mut hashes = HashSet::new();
        let mut raw_hasher = Sha256::new();
        raw_hasher.update(file_path.to_string_lossy().as_bytes());
        hashes.insert(format!("{:x}", raw_hasher.finalize()));
        hashes.insert(Self::path_hash(file_path));

        for h in hashes {
            let item_dir = self.item_dir(&h);
            if tokio::fs::try_exists(&item_dir).await.unwrap_or(false) {
                let _ = tokio::fs::remove_dir_all(&item_dir).await;
            }
            let frames_dir = self.frames_dir(&h);
            if tokio::fs::try_exists(&frames_dir).await.unwrap_or(false) {
                let _ = tokio::fs::remove_dir_all(&frames_dir).await;
            }
        }
        Ok(())
    }

    pub async fn get_or_enqueue_manifest(
        &self,
        item_id: i64,
        file_path: &Path,
        duration_seconds: i64,
    ) -> std::io::Result<StoryboardStatus> {
        if !self.is_available() {
            return Ok(StoryboardStatus::Unavailable);
        }
        let Some(plan) = compute_storyboard_plan(
            duration_seconds,
            self.inner.settings.interval_secs,
            self.inner.settings.tile_width,
        ) else {
            return Ok(StoryboardStatus::NoVideoStream);
        };

        let (canonical, mtime_secs, mtime_nanos, file_size) =
            Self::file_fingerprint(file_path).await?;
        let path_hash = Self::path_hash(&canonical);
        let source_hash = Self::compute_source_hash(
            &canonical,
            mtime_secs,
            mtime_nanos,
            file_size,
            plan.tile_width,
            plan.interval_ms,
        );

        let source_dir = self.item_dir(&path_hash).join(&source_hash);
        let no_video_marker = source_dir.join(".novideo");
        if tokio::fs::try_exists(&no_video_marker).await.unwrap_or(false) {
            return Ok(StoryboardStatus::NoVideoStream);
        }

        let manifest_path = source_dir.join("manifest.json");
        if let Ok(bytes) = tokio::fs::read(&manifest_path).await {
            if let Ok(manifest) = serde_json::from_slice::<StoryboardManifest>(&bytes) {
                return Ok(StoryboardStatus::Ready(rewrite_manifest_urls(
                    manifest, item_id,
                )));
            }
        }

        self.spawn_generation_if_needed(canonical, path_hash, source_hash, plan)
            .await;
        Ok(StoryboardStatus::Generating)
    }

    pub async fn get_or_enqueue_sheet(
        &self,
        item_id: i64,
        index: u32,
        file_path: &Path,
        duration_seconds: i64,
    ) -> std::io::Result<SheetStatus> {
        if !self.is_available() {
            return Ok(SheetStatus::Unavailable);
        }
        let Some(plan) = compute_storyboard_plan(
            duration_seconds,
            self.inner.settings.interval_secs,
            self.inner.settings.tile_width,
        ) else {
            return Ok(SheetStatus::NoVideoStream);
        };

        let (canonical, mtime_secs, mtime_nanos, file_size) =
            Self::file_fingerprint(file_path).await?;
        let path_hash = Self::path_hash(&canonical);
        let source_hash = Self::compute_source_hash(
            &canonical,
            mtime_secs,
            mtime_nanos,
            file_size,
            plan.tile_width,
            plan.interval_ms,
        );

        let source_dir = self.item_dir(&path_hash).join(&source_hash);
        let no_video_marker = source_dir.join(".novideo");
        if tokio::fs::try_exists(&no_video_marker).await.unwrap_or(false) {
            return Ok(SheetStatus::NoVideoStream);
        }

        let manifest_path = source_dir.join("manifest.json");
        if let Ok(bytes) = tokio::fs::read(&manifest_path).await {
            if let Ok(manifest) = serde_json::from_slice::<StoryboardManifest>(&bytes) {
                let _ = item_id;
                if (index as usize) >= manifest.sheets.len() {
                    return Ok(SheetStatus::OutOfRange);
                }
                let sheet_path = source_dir.join(format!("{index}.jpg"));
                if tokio::fs::try_exists(&sheet_path).await.unwrap_or(false) {
                    let etag = format!("\"{source_hash}-{index}\"");
                    return Ok(SheetStatus::Ready {
                        path: sheet_path,
                        etag,
                    });
                }
                return Ok(SheetStatus::OutOfRange);
            }
        }

        if index >= plan.sheet_count {
            return Ok(SheetStatus::OutOfRange);
        }

        self.spawn_generation_if_needed(canonical, path_hash, source_hash, plan)
            .await;
        Ok(SheetStatus::Generating)
    }

    pub async fn extract_or_get_frame(
        &self,
        file_path: &Path,
        duration_seconds: i64,
        snapped_t: u64,
        clamped_w: u32,
    ) -> std::io::Result<FrameStatus> {
        if !self.is_available() {
            return Ok(FrameStatus::Unavailable);
        }
        if duration_seconds <= 0 {
            return Ok(FrameStatus::NoVideoStream);
        }

        let (canonical, mtime_secs, mtime_nanos, file_size) =
            Self::file_fingerprint(file_path).await?;
        let path_hash = Self::path_hash(&canonical);
        let frame_hash = Self::compute_frame_hash(
            &canonical,
            mtime_secs,
            mtime_nanos,
            file_size,
            snapped_t,
            clamped_w,
        );

        let frames_dir = self.frames_dir(&path_hash);
        let target_path = frames_dir.join(format!("{frame_hash}.jpg"));
        let etag = format!("\"frame-{frame_hash}\"");

        if let Ok(meta) = tokio::fs::metadata(&target_path).await {
            if meta.is_file() && meta.len() > 0 {
                return Ok(FrameStatus::Ready {
                    path: target_path,
                    etag,
                });
            }
        }

        let _permit = self
            .inner
            .frame_semaphore
            .acquire()
            .await
            .map_err(|_| std::io::Error::other("frame semaphore closed"))?;

        if let Ok(meta) = tokio::fs::metadata(&target_path).await {
            if meta.is_file() && meta.len() > 0 {
                return Ok(FrameStatus::Ready {
                    path: target_path,
                    etag,
                });
            }
        }

        tokio::fs::create_dir_all(&frames_dir).await?;
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = frames_dir.join(format!(
            ".tmp.{}.{}.{}.jpg",
            std::process::id(),
            counter,
            frame_hash
        ));

        self.inner.frame_generations.fetch_add(1, Ordering::SeqCst);
        let mut ok = self
            .run_frame_ffmpeg(&canonical, &tmp_path, snapped_t, clamped_w)
            .await;
        if !ok && snapped_t > 0 {
            ok = self
                .run_frame_ffmpeg(&canonical, &tmp_path, 0, clamped_w)
                .await;
        }

        if ok {
            if tokio::fs::rename(&tmp_path, &target_path).await.is_ok() {
                return Ok(FrameStatus::Ready {
                    path: target_path,
                    etag,
                });
            }
        }
        let _ = tokio::fs::remove_file(&tmp_path).await;
        Ok(FrameStatus::NoVideoStream)
    }

    async fn spawn_generation_if_needed(
        &self,
        canonical_path: PathBuf,
        path_hash: String,
        source_hash: String,
        plan: StoryboardPlan,
    ) {
        {
            let mut guard = self.inner.in_flight.lock().await;
            if !guard.insert(source_hash.clone()) {
                return;
            }
        }

        let svc = self.clone();
        tokio::spawn(async move {
            let _permit = svc.inner.storyboard_semaphore.acquire().await;
            if let Err(e) = svc
                .generate_storyboard_job(&canonical_path, &path_hash, &source_hash, &plan)
                .await
            {
                warn!(
                    path = ?canonical_path,
                    error = %e,
                    "Storyboard generation job failed"
                );
            }
            let mut guard = svc.inner.in_flight.lock().await;
            guard.remove(&source_hash);
        });
    }

    async fn generate_storyboard_job(
        &self,
        canonical_path: &Path,
        path_hash: &str,
        source_hash: &str,
        plan: &StoryboardPlan,
    ) -> std::io::Result<()> {
        let item_dir = self.item_dir(path_hash);
        let final_dir = item_dir.join(source_hash);
        if tokio::fs::try_exists(final_dir.join("manifest.json"))
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }

        tokio::fs::create_dir_all(&item_dir).await?;
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_dir = item_dir.join(format!(
            ".tmp.{}.{}.{}",
            std::process::id(),
            counter,
            source_hash
        ));
        tokio::fs::create_dir_all(&tmp_dir).await?;

        self.inner
            .storyboard_generations
            .fetch_add(1, Ordering::SeqCst);

        let mut produced = self
            .run_storyboard_ffmpeg(canonical_path, &tmp_dir, plan, true)
            .await;
        if produced == 0 {
            debug!(
                path = ?canonical_path,
                "Keyframe-only pass produced 0 sheets; retrying without -skip_frame nokey"
            );
            produced = self
                .run_storyboard_ffmpeg(canonical_path, &tmp_dir, plan, false)
                .await;
        }

        if produced == 0 {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            tokio::fs::create_dir_all(&final_dir).await?;
            tokio::fs::write(final_dir.join(".novideo"), b"").await?;
            return Ok(());
        }

        let first_sheet_tmp = tmp_dir.join("sheet_001.jpg");
        let first_bytes = tokio::fs::read(&first_sheet_tmp).await?;
        let (_, sheet_height) = parse_jpeg_dimensions(&first_bytes).unwrap_or((
            plan.tile_width * plan.columns,
            ((plan.tile_width * 9 / 16) & !1) * STORYBOARD_ROWS,
        ));
        let tile_height = ((sheet_height / STORYBOARD_ROWS).max(2)) & !1;

        for idx in 0..produced {
            let src = tmp_dir.join(format!("sheet_{:03}.jpg", idx + 1));
            let dst = tmp_dir.join(format!("{idx}.jpg"));
            tokio::fs::rename(&src, &dst).await?;
        }

        let manifest = build_manifest(0, plan, tile_height, Some(produced));
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        tokio::fs::write(tmp_dir.join("manifest.json"), manifest_bytes).await?;

        if tokio::fs::try_exists(&final_dir).await.unwrap_or(false) {
            let _ = tokio::fs::remove_dir_all(&final_dir).await;
        }
        if let Err(e) = tokio::fs::rename(&tmp_dir, &final_dir).await {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            return Err(e);
        }

        // Prune older sibling source_hash directories for this file path
        if let Ok(mut entries) = tokio::fs::read_dir(&item_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str != source_hash && !name_str.starts_with(".tmp.") {
                    let _ = tokio::fs::remove_dir_all(entry.path()).await;
                }
            }
        }

        Ok(())
    }

    async fn run_storyboard_ffmpeg(
        &self,
        media_path: &Path,
        out_dir: &Path,
        plan: &StoryboardPlan,
        keyframe_only: bool,
    ) -> u32 {
        let mut cmd = if self.inner.nice_available {
            let mut c = tokio::process::Command::new("nice");
            c.args(["-n", "10", &self.inner.ffmpeg_path]);
            c
        } else {
            tokio::process::Command::new(&self.inner.ffmpeg_path)
        };

        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        if keyframe_only {
            cmd.args(["-skip_frame", "nokey"]);
        }
        let vf = format!(
            "fps=1/{},scale={}:-2,tile={}x{}",
            plan.interval_secs, plan.tile_width, plan.columns, plan.rows
        );
        let pattern = out_dir.join("sheet_%03d.jpg");

        cmd.arg("-i")
            .arg(media_path)
            .args(["-an", "-sn", "-vf", &vf, "-frames:v"])
            .arg(plan.sheet_count.to_string())
            .args(["-q:v", "6"])
            .arg(&pattern)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let _ = cmd.output().await;

        let mut count = 0u32;
        for idx in 1..=plan.sheet_count {
            let candidate = out_dir.join(format!("sheet_{idx:03}.jpg"));
            if let Ok(meta) = tokio::fs::metadata(&candidate).await {
                if meta.is_file() && meta.len() > 0 {
                    count += 1;
                    continue;
                }
            }
            break;
        }
        count
    }

    async fn run_frame_ffmpeg(
        &self,
        media_path: &Path,
        target_path: &Path,
        snapped_t: u64,
        clamped_w: u32,
    ) -> bool {
        let vf = format!("scale={clamped_w}:-2");
        let res = tokio::process::Command::new(&self.inner.ffmpeg_path)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-ss"])
            .arg(snapped_t.to_string())
            .arg("-i")
            .arg(media_path)
            .args(["-frames:v", "1", "-vf", &vf, "-q:v", "5"])
            .arg(target_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await;

        match res {
            Ok(out) if out.status.success() => tokio::fs::metadata(target_path)
                .await
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false),
            _ => false,
        }
    }
}

fn probe_binary(bin: &str, arg: &str) -> bool {
    std::process::Command::new(bin)
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kadr-server --test storyboard_service_test`
Expected: PASS (2 tests pass).

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/storyboard/mod.rs crates/kadr-server/src/storyboard/service.rs crates/kadr-server/tests/storyboard_service_test.rs
git commit -m "feat(storyboard): implement StoryboardService cache, ffmpeg runner, and frame extraction"
```

---

### Task 3: Storyboard & Frame HTTP Routes (`/storyboard`, `/storyboard/{sheet}`, `/frame`) and Router Wiring

**Files:**
- Create: `crates/kadr-server/src/api/storyboard_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Create: `crates/kadr-server/tests/storyboard_routes_test.rs`

**Interfaces:**
- Consumes: `StoryboardService`, `StoryboardStatus`, `SheetStatus`, `FrameStatus`, `clamp_tile_width`, `snap_frame_time`, `AuthUser`, `UnlockedLibraries`, `MediaItemRepository`, `LibraryRepository`
- Produces:
  - `GET /api/v1/media/{item_id}/storyboard`
  - `GET /api/v1/media/{item_id}/storyboard/{sheet}`
  - `GET /api/v1/media/{item_id}/frame?t=<seconds>&w=<width>`
  - `kadr_server::api::storyboard_routes::storyboard_routes() -> axum::Router`

- [ ] **Step 1: Write the failing HTTP route integration tests in `crates/kadr-server/tests/storyboard_routes_test.rs`**

Create `crates/kadr-server/tests/storyboard_routes_test.rs`:

```rust
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
};
use kadr_server::api::create_router_with_ingest;
use kadr_server::api::unlock_token::UnlockTokenService;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::events::EventBus;
use kadr_server::identity::ServerIdentity;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::storyboard::StoryboardService;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tower::ServiceExt;

const TEST_JWT_SECRET: &str = "storyboard-test-secret-key-at-least-32-bytes!";

fn generate_test_video(path: &Path, duration_secs: u32, width: u32, height: u32) -> bool {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size={width}x{height}:rate=10"),
            "-c:v",
            "libx264",
            "-g",
            "10",
            "-pix_fmt",
            "yuv420p",
            path.to_str().unwrap(),
        ])
        .output();

    matches!(output, Ok(out) if out.status.success())
}

struct TestCtx {
    app: axum::Router,
    token: String,
    video_item_id: i64,
    locked_item_id: i64,
    audio_item_id: i64,
    missing_file_item_id: i64,
    storyboard_svc: Arc<StoryboardService>,
    _dir: tempfile::TempDir,
}

async fn setup_ctx(storyboard_enabled: bool) -> Option<TestCtx> {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("movie.mp4");
    if !generate_test_video(&video_path, 12, 640, 268) {
        return None;
    }
    let audio_path = dir.path().join("song.mp3");
    std::fs::write(&audio_path, b"not-a-video").unwrap();

    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let sub_repo = SubtitleRepository::new(pool.clone());

    lib_repo
        .insert(&Library {
            id: "pub-lib".to_string(),
            name: "Public Movies".to_string(),
            path: dir.path().to_path_buf(),
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "priv-lib".to_string(),
            name: "Private Movies".to_string(),
            path: dir.path().to_path_buf(),
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("hash".to_string()),
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    media_repo
        .upsert_batch(&[
            MediaItem {
                id: None,
                library_id: "pub-lib".to_string(),
                item_type: MediaType::Movie,
                title: "Test Movie".to_string(),
                file_path: video_path.clone(),
                file_name: "movie.mp4".to_string(),
                file_size: 1024,
                technical: TechnicalInfo {
                    duration_seconds: 12,
                    video_codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "priv-lib".to_string(),
                item_type: MediaType::Movie,
                title: "Locked Movie".to_string(),
                file_path: dir.path().join("locked_copy.mp4"),
                file_name: "locked_copy.mp4".to_string(),
                file_size: 1024,
                technical: TechnicalInfo {
                    duration_seconds: 12,
                    video_codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "pub-lib".to_string(),
                item_type: MediaType::Music,
                title: "Audio Track".to_string(),
                file_path: audio_path,
                file_name: "song.mp3".to_string(),
                file_size: 12,
                technical: TechnicalInfo {
                    duration_seconds: 180,
                    video_codec: None,
                    audio_codec: Some("mp3".to_string()),
                    container: Some("mp3".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
            MediaItem {
                id: None,
                library_id: "pub-lib".to_string(),
                item_type: MediaType::Movie,
                title: "Missing File Movie".to_string(),
                file_path: dir.path().join("does_not_exist.mp4"),
                file_name: "does_not_exist.mp4".to_string(),
                file_size: 100,
                technical: TechnicalInfo {
                    duration_seconds: 60,
                    video_codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                    ..Default::default()
                },
                metadata: MediaMetadata::default(),
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    std::fs::copy(&video_path, dir.path().join("locked_copy.mp4")).unwrap();

    let pub_items = media_repo.list_by_library("pub-lib", 10, 0).await.unwrap();
    let priv_items = media_repo.list_by_library("priv-lib", 10, 0).await.unwrap();
    let video_item_id = pub_items
        .iter()
        .find(|i| i.title == "Test Movie")
        .unwrap()
        .id
        .unwrap();
    let audio_item_id = pub_items
        .iter()
        .find(|i| i.title == "Audio Track")
        .unwrap()
        .id
        .unwrap();
    let missing_file_item_id = pub_items
        .iter()
        .find(|i| i.title == "Missing File Movie")
        .unwrap()
        .id
        .unwrap();
    let locked_item_id = priv_items[0].id.unwrap();

    let jwt = JwtService::new(TEST_JWT_SECRET, 3600);
    let token = jwt
        .generate_token(&User {
            id: "u1".to_string(),
            username: "alice".to_string(),
            pin_hash: String::new(),
            role: UserRole::Standard,
            created_at: 1000,
        })
        .unwrap();

    let mut cfg = AppConfig::default();
    cfg.server.data_dir = dir.path().to_path_buf();
    cfg.storyboard.enabled = storyboard_enabled;
    cfg.storyboard.interval_secs = 5;
    let shared_cfg = Arc::new(tokio::sync::RwLock::new(cfg.clone()));

    let storyboard_svc = Arc::new(StoryboardService::new(
        dir.path().join("storyboards"),
        cfg.storyboard.clone(),
        "ffmpeg".to_string(),
    ));

    let session_registry = Arc::new(SessionRegistry::new());
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry = Arc::new(TelemetryCollector::new(
        PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let (tx, _) = tokio::sync::mpsc::channel(1);
    let pipeline = Arc::new(kadr_ingest::watcher::IngestPipeline::new(false, None));

    let app = create_router_with_ingest(
        user_repo,
        playback_repo.clone(),
        media_repo.clone(),
        lib_repo,
        jwt,
        RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300)),
        session_registry,
        LayoutRegistry::new(),
        Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo)),
        Arc::new(SubtitleDeliveryService::new(
            dir.path().join("subs"),
            sub_repo,
            media_repo,
        )),
        Arc::new(OpenSubtitlesClient::new(None, None)),
        event_bus,
        telemetry,
        tx,
        pipeline,
        shared_cfg,
        Arc::new(ServerIdentity {
            id: "00000000-0000-0000-0000-000000000000".to_string(),
        }),
    )
    .layer(axum::Extension(storyboard_svc.clone()));

    Some(TestCtx {
        app,
        token,
        video_item_id,
        locked_item_id,
        audio_item_id,
        missing_file_item_id,
        storyboard_svc,
        _dir: dir,
    })
}

#[tokio::test]
async fn storyboard_202_then_200_flow_and_cache_reuse() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    // 1. Initial manifest request returns 202 Accepted with Retry-After: 5
    let req = Request::builder()
        .uri(format!("/api/v1/media/{}/storyboard", ctx.video_item_id))
        .header(header::AUTHORIZATION, format!("Bearer {}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    assert_eq!(res.headers().get("retry-after").unwrap(), "5");
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "generating");

    // 2. Poll until 200 OK
    let mut manifest_json = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let req = Request::builder()
            .uri(format!(
                "/api/v1/media/{}/storyboard?token={}",
                ctx.video_item_id, ctx.token
            ))
            .body(Body::empty())
            .unwrap();
        let res = ctx.app.clone().oneshot(req).await.unwrap();
        if res.status() == StatusCode::OK {
            let bytes = axum::body::to_bytes(res.into_body(), 16384).await.unwrap();
            manifest_json = Some(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap());
            break;
        }
    }

    let manifest = manifest_json.expect("manifest should become ready");
    assert_eq!(manifest["interval_ms"], 5000);
    assert_eq!(manifest["tile_width"], 320);
    assert_eq!(manifest["tile_height"], 134);
    assert_eq!(manifest["columns"], 10);
    assert_eq!(manifest["rows"], 10);
    let sheet_url = manifest["sheets"][0]["url"].as_str().unwrap();
    assert_eq!(
        sheet_url,
        format!("/api/v1/media/{}/storyboard/0.jpg", ctx.video_item_id)
    );
    assert!(!sheet_url.contains("token="));

    // 3. Fetch sheet 0.jpg via ?token= query param
    let req = Request::builder()
        .uri(format!("{sheet_url}?token={}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=604800"
    );
    let etag = res
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let jpeg_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(jpeg_bytes.starts_with(&[0xFF, 0xD8]));

    // 4. Conditional GET with If-None-Match returns 304 Not Modified
    let req = Request::builder()
        .uri(format!("{sheet_url}?token={}", ctx.token))
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);

    // 5. Out-of-range sheet returns 404
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard/99.jpg?token={}",
            ctx.video_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 6. Verify cache reuse: ffmpeg ran only once
    assert_eq!(ctx.storyboard_svc.storyboard_generation_count(), 1);
}

#[tokio::test]
async fn storyboard_enforces_auth_and_private_library_locks() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    // Unauthenticated -> 401
    let req = Request::builder()
        .uri(format!("/api/v1/media/{}/storyboard", ctx.video_item_id))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Locked private library -> 403 LIBRARY_LOCKED
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            ctx.locked_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "LIBRARY_LOCKED");

    // Unlocked via ?unlocked= query param -> 202 Accepted
    let unlock_svc = UnlockTokenService::new(TEST_JWT_SECRET, 3600);
    let (unlock_tok, _) = unlock_svc.generate_token("priv-lib").unwrap();
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}&unlocked={}",
            ctx.locked_item_id, ctx.token, unlock_tok
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn storyboard_error_statuses_404_and_503() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    // Nonexistent item -> 404 Media item not found
    let req = Request::builder()
        .uri(format!("/api/v1/media/999999/storyboard?token={}", ctx.token))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Media item not found");

    // Missing disk file -> 404 Media file missing from disk
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            ctx.missing_file_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Media file missing from disk");

    // Audio-only item -> 404
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            ctx.audio_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Disabled storyboard -> 503 Storyboard generation unavailable
    let Some(disabled_ctx) = setup_ctx(false).await else {
        return;
    };
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/storyboard?token={}",
            disabled_ctx.video_item_id, disabled_ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = disabled_ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Storyboard generation unavailable");
}

#[tokio::test]
async fn single_frame_fallback_endpoint_extracts_and_caches() {
    let Some(ctx) = setup_ctx(true).await else {
        eprintln!("ffmpeg not available; skipping test");
        return;
    };

    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/frame?t=2.4&w=280&token={}",
            ctx.video_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=604800"
    );
    assert!(res.headers().get(header::ETAG).is_some());
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
    assert_eq!(ctx.storyboard_svc.frame_generation_count(), 1);

    // Second request at t=1.8 (snaps to same 2s) and w=280 hits disk cache
    let req = Request::builder()
        .uri(format!(
            "/api/v1/media/{}/frame?t=1.8&w=280&token={}",
            ctx.video_item_id, ctx.token
        ))
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(ctx.storyboard_svc.frame_generation_count(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p kadr-server --test storyboard_routes_test`
Expected: FAIL (`404 Not Found` on `/api/v1/media/{id}/storyboard` because routes are not mounted yet).

- [ ] **Step 3: Implement `crates/kadr-server/src/api/storyboard_routes.rs` and wire into `crates/kadr-server/src/api/mod.rs`**

Create `crates/kadr-server/src/api/storyboard_routes.rs`:

```rust
use axum::{
    body::Body,
    extract::{Extension, Path, Query},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kadr_core::models::{MediaItem, MediaType};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use serde::Deserialize;
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::api::unlock_token::UnlockedLibraries;
use crate::auth::jwt::AuthUser;
use crate::storyboard::{
    clamp_tile_width, snap_frame_time, FrameStatus, SheetStatus, StoryboardService,
    StoryboardStatus,
};

#[derive(Debug, Default, Deserialize)]
pub struct FrameQuery {
    pub t: Option<f64>,
    pub w: Option<u32>,
}

fn has_video_stream(item: &MediaItem) -> bool {
    if item.plugin_ref().is_some() {
        return false;
    }
    if item.technical.duration_seconds <= 0 {
        return false;
    }
    if matches!(
        item.item_type,
        MediaType::Music
            | MediaType::Audiobook
            | MediaType::Show
            | MediaType::Season
            | MediaType::LiveChannel
    ) {
        return false;
    }
    if let Some(ref container) = item.technical.container {
        if matches!(
            container.to_ascii_lowercase().as_str(),
            "mp3" | "flac" | "aac" | "ogg" | "wav" | "m4a" | "opus"
        ) && item.technical.video_codec.is_none()
        {
            return false;
        }
    }
    if let Some(ext) = item.file_path.extension().and_then(|e| e.to_str()) {
        if matches!(
            ext.to_ascii_lowercase().as_str(),
            "mp3" | "flac" | "aac" | "ogg" | "wav" | "m4a" | "opus"
        ) && item.technical.video_codec.is_none()
        {
            return false;
        }
    }
    true
}

async fn load_and_authorize_item(
    item_id: i64,
    unlocked: &UnlockedLibraries,
    media_repo: &MediaItemRepository,
    lib_repo: &LibraryRepository,
    storyboard_svc: &StoryboardService,
) -> Result<MediaItem, Response> {
    let item = match media_repo.get_by_id(item_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Media item not found" })),
            )
                .into_response());
        }
        Err(err) => {
            tracing::error!("Failed to fetch media item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response());
        }
    };

    match lib_repo.get_by_id(&item.library_id).await {
        Ok(Some(library)) => {
            if library.is_private && !unlocked.is_unlocked(&item.library_id) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "LIBRARY_LOCKED" })),
                )
                    .into_response());
            }
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!("Failed to fetch library for media item {item_id}: {err}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database error" })),
            )
                .into_response());
        }
    }

    if !storyboard_svc.is_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response());
    }

    if !has_video_stream(&item) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response());
    }

    match tokio::fs::metadata(&item.file_path).await {
        Ok(meta) if meta.is_file() => Ok(item),
        _ => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media file missing from disk" })),
        )
            .into_response()),
    }
}

fn generating_response() -> Response {
    (
        StatusCode::ACCEPTED,
        [(header::RETRY_AFTER, "5")],
        Json(serde_json::json!({ "status": "generating" })),
    )
        .into_response()
}

async fn serve_cached_jpeg(path: &std::path::Path, etag: &str, headers: &HeaderMap) -> Response {
    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if if_none_match
            .split(',')
            .any(|candidate| candidate.trim() == etag || candidate.trim() == "*")
        {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, etag)
                .header(header::CACHE_CONTROL, "public, max-age=604800")
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::NOT_MODIFIED.into_response());
        }
    }

    let file = match File::open(path).await {
        Ok(f) => f,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Storyboard sheet not found" })),
            )
                .into_response();
        }
    };

    let content_len = file.metadata().await.ok().map(|m| m.len());
    let stream = ReaderStream::with_capacity(file, 64 * 1024);
    let body = Body::from_stream(stream);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "public, max-age=604800")
        .header(header::ETAG, etag);

    if let Some(len) = content_len {
        builder = builder.header(header::CONTENT_LENGTH, len.to_string());
    }

    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub async fn get_storyboard_manifest(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(storyboard_svc): Extension<Arc<StoryboardService>>,
) -> Response {
    let item = match load_and_authorize_item(
        item_id,
        &unlocked,
        &media_repo,
        &lib_repo,
        &storyboard_svc,
    )
    .await
    {
        Ok(i) => i,
        Err(resp) => return resp,
    };

    match storyboard_svc
        .get_or_enqueue_manifest(item_id, &item.file_path, item.technical.duration_seconds)
        .await
    {
        Ok(StoryboardStatus::Ready(manifest)) => (StatusCode::OK, Json(manifest)).into_response(),
        Ok(StoryboardStatus::Generating) => generating_response(),
        Ok(StoryboardStatus::NoVideoStream) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response(),
        Ok(StoryboardStatus::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Storyboard manifest error for item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read storyboard" })),
            )
                .into_response()
        }
    }
}

pub async fn get_storyboard_sheet(
    _auth_user: AuthUser,
    Path((item_id, sheet_name)): Path<(i64, String)>,
    headers: HeaderMap,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(storyboard_svc): Extension<Arc<StoryboardService>>,
) -> Response {
    let Some(index_str) = sheet_name.strip_suffix(".jpg") else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Storyboard sheet not found" })),
        )
            .into_response();
    };
    let Ok(index) = index_str.parse::<u32>() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Storyboard sheet not found" })),
        )
            .into_response();
    };

    let item = match load_and_authorize_item(
        item_id,
        &unlocked,
        &media_repo,
        &lib_repo,
        &storyboard_svc,
    )
    .await
    {
        Ok(i) => i,
        Err(resp) => return resp,
    };

    match storyboard_svc
        .get_or_enqueue_sheet(
            item_id,
            index,
            &item.file_path,
            item.technical.duration_seconds,
        )
        .await
    {
        Ok(SheetStatus::Ready { path, etag }) => serve_cached_jpeg(&path, &etag, &headers).await,
        Ok(SheetStatus::Generating) => generating_response(),
        Ok(SheetStatus::OutOfRange) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Storyboard sheet index out of range" })),
        )
            .into_response(),
        Ok(SheetStatus::NoVideoStream) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response(),
        Ok(SheetStatus::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Storyboard sheet error for item {item_id} index {index}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to read storyboard sheet" })),
            )
                .into_response()
        }
    }
}

pub async fn get_single_frame(
    _auth_user: AuthUser,
    Path(item_id): Path<i64>,
    Query(query): Query<FrameQuery>,
    headers: HeaderMap,
    unlocked: UnlockedLibraries,
    Extension(media_repo): Extension<MediaItemRepository>,
    Extension(lib_repo): Extension<LibraryRepository>,
    Extension(storyboard_svc): Extension<Arc<StoryboardService>>,
) -> Response {
    let item = match load_and_authorize_item(
        item_id,
        &unlocked,
        &media_repo,
        &lib_repo,
        &storyboard_svc,
    )
    .await
    {
        Ok(i) => i,
        Err(resp) => return resp,
    };

    let raw_t = query.t.unwrap_or(0.0);
    let Some(snapped_t) = snap_frame_time(raw_t, item.technical.duration_seconds) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid frame timestamp" })),
        )
            .into_response();
    };
    let clamped_w = clamp_tile_width(query.w.unwrap_or(320));

    match storyboard_svc
        .extract_or_get_frame(
            &item.file_path,
            item.technical.duration_seconds,
            snapped_t,
            clamped_w,
        )
        .await
    {
        Ok(FrameStatus::Ready { path, etag }) => serve_cached_jpeg(&path, &etag, &headers).await,
        Ok(FrameStatus::NoVideoStream) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Media item has no video stream" })),
        )
            .into_response(),
        Ok(FrameStatus::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Storyboard generation unavailable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Frame extraction error for item {item_id}: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to extract frame" })),
            )
                .into_response()
        }
    }
}

pub fn storyboard_routes() -> Router {
    Router::new()
        .route(
            "/api/v1/media/{item_id}/storyboard",
            get(get_storyboard_manifest),
        )
        .route(
            "/api/v1/media/{item_id}/storyboard/{sheet}",
            get(get_storyboard_sheet),
        )
        .route("/api/v1/media/{item_id}/frame", get(get_single_frame))
}
```

In `crates/kadr-server/src/api/mod.rs`:
1. Add `pub mod storyboard_routes;` at top of `crates/kadr-server/src/api/mod.rs`.
2. In `create_router_with_ingest`, construct a default `Arc<crate::storyboard::StoryboardService>` from `config` and merge `.merge(storyboard_routes::storyboard_routes())`, plus a middleware layer (just like `hls_manager`) that inserts `default_storyboard_svc` into request extensions if a test hasn't overridden it via `.layer(Extension(...))`:

```rust
    let (storyboard_settings, storyboard_dir, ffmpeg_path) = config
        .try_read()
        .map(|c| {
            (
                c.storyboard.clone(),
                c.server.data_dir.join("storyboards"),
                c.transcoding.ffmpeg_path.clone(),
            )
        })
        .unwrap_or_else(|_| {
            (
                crate::config::StoryboardSettings::default(),
                std::path::PathBuf::from("./data/storyboards"),
                "ffmpeg".to_string(),
            )
        });
    let storyboard_service = Arc::new(crate::storyboard::StoryboardService::new(
        storyboard_dir,
        storyboard_settings,
        ffmpeg_path,
    ));
```

Merge `.merge(storyboard_routes::storyboard_routes())` on `Router::new()` alongside `.merge(hls_routes::hls_routes())`, and in the middleware layer that injects `default_mgr`:
```rust
        .layer(axum::middleware::from_fn(
            move |mut req: axum::extract::Request, next: axum::middleware::Next| {
                let default_mgr = hls_manager.clone();
                let default_sb = storyboard_service.clone();
                async move {
                    if req
                        .extensions()
                        .get::<Arc<crate::streaming::hls::HlsSessionManager>>()
                        .is_none()
                    {
                        req.extensions_mut().insert(default_mgr);
                    }
                    if req
                        .extensions()
                        .get::<Arc<crate::storyboard::StoryboardService>>()
                        .is_none()
                    {
                        req.extensions_mut().insert(default_sb);
                    }
                    next.run(req).await
                }
            },
        ))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kadr-server --test storyboard_routes_test`
Expected: PASS (4 tests pass).

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/api/storyboard_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/tests/storyboard_routes_test.rs
git commit -m "feat(storyboard): add storyboard manifest, sheet, and frame HTTP routes"
```

---

### Task 4: Ingest Worker Cache Cleanup on Delete, Optional Post-Scan Pre-Generation & Changelog

**Files:**
- Modify: `crates/kadr-ingest/src/watcher/worker.rs`
- Modify: `crates/kadr-server/src/main.rs`
- Modify: `crates/kadr-server/tests/storyboard_service_test.rs`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: `IngestWorker`, `IngestMessage`, `StoryboardService::purge_for_path`, `StoryboardService::get_or_enqueue_manifest`
- Produces:
  - `IngestWorker::with_delete_callback<F>(self, callback: F) -> Self` where `F: Fn(PathBuf) + Send + Sync + 'static`
  - `IngestWorker::with_upsert_callback<F>(self, callback: F) -> Self` where `F: Fn(Vec<MediaItem>) + Send + Sync + 'static`

- [ ] **Step 1: Add failing test in `crates/kadr-server/tests/storyboard_service_test.rs` verifying `IngestWorker` delete cleanup and `generate_on_scan` upsert hook**

Append to `crates/kadr-server/tests/storyboard_service_test.rs`:

```rust
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_ingest::watcher::{IngestMessage, IngestWorker};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use std::sync::Arc;

#[tokio::test]
async fn ingest_worker_pregenerates_on_scan_and_purges_on_delete() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("scanned.mp4");
    if !generate_test_video(&video_path, 6, 640, 360) {
        eprintln!("ffmpeg not available; skipping test");
        return;
    }

    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    lib_repo
        .insert(&kadr_core::models::Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: dir.path().to_path_buf(),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let settings = StoryboardSettings {
        enabled: true,
        max_concurrent: 1,
        interval_secs: 5,
        tile_width: 320,
        generate_on_scan: true,
    };
    let svc = Arc::new(StoryboardService::new(
        dir.path().join("storyboards"),
        settings,
        "ffmpeg".to_string(),
    ));

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let svc_for_upsert = svc.clone();
    let svc_for_delete = svc.clone();
    let worker = IngestWorker::new(rx, media_repo.clone())
        .with_upsert_callback(move |items| {
            if !svc_for_upsert.settings().generate_on_scan {
                return;
            }
            for item in items {
                let svc_clone = svc_for_upsert.clone();
                tokio::spawn(async move {
                    let _ = svc_clone
                        .get_or_enqueue_manifest(
                            item.id.unwrap_or(0),
                            &item.file_path,
                            item.technical.duration_seconds,
                        )
                        .await;
                });
            }
        })
        .with_delete_callback(move |path| {
            let svc_clone = svc_for_delete.clone();
            tokio::spawn(async move {
                let _ = svc_clone.purge_for_path(&path).await;
            });
        });
    let worker_handle = tokio::spawn(worker.run());

    tx.send(IngestMessage::Upsert(
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Scanned".to_string(),
            file_path: video_path.clone(),
            file_name: "scanned.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 6,
                video_codec: Some("h264".to_string()),
                container: Some("mp4".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
            ..Default::default()
        },
        Vec::new(),
    ))
    .await
    .unwrap();

    // Wait for background scan pre-generation to finish
    let mut ready = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if svc.storyboard_generation_count() >= 1 {
            if let StoryboardStatus::Ready(_) =
                svc.get_or_enqueue_manifest(1, &video_path, 6).await.unwrap()
            {
                ready = true;
                break;
            }
        }
    }
    assert!(ready, "storyboard should be pre-generated after scan upsert");

    let path_hash = StoryboardService::path_hash(&video_path);
    let item_cache_dir = dir.path().join("storyboards").join("items").join(&path_hash);
    assert!(item_cache_dir.exists());

    // Now send Delete and confirm cache directory is removed
    tx.send(IngestMessage::Delete(video_path.clone()))
        .await
        .unwrap();
    drop(tx);
    worker_handle.await.unwrap();

    for _ in 0..20 {
        if !item_cache_dir.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !item_cache_dir.exists(),
        "storyboard cache should be deleted when item is removed"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p kadr-server --test storyboard_service_test`
Expected: FAIL (`no method named with_upsert_callback found for struct IngestWorker`).

- [ ] **Step 3: Add `with_delete_callback` and `with_upsert_callback` to `IngestWorker`, wire in `crates/kadr-server/src/main.rs`, and update `CHANGELOG.md`**

In `crates/kadr-ingest/src/watcher/worker.rs`:
1. Add two optional fields to `IngestWorker`:
```rust
    delete_callback: Option<Arc<dyn Fn(PathBuf) + Send + Sync>>,
    upsert_callback: Option<Arc<dyn Fn(Vec<MediaItem>) + Send + Sync>>,
```
Initialize both to `None` in `new` and `new_with_subtitles`.
2. Add builder methods on `IngestWorker`:
```rust
    pub fn with_delete_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(PathBuf) + Send + Sync + 'static,
    {
        self.delete_callback = Some(Arc::new(callback));
        self
    }

    pub fn with_upsert_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(Vec<MediaItem>) + Send + Sync + 'static,
    {
        self.upsert_callback = Some(Arc::new(callback));
        self
    }
```
3. In `IngestWorker::run`, inside `Some(IngestMessage::Delete(path)) =>`:
```rust
                        Some(IngestMessage::Delete(path)) => {
                            batch.retain(|(item, _)| item.file_path != path);
                            if let Err(e) = self.repo.delete_by_path(&path).await {
                                error!(path = ?path, error = ?e, "Failed to delete removed media file");
                            }
                            if let Some(ref cb) = self.delete_callback {
                                cb(path);
                            }
                        }
```
4. In `IngestWorker::flush_batch`, inside `Ok(count) =>` after `info!(count = count, "Successfully ingested media batch");`:
```rust
                if let Some(ref upsert_cb) = self.upsert_callback {
                    upsert_cb(items.clone());
                }
```

In `crates/kadr-server/src/main.rs`, construct `storyboard_service` before `IngestWorker::new` and wire `with_delete_callback` and `with_upsert_callback`:
```rust
    let storyboard_service = Arc::new(kadr_server::storyboard::StoryboardService::new(
        config.server.data_dir.join("storyboards"),
        config.storyboard.clone(),
        config.transcoding.ffmpeg_path.clone(),
    ));

    let (ingest_tx, ingest_rx) = tokio::sync::mpsc::channel(200);
    let bus_for_worker = event_bus.clone();
    let sb_for_delete = storyboard_service.clone();
    let sb_for_upsert = storyboard_service.clone();
    let worker = IngestWorker::new(ingest_rx, media_repo.clone())
        .with_subtitles(subtitle_repo.clone())
        .with_event_callback(move |ev| {
            bus_for_worker.publish(ev);
        })
        .with_delete_callback(move |path| {
            let sb = sb_for_delete.clone();
            tokio::spawn(async move {
                let _ = sb.purge_for_path(&path).await;
            });
        })
        .with_upsert_callback(move |items| {
            if !sb_for_upsert.settings().generate_on_scan || !sb_for_upsert.is_available() {
                return;
            }
            for item in items {
                if item.technical.duration_seconds > 0 && item.plugin_ref().is_none() {
                    let sb = sb_for_upsert.clone();
                    tokio::spawn(async move {
                        let _ = sb
                            .get_or_enqueue_manifest(
                                item.id.unwrap_or(0),
                                &item.file_path,
                                item.technical.duration_seconds,
                            )
                            .await;
                    });
                }
            }
        });
```

In `CHANGELOG.md`, add under `## [Unreleased]` -> `### Added`:
```markdown
- Seek preview thumbnails (trickplay): storyboard sprite sheet manifest (`GET /api/v1/media/{item_id}/storyboard`), cached 10x10 JPEG sprite sheets (`GET /api/v1/media/{item_id}/storyboard/{index}.jpg`), single-frame fallback (`GET /api/v1/media/{item_id}/frame`), and `[storyboard]` configuration settings.
```

- [ ] **Step 4: Run full workspace tests and clippy**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: All unit, integration, and doc tests PASS with 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest/src/watcher/worker.rs crates/kadr-server/src/main.rs crates/kadr-server/tests/storyboard_service_test.rs CHANGELOG.md
git commit -m "feat(storyboard): wire ingest cache cleanup, scan pre-generation, and changelog"
```
