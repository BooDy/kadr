// crates/kadr-storage/tests/subtitle_repo_test.rs
use std::path::PathBuf;

use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource, SubtitleTrack};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, SubtitleRepository};

#[tokio::test]
async fn test_subtitle_migration_and_repository() {
    let pool = create_in_memory_pool().expect("failed to create pool");
    initialize_database(&pool)
        .await
        .expect("failed to initialize database");

    // 1. Verify Migration 004 applied cleanly: table and indexes exist
    {
        let conn = pool.get().await.expect("failed to get connection");
        conn.interact(|c| {
            let table_count: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='media_subtitles'",
                    [],
                    |row| row.get(0),
                )
                .expect("failed to query table");
            assert_eq!(table_count, 1, "media_subtitles table should exist");

            let idx_item_count: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_subtitles_item'",
                    [],
                    |row| row.get(0),
                )
                .expect("failed to query idx_subtitles_item");
            assert_eq!(idx_item_count, 1, "idx_subtitles_item index should exist");

            let idx_lang_count: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_subtitles_lang'",
                    [],
                    |row| row.get(0),
                )
                .expect("failed to query idx_subtitles_lang");
            assert_eq!(idx_lang_count, 1, "idx_subtitles_lang index should exist");
        })
        .await
        .expect("interact failed");
    }

    // 2. Setup library and media item for foreign key reference
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let sub_repo = SubtitleRepository::new(pool.clone());

    lib_repo
        .create(&Library {
            id: "lib1".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/media/movies"),
            media_type: MediaType::Movie,
            created_at: 1700000000,
        })
        .await
        .expect("failed to create library");

    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Test Movie".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1700000000,
            file_path: PathBuf::from("/media/movies/movie1.mkv"),
            file_name: "movie1.mkv".to_string(),
            file_size: 2_000_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        }])
        .await
        .expect("failed to insert media item");

    let items = media_repo
        .list_by_library("lib1", 1, 0)
        .await
        .expect("failed to list items");
    let media_item_id = items[0].id.expect("media item id should be populated");

    // 3. Test SubtitleRepository::create
    let track1 = SubtitleTrack {
        id: 0,
        media_item_id,
        source: SubtitleSource::Sidecar,
        language: "eng".to_string(),
        title: Some("English SDH".to_string()),
        format: SubtitleFormat::Srt,
        file_path: Some("/media/movies/movie1.en.srt".to_string()),
        stream_index: None,
        is_default: false,
        is_forced: false,
        created_at: 1700000100,
    };

    let id1 = sub_repo.create(&track1).await.expect("create failed");
    assert!(id1 > 0, "generated primary key ID should be > 0");

    let fetched1 = sub_repo
        .find_by_id(id1)
        .await
        .expect("find_by_id failed")
        .expect("track should exist");
    assert_eq!(fetched1.id, id1);
    assert_eq!(fetched1.media_item_id, media_item_id);
    assert_eq!(fetched1.source, SubtitleSource::Sidecar);
    assert_eq!(fetched1.language, "eng");
    assert_eq!(fetched1.title.as_deref(), Some("English SDH"));
    assert_eq!(fetched1.format, SubtitleFormat::Srt);
    assert_eq!(
        fetched1.file_path.as_deref(),
        Some("/media/movies/movie1.en.srt")
    );
    assert_eq!(fetched1.stream_index, None);
    assert!(!fetched1.is_default);
    assert!(!fetched1.is_forced);
    assert_eq!(fetched1.created_at, 1700000100);

    // 4. Test SubtitleRepository::batch_insert (including empty slice)
    sub_repo
        .batch_insert(&[])
        .await
        .expect("batch_insert empty slice should succeed");

    let track2 = SubtitleTrack {
        id: 0,
        media_item_id,
        source: SubtitleSource::Embedded,
        language: "ara".to_string(),
        title: Some("Arabic Forced".to_string()),
        format: SubtitleFormat::Vtt,
        file_path: None,
        stream_index: Some(2),
        is_default: false,
        is_forced: true,
        created_at: 1700000200,
    };

    let track3 = SubtitleTrack {
        id: 0,
        media_item_id,
        source: SubtitleSource::Downloaded,
        language: "fre".to_string(),
        title: None,
        format: SubtitleFormat::Ass,
        file_path: Some("/cache/subtitles/fre.ass".to_string()),
        stream_index: None,
        is_default: true,
        is_forced: false,
        created_at: 1700000300,
    };

    sub_repo
        .batch_insert(&[track2, track3])
        .await
        .expect("batch_insert should succeed");

    // 5. Test SubtitleRepository::find_by_media_item with ordering
    // Expected order: is_default DESC, is_forced DESC, language ASC, id ASC
    // 1st: track3 (is_default = true)
    // 2nd: track2 (is_default = false, is_forced = true)
    // 3rd: track1 (is_default = false, is_forced = false, language = "eng")
    let tracks = sub_repo
        .find_by_media_item(media_item_id)
        .await
        .expect("find_by_media_item failed");
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].language, "fre");
    assert!(tracks[0].is_default);
    assert_eq!(tracks[1].language, "ara");
    assert!(tracks[1].is_forced);
    assert_eq!(tracks[2].language, "eng");
    assert!(!tracks[2].is_default);
    assert!(!tracks[2].is_forced);

    let id3 = tracks[0].id;
    let id2 = tracks[1].id;

    // 6. Test SubtitleRepository::set_default
    // Previously track3 was default. Now set track1 as default.
    sub_repo
        .set_default(id1, media_item_id)
        .await
        .expect("set_default failed");

    let updated_track1 = sub_repo
        .find_by_id(id1)
        .await
        .expect("find_by_id failed")
        .expect("track1 should exist");
    assert!(updated_track1.is_default);

    let updated_track3 = sub_repo
        .find_by_id(id3)
        .await
        .expect("find_by_id failed")
        .expect("track3 should exist");
    assert!(!updated_track3.is_default);

    // Verify find_by_media_item now reflects track1 as first
    let tracks_after_default = sub_repo
        .find_by_media_item(media_item_id)
        .await
        .expect("find_by_media_item failed");
    assert_eq!(tracks_after_default[0].id, id1);
    assert!(tracks_after_default[0].is_default);

    // 7. Test SubtitleRepository::delete
    let deleted = sub_repo.delete(id2).await.expect("delete failed");
    assert!(deleted, "delete should return true for existing row");

    let not_found = sub_repo.find_by_id(id2).await.expect("find_by_id failed");
    assert!(not_found.is_none(), "deleted track should return None");

    let deleted_again = sub_repo.delete(id2).await.expect("delete failed");
    assert!(
        !deleted_again,
        "deleting already deleted track should return false"
    );

    // 8. Test Foreign Key Cascade Delete
    // Insert another media item and a subtitle for it
    media_repo
        .upsert_batch(&[MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Movie to Delete".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: 1700000400,
            file_path: PathBuf::from("/media/movies/to_delete.mkv"),
            file_name: "to_delete.mkv".to_string(),
            file_size: 1_000_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        }])
        .await
        .expect("failed to insert second media item");

    let items_all = media_repo
        .list_by_library("lib1", 10, 0)
        .await
        .expect("failed to list items");
    let to_delete_item = items_all
        .iter()
        .find(|i| i.title == "Movie to Delete")
        .expect("should find Movie to Delete");
    let to_delete_id = to_delete_item.id.expect("id should be present");

    let temp_track = SubtitleTrack {
        id: 0,
        media_item_id: to_delete_id,
        source: SubtitleSource::Sidecar,
        language: "spa".to_string(),
        title: None,
        format: SubtitleFormat::Srt,
        file_path: None,
        stream_index: None,
        is_default: false,
        is_forced: false,
        created_at: 1700000500,
    };
    let temp_sub_id = sub_repo
        .create(&temp_track)
        .await
        .expect("failed to create temp track");

    assert!(
        sub_repo
            .find_by_id(temp_sub_id)
            .await
            .expect("find_by_id failed")
            .is_some(),
        "temp track should exist before media item deletion"
    );

    // Delete media item
    let media_deleted = media_repo
        .delete_by_path(PathBuf::from("/media/movies/to_delete.mkv"))
        .await
        .expect("delete_by_path failed");
    assert!(media_deleted, "media item should be deleted");

    // Verify foreign key cascade deleted the subtitle row
    let cascaded = sub_repo
        .find_by_id(temp_sub_id)
        .await
        .expect("find_by_id failed");
    assert!(
        cascaded.is_none(),
        "subtitle row should be cascade-deleted when media item is deleted"
    );
}
