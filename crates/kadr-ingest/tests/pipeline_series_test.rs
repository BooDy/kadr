use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType};
use kadr_ingest::watcher::pipeline::IngestPipeline;
use kadr_ingest::watcher::{IngestMessage, IngestWorker};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_pipeline_ingests_episode_and_populates_series_meta() {
    let pipeline = IngestPipeline::new(false, None);
    let lib = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: PathBuf::from("/media/shows"),
        media_type: MediaType::Show,
        ..Default::default()
    };
    let _file = PathBuf::from(
        "/media/shows/What We Do in the Shadows/Season 04/What We Do in the Shadows - S04E01 - Reunited.mkv",
    );
    // Create dummy file for metadata
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_file = temp_dir
        .path()
        .join("What We Do in the Shadows - S04E01 - Reunited.mkv");
    std::fs::write(&temp_file, b"dummy").unwrap();

    let res = pipeline.process_file(&lib, &temp_file).await.unwrap();
    assert!(res.is_some());
    let (item, _) = res.unwrap();
    assert_eq!(item.item_type, MediaType::Episode);
    assert_eq!(
        item.metadata.series_title.as_deref(),
        Some("What We Do in the Shadows")
    );
    assert_eq!(item.metadata.season, Some(4));
    assert_eq!(item.metadata.episode, Some(1));
    assert_eq!(item.title, "Reunited");
}

#[tokio::test]
async fn test_pipeline_folder_cues_populates_series_meta() {
    let pipeline = IngestPipeline::new(false, None);
    let temp_dir = tempfile::tempdir().unwrap();
    let lib_path = temp_dir.path().to_path_buf();
    let lib = Library {
        id: "tv".to_string(),
        name: "Television".to_string(),
        path: lib_path.clone(),
        media_type: MediaType::Show,
        ..Default::default()
    };

    let show_dir = lib_path.join("Severance");
    let season_dir = show_dir.join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();

    let ep_file = season_dir.join("01 - Good News About Hell.mkv");
    std::fs::write(&ep_file, b"dummy").unwrap();

    let res = pipeline.process_file(&lib, &ep_file).await.unwrap();
    assert!(res.is_some());
    let (item, _) = res.unwrap();
    assert_eq!(item.item_type, MediaType::Episode);
    assert_eq!(item.metadata.series_title.as_deref(), Some("Severance"));
    assert_eq!(item.metadata.season, Some(1));
    assert_eq!(item.metadata.episode, Some(1));
    assert_eq!(item.title, "Good News About Hell");
}

