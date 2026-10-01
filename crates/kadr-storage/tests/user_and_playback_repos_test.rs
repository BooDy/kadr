use std::path::PathBuf;
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};

#[tokio::test]
async fn test_user_and_playback_repositories() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // 1. Create User
    let user = User {
        id: "u123".to_string(),
        username: "ahmed".to_string(),
        pin_hash: "argon2id_hash_sample".to_string(),
        role: UserRole::Standard,
        created_at: 1700000000,
    };
    user_repo.create(&user).await.unwrap();

    let fetched = user_repo.get_by_id("u123").await.unwrap().expect("user not found");
    assert_eq!(fetched.username, "ahmed");
    assert_eq!(fetched.role, UserRole::Standard);

    let fetched_by_name = user_repo.get_by_username("ahmed").await.unwrap().expect("user by name not found");
    assert_eq!(fetched_by_name.id, "u123");

    assert_eq!(user_repo.count().await.unwrap(), 1);
    let all_users = user_repo.list_all().await.unwrap();
    assert_eq!(all_users.len(), 1);
    assert_eq!(all_users[0].id, "u123");

    // 2. Setup library and media item for foreign key reference
    lib_repo.create(&Library {
        id: "lib1".to_string(),
        name: "Films".to_string(),
        path: PathBuf::from("/media"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    }).await.unwrap();

    media_repo.upsert_batch(&[MediaItem {
        id: None,
        library_id: "lib1".to_string(),
        item_type: MediaType::Movie,
        title: "Test Movie".to_string(),
        original_title: None,
        release_year: Some(2020),
        added_at: 1700000000,
        file_path: PathBuf::from("/media/test.mp4"),
        file_name: "test.mp4".to_string(),
        file_size: 1000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata::default(),
    }]).await.unwrap();

    let items = media_repo.list_by_library("lib1", 1, 0).await.unwrap();
    let item_id = items[0].id.unwrap();

    // 3. Upsert playback progress
    playback_repo.upsert_progress("u123", item_id, 350, WatchState::InProgress, 1700000500).await.unwrap();

    let state = playback_repo.get_state("u123", item_id).await.unwrap().expect("state not found");
    assert_eq!(state.playback_position_seconds, 350);
    assert_eq!(state.watch_state, WatchState::InProgress);

    // 4. Update to completed and increment play count
    playback_repo.upsert_progress("u123", item_id, 5000, WatchState::Completed, 1700001000).await.unwrap();
    let state_completed = playback_repo.get_state("u123", item_id).await.unwrap().unwrap();
    assert_eq!(state_completed.watch_state, WatchState::Completed);
    assert_eq!(state_completed.play_count, 1);

    // 5. Query user in-progress states
    let in_progress = playback_repo.list_user_states("u123", Some(WatchState::InProgress), 10).await.unwrap();
    assert_eq!(in_progress.len(), 0);
    let completed = playback_repo.list_user_states("u123", Some(WatchState::Completed), 10).await.unwrap();
    assert_eq!(completed.len(), 1);

    let all_states = playback_repo.list_user_states("u123", None, 10).await.unwrap();
    assert_eq!(all_states.len(), 1);

    // 6. Delete user and verify cascading deletion
    assert!(user_repo.delete("u123").await.unwrap());
    assert_eq!(user_repo.count().await.unwrap(), 0);
    assert!(user_repo.get_by_id("u123").await.unwrap().is_none());
    assert!(playback_repo.get_state("u123", item_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_get_states_for_items_batch() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // 1. Test empty query returns empty map immediately
    let empty_res = playback_repo.get_states_for_items("user1", &[]).await.unwrap();
    assert!(empty_res.is_empty());

    // 2. Setup users
    user_repo.create(&User {
        id: "user1".to_string(),
        username: "user1".to_string(),
        pin_hash: "hash1".to_string(),
        role: UserRole::Standard,
        created_at: 1700000000,
    }).await.unwrap();

    user_repo.create(&User {
        id: "user2".to_string(),
        username: "user2".to_string(),
        pin_hash: "hash2".to_string(),
        role: UserRole::Standard,
        created_at: 1700000000,
    }).await.unwrap();

    // 3. Setup library & media items
    lib_repo.create(&Library {
        id: "lib1".to_string(),
        name: "Films".to_string(),
        path: PathBuf::from("/media"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    }).await.unwrap();

    media_repo.upsert_batch(&[
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Item 1".to_string(),
            original_title: None,
            release_year: Some(2020),
            added_at: 1700000000,
            file_path: PathBuf::from("/media/item1.mp4"),
            file_name: "item1.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        },
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Item 2".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: 1700000000,
            file_path: PathBuf::from("/media/item2.mp4"),
            file_name: "item2.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        },
        MediaItem {
            id: None,
            library_id: "lib1".to_string(),
            item_type: MediaType::Movie,
            title: "Item 3".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 1700000000,
            file_path: PathBuf::from("/media/item3.mp4"),
            file_name: "item3.mp4".to_string(),
            file_size: 1000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata::default(),
        },
    ]).await.unwrap();

    let items = media_repo.list_by_library("lib1", 10, 0).await.unwrap();
    assert_eq!(items.len(), 3);
    let id1 = items[0].id.unwrap();
    let id2 = items[1].id.unwrap();
    let id3 = items[2].id.unwrap();

    // 4. Upsert progress:
    // user1 has progress on item 1 & 2
    playback_repo.upsert_progress("user1", id1, 100, WatchState::InProgress, 1700000100).await.unwrap();
    playback_repo.upsert_progress("user1", id2, 500, WatchState::Completed, 1700000200).await.unwrap();
    // user2 has progress on item 3
    playback_repo.upsert_progress("user2", id3, 300, WatchState::InProgress, 1700000300).await.unwrap();

    // 5. Query user1 states for items 1, 2, 3 and a non-existent item 9999
    let states_user1 = playback_repo.get_states_for_items("user1", &[id1, id2, id3, 9999]).await.unwrap();
    assert_eq!(states_user1.len(), 2);
    assert_eq!(states_user1.get(&id1).unwrap().playback_position_seconds, 100);
    assert_eq!(states_user1.get(&id1).unwrap().watch_state, WatchState::InProgress);
    assert_eq!(states_user1.get(&id2).unwrap().playback_position_seconds, 500);
    assert_eq!(states_user1.get(&id2).unwrap().watch_state, WatchState::Completed);
    assert!(!states_user1.contains_key(&id3)); // user2's state is isolated
    assert!(!states_user1.contains_key(&9999));

    // 6. Query user2 states for items 1, 2, 3
    let states_user2 = playback_repo.get_states_for_items("user2", &[id1, id2, id3]).await.unwrap();
    assert_eq!(states_user2.len(), 1);
    assert_eq!(states_user2.get(&id3).unwrap().playback_position_seconds, 300);
}

