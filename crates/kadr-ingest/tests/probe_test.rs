use kadr_ingest::probe::TechnicalProber;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

#[tokio::test]
async fn test_pure_rust_container_detection() {
    let dir = tempdir().unwrap();
    let mkv_path = dir.path().join("sample.mkv");
    // Write Matroska EBML header magic: 0x1A, 0x45, 0xDF, 0xA3
    let mut file = File::create(&mkv_path).unwrap();
    file.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00])
        .unwrap();

    let prober = TechnicalProber::new(false); // disable ffprobe for pure rust test
    let info = prober.probe(&mkv_path).await.unwrap();

    assert_eq!(info.container.as_deref(), Some("mkv"));
}

#[tokio::test]
async fn test_pure_rust_mp4_detection() {
    let dir = tempdir().unwrap();
    let mp4_path = dir.path().join("sample.unknown");
    let mut file = File::create(&mp4_path).unwrap();
    // ftyp box at offset 4: 00 00 00 20 'f' 't' 'y' 'p'
    file.write_all(&[0x00, 0x00, 0x00, 0x20, b'f', b't', b'y', b'p'])
        .unwrap();

    let prober = TechnicalProber::new(false);
    let info = prober.probe(&mp4_path).await.unwrap();

    assert_eq!(info.container.as_deref(), Some("mp4"));
}

#[tokio::test]
async fn test_pure_rust_extension_fallback() {
    let dir = tempdir().unwrap();
    let avi_path = dir.path().join("sample.avi");
    let mut file = File::create(&avi_path).unwrap();
    // Non-matching header
    file.write_all(&[0x01, 0x02, 0x03, 0x04]).unwrap();

    let prober = TechnicalProber::new(false);
    let info = prober.probe(&avi_path).await.unwrap();

    assert_eq!(info.container.as_deref(), Some("avi"));
}

#[tokio::test]
async fn test_pure_rust_empty_file_handling() {
    let dir = tempdir().unwrap();
    let empty_path = dir.path().join("empty.mkv");
    File::create(&empty_path).unwrap();

    let prober = TechnicalProber::new(false);
    let info = prober.probe(&empty_path).await.unwrap();

    // Fallback to extension since file has 0 bytes
    assert_eq!(info.container.as_deref(), Some("mkv"));
}

#[tokio::test]
async fn test_pure_rust_truncated_file_handling() {
    let dir = tempdir().unwrap();
    let trunc_path = dir.path().join("truncated");
    let mut file = File::create(&trunc_path).unwrap();
    file.write_all(&[0x1A, 0x45]).unwrap(); // Less than 4 bytes, no extension

    let prober = TechnicalProber::new(false);
    let info = prober.probe(&trunc_path).await.unwrap();

    assert_eq!(info.container, None);
}

#[tokio::test]
async fn test_pure_rust_nonexistent_file() {
    let dir = tempdir().unwrap();
    let nonexistent = dir.path().join("does_not_exist.mkv");

    let prober = TechnicalProber::new(false);
    let info = prober.probe(&nonexistent).await.unwrap();

    // File::open fails, gracefully returns default TechnicalInfo
    assert_eq!(info.container, None);
}

#[tokio::test]
async fn test_technical_prober_debug_and_clone() {
    let prober = TechnicalProber::new(true);
    let cloned = prober.clone();
    let debug_str = format!("{:?}", cloned);
    assert!(debug_str.contains("TechnicalProber"));
}

#[tokio::test]
async fn test_ffprobe_failure_fallback_to_pure_rust() {
    let dir = tempdir().unwrap();
    let mkv_path = dir.path().join("invalid.mkv");
    let mut file = File::create(&mkv_path).unwrap();
    file.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00])
        .unwrap();

    // With enable_ffprobe = true, but ffprobe fails on invalid file contents:
    let prober = TechnicalProber::new(true);
    let info = prober.probe(&mkv_path).await.unwrap();

    // Container still detected by pure Rust
    assert_eq!(info.container.as_deref(), Some("mkv"));
    // Other fields remain default
    assert_eq!(info.duration_seconds, 0);
    assert_eq!(info.video_codec, None);
}

