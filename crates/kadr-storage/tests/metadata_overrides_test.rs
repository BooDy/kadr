use std::path::PathBuf;

use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, MetadataUpdate, TechnicalInfo,
};
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use tempfile::TempDir;

async fn setup_repos() -> (TempDir, LibraryRepository, MediaItemRepository) {
    let dir = TempDir::new().expect("tempdir");
    let db_path = dir.path().join("test_meta.db");
    let pool = create_pool(&db_path, 4).expect("pool");
    initialize_database(&pool).await.expect("init db");

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool);

    lib_repo
        .insert(&Library {
            id: "lib-movies".to_string(),
            name: "Movies".to_string(),
            path: dir.path().join("movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .expect("insert lib");

    (dir, lib_repo, media_repo)
}

#[tokio::test]
async fn update_metadata_locks_edited_fields_and_preserves_across_rescan() {
    let (dir, _lib_repo, media_repo) = setup_repos().await;
    let file_path: PathBuf = dir.path().join("movies/Dune.2021.1080p.mkv");

    let initial = MediaItem {
        id: None,
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        title: "Dune.2021.1080p".to_string(),
        original_title: None,
        release_year: Some(2020),
        added_at: 1000,
        file_path: file_path.clone(),
        file_name: "Dune.2021.1080p.mkv".to_string(),
        file_size: 1_000_000,
        technical: TechnicalInfo {
            duration_seconds: 9000,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata {
            overview: Some("Scanner parsed overview".to_string()),
            director: Some("Unknown".to_string()),
            genres: vec!["Action".to_string()],
            country: Some("CA".to_string()),
            ..Default::default()
        },
        locked_fields: Vec::new(),
    };

    media_repo.upsert_batch(&[initial]).await.unwrap();
    let stored = media_repo.find_by_path(&file_path).await.unwrap().unwrap();
    let item_id = stored.id.unwrap();
    assert!(stored.locked_fields.is_empty());

    // Edit a subset of metadata fields + tmdb_id (leaving `country` unlocked).
    let updated = media_repo
        .update_metadata(
            item_id,
            &MetadataUpdate {
                title: Some("Dune: Part One".to_string()),
                original_title: Some(Some("Dune".to_string())),
                release_year: Some(Some(2021)),
                overview: Some(Some("Paul Atreides unites with the Fremen.".to_string())),
                director: Some(Some("Denis Villeneuve".to_string())),
                actors: Some(vec![
                    "Timothée Chalamet".to_string(),
                    "Rebecca Ferguson".to_string(),
                ]),
                genres: Some(vec!["Sci-Fi".to_string(), "Adventure".to_string()]),
                rating: Some(Some(8.0)),
                poster_path: Some(Some("/data/artwork/item_1_poster.jpg".to_string())),
                tmdb_id: Some(Some(438631)),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .expect("updated item");

    assert_eq!(updated.title, "Dune: Part One");
    assert_eq!(updated.original_title.as_deref(), Some("Dune"));
    assert_eq!(updated.release_year, Some(2021));
    assert!(updated.locked_fields.contains(&"title".to_string()));
    assert!(updated.locked_fields.contains(&"director".to_string()));
    assert!(updated.locked_fields.contains(&"poster_path".to_string()));
    assert!(!updated.locked_fields.contains(&"country".to_string()));

    // Simulate a library rescan where the file was replaced with a 4K HEVC remux
    // AND filename/NFO parser emits raw/noisy metadata.
    let rescan_item = MediaItem {
        id: None,
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        title: "Dune.RAW.FILENAME".to_string(),
        original_title: Some("Overwritten Orig".to_string()),
        release_year: Some(1984),
        added_at: 2000,
        file_path: file_path.clone(),
        file_name: "Dune.2021.2160p.mkv".to_string(),
        file_size: 8_500_000,
        technical: TechnicalInfo {
            duration_seconds: 9320,
            resolution: Some("2160p".to_string()),
            video_codec: Some("hevc".to_string()),
            audio_codec: Some("truehd".to_string()),
            audio_channels: Some(8),
            container: Some("mkv".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata {
            overview: Some("Noisy NFO plot".to_string()),
            director: Some("Wrong Director".to_string()),
            actors: vec!["Wrong Actor".to_string()],
            genres: vec!["Comedy".to_string()],
            country: Some("US".to_string()), // unlocked -> should update!
            rating: Some(3.2),
            poster_path: Some("/media/movies/sidecar.jpg".to_string()),
            ..Default::default()
        },
        locked_fields: Vec::new(),
    };

    media_repo.upsert_batch(&[rescan_item]).await.unwrap();
    let after_rescan = media_repo.get_by_id(item_id).await.unwrap().unwrap();

    // Technical columns MUST update from rescan.
    assert_eq!(after_rescan.technical.duration_seconds, 9320);
    assert_eq!(after_rescan.file_size, 8_500_000);
    assert_eq!(after_rescan.technical.resolution.as_deref(), Some("2160p"));
    assert_eq!(after_rescan.technical.video_codec.as_deref(), Some("hevc"));
    assert_eq!(
        after_rescan.technical.audio_codec.as_deref(),
        Some("truehd")
    );
    assert_eq!(after_rescan.technical.audio_channels, Some(8));

    // Locked metadata fields + tmdb_id MUST remain untouched!
    assert_eq!(after_rescan.title, "Dune: Part One");
    assert_eq!(after_rescan.original_title.as_deref(), Some("Dune"));
    assert_eq!(after_rescan.release_year, Some(2021));
    assert_eq!(
        after_rescan.metadata.overview.as_deref(),
        Some("Paul Atreides unites with the Fremen.")
    );
    assert_eq!(
        after_rescan.metadata.director.as_deref(),
        Some("Denis Villeneuve")
    );
    assert_eq!(
        after_rescan.metadata.actors,
        vec!["Timothée Chalamet", "Rebecca Ferguson"]
    );
    assert_eq!(after_rescan.metadata.genres, vec!["Sci-Fi", "Adventure"]);
    assert_eq!(after_rescan.metadata.rating, Some(8.0));
    assert_eq!(
        after_rescan.metadata.poster_path.as_deref(),
        Some("/data/artwork/item_1_poster.jpg")
    );
    assert_eq!(
        after_rescan
            .metadata
            .extra
            .get("tmdb_id")
            .and_then(|v| v.as_i64()),
        Some(438631)
    );

    // Unlocked field (`country`) WAS updated by the rescan.
    assert_eq!(after_rescan.metadata.country.as_deref(), Some("US"));

    // Reset locks -> subsequent rescan overwrites previously locked fields.
    let reset = media_repo
        .update_metadata(
            item_id,
            &MetadataUpdate {
                reset_locks: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(reset.locked_fields.is_empty());

    let unlocked_rescan = MediaItem {
        title: "Dune Rescanned Title".to_string(),
        release_year: Some(2021),
        file_path: file_path.clone(),
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        file_name: "Dune.mkv".to_string(),
        file_size: 8_500_000,
        added_at: 3000,
        metadata: MediaMetadata {
            director: Some("Rescanned Director".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    media_repo.upsert_batch(&[unlocked_rescan]).await.unwrap();
    let final_item = media_repo.get_by_id(item_id).await.unwrap().unwrap();
    assert_eq!(final_item.title, "Dune Rescanned Title");
    assert_eq!(
        final_item.metadata.director.as_deref(),
        Some("Rescanned Director")
    );
}
