use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::tempdir;

use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource, SubtitleTrack};
use kadr_server::subtitles::{SubtitleDeliveryService, SubtitleServiceError};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, SubtitleRepository};

async fn setup_test_context() -> (
    SubtitleDeliveryService,
    SubtitleRepository,
    MediaItemRepository,
    i64,               // media_item_id
    tempfile::TempDir, // cache_dir
    tempfile::TempDir, // media_dir
) {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
        })
        .await
        .unwrap();

    let media_dir = tempdir().unwrap();
    let video_path = media_dir.path().join("movie.mp4");
    tokio::fs::write(&video_path, b"dummy video content")
        .await
        .unwrap();

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Test Movie".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1000,
            file_path: video_path.clone(),
            file_name: "movie.mp4".to_string(),
            file_size: 19,
            technical: TechnicalInfo {
                duration_seconds: 120,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mp4".to_string()),
            },
            metadata: MediaMetadata::default(),
        }])
        .await
        .unwrap();

    let media_item = media_repo
        .find_by_path(&video_path)
        .await
        .unwrap()
        .expect("media item should exist");
    let media_item_id = media_item.id.expect("media item should have id");

    let cache_dir = tempdir().unwrap();
    let service = SubtitleDeliveryService::new(
        cache_dir.path().to_path_buf(),
        subtitle_repo.clone(),
        media_repo.clone(),
    );

    (
        service,
        subtitle_repo,
        media_repo,
        media_item_id,
        cache_dir,
        media_dir,
    )
}

