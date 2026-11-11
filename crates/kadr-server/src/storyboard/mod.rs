pub mod math;
pub mod service;

pub use math::{
    build_manifest, clamp_tile_width, compute_storyboard_plan, parse_jpeg_dimensions,
    rewrite_manifest_urls, snap_frame_time, StoryboardManifest, StoryboardPlan, StoryboardSheet,
    STORYBOARD_COLUMNS, STORYBOARD_ROWS, TILES_PER_SHEET,
};
pub use service::{FrameStatus, SheetStatus, StoryboardService, StoryboardStatus};

use kadr_core::models::{MediaItem, MediaType};

pub fn has_video_stream(item: &MediaItem) -> bool {
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
