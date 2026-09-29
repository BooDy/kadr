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
