use kadr_core::models::{MediaItem, MediaMetadata, MediaType};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepo, MediaItemRepo, WidgetQueries};

#[tokio::test]
async fn test_private_library_media_filtering() {
    let pool = create_in_memory_pool().expect("db init failed");
    initialize_database(&pool).await.expect("db init failed");
    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());

    lib_repo
        .create(
            "pub-1",
            "Public",
            "/media/pub",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();
    lib_repo
        .create(
            "priv-1",
            "Private",
            "/media/priv",
            MediaType::Movie,
            true,
            Some("pin_hash"),
        )
        .await
        .unwrap();

    let pub_item = MediaItem {
        id: Some(1),
        library_id: "pub-1".into(),
        item_type: MediaType::Movie,
        title: "Public Movie".into(),
        file_path: "/media/pub/m.mp4".into(),
        file_name: "m.mp4".into(),
        ..Default::default()
    };
    let priv_item = MediaItem {
        id: Some(2),
        library_id: "priv-1".into(),
        item_type: MediaType::Movie,
        title: "Private Video".into(),
        file_path: "/media/priv/v.mp4".into(),
        file_name: "v.mp4".into(),
        ..Default::default()
    };
    item_repo
        .batch_upsert(&[pub_item, priv_item])
        .await
        .unwrap();

    // Query with empty unlocked list -> only public item returned
    let locked_results = item_repo.find_recently_added(10, &[]).await.unwrap();
    assert_eq!(locked_results.len(), 1);
    assert_eq!(locked_results[0].title, "Public Movie");

    // Query with "priv-1" unlocked -> both returned
    let unlocked_results = item_repo
        .find_recently_added(10, &["priv-1".to_string()])
        .await
        .unwrap();
    assert_eq!(unlocked_results.len(), 2);
}

#[tokio::test]
async fn test_widget_queries_and_media_isolation() {
    let pool = create_in_memory_pool().expect("db init failed");
    initialize_database(&pool).await.expect("db init failed");
    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());
    let widget_queries = WidgetQueries::new(pool.clone());

    lib_repo
        .create("pub", "Public", "/pub", MediaType::Movie, false, None)
        .await
        .unwrap();
    lib_repo
        .create(
            "priv",
            "Private",
            "/priv",
            MediaType::Movie,
            true,
            Some("pin"),
        )
        .await
        .unwrap();

    let pub_item = MediaItem {
        id: Some(1),
        library_id: "pub".into(),
        item_type: MediaType::Movie,
        title: "Public Action".into(),
        file_path: "/pub/a.mp4".into(),
        file_name: "a.mp4".into(),
        metadata: MediaMetadata {
            rating: Some(7.5),
            genres: vec!["Action".to_string()],
            backdrop_path: Some("/pub/backdrop.jpg".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let priv_item = MediaItem {
        id: Some(2),
        library_id: "priv".into(),
        item_type: MediaType::Movie,
        title: "Private Action".into(),
        file_path: "/priv/b.mp4".into(),
        file_name: "b.mp4".into(),
        metadata: MediaMetadata {
            rating: Some(9.9), // Higher rating
            genres: vec!["Action".to_string()],
            backdrop_path: Some("/priv/backdrop.jpg".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    item_repo
        .batch_upsert(&[pub_item, priv_item])
        .await
        .unwrap();

    // 1. Top rated query
    let top_locked = widget_queries.find_top_rated(10, &[]).await.unwrap();
    assert_eq!(top_locked.len(), 1);
    assert_eq!(top_locked[0].title, "Public Action");

    let top_unlocked = widget_queries
        .find_top_rated(10, &["priv".to_string()])
        .await
        .unwrap();
    assert_eq!(top_unlocked.len(), 2);
    assert_eq!(top_unlocked[0].title, "Private Action");

    // 2. Spotlight candidate query
    let spot_locked = widget_queries.find_spotlight_candidate(None, &[], None).await.unwrap();
    assert_eq!(spot_locked.unwrap().title, "Public Action");

    let spot_unlocked = widget_queries
        .find_spotlight_candidate(None, &["priv".to_string()], None)
        .await
        .unwrap();
    assert_eq!(spot_unlocked.unwrap().title, "Private Action");

    // 3. Genre query
    let genre_locked = widget_queries
        .find_by_genre("Action", 10, &[])
        .await
        .unwrap();
    assert_eq!(genre_locked.len(), 1);
    assert_eq!(genre_locked[0].title, "Public Action");

    let genre_unlocked = widget_queries
        .find_by_genre("Action", 10, &["priv".to_string()])
        .await
        .unwrap();
    assert_eq!(genre_unlocked.len(), 2);

    // 4. Library paginated query
    let (priv_lib_locked, total_locked) = item_repo
        .find_by_library_paginated("priv", 10, 0, None, &[], None)
        .await
        .unwrap();
    assert_eq!(total_locked, 0);
    assert!(priv_lib_locked.is_empty());

    let (priv_lib_unlocked, total_unlocked) = item_repo
        .find_by_library_paginated("priv", 10, 0, None, &["priv".to_string()], None)
        .await
        .unwrap();
    assert_eq!(total_unlocked, 1);
    assert_eq!(priv_lib_unlocked[0].title, "Private Action");
}
