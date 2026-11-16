use kadr_core::{is_video_extension, is_video_path, video_mime_for_extension, VIDEO_EXTENSIONS};
use std::path::Path;

#[test]
fn test_video_extensions_constant() {
    assert_eq!(
        VIDEO_EXTENSIONS,
        &["mkv", "mp4", "m4v", "mov", "webm", "avi", "ts", "m2ts", "mts", "3gp", "3g2"]
    );
}

#[test]
fn test_is_video_extension_case_insensitive_and_leading_dot() {
    for &ext in VIDEO_EXTENSIONS {
        assert!(is_video_extension(ext), "expected {ext} to be recognized");
        assert!(
            is_video_extension(&ext.to_ascii_uppercase()),
            "expected uppercase {ext} to be recognized"
        );
        assert!(
            is_video_extension(&format!(".{ext}")),
            "expected .{ext} to be recognized"
        );
    }

    for non_video in [
        "", "srt", "vtt", "ass", "nfo", "jpg", "png", "mp3", "flac", "..mp4",
    ] {
        assert!(
            !is_video_extension(non_video),
            "expected {non_video:?} to be rejected"
        );
    }
}

#[test]
fn test_is_video_path() {
    for &ext in VIDEO_EXTENSIONS {
        let lower = format!("/media/library/Sample.2024.{ext}");
        let upper = format!("/media/library/Sample.2024.{}", ext.to_ascii_uppercase());
        assert!(is_video_path(Path::new(&lower)));
        assert!(is_video_path(Path::new(&upper)));
    }

    assert!(!is_video_path(Path::new("/media/library/.mkv")));
    assert!(!is_video_path(Path::new("/media/library/no_extension")));
    assert!(!is_video_path(Path::new("/media/library/movie.en.srt")));
    assert!(!is_video_path(Path::new("/media/library/poster.jpg")));
}

#[test]
fn test_video_mime_for_extension() {
    let cases = [
        ("mp4", "video/mp4"),
        ("MP4", "video/mp4"),
        (".mp4", "video/mp4"),
        ("m4v", "video/mp4"),
        ("M4V", "video/mp4"),
        ("mov", "video/quicktime"),
        ("MOV", "video/quicktime"),
        ("mkv", "video/x-matroska"),
        ("MKV", "video/x-matroska"),
        ("webm", "video/webm"),
        ("WEBM", "video/webm"),
        ("avi", "video/x-msvideo"),
        ("AVI", "video/x-msvideo"),
        ("ts", "video/mp2t"),
        ("TS", "video/mp2t"),
        ("m2ts", "video/mp2t"),
        ("M2TS", "video/mp2t"),
        ("mts", "video/mp2t"),
        ("MTS", "video/mp2t"),
        ("3gp", "video/3gpp"),
        ("3GP", "video/3gpp"),
        ("3g2", "video/3gpp2"),
        ("3G2", "video/3gpp2"),
        ("mp3", "application/octet-stream"),
        ("srt", "application/octet-stream"),
        ("", "application/octet-stream"),
    ];

    for (ext, expected_mime) in cases {
        assert_eq!(
            video_mime_for_extension(ext),
            expected_mime,
            "unexpected MIME for {ext:?}"
        );
    }
}