#[tokio::test]
async fn test_worker_flushes_batch_and_creates_parent_show() {
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let temp_dir = tempfile::tempdir().unwrap();
    let show_dir = temp_dir.path().join("What We Do in the Shadows");
    let season_dir = show_dir.join("Season 04");
    std::fs::create_dir_all(&season_dir).unwrap();

    // Create a dummy show poster in the show folder
    let poster_file = show_dir.join("poster.jpg");
    std::fs::write(&poster_file, b"fake-poster").unwrap();

    let library = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: temp_dir.path().to_path_buf(),
        media_type: MediaType::Show,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let repo = MediaItemRepository::new(pool.clone());
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, repo.clone());
    tokio::spawn(worker.run());

    let ep1_file = season_dir.join("What We Do in the Shadows - S04E01 - Reunited.mkv");
    std::fs::write(&ep1_file, b"dummy1").unwrap();
    let ep2_file = season_dir.join("What We Do in the Shadows - S04E02 - The Lamp.mkv");
    std::fs::write(&ep2_file, b"dummy2").unwrap();

    let ep1_meta = MediaMetadata {
        series_title: Some("What We Do in the Shadows".to_string()),
        season: Some(4),
        episode: Some(1),
        ..Default::default()
    };

    let ep1 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Reunited".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: 1700000010,
        file_path: ep1_file,
        file_name: "What We Do in the Shadows - S04E01 - Reunited.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: ep1_meta,
    };

    let ep2_meta = MediaMetadata {
        series_title: Some("What We Do in the Shadows".to_string()),
        season: Some(4),
        episode: Some(2),
        ..Default::default()
    };

    let ep2 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "The Lamp".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: 1700000020,
        file_path: ep2_file,
        file_name: "What We Do in the Shadows - S04E02 - The Lamp.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: ep2_meta,
    };

    tx.send(IngestMessage::Upsert(ep1, vec![])).await.unwrap();
    tx.send(IngestMessage::Upsert(ep2, vec![])).await.unwrap();

    // Allow worker flush timeout (100ms) to trigger
    sleep(Duration::from_millis(150)).await;

    // Verify parent show was automatically created
    let show = repo
        .find_show_by_title("shows", "What We Do in the Shadows")
        .await
        .unwrap()
        .expect("parent show should exist");

    assert_eq!(show.item_type, MediaType::Show);
    assert_eq!(show.title, "What We Do in the Shadows");
    assert_eq!(show.release_year, Some(2022));
    assert_eq!(show.file_path, show_dir);
    assert_eq!(
        show.metadata.poster_path.as_deref(),
        Some(poster_file.to_str().unwrap())
    );

    // Verify total items in library: 1 parent show + 2 episodes = 3
    let count = repo.count_by_library("shows").await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_worker_preserves_existing_parent_show() {
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let library = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: PathBuf::from("/media/shows"),
        media_type: MediaType::Show,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let repo = MediaItemRepository::new(pool.clone());

    // Pre-insert a show with existing custom metadata
    let custom_meta = MediaMetadata {
        overview: Some("Pre-existing show overview".to_string()),
        ..Default::default()
    };
    let existing_show = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Show,
        title: "Severance".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: 1700000000,
        file_path: PathBuf::from("/media/shows/Severance"),
        file_name: String::new(),
        file_size: 0,
        technical: Default::default(),
        metadata: custom_meta,
    };
    repo.upsert_batch(&[existing_show]).await.unwrap();

    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, repo.clone());
    tokio::spawn(worker.run());

    let ep_meta = MediaMetadata {
        series_title: Some("Severance".to_string()),
        season: Some(1),
        episode: Some(1),
        ..Default::default()
    };

    let ep = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Good News About Hell".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: 1700000050,
        file_path: PathBuf::from("/media/shows/Severance/Season 01/01.mkv"),
        file_name: "01.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: ep_meta,
    };

    tx.send(IngestMessage::Upsert(ep, vec![])).await.unwrap();
    sleep(Duration::from_millis(150)).await;

    // Verify existing show's metadata was preserved and not replaced
    let show = repo
        .find_show_by_title("shows", "Severance")
        .await
        .unwrap()
        .expect("show should exist");
    assert_eq!(
        show.metadata.overview.as_deref(),
        Some("Pre-existing show overview")
    );
}

#[tokio::test]
async fn test_pipeline_show_library_non_episode_preserves_title() {
    let pipeline = IngestPipeline::new(false, None);
    let temp_dir = tempfile::tempdir().unwrap();
    let lib = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: temp_dir.path().to_path_buf(),
        media_type: MediaType::Show,
        ..Default::default()
    };

    let temp_file = temp_dir.path().join("Special.Feature.2023.1080p.mkv");
    std::fs::write(&temp_file, b"dummy").unwrap();

    let res = pipeline.process_file(&lib, &temp_file).await.unwrap();
    assert!(res.is_some());
    let (item, _) = res.unwrap();
    assert_eq!(item.item_type, MediaType::Episode);
    assert_eq!(item.title, "Special Feature");
}

