use kadr_core::models::{Library, MediaType};
use kadr_ingest::thumbnail::ThumbnailExtractor;
use kadr_ingest::watcher::pipeline::IngestPipeline;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

fn generate_test_video(path: &std::path::Path) -> bool {
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=10",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            path.to_str().unwrap(),
        ])
        .output();

    matches!(output, Ok(out) if out.status.success())
}

#[test]
fn test_calculate_seek_seconds() {
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(-10), 15.0);
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(0), 15.0);
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(30), 5.0);
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(50), 5.0);
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(200), 20.0);
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(300), 30.0);
    assert_eq!(ThumbnailExtractor::calculate_seek_seconds(600), 30.0);
}

#[test]
fn test_cache_path_deterministic() {
    let dir = tempdir().unwrap();
    let extractor = ThumbnailExtractor::new(dir.path().to_path_buf());

    let path1 = dir.path().join("movie1.mp4");
    let path2 = dir.path().join("movie2.mp4");

    let cache1 = extractor.cache_path(&path1);
    let cache2 = extractor.cache_path(&path1);
    let cache3 = extractor.cache_path(&path2);

    assert_eq!(cache1, cache2);
    assert_ne!(cache1, cache3);
    assert_eq!(cache1.extension().and_then(|e| e.to_str()), Some("jpg"));
    assert_eq!(cache1.parent(), Some(dir.path()));
}

#[tokio::test]
async fn test_extract_thumbnail_non_existent_file() {
    let dir = tempdir().unwrap();
    let thumbs_dir = dir.path().join("thumbnails");
    let extractor = ThumbnailExtractor::new(thumbs_dir);

    let non_existent = dir.path().join("missing.mp4");
    let result = extractor
        .extract_thumbnail(&non_existent, 100)
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn test_extract_thumbnail_success_and_cache_reuse() {
    let dir = tempdir().unwrap();
    let thumbs_dir = dir.path().join("thumbnails");
    let extractor = ThumbnailExtractor::new(thumbs_dir.clone());

    let video_path = dir.path().join("sample.mp4");
    if !generate_test_video(&video_path) {
        eprintln!("ffmpeg not available or failed; skipping video test");
        return;
    }

    let thumb_path = extractor
        .extract_thumbnail(&video_path, 2)
        .await
        .unwrap()
        .expect("Thumbnail should be generated");

    assert!(thumb_path.exists());
    let meta = std::fs::metadata(&thumb_path).unwrap();
    assert!(meta.len() > 0);

    // Second call should reuse cached thumbnail
    let cached = extractor
        .extract_thumbnail(&video_path, 2)
        .await
        .unwrap()
        .expect("Thumbnail should be returned from cache");

    assert_eq!(thumb_path, cached);
}

#[tokio::test]
async fn test_pipeline_with_thumbnails_dir_generates_poster() {
    let dir = tempdir().unwrap();
    let thumbs_dir = dir.path().join("thumbnails");
    let video_path = dir.path().join("Movie.2023.1080p.mkv");

    if !generate_test_video(&video_path) {
        eprintln!("ffmpeg not available or failed; skipping pipeline test");
        return;
    }

    let library = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new(true, Some(thumbs_dir.clone()));
    let (item, _) = pipeline
        .process_file(&library, &video_path)
        .await
        .unwrap()
        .expect("Should process");

    assert!(item.metadata.poster_path.is_some());
    let poster = item.metadata.poster_path.unwrap();
    let poster_path = std::path::PathBuf::from(&poster);
    assert!(poster_path.exists());
    assert_eq!(poster_path.parent(), Some(thumbs_dir.as_path()));
}

#[tokio::test]
async fn test_pipeline_prefers_existing_folder_artwork() {
    let dir = tempdir().unwrap();
    let thumbs_dir = dir.path().join("thumbnails");
    let video_path = dir.path().join("Movie.2023.1080p.mkv");
    let folder_poster = dir.path().join("poster.jpg");

    // Create a dummy poster.jpg
    File::create(&folder_poster)
        .unwrap()
        .write_all(b"fake poster")
        .unwrap();

    if !generate_test_video(&video_path) {
        eprintln!("ffmpeg not available or failed; skipping pipeline test");
        return;
    }

    let library = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new(true, Some(thumbs_dir));
    let (item, _) = pipeline
        .process_file(&library, &video_path)
        .await
        .unwrap()
        .expect("Should process");

    assert_eq!(
        item.metadata.poster_path.as_deref(),
        Some(folder_poster.to_str().unwrap())
    );
}

#[tokio::test]
async fn test_pipeline_without_thumbnails_dir_leaves_poster_none() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("Movie.2023.1080p.mkv");

    if !generate_test_video(&video_path) {
        eprintln!("ffmpeg not available or failed; skipping pipeline test");
        return;
    }

    let library = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new(true, None);
    let (item, _) = pipeline
        .process_file(&library, &video_path)
        .await
        .unwrap()
        .expect("Should process");

    assert_eq!(item.metadata.poster_path, None);
}
