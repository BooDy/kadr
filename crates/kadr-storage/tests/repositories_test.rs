use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_storage::error::StorageError;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use std::path::PathBuf;

#[tokio::test]
async fn test_library_and_media_item_repositories() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // 1. Create Library
    let lib = Library {
        id: "movies".to_string(),
        name: "Feature Films".to_string(),
        path: PathBuf::from("/media/movies"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    let retrieved_lib = lib_repo
        .get_by_id("movies")
        .await
        .unwrap()
        .expect("library not found");
    assert_eq!(retrieved_lib.name, "Feature Films");

    // 2. Insert Batch of Media Items
    let item1 = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "The Nightingale's Prayer".to_string(),
        original_title: Some("Doaa al-Karawan".to_string()),
        release_year: Some(1959),
        added_at: 1700000100,
        file_path: PathBuf::from("/media/movies/The.Nightingales.Prayer.1959.1080p.mkv"),
        file_name: "The.Nightingales.Prayer.1959.1080p.mkv".to_string(),
        file_size: 3_200_000_000,
        technical: TechnicalInfo {
            duration_seconds: 6540,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
        },
        metadata: MediaMetadata {
            director: Some("Henry Barakat".to_string()),
            writers: vec!["Taha Hussein".to_string()],
            actors: vec!["Faten Hamama".to_string(), "Ahmed Mazhar".to_string()],
            overview: Some(
                "A young woman seeks revenge for her sister's honor killing.".to_string(),
            ),
            country: Some("Egypt".to_string()),
            language: Some("ara".to_string()),
            tags: vec!["drama".to_string()],
            studio: None,
            poster_path: None,
            backdrop_path: None,
            release_group: None,
            ..Default::default()
        },
    };

    let count = media_repo
        .upsert_batch(std::slice::from_ref(&item1))
        .await
        .unwrap();
    assert_eq!(count, 1);

    // 3. Query items
    let items = media_repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "The Nightingale's Prayer");
    assert_eq!(items[0].technical.resolution.as_deref(), Some("1080p"));

    // 4. Delete item
    let deleted = media_repo.delete_by_path(&item1.file_path).await.unwrap();
    assert!(deleted);
    let after_delete = media_repo.list_by_library("movies", 10, 0).await.unwrap();
    assert_eq!(after_delete.len(), 0);
}

#[tokio::test]
async fn test_library_repository_crud() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool);

    let lib_b = Library {
        id: "series".to_string(),
        name: "TV Shows".to_string(),
        path: PathBuf::from("/media/series"),
        media_type: MediaType::Show,
        created_at: 1700000010,
        ..Default::default()
    };
    let lib_a = Library {
        id: "anime".to_string(),
        name: "Anime Series".to_string(),
        path: PathBuf::from("/media/anime"),
        media_type: MediaType::Show,
        created_at: 1700000020,
        ..Default::default()
    };

    lib_repo.insert(&lib_b).await.unwrap();
    lib_repo.insert(&lib_a).await.unwrap();

    // get_all ordered by name ASC
    let all = lib_repo.get_all().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "Anime Series");
    assert_eq!(all[1].name, "TV Shows");

    // Upsert update
    let mut updated_b = lib_b.clone();
    updated_b.name = "Television Series".to_string();
    lib_repo.insert(&updated_b).await.unwrap();

    let fetched_b = lib_repo.get_by_id("series").await.unwrap().expect("found");
    assert_eq!(fetched_b.name, "Television Series");

    // Delete
    let deleted = lib_repo.delete("series").await.unwrap();
    assert!(deleted);
    let deleted_again = lib_repo.delete("series").await.unwrap();
    assert!(!deleted_again);

    let remaining = lib_repo.get_all().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "anime");
}

