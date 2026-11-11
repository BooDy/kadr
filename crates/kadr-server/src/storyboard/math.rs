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
        base_secs.clamp(1, 5)
    } else {
        base_secs
    };

    let min_secs_for_cap = duration_u64.div_ceil(MAX_TARGET_TILES).max(1);
    let interval_secs = initial_secs.max(min_secs_for_cap);
    let interval_ms = interval_secs * 1000;

    let total_tiles = (duration_u64 * 1000).div_ceil(interval_ms).max(1) as u32;
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
        if matches!(marker, 0xC0..=0xC2) {
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
        assert_eq!(snap_frame_time(10.0, 0), None);
        assert_eq!(snap_frame_time(10.0, -1), None);
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

    #[test]
    fn build_manifest_respects_actual_sheet_count_and_tile_height() {
        let plan = compute_storyboard_plan(3600, 10, 320).unwrap();
        // 3600s / 10s = 360 tiles -> 4 sheets expected
        assert_eq!(plan.sheet_count, 4);

        // Suppose ffmpeg only produced 2 sheets (e.g., short/truncated media)
        let manifest = build_manifest(99, &plan, 135, Some(2));
        assert_eq!(manifest.tile_height, 134); // clamped & rounded down to even
        assert_eq!(manifest.sheets.len(), 2);
        assert_eq!(manifest.total_tiles, 200);
        assert_eq!(manifest.sheets[1].tile_count, 100);
    }

    #[test]
    fn rewrite_manifest_urls_updates_item_id() {
        let plan = compute_storyboard_plan(60, 10, 320).unwrap();
        let manifest = build_manifest(1, &plan, 180, None);
        assert_eq!(manifest.sheets[0].url, "/api/v1/media/1/storyboard/0.jpg");

        let rewritten = rewrite_manifest_urls(manifest, 999);
        assert_eq!(
            rewritten.sheets[0].url,
            "/api/v1/media/999/storyboard/0.jpg"
        );
    }

    #[test]
    fn jpeg_sof_dimension_parser_handles_edge_cases() {
        // Empty bytes
        assert_eq!(parse_jpeg_dimensions(&[]), None);
        // Non-JPEG bytes
        assert_eq!(parse_jpeg_dimensions(&[0x00, 0x00, 0x00, 0x00]), None);
        // Just SOI
        assert_eq!(parse_jpeg_dimensions(&[0xFF, 0xD8]), None);

        // SOF2 (progressive) marker
        let bytes: Vec<u8> = vec![
            0xFF, 0xD8, // SOI
            0xFF, 0xC2, 0x00, 0x0B, // SOF2 length 11
            0x08, // precision
            0x00, 0xC8, // height = 200
            0x01, 0x40, // width = 320
            0x01, 0x01, 0x11, 0x00, // component info
            0xFF, 0xD9,
        ];
        assert_eq!(parse_jpeg_dimensions(&bytes), Some((320, 200)));

        // Truncated segment
        let truncated: Vec<u8> = vec![
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x20, // claims 32 bytes but file ends
            0x08,
        ];
        assert_eq!(parse_jpeg_dimensions(&truncated), None);
    }
}
