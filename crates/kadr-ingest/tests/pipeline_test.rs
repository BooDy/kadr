use kadr_core::models::{Library, MediaItem, MediaType};
use kadr_ingest::watcher::pipeline::IngestPipeline;
use kadr_ingest::watcher::{
    scan_directory_recursive, start_library_watcher, IngestMessage, IngestWorker,
};
use kadr_storage::repos::MediaItemRepository;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[tokio::test]
async fn test_pipeline_processes_file_and_extracts_all_metadata() {
    let dir = tempdir().unwrap();
    let video_path = dir
        .path()
        .join("Cairo.Station.1958.1080p.BluRay.x264-Ghareeb.mkv");
    let nfo_path = dir
        .path()
        .join("Cairo.Station.1958.1080p.BluRay.x264-Ghareeb.nfo");

    // Write dummy video file with MKV magic
    let mut vfile = File::create(&video_path).unwrap();
    vfile
        .write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00])
        .unwrap();

    // Write nfo
    let mut nfile = File::create(&nfo_path).unwrap();
    nfile
        .write_all(r#"<movie><director>Youssef Chahine</director></movie>"#.as_bytes())
        .unwrap();

    let library = Library {
        id: "classics".to_string(),
        name: "Classics".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new(false, None);
    let (item, _) = pipeline
        .process_file(&library, &video_path)
        .await
        .unwrap()
        .expect("should process");

    assert_eq!(item.title, "Cairo Station");
    assert_eq!(item.release_year, Some(1958));
    assert_eq!(item.technical.resolution.as_deref(), Some("1080p"));
    assert_eq!(item.metadata.director.as_deref(), Some("Youssef Chahine"));
    assert_eq!(item.metadata.release_group.as_deref(), Some("Ghareeb"));
}

#[tokio::test]
async fn test_pipeline_ignores_unparseable_or_non_media_files() {
    let dir = tempdir().unwrap();
    let txt_path = dir.path().join("notes.txt");
    let mut file = File::create(&txt_path).unwrap();
    file.write_all(b"not a movie").unwrap();

    let library = Library {
        id: "classics".to_string(),
        name: "Classics".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new(false, None);
    let item = pipeline.process_file(&library, &txt_path).await.unwrap();
    assert!(item.is_none());
}

#[test]
fn test_scan_directory_recursive_finds_media_files_in_nested_dirs() {
    let dir = tempdir().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    let mkv = sub.join("Movie.1999.1080p.mkv");
    let mp4 = dir.path().join("Video.2020.720p.mp4");
    let avi = dir.path().join("Old.2005.avi");
    let webm = dir.path().join("Clip.2021.webm");
    let txt = dir.path().join("readme.txt");
    let nfo = dir.path().join("Movie.1999.1080p.nfo");

    File::create(&mkv).unwrap();
    File::create(&mp4).unwrap();
    File::create(&avi).unwrap();
    File::create(&webm).unwrap();
    File::create(&txt).unwrap();
    File::create(&nfo).unwrap();

    let files = scan_directory_recursive(dir.path());
    assert_eq!(files.len(), 4);
    assert!(files.contains(&mkv));
    assert!(files.contains(&mp4));
    assert!(files.contains(&avi));
    assert!(files.contains(&webm));
    assert!(!files.contains(&txt));
    assert!(!files.contains(&nfo));
}

#[tokio::test]
async fn test_ingest_worker_upsert_and_delete() {
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = kadr_storage::LibraryRepository::new(pool.clone());
    let library = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: std::path::PathBuf::from("/media"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let repo = MediaItemRepository::new(pool.clone());
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, repo.clone());

    let worker_handle = tokio::spawn(worker.run());

    let item = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Test Movie".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: 1700000000,
        file_path: std::path::PathBuf::from("/media/Test.Movie.2022.1080p.mkv"),
        file_name: "Test.Movie.2022.1080p.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: Default::default(),
        locked_fields: Vec::new(),
    };

    tx.send(IngestMessage::Upsert(item.clone(), Vec::new()))
        .await
        .unwrap();

    // Allow worker to flush via 100ms timeout
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let items = repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Test Movie");

    // Send delete message
    tx.send(IngestMessage::Delete(item.file_path.clone()))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let items_after_del = repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(items_after_del.len(), 0);

    drop(tx);
    worker_handle.await.unwrap();
}

#[tokio::test]
async fn test_start_library_watcher_initial_scan() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("Alexandria.Why.1979.1080p.BluRay.x264.mkv");
    let mut file = File::create(&video_path).unwrap();
    file.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00])
        .unwrap();

    let library = Library {
        id: "chahine".to_string(),
        name: "Chahine".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };

    let pipeline = std::sync::Arc::new(IngestPipeline::new(false, None));
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    let _watcher =
        start_library_watcher(library, pipeline, tx, std::time::Duration::from_millis(50))
            .await
            .unwrap();

    // The startup scan should discover Alexandria.Why.1979...
    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for message")
        .expect("channel closed");

    match msg {
        IngestMessage::Upsert(item, _) => {
            assert_eq!(item.title, "Alexandria Why");
            assert_eq!(item.release_year, Some(1979));
        }
        _ => panic!("Expected Upsert message"),
    }
}