#[tokio::test]
async fn test_ffprobe_real_media_file_inspection() {
    // Check if ffmpeg and ffprobe are available
    let ffmpeg_available = Command::new("ffmpeg").arg("-version").output().is_ok();
    let ffprobe_available = Command::new("ffprobe").arg("-version").output().is_ok();

    if !ffmpeg_available || !ffprobe_available {
        eprintln!(
            "Skipping test_ffprobe_real_media_file_inspection: ffmpeg or ffprobe not available"
        );
        return;
    }

    let dir = tempdir().unwrap();
    let test_video = dir.path().join("test_video.mkv");

    // Generate a 1-second 1280x720 video with a sine wave audio track
    let gen_status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=1280x720:rate=10",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=1:frequency=440",
            "-c:v",
            "ffv1",
            "-c:a",
            "flac",
            test_video.to_str().unwrap(),
        ])
        .output();

    if let Ok(output) = gen_status {
        if !output.status.success() {
            eprintln!(
                "Failed to generate test video: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
    } else {
        return;
    }

    let prober = TechnicalProber::new(true);
    let info = prober.probe(&test_video).await.unwrap();

    assert_eq!(info.container.as_deref(), Some("mkv"));
    assert_eq!(info.resolution.as_deref(), Some("720p"));
    assert_eq!(info.video_codec.as_deref(), Some("ffv1"));
    assert_eq!(info.audio_codec.as_deref(), Some("flac"));
    assert_eq!(info.audio_channels, Some(1));
    assert!(info.duration_seconds >= 1);
}

#[tokio::test]
async fn test_ffprobe_attached_pic_ignored() {
    let ffmpeg_available = Command::new("ffmpeg").arg("-version").output().is_ok();
    let ffprobe_available = Command::new("ffprobe").arg("-version").output().is_ok();

    if !ffmpeg_available || !ffprobe_available {
        eprintln!("Skipping test_ffprobe_attached_pic_ignored: ffmpeg or ffprobe not available");
        return;
    }

    let dir = tempdir().unwrap();
    let test_audio = dir.path().join("test_with_cover.flac");

    // Generate audio with attached picture
    let gen_status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=1",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=300x300:d=1",
            "-map",
            "0:a",
            "-map",
            "1:v",
            "-c:a",
            "flac",
            "-c:v",
            "png",
            "-disposition:v:0",
            "attached_pic",
            test_audio.to_str().unwrap(),
        ])
        .output();

    if let Ok(output) = gen_status {
        if !output.status.success() {
            eprintln!(
                "Failed to generate test audio with cover: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
    } else {
        return;
    }

    let prober = TechnicalProber::new(true);
    let info = prober.probe(&test_audio).await.unwrap();

    // Attached picture video stream must be ignored
    assert_eq!(info.video_codec, None);
    assert_eq!(info.resolution, None);
    assert_eq!(info.audio_codec.as_deref(), Some("flac"));
    assert!(info.duration_seconds >= 1);
}

use kadr_core::subtitles::{SubtitleFormat, SubtitleSource};
use kadr_ingest::probe::{
    ffprobe::parse_embedded_subtitles_from_json, probe_embedded_subtitles,
};
use serde_json::json;

#[test]
fn test_parse_embedded_subtitles_filters_bitmap_and_normalizes_metadata() {
    let sample = json!({
        "streams": [
            { "index": 0, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080 },
            { "index": 1, "codec_type": "audio", "codec_name": "aac", "channels": 2 },
            {
                "index": 2,
                "codec_type": "subtitle",
                "codec_name": "subrip",
                "disposition": { "default": 1, "forced": 0, "hearing_impaired": 0 },
                "tags": { "language": "ar", "title": "Arabic [Official]" }
            },
            {
                "index": 3,
                "codec_type": "subtitle",
                "codec_name": "ass",
                "disposition": { "default": 0, "forced": 1, "hearing_impaired": 1 },
                "tags": { "language": "en" }
            },
            {
                "index": 4,
                "codec_type": "subtitle",
                "codec_name": "hdmv_pgs_subtitle",
                "disposition": { "default": 0, "forced": 0 },
                "tags": { "language": "eng", "title": "PGS BluRay" }
            }
        ]
    });

    let subs = parse_embedded_subtitles_from_json(&sample);
    assert_eq!(subs.len(), 2, "PGS bitmap stream must be skipped");

    assert_eq!(subs[0].source, SubtitleSource::Embedded);
    assert_eq!(subs[0].stream_index, Some(2));
    assert_eq!(subs[0].language, "ara");
    assert_eq!(subs[0].title.as_deref(), Some("Arabic [Official]"));
    assert_eq!(subs[0].format, SubtitleFormat::Srt);
    assert!(subs[0].is_default);
    assert!(!subs[0].is_forced);
    assert!(subs[0].file_path.is_none());

    assert_eq!(subs[1].source, SubtitleSource::Embedded);
    assert_eq!(subs[1].stream_index, Some(3));
    assert_eq!(subs[1].language, "eng");
    assert_eq!(subs[1].title.as_deref(), Some("English [SDH] [Forced]"));
    assert_eq!(subs[1].format, SubtitleFormat::Ass);
    assert!(!subs[1].is_default);
    assert!(subs[1].is_forced);
}

#[tokio::test]
async fn test_probe_embedded_subtitles_on_real_mkv() {
    let dir = tempdir().unwrap();
    let srt_ar = dir.path().join("ar.srt");
    let srt_en = dir.path().join("en.srt");
    std::fs::write(&srt_ar, "1\n00:00:00,500 --> 00:00:01,500\nمرحبا!\n").unwrap();
    std::fs::write(&srt_en, "1\n00:00:00,500 --> 00:00:01,500\nHello!\n").unwrap();

    let mkv_path = dir.path().join("episode.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:d=2",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo:d=2",
            "-i",
            srt_ar.to_str().unwrap(),
            "-i",
            srt_en.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:s:0",
            "-map",
            "3:s:0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-c:s",
            "srt",
            "-metadata:s:s:0",
            "language=ara",
            "-metadata:s:s:0",
            "title=Arabic",
            "-disposition:s:0",
            "default",
            "-metadata:s:s:1",
            "language=eng",
            "-disposition:s:1",
            "0",
            mkv_path.to_str().unwrap(),
        ])
        .status();

    let Ok(st) = status else { return };
    if !st.success() {
        return;
    }

    let subs = probe_embedded_subtitles(&mkv_path).await.unwrap();
    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0].language, "ara");
    assert_eq!(subs[0].stream_index, Some(2));
    assert!(subs[0].is_default);
    assert_eq!(subs[1].language, "eng");
    assert_eq!(subs[1].stream_index, Some(3));
}
