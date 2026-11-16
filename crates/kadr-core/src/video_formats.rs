use std::path::Path;

/// Canonical list of video file extensions supported across Kadr ingest, browsing,
/// subtitle discovery/probing, and streaming.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "mov", "webm", "avi", "ts", "m2ts", "mts", "3gp", "3g2",
];

/// Returns `true` if `ext` (with or without a leading `.`) matches a supported video extension
/// case-insensitively.
pub fn is_video_extension(ext: &str) -> bool {
    let normalized = ext.strip_prefix('.').unwrap_or(ext);
    VIDEO_EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(normalized))
}

/// Returns `true` if `path` has a file extension matching a supported video format.
pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(is_video_extension)
}

/// Maps a video file extension or container identifier (case-insensitively, with or without a
/// leading `.`) to its HTTP `Content-Type` MIME string, falling back to
/// `"application/octet-stream"`.
pub fn video_mime_for_extension(ext: &str) -> &'static str {
    let normalized = ext.strip_prefix('.').unwrap_or(ext).to_ascii_lowercase();
    match normalized.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        _ => "application/octet-stream",
    }
}
