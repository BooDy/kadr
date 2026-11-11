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