#[tokio::test]
async fn test_media_item_batch_upsert_pagination_and_cascade() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    let lib = Library {
        id: "classic-movies".to_string(),
        name: "Classics".to_string(),
        path: PathBuf::from("/media/classics"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    let items = (1..=5)
        .map(|i| MediaItem {
            id: None,
            library_id: "classic-movies".to_string(),
            item_type: MediaType::Movie,
            title: format!("Movie {i:02}"),
            original_title: None,
            release_year: Some(1950 + i),
            added_at: 1700000000 + i as i64,
            file_path: PathBuf::from(format!("/media/classics/movie_{i:02}.mkv")),
            file_name: format!("movie_{i:02}.mkv"),
            file_size: 1_000_000_000 * i as u64,
            technical: TechnicalInfo {
                duration_seconds: 5000 + i as i64,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
            },
            metadata: MediaMetadata::default(),
        })
        .collect::<Vec<_>>();

    let empty_count = media_repo.upsert_batch(&[]).await.unwrap();
    assert_eq!(empty_count, 0);

    let count = media_repo.upsert_batch(&items).await.unwrap();
    assert_eq!(count, 5);

    let total = media_repo.count_by_library("classic-movies").await.unwrap();
    assert_eq!(total, 5);

    // Test pagination (ORDER BY title ASC)
    let page1 = media_repo
        .list_by_library("classic-movies", 2, 0)
        .await
        .unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].title, "Movie 01");
    assert_eq!(page1[1].title, "Movie 02");

    let page2 = media_repo
        .list_by_library("classic-movies", 2, 2)
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);
    assert_eq!(page2[0].title, "Movie 03");
    assert_eq!(page2[1].title, "Movie 04");

    let page3 = media_repo
        .list_by_library("classic-movies", 2, 4)
        .await
        .unwrap();
    assert_eq!(page3.len(), 1);
    assert_eq!(page3[0].title, "Movie 05");

    // Upsert update on conflict
    let mut updated_item1 = items[0].clone();
    updated_item1.title = "Movie 01 (Remastered)".to_string();
    let upsert_count = media_repo
        .upsert_batch(std::slice::from_ref(&updated_item1))
        .await
        .unwrap();
    assert_eq!(upsert_count, 1);

    let total_after_upsert = media_repo.count_by_library("classic-movies").await.unwrap();
    assert_eq!(total_after_upsert, 5);

    let page1_after_update = media_repo
        .list_by_library("classic-movies", 1, 0)
        .await
        .unwrap();
    assert_eq!(page1_after_update[0].title, "Movie 01 (Remastered)");

    // Cascade delete on library deletion
    lib_repo.delete("classic-movies").await.unwrap();
    let total_after_lib_delete = media_repo.count_by_library("classic-movies").await.unwrap();
    assert_eq!(total_after_lib_delete, 0);
}

#[tokio::test]
async fn test_media_item_get_by_ids_batch() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // 1. Empty IDs returns empty map
    let empty_res = media_repo.get_by_ids(&[]).await.unwrap();
    assert!(empty_res.is_empty());

    // 2. Setup library and items
    let lib = Library {
        id: "lib1".to_string(),
        name: "Test Lib".to_string(),
        path: PathBuf::from("/media/test"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    let items = vec![
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Alpha".to_string(),
            original_title: None,
            release_year: Some(2001),
            added_at: 1700000001,
            file_path: PathBuf::from("/media/test/alpha.mp4"),
            file_name: "alpha.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                overview: Some("Alpha overview".to_string()),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Beta".to_string(),
            original_title: None,
            release_year: Some(2002),
            added_at: 1700000002,
            file_path: PathBuf::from("/media/test/beta.mp4"),
            file_name: "beta.mp4".to_string(),
            file_size: 2000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                overview: Some("Beta overview".to_string()),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Gamma".to_string(),
            original_title: None,
            release_year: Some(2003),
            added_at: 1700000003,
            file_path: PathBuf::from("/media/test/gamma.mp4"),
            file_name: "gamma.mp4".to_string(),
            file_size: 3000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                overview: Some("Gamma overview".to_string()),
                ..Default::default()
            },
        },
    ];

    media_repo.upsert_batch(&items).await.unwrap();
    let fetched_items = media_repo.list_by_library("lib1", 10, 0).await.unwrap();
    assert_eq!(fetched_items.len(), 3);

    let id_alpha = fetched_items
        .iter()
        .find(|i| i.title == "Alpha")
        .unwrap()
        .id
        .unwrap();
    let id_gamma = fetched_items
        .iter()
        .find(|i| i.title == "Gamma")
        .unwrap()
        .id
        .unwrap();

    // Query for Alpha, Gamma, and a nonexistent ID 99999
    let map = media_repo
        .get_by_ids(&[id_alpha, id_gamma, 99999])
        .await
        .unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&id_alpha).unwrap().title, "Alpha");
    assert_eq!(
        map.get(&id_alpha).unwrap().metadata.overview.as_deref(),
        Some("Alpha overview")
    );
    assert_eq!(map.get(&id_gamma).unwrap().title, "Gamma");
    assert_eq!(
        map.get(&id_gamma).unwrap().metadata.overview.as_deref(),
        Some("Gamma overview")
    );
    assert!(!map.contains_key(&99999));
}

