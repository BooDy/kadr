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