#[tokio::test]
async fn test_conversion_srt_to_vtt_on_cache_miss_and_writing_to_cache() {
    let (service, subtitle_repo, _media_repo, media_item_id, cache_dir, media_dir) =
        setup_test_context().await;

    let srt_content = "\
1
00:00:01,000 --> 00:00:04,000
Hello <i>world</i>!

2
00:00:05,500 --> 00:00:08,000
Second subtitle line.
";
    let srt_path = media_dir.path().join("movie.en.srt");
    tokio::fs::write(&srt_path, srt_content).await.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: Some("English Sidecar".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(srt_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    // Cache should not have this subtitle yet
    let expected_cache_path = cache_dir.path().join(format!("{}.vtt", track_id));
    assert!(!expected_cache_path.exists());

    // Fetch WebVTT path
    let vtt_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("get_webvtt_path should succeed");

    assert_eq!(vtt_path, expected_cache_path);
    assert!(vtt_path.exists());

    let converted_content = tokio::fs::read_to_string(&vtt_path).await.unwrap();
    assert!(converted_content.starts_with("WEBVTT"));
    assert!(converted_content.contains("00:00:01.000 --> 00:00:04.000"));
    assert!(converted_content.contains("Hello <i>world</i>!"));
    assert!(converted_content.contains("00:00:05.500 --> 00:00:08.000"));
    assert!(converted_content.contains("Second subtitle line."));
}

#[tokio::test]
async fn test_fast_return_on_cached_hit() {
    let (service, subtitle_repo, _media_repo, media_item_id, _cache_dir, media_dir) =
        setup_test_context().await;

    let srt_content = "\
1
00:00:01,000 --> 00:00:04,000
Hit benchmark test line.
";
    let srt_path = media_dir.path().join("movie.benchmark.srt");
    tokio::fs::write(&srt_path, srt_content).await.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: None,
            format: SubtitleFormat::Srt,
            file_path: Some(srt_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    // Warm up / prime cache
    let first_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("first call should succeed");

    // Subsequent hit benchmark: must be < 5 ms
    let start = Instant::now();
    let second_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("second call should succeed");
    let elapsed = start.elapsed();

    assert_eq!(first_path, second_path);
    assert!(
        elapsed < Duration::from_millis(5),
        "Cached WebVTT hit took {:?}, which exceeds the 5ms budget",
        elapsed
    );
}

#[tokio::test]
async fn test_serving_vtt_sidecars_directly() {
    let (service, subtitle_repo, _media_repo, media_item_id, cache_dir, media_dir) =
        setup_test_context().await;

    let vtt_content = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nAlready WebVTT sidecar.\n";
    let vtt_file_path = media_dir.path().join("movie.vtt");
    tokio::fs::write(&vtt_file_path, vtt_content).await.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "fra".to_string(),
            title: Some("French VTT".to_string()),
            format: SubtitleFormat::Vtt,
            file_path: Some(vtt_file_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let vtt_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("get_webvtt_path should succeed for VTT sidecar");

    assert!(vtt_path.exists());
    let cached_expected = cache_dir.path().join(format!("{}.vtt", track_id));
    assert!(cached_expected.exists());

    let content = tokio::fs::read_to_string(&vtt_path).await.unwrap();
    assert!(content.starts_with("WEBVTT"));
    assert!(content.contains("Already WebVTT sidecar."));
}

#[tokio::test]
async fn test_clean_error_when_subtitle_id_or_disk_file_is_missing() {
    let (service, subtitle_repo, _media_repo, media_item_id, _cache_dir, media_dir) =
        setup_test_context().await;

    // 1. Missing subtitle track in database
    let err_missing_id = service.get_webvtt_path(9999).await;
    assert!(
        matches!(err_missing_id, Err(SubtitleServiceError::NotFound)),
        "Expected SubtitleServiceError::NotFound, got {:?}",
        err_missing_id
    );

    // 2. Track exists, but file_path is None
    let track_none_path = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Embedded,
            language: "eng".to_string(),
            title: Some("Embedded Track No File".to_string()),
            format: SubtitleFormat::Srt,
            file_path: None,
            stream_index: Some(2),
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let err_none_path = service.get_webvtt_path(track_none_path).await;
    assert!(
        matches!(err_none_path, Err(SubtitleServiceError::SourceFileNotFound)),
        "Expected SubtitleServiceError::SourceFileNotFound for None file_path, got {:?}",
        err_none_path
    );

    // 3. Track exists, but file on disk is deleted / missing
    let missing_disk_path = media_dir.path().join("ghost_file.srt");
    let track_missing_disk = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: Some("Missing Disk File".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(missing_disk_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let err_missing_file = service.get_webvtt_path(track_missing_disk).await;
    assert!(
        matches!(
            err_missing_file,
            Err(SubtitleServiceError::SourceFileNotFound)
        ),
        "Expected SubtitleServiceError::SourceFileNotFound for missing file, got {:?}",
        err_missing_file
    );
}

#[tokio::test]
async fn test_fallback_lossy_utf8_reading() {
    let (service, subtitle_repo, _media_repo, media_item_id, cache_dir, media_dir) =
        setup_test_context().await;

    // Byte 0xE9 is 'é' in Windows-1252 / ISO-8859-1, but invalid UTF-8 sequence alone
    let raw_bytes: &[u8] = b"1\n00:00:01,000 --> 00:00:04,000\nCaf\xe9 and movie\n";
    let srt_path = media_dir.path().join("lossy.srt");
    tokio::fs::write(&srt_path, raw_bytes).await.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "fre".to_string(),
            title: Some("French Lossy".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(srt_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let vtt_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("get_webvtt_path should succeed even with non-UTF-8 characters via lossy decode");

    assert_eq!(vtt_path, cache_dir.path().join(format!("{}.vtt", track_id)));
    let content = tokio::fs::read_to_string(&vtt_path).await.unwrap();
    assert!(content.starts_with("WEBVTT"));
    assert!(content.contains("Caf\u{FFFD} and movie"));
}

#[tokio::test]
async fn test_delete_track_cleans_db_cache_and_downloaded_source() {
    let (service, subtitle_repo, _media_repo, media_item_id, cache_dir, media_dir) =
        setup_test_context().await;

    // Case 1: SubtitleSource::Downloaded
    let downloaded_srt_path = media_dir.path().join("downloaded_1.srt");
    tokio::fs::write(
        &downloaded_srt_path,
        "1\n00:00:01,000 --> 00:00:02,000\nDownloaded subtitle\n",
    )
    .await
    .unwrap();

    let downloaded_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Downloaded,
            language: "ara".to_string(),
            title: Some("Arabic Downloaded".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(downloaded_srt_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    // Populate cache
    let cached_vtt = service.get_webvtt_path(downloaded_id).await.unwrap();
    assert!(cached_vtt.exists());
    assert!(downloaded_srt_path.exists());

    // Delete track
    let deleted = service
        .delete_track(downloaded_id)
        .await
        .expect("delete_track should succeed");
    assert!(deleted);

    // Verify removed from DB
    let track_in_db = subtitle_repo.find_by_id(downloaded_id).await.unwrap();
    assert!(track_in_db.is_none());

    // Verify cache file deleted
    assert!(!cached_vtt.exists());

    // Verify downloaded source file deleted
    assert!(!downloaded_srt_path.exists());

    // Case 2: SubtitleSource::Sidecar (Sidecar source file should NOT be deleted)
    let sidecar_srt_path = media_dir.path().join("sidecar_keep.srt");
    tokio::fs::write(
        &sidecar_srt_path,
        "1\n00:00:01,000 --> 00:00:02,000\nSidecar subtitle\n",
    )
    .await
    .unwrap();

    let sidecar_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: Some("Sidecar Preserve".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(sidecar_srt_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let sidecar_cached_vtt = service.get_webvtt_path(sidecar_id).await.unwrap();
    assert!(sidecar_cached_vtt.exists());
    assert!(sidecar_srt_path.exists());

    let deleted_sidecar = service
        .delete_track(sidecar_id)
        .await
        .expect("delete_track for sidecar should succeed");
    assert!(deleted_sidecar);

    // Verify removed from DB
    assert!(subtitle_repo
        .find_by_id(sidecar_id)
        .await
        .unwrap()
        .is_none());

    // Verify cache file removed
    let sidecar_cache_path = cache_dir.path().join(format!("{}.vtt", sidecar_id));
    assert!(!sidecar_cache_path.exists());

    // CRITICAL: Sidecar source file MUST still exist!
    assert!(sidecar_srt_path.exists());

    // Case 3: Non-existent track ID
    let deleted_missing = service
        .delete_track(88888)
        .await
        .expect("delete_track on missing track should succeed returning false");
    assert!(!deleted_missing);
}
