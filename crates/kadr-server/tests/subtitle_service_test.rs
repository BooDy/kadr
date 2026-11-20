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
        .insert(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/tmp/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
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
                ..Default::default()
            },
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
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

    // Remove source SRT so second call deterministically proves cached WebVTT fast-path
    tokio::fs::remove_file(&srt_path).await.unwrap();

    let start = Instant::now();
    let second_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("second call should succeed from cache");
    let elapsed = start.elapsed();

    assert_eq!(first_path, second_path);
    assert!(
        elapsed < Duration::from_millis(2000),
        "Cached WebVTT hit took {:?}, exceeding 2000ms debug-build ceiling",
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
            stream_index: None,
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

#[tokio::test]
async fn test_embedded_mkv_extraction_and_caching() {
    let (service, subtitle_repo, media_repo, _media_item_id, cache_dir, media_dir) =
        setup_test_context().await;

    let srt_ar = media_dir.path().join("ar_sub.srt");
    tokio::fs::write(
        &srt_ar,
        "1\n00:00:00,500 --> 00:00:01,500\nمرحبا بالمشاهد!\n",
    )
    .await
    .unwrap();

    let mkv_path = media_dir.path().join("movie_embedded.mkv");
    let status = std::process::Command::new("ffmpeg")
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
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:s:0",
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
            "title=Arabic Embedded",
            "-disposition:s:0",
            "default",
            mkv_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should create mkv");
    assert!(status.success());

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "MKV Embedded Movie".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1000,
            file_path: mkv_path.clone(),
            file_name: "movie_embedded.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .unwrap();

    let mkv_item = media_repo.find_by_path(&mkv_path).await.unwrap().unwrap();
    let mkv_item_id = mkv_item.id.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id: mkv_item_id,
            source: SubtitleSource::Embedded,
            language: "ara".to_string(),
            title: Some("Arabic Embedded".to_string()),
            format: SubtitleFormat::Srt,
            file_path: None,
            stream_index: Some(2),
            is_default: true,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let cached_expected = cache_dir.path().join(format!("{}.vtt", track_id));
    assert!(!cached_expected.exists());

    // First call: extracts via ffmpeg
    let vtt_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("get_webvtt_path should extract embedded subtitle");

    assert_eq!(vtt_path, cached_expected);
    assert!(vtt_path.exists());
    let content = tokio::fs::read_to_string(&vtt_path).await.unwrap();
    assert!(content.starts_with("WEBVTT"));
    assert!(content.contains("مرحبا بالمشاهد!"));

    // Second call: remove source container to prove cache hit without re-running ffmpeg
    tokio::fs::remove_file(&mkv_path).await.unwrap();
    let start = Instant::now();
    let second_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("get_webvtt_path cache hit should succeed");
    let elapsed = start.elapsed();
    assert_eq!(second_path, vtt_path);
    assert!(elapsed < Duration::from_millis(2000));
}

#[tokio::test]
async fn test_singleflight_concurrent_extractions() {
    let (service, subtitle_repo, media_repo, _media_item_id, _cache_dir, media_dir) =
        setup_test_context().await;

    let srt_path = media_dir.path().join("singleflight.srt");
    tokio::fs::write(
        &srt_path,
        "1\n00:00:00,500 --> 00:00:01,500\nConcurrent Test\n",
    )
    .await
    .unwrap();

    let mkv_path = media_dir.path().join("singleflight.mkv");
    let status = std::process::Command::new("ffmpeg")
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
            srt_path.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:s:0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-c:s",
            "srt",
            mkv_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should create mkv");
    assert!(status.success());

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Singleflight Movie".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1000,
            file_path: mkv_path.clone(),
            file_name: "singleflight.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .unwrap();

    let mkv_item = media_repo.find_by_path(&mkv_path).await.unwrap().unwrap();
    let mkv_item_id = mkv_item.id.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id: mkv_item_id,
            source: SubtitleSource::Embedded,
            language: "und".to_string(),
            title: Some("Singleflight Track".to_string()),
            format: SubtitleFormat::Srt,
            file_path: None,
            stream_index: Some(2),
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let svc1 = service.clone();
    let svc2 = service.clone();
    let svc3 = service.clone();

    let (res1, res2, res3) = tokio::join!(
        svc1.get_webvtt_path(track_id),
        svc2.get_webvtt_path(track_id),
        svc3.get_webvtt_path(track_id),
    );

    let p1 = res1.expect("res1 should succeed");
    let p2 = res2.expect("res2 should succeed");
    let p3 = res3.expect("res3 should succeed");

    assert_eq!(p1, p2);
    assert_eq!(p2, p3);
    assert!(p1.exists());

    let content = tokio::fs::read_to_string(&p1).await.unwrap();
    assert!(content.starts_with("WEBVTT"));
    assert!(content.contains("Concurrent Test"));

    // Verify in-flight singleflight lock is completely cleaned up
    assert_eq!(
        service.in_flight_count(),
        0,
        "in_flight_extractions map should be empty after extractions finish"
    );
}

#[tokio::test]
async fn test_sidecar_ass_conversion_via_ffmpeg() {
    let (service, subtitle_repo, _media_repo, media_item_id, cache_dir, media_dir) =
        setup_test_context().await;

    let ass_content = "\
[Script Info]
ScriptType: v4.00+
PlayResX: 384
PlayResY: 288

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Styled ASS Subtitle!
";
    let ass_path = media_dir.path().join("movie.ass");
    tokio::fs::write(&ass_path, ass_content).await.unwrap();

    let track_id = subtitle_repo
        .create(&SubtitleTrack {
            id: 0,
            media_item_id,
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: Some("English ASS".to_string()),
            format: SubtitleFormat::Ass,
            file_path: Some(ass_path.to_string_lossy().to_string()),
            stream_index: None,
            is_default: false,
            is_forced: false,
            created_at: 1000,
        })
        .await
        .unwrap();

    let expected_cache_path = cache_dir.path().join(format!("{}.vtt", track_id));
    assert!(!expected_cache_path.exists());

    let vtt_path = service
        .get_webvtt_path(track_id)
        .await
        .expect("get_webvtt_path should convert ASS sidecar");

    assert_eq!(vtt_path, expected_cache_path);
    assert!(vtt_path.exists());

    let converted_content = tokio::fs::read_to_string(&vtt_path).await.unwrap();
    assert!(converted_content.starts_with("WEBVTT"));
    assert!(converted_content.contains("Styled ASS Subtitle!"));
}

#[tokio::test]
async fn test_singleflight_concurrent_list_or_probe_subtitles() {
    let (service, subtitle_repo, media_repo, _media_item_id, _cache_dir, media_dir) =
        setup_test_context().await;

    let srt_ar = media_dir.path().join("probe_ar.srt");
    let srt_en = media_dir.path().join("probe_en.srt");
    tokio::fs::write(&srt_ar, "1\n00:00:00,500 --> 00:00:01,500\nمرحبا!\n")
        .await
        .unwrap();
    tokio::fs::write(&srt_en, "1\n00:00:00,500 --> 00:00:01,500\nHello!\n")
        .await
        .unwrap();

    let mkv_path = media_dir.path().join("concurrent_probe.mkv");
    let status = std::process::Command::new("ffmpeg")
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
            "-disposition:s:0",
            "default",
            "-metadata:s:s:1",
            "language=eng",
            "-disposition:s:1",
            "0",
            mkv_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should create mkv");
    assert!(status.success());

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Concurrent Probe Movie".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1000,
            file_path: mkv_path.clone(),
            file_name: "concurrent_probe.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .unwrap();

    let mkv_item = media_repo.find_by_path(&mkv_path).await.unwrap().unwrap();
    let mkv_item_id = mkv_item.id.unwrap();

    // Verify 0 tracks initially
    let initial_tracks = subtitle_repo.find_by_media_item(mkv_item_id).await.unwrap();
    assert_eq!(initial_tracks.len(), 0);

    let svc1 = service.clone();
    let svc2 = service.clone();
    let item1 = mkv_item.clone();
    let item2 = mkv_item.clone();

    // Run simultaneous list_or_probe_subtitles calls
    let (res1, res2) = tokio::join!(
        svc1.list_or_probe_subtitles(&item1),
        svc2.list_or_probe_subtitles(&item2),
    );

    let tracks1 = res1.expect("res1 should succeed");
    let tracks2 = res2.expect("res2 should succeed");

    assert_eq!(tracks1.len(), 2);
    assert_eq!(tracks2.len(), 2);

    // CRITICAL: Verify DB contains strictly 2 tracks (no duplicate SQLite rows)
    let db_tracks = subtitle_repo.find_by_media_item(mkv_item_id).await.unwrap();
    assert_eq!(
        db_tracks.len(),
        2,
        "Expected exactly 2 tracks in SQLite, no duplicates!"
    );

    let stream_indices: Vec<Option<u32>> = db_tracks.iter().map(|t| t.stream_index).collect();
    assert!(stream_indices.contains(&Some(2)));
    assert!(stream_indices.contains(&Some(3)));

    // Verify in_flight_probes cleaned up
    assert_eq!(
        service.in_flight_probe_count(),
        0,
        "in_flight_probes map should be empty after simultaneous probes finish"
    );
}

#[tokio::test]
async fn test_embedded_mov_text_subtitles_probe_and_extract_across_mov_m4v_3gp_and_mts() {
    let ffmpeg_ok = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok();
    let ffprobe_ok = std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .is_ok();
    if !ffmpeg_ok || !ffprobe_ok {
        return;
    }

    let (service, _subtitle_repo, media_repo, _media_item_id, _cache_dir, media_dir) =
        setup_test_context().await;

    let srt_path = media_dir.path().join("dialog.srt");
    tokio::fs::write(
        &srt_path,
        "1\n00:00:00,200 --> 00:00:00,900\nEmbedded Timed Text Works!\n",
    )
    .await
    .unwrap();

    for ext in ["mov", "m4v", "3gp"] {
        let video_path = media_dir.path().join(format!("sample_tx3g.{ext}"));
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=176x144:d=1",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=8000:cl=mono:d=1",
                "-i",
                srt_path.to_str().unwrap(),
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-map",
                "2:s:0",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-c:s",
                "mov_text",
                "-metadata:s:s:0",
                "language=eng",
                video_path.to_str().unwrap(),
            ])
            .status()
            .expect("ffmpeg should mux mov_text container");
        assert!(status.success(), "ffmpeg mux failed for .{ext}");

        media_repo
            .upsert_batch(&[MediaItem {
                id: None,
                library_id: "lib1".to_string(),
                item_type: MediaType::Movie,
                title: format!("Sample {ext}"),
                original_title: None,
                release_year: Some(2024),
                added_at: 2000,
                file_path: video_path.clone(),
                file_name: format!("sample_tx3g.{ext}"),
                file_size: 1024,
                technical: TechnicalInfo::default(),
                metadata: MediaMetadata::default(),
                locked_fields: Vec::new(),
            }])
            .await
            .unwrap();

        let item = media_repo.find_by_path(&video_path).await.unwrap().unwrap();
        let tracks = service
            .list_or_probe_subtitles(&item)
            .await
            .expect("list_or_probe_subtitles should succeed");
        assert_eq!(tracks.len(), 1, "expected 1 embedded track for .{ext}");
        assert_eq!(tracks[0].source, SubtitleSource::Embedded);
        assert_eq!(tracks[0].language, "eng");

        let vtt_path = service
            .get_webvtt_path(tracks[0].id)
            .await
            .expect("get_webvtt_path should extract mov_text to WebVTT");
        let vtt_text = tokio::fs::read_to_string(&vtt_path).await.unwrap();
        assert!(vtt_text.starts_with("WEBVTT"));
        assert!(
            vtt_text.contains("Embedded Timed Text Works!"),
            "extracted WebVTT for .{ext} missing cue text: {vtt_text}"
        );
    }

    // Verify .mts / .m2ts container is recognized as a supported probe target without erroring
    let mts_path = media_dir.path().join("camcorder.mts");
    let mts_status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=176x144:d=1",
            "-c:v",
            "mpeg2video",
            "-f",
            "mpegts",
            "-mpegts_m2ts_mode",
            "1",
            mts_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should create .mts");
    assert!(mts_status.success());

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Camcorder MTS".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 2005,
            file_path: mts_path.clone(),
            file_name: "camcorder.mts".to_string(),
            file_size: 1024,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
            locked_fields: Vec::new(),
        }])
        .await
        .unwrap();

    let mts_item = media_repo.find_by_path(&mts_path).await.unwrap().unwrap();
    let mts_tracks = service
        .list_or_probe_subtitles(&mts_item)
        .await
        .expect("list_or_probe_subtitles should cleanly probe .mts");
    assert!(mts_tracks.is_empty());
}