#[tokio::test]
async fn test_pipeline_malformed_nfo_falls_back_to_filename_metadata() {
    let dir = tempdir().unwrap();
    let video_path = dir
        .path()
        .join("The.Nightingale.Prayer.1959.1080p.BluRay.x264.mkv");
    let nfo_path = dir
        .path()
        .join("The.Nightingale.Prayer.1959.1080p.BluRay.x264.nfo");

    // Write dummy video file with MKV magic
    let mut vfile = File::create(&video_path).unwrap();
    vfile
        .write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00])
        .unwrap();

    // Write malformed/corrupted nfo
    let mut nfile = File::create(&nfo_path).unwrap();
    nfile
        .write_all(b"<movie><title>Unclosed Tag<broken")
        .unwrap();

    let library = Library {
        id: "classics".to_string(),
        name: "Classics".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new(false, None);
    let (item, _) = pipeline
        .process_file(&library, &video_path)
        .await
        .unwrap()
        .expect("should process despite bad nfo");

    assert_eq!(item.title, "The Nightingale Prayer");
    assert_eq!(item.release_year, Some(1959));
    assert_eq!(item.technical.resolution.as_deref(), Some("1080p"));
}

#[tokio::test]
async fn test_ingest_worker_delete_purges_inflight_batch() {
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = kadr_storage::LibraryRepository::new(pool.clone());
    let library = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: std::path::PathBuf::from("/media"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let repo = MediaItemRepository::new(pool.clone());
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, repo.clone());

    let worker_handle = tokio::spawn(worker.run());

    let item = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Inflight Movie".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 1700000000,
        file_path: std::path::PathBuf::from("/media/Inflight.Movie.2023.1080p.mkv"),
        file_name: "Inflight.Movie.2023.1080p.mkv".to_string(),
        file_size: 2048,
        technical: Default::default(),
        metadata: Default::default(),
        locked_fields: Vec::new(),
    };

    // Send Upsert followed immediately by Delete before 100ms batch flush
    tx.send(IngestMessage::Upsert(item.clone(), Vec::new()))
        .await
        .unwrap();
    tx.send(IngestMessage::Delete(item.file_path.clone()))
        .await
        .unwrap();

    // Give worker time to process messages and potential flush timeout
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Verify it was purged from the in-flight batch and never inserted
    let items = repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(items.len(), 0);

    drop(tx);
    worker_handle.await.unwrap();
}

#[test]
fn test_scan_directory_recursive_ignores_symlink_loops_and_avoids_duplicate_files() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create a real movie file
    let movie_file = root.join("Movie.2023.1080p.mkv");
    File::create(&movie_file).unwrap();

    // Create a subfolder
    let subfolder = root.join("subfolder");
    std::fs::create_dir(&subfolder).unwrap();

    // Create another movie in subfolder
    let sub_movie = subfolder.join("SubMovie.2024.1080p.mkv");
    File::create(&sub_movie).unwrap();

    // Create a symlink pointing back to root (circular loop)
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let circular_link = subfolder.join("loop_to_root");
        let _ = symlink(root, &circular_link);

        // Create an alias symlink to subfolder
        let alias_link = root.join("alias_to_subfolder");
        let _ = symlink(&subfolder, &alias_link);
    }

    let files = scan_directory_recursive(root);

    // Verify exactly 2 files returned, no infinite loop, no duplicates
    assert_eq!(files.len(), 2);
    let names: Vec<String> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"Movie.2023.1080p.mkv".to_string()));
    assert!(names.contains(&"SubMovie.2024.1080p.mkv".to_string()));
}

