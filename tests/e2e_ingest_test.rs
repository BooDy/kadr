use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;
use kadr_core::models::{Library, MediaType};
use kadr_ingest::watcher::{start_library_watcher, IngestPipeline, IngestWorker};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

#[tokio::test]
async fn test_end_to_end_library_scan_and_reactive_ingest() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    let dir = tempdir().unwrap();
    let library = Library {
        id: "classics".to_string(),
        name: "Classic Cinema".to_string(),
        path: dir.path().to_path_buf(),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    };
    lib_repo.create(&library).await.unwrap();

    // 1. Pre-create a file before watcher starts
    let movie1 = dir.path().join("The.Flirtation.of.Girls.1949.1080p.BluRay.x264-Scene.mkv");
    let mut f1 = File::create(&movie1).unwrap();
    f1.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00]).unwrap();

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let worker = IngestWorker::new(rx, media_repo.clone());
    let _worker_handle = tokio::spawn(worker.run());

    let pipeline = Arc::new(IngestPipeline::new(false));
    let _watcher = start_library_watcher(
        library.clone(),
        pipeline.clone(),
        tx.clone(),
        Duration::from_millis(100),
    ).await.unwrap();

    // Wait for initial scan to flush to DB
    sleep(Duration::from_millis(500)).await;

    let items = media_repo.list_by_library("classics", 10, 0).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "The Flirtation of Girls");
    assert_eq!(items[0].release_year, Some(1949));
    assert_eq!(items[0].technical.resolution.as_deref(), Some("1080p"));

    // 2. Reactively add a new movie with an .nfo sidecar
    let movie2 = dir.path().join("Terror.and.Kebab.1992.mkv");
    let nfo2 = dir.path().join("Terror.and.Kebab.1992.nfo");

    let mut f2 = File::create(&movie2).unwrap();
    f2.write_all(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00]).unwrap();

    let mut n2 = File::create(&nfo2).unwrap();
    n2.write_all(r#"<movie><director>Sherif Arafa</director><actor><name>Adel Emam</name></actor></movie>"#.as_bytes()).unwrap();

    // Wait for debouncer (100ms) + batch flush (100ms)
    sleep(Duration::from_millis(600)).await;

    let all_items = media_repo.list_by_library("classics", 10, 0).await.unwrap();
    assert_eq!(all_items.len(), 2);

    let second_item = all_items.iter().find(|i| i.title == "Terror and Kebab").expect("item not found");
    assert_eq!(second_item.release_year, Some(1992));
    assert_eq!(second_item.metadata.director.as_deref(), Some("Sherif Arafa"));
    assert_eq!(second_item.metadata.actors, vec!["Adel Emam"]);
}