#[tokio::test]
async fn test_media_item_find_by_paths_batch() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    lib_repo
        .insert(&Library {
            id: "lib1".to_string(),
            name: "Lib 1".to_string(),
            path: PathBuf::from("/media/test"),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let path_a = PathBuf::from("/media/test/alpha.mp4");
    let path_b = PathBuf::from("/media/test/beta.mp4");
    let path_nonexistent = PathBuf::from("/media/test/ghost.mp4");

    let items = vec![
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Alpha".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: 1001,
            file_path: path_a.clone(),
            file_name: "alpha.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        },
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Beta".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 1002,
            file_path: path_b.clone(),
            file_name: "beta.mp4".to_string(),
            file_size: 2000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        },
    ];

    media_repo.upsert_batch(&items).await.unwrap();

    // Empty paths slice returns empty map
    let empty_map = media_repo.find_by_paths(&[]).await.unwrap();
    assert!(empty_map.is_empty());

    // Batch query with existing paths and one nonexistent path
    let map = media_repo
        .find_by_paths(&[
            path_a.as_path(),
            path_b.as_path(),
            path_nonexistent.as_path(),
        ])
        .await
        .unwrap();

    assert_eq!(map.len(), 2);
    assert_eq!(map.get(&path_a).unwrap().title, "Alpha");
    assert_eq!(map.get(&path_b).unwrap().title, "Beta");
    assert!(!map.contains_key(&path_nonexistent));
}

#[tokio::test]
async fn test_deduplicate_media_items() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    let lib = Library {
        id: "lib1".to_string(),
        name: "Test Lib".to_string(),
        path: PathBuf::from("/media/test"),
        media_type: MediaType::Movie,
        created_at: 1000,
        ..Default::default()
    };
    lib_repo.insert(&lib).await.unwrap();

    // Create temp files to test real canonicalization
    let temp_dir = tempfile::tempdir().unwrap();
    let real_file = temp_dir.path().join("real_movie.mp4");
    std::fs::File::create(&real_file).unwrap();

    // Insert original item
    let item1 = MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "Original".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 1000,
        file_path: real_file.clone(),
        file_name: "real_movie.mp4".to_string(),
        file_size: 1000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata::default(),
    };
    media_repo.upsert_batch(&[item1]).await.unwrap();

    // Create symlink
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let sym_file = temp_dir.path().join("symlink_movie.mp4");
        symlink(&real_file, &sym_file).unwrap();

        // Insert duplicate item using symlinked path
        let item2 = MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Duplicate".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1001,
            file_path: sym_file,
            file_name: "symlink_movie.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        };
        media_repo.upsert_batch(&[item2]).await.unwrap();

        let count_before = media_repo.count_by_library("lib1").await.unwrap();
        assert_eq!(count_before, 2);

        // Run deduplication
        let removed = media_repo.deduplicate_media_items().await.unwrap();
        assert_eq!(removed, 1);

        let count_after = media_repo.count_by_library("lib1").await.unwrap();
        assert_eq!(count_after, 1);
    }
}

#[tokio::test]
async fn test_library_update_name() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());

    let lib = lib_repo
        .create(
            "lib-orig",
            "Original Name",
            "/media/movies",
            kadr_core::models::MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    // 1. Successful update
    let updated = lib_repo
        .update_name(&lib.id, "Renamed Movies")
        .await
        .unwrap();
    assert_eq!(updated.name, "Renamed Movies");
    assert_eq!(updated.id, "lib-orig");

    let fetched = lib_repo
        .get_by_id(&lib.id)
        .await
        .unwrap()
        .expect("library should exist");
    assert_eq!(fetched.name, "Renamed Movies");

    // 2. Reject empty or whitespace-only name
    let empty_str_err = lib_repo.update_name(&lib.id, "").await.unwrap_err();
    match empty_str_err {
        StorageError::InvalidInput(msg) => {
            assert!(msg.to_lowercase().contains("empty"));
        }
        other => panic!("Expected InvalidInput error, got: {:?}", other),
    }

    let empty_err = lib_repo.update_name(&lib.id, "   ").await.unwrap_err();
    match empty_err {
        StorageError::InvalidInput(msg) => {
            assert!(msg.to_lowercase().contains("empty"));
        }
        other => panic!("Expected InvalidInput error, got: {:?}", other),
    }

    // 3. Return NotFound on non-existent id
    let not_found_err = lib_repo
        .update_name("non-existent-lib", "New Name")
        .await
        .unwrap_err();
    match not_found_err {
        StorageError::NotFound(_) => {}
        other => panic!("Expected NotFound error, got: {:?}", other),
    }
}