#[tokio::test]
async fn test_worker_handles_multiple_flat_shows_in_shared_dir_without_collision() {
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let temp_dir = tempfile::tempdir().unwrap();
    let library = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: temp_dir.path().to_path_buf(),
        media_type: MediaType::Show,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let repo = MediaItemRepository::new(pool.clone());
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, repo.clone());
    tokio::spawn(worker.run());

    let ep1_file = temp_dir.path().join("Show One - S01E01 - Pilot.mkv");
    let ep2_file = temp_dir.path().join("Show Two - S01E01 - Premiere.mkv");
    std::fs::write(&ep1_file, b"ep1").unwrap();
    std::fs::write(&ep2_file, b"ep2").unwrap();

    let ep1 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Pilot".to_string(),
        original_title: None,
        release_year: Some(2021),
        added_at: 1700000010,
        file_path: ep1_file,
        file_name: "Show One - S01E01 - Pilot.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: MediaMetadata {
            series_title: Some("Show One".to_string()),
            season: Some(1),
            episode: Some(1),
            ..Default::default()
        },
    };

    let ep2 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Premiere".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: 1700000020,
        file_path: ep2_file,
        file_name: "Show Two - S01E01 - Premiere.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: MediaMetadata {
            series_title: Some("Show Two".to_string()),
            season: Some(1),
            episode: Some(1),
            ..Default::default()
        },
    };

    tx.send(IngestMessage::Upsert(ep1, vec![])).await.unwrap();
    tx.send(IngestMessage::Upsert(ep2, vec![])).await.unwrap();
    sleep(Duration::from_millis(150)).await;

    let show1 = repo
        .find_show_by_title("shows", "Show One")
        .await
        .unwrap()
        .expect("show1 should exist");
    let show2 = repo
        .find_show_by_title("shows", "Show Two")
        .await
        .unwrap()
        .expect("show2 should exist");

    assert_ne!(show1.file_path, show2.file_path);
    assert_eq!(show1.file_path, temp_dir.path().join("Show One"));
    assert_eq!(show2.file_path, temp_dir.path().join("Show Two"));
}

#[tokio::test]
async fn test_worker_deduplicates_case_variation_series_titles_in_same_batch() {
    let pool = kadr_storage::create_in_memory_pool().unwrap();
    kadr_storage::initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let temp_dir = tempfile::tempdir().unwrap();
    let library = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: temp_dir.path().to_path_buf(),
        media_type: MediaType::Show,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let repo = MediaItemRepository::new(pool.clone());
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, repo.clone());
    tokio::spawn(worker.run());

    let ep1_file = temp_dir.path().join("The.Wire.S01E01.mkv");
    let ep2_file = temp_dir.path().join("the.wire.S01E02.mkv");
    std::fs::write(&ep1_file, b"wire1").unwrap();
    std::fs::write(&ep2_file, b"wire2").unwrap();

    let ep1 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Episode 1".to_string(),
        original_title: None,
        release_year: Some(2002),
        added_at: 1700000010,
        file_path: ep1_file,
        file_name: "The.Wire.S01E01.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: MediaMetadata {
            series_title: Some("The Wire".to_string()),
            season: Some(1),
            episode: Some(1),
            ..Default::default()
        },
    };

    let ep2 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Episode 2".to_string(),
        original_title: None,
        release_year: Some(2002),
        added_at: 1700000020,
        file_path: ep2_file,
        file_name: "the.wire.S01E02.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: MediaMetadata {
            series_title: Some("the wire".to_string()),
            season: Some(1),
            episode: Some(2),
            ..Default::default()
        },
    };

    // Both sent in the same batch
    tx.send(IngestMessage::Upsert(ep1, vec![])).await.unwrap();
    tx.send(IngestMessage::Upsert(ep2, vec![])).await.unwrap();
    sleep(Duration::from_millis(150)).await;

    let show = repo
        .find_show_by_title("shows", "The Wire")
        .await
        .unwrap()
        .expect("The Wire show should exist");
    assert_eq!(show.item_type, MediaType::Show);

    // Total items: 1 parent show + 2 episodes = 3
    let count = repo.count_by_library("shows").await.unwrap();
    assert_eq!(count, 3);
}