#[tokio::test]
async fn test_pipeline_and_worker_embedded_subtitles_ingest_and_deduplication() {
    let ffmpeg_available = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok();
    let ffprobe_available = std::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .is_ok();
    if !ffmpeg_available || !ffprobe_available {
        return;
    }

    let dir = tempdir().unwrap();
    let srt_ar = dir.path().join("ar.srt");
    let srt_en = dir.path().join("en.srt");
    std::fs::write(&srt_ar, "1\n00:00:00,500 --> 00:00:01,500\nمرحبا!\n").unwrap();
    std::fs::write(&srt_en, "1\n00:00:00,500 --> 00:00:01,500\nHello!\n").unwrap();

    let mkv_path = dir.path().join("Sample.Movie.2024.1080p.mkv");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:d=1",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo:d=1",
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
            "-metadata:s:s:1",
            "language=eng",
            "-metadata:s:s:1",
            "title=English",
            mkv_path.to_str().unwrap(),
        ])
        .status();

    let Ok(st) = status else { return };
    if !st.success() {
        return;
    }

    let library = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };

    // 1. Verify IngestPipeline::process_file finds both embedded subtitle tracks
    let pipeline = IngestPipeline::new(true, None);
    let (item, subs) = pipeline
        .process_file(&library, &mkv_path)
        .await
        .unwrap()
        .expect("should process mkv with embedded subtitles");

    assert_eq!(
        subs.len(),
        2,
        "pipeline must discover both embedded subtitle streams"
    );
    assert_eq!(
        subs[0].source,
        kadr_core::subtitles::SubtitleSource::Embedded
    );
    assert_eq!(subs[0].stream_index, Some(2));
    assert!(subs[0].file_path.is_none());
    assert_eq!(
        subs[1].source,
        kadr_core::subtitles::SubtitleSource::Embedded
    );
    assert_eq!(subs[1].stream_index, Some(3));
    assert!(subs[1].file_path.is_none());

    // 2. Set up DB & IngestWorker to test insertion and multi-track deduplication
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = kadr_storage::LibraryRepository::new(pool.clone());
    lib_repo.insert(&library).await.unwrap();

    let media_repo = MediaItemRepository::new(pool.clone());
    let sub_repo = kadr_storage::repos::SubtitleRepository::new(pool.clone());

    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new_with_subtitles(rx, media_repo.clone(), sub_repo.clone());
    let worker_handle = tokio::spawn(worker.run());

    // Send first Upsert with the 2 embedded tracks
    tx.send(IngestMessage::Upsert(item.clone(), subs.clone()))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let persisted_media = media_repo
        .find_by_path(&item.file_path)
        .await
        .unwrap()
        .expect("media item should be persisted");
    let media_id = persisted_media.id.unwrap();

    let persisted_subs = sub_repo.find_by_media_item(media_id).await.unwrap();
    assert_eq!(
        persisted_subs.len(),
        2,
        "both embedded tracks should be persisted"
    );
    assert!(persisted_subs
        .iter()
        .any(|s| s.stream_index == Some(2) && s.file_path.is_none()));
    assert!(persisted_subs
        .iter()
        .any(|s| s.stream_index == Some(3) && s.file_path.is_none()));

    // Send second Upsert with exact same item & tracks - deduplication should prevent duplicate rows
    tx.send(IngestMessage::Upsert(item.clone(), subs.clone()))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let persisted_subs_after = sub_repo.find_by_media_item(media_id).await.unwrap();
    assert_eq!(
        persisted_subs_after.len(),
        2,
        "deduplication must keep exactly 2 tracks, no duplicates"
    );

    // Send third Upsert containing a 3rd distinct stream_index with file_path: None
    let mut more_subs = subs.clone();
    more_subs.push(kadr_ingest::sidecars::DiscoveredSubtitle {
        source: kadr_core::subtitles::SubtitleSource::Embedded,
        language: "fra".to_string(),
        title: Some("French".to_string()),
        format: kadr_core::subtitles::SubtitleFormat::Srt,
        file_path: None,
        stream_index: Some(4),
        is_default: false,
        is_forced: false,
    });

    tx.send(IngestMessage::Upsert(item.clone(), more_subs))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let persisted_subs_final = sub_repo.find_by_media_item(media_id).await.unwrap();
    assert_eq!(
        persisted_subs_final.len(),
        3,
        "new stream_index should be inserted while streams 2 and 3 are deduplicated"
    );

    drop(tx);
    worker_handle.await.unwrap();
}

#[test]
fn test_scan_directory_recursive_discovers_all_supported_video_extensions() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let expected_files = [
        "film1.mkv",
        "film2.mp4",
        "film3.m4v",
        "film4.MOV",
        "film5.webm",
        "film6.avi",
        "film7.ts",
        "film8.m2ts",
        "film9.MTS",
        "film10.3gp",
        "film11.3G2",
    ];
    for name in expected_files {
        File::create(root.join(name)).unwrap();
    }

    // Non-video sidecars & assets must be ignored
    for ignored in ["film1.en.srt", "film1.nfo", "poster.jpg", "soundtrack.mp3"] {
        File::create(root.join(ignored)).unwrap();
    }

    let discovered = scan_directory_recursive(root);
    assert_eq!(discovered.len(), expected_files.len());

    let names: Vec<String> = discovered
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    for expected in expected_files {
        assert!(
            names.contains(&expected.to_string()),
            "expected scan_directory_recursive to discover {expected}"
        );
    }
}
