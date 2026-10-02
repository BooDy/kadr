// crates/kadr-storage/tests/widget_queries_test.rs
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};
use std::path::PathBuf;

async fn setup_test_data() -> (MediaItemRepository, LibraryRepository) {
    let pool = create_in_memory_pool().expect("failed to create pool");
    initialize_database(&pool)
        .await
        .expect("failed to initialize db");

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    let movies_lib = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: PathBuf::from("/media/movies"),
        media_type: MediaType::Movie,
        created_at: 1000,
    };
    let shows_lib = Library {
        id: "shows".to_string(),
        name: "Shows".to_string(),
        path: PathBuf::from("/media/shows"),
        media_type: MediaType::Show,
        created_at: 1000,
    };

    lib_repo.create(&movies_lib).await.unwrap();
    lib_repo.create(&shows_lib).await.unwrap();

    let items = vec![
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Inception".to_string(),
            original_title: None,
            release_year: Some(2010),
            added_at: 1000,
            file_path: PathBuf::from("/media/movies/Inception.mkv"),
            file_name: "Inception.mkv".to_string(),
            file_size: 10_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.8),
                genres: vec!["Action".to_string(), "Sci-Fi".to_string()],
                backdrop_path: Some("/media/movies/inception-backdrop.jpg".to_string()),
                poster_path: Some("/media/movies/inception-poster.jpg".to_string()),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Interstellar".to_string(),
            original_title: None,
            release_year: Some(2014),
            added_at: 2000,
            file_path: PathBuf::from("/media/movies/Interstellar.mkv"),
            file_name: "Interstellar.mkv".to_string(),
            file_size: 12_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.7),
                genres: vec!["Sci-Fi".to_string(), "Drama".to_string()],
                backdrop_path: None,
                poster_path: Some("/media/movies/interstellar-poster.jpg".to_string()),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Memento".to_string(),
            original_title: None,
            release_year: Some(2000),
            added_at: 3000,
            file_path: PathBuf::from("/media/movies/Memento.mkv"),
            file_name: "Memento.mkv".to_string(),
            file_size: 8_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.4),
                genres: vec!["Mystery".to_string(), "Thriller".to_string()],
                backdrop_path: Some("/media/movies/memento-backdrop.jpg".to_string()),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "The Prestige".to_string(),
            original_title: None,
            release_year: Some(2006),
            added_at: 4000,
            file_path: PathBuf::from("/media/movies/ThePrestige.mkv"),
            file_name: "ThePrestige.mkv".to_string(),
            file_size: 9_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.5),
                genres: vec!["Drama".to_string(), "Mystery".to_string()],
                backdrop_path: Some("/media/movies/prestige-backdrop.jpg".to_string()),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Primer".to_string(),
            original_title: None,
            release_year: Some(2004),
            added_at: 7000,
            file_path: PathBuf::from("/media/movies/Primer.mkv"),
            file_name: "Primer.mkv".to_string(),
            file_size: 5_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: None,
                genres: vec!["Sci-Fi".to_string()],
                backdrop_path: None,
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Severance - S01E02 - Half Loop".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 5000,
            file_path: PathBuf::from("/media/shows/Severance/S01E02.mkv"),
            file_name: "S01E02.mkv".to_string(),
            file_size: 6_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.6),
                genres: vec!["Sci-Fi".to_string(), "Thriller".to_string()],
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(2),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Severance - S01E01 - Good News About Hell".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 4900,
            file_path: PathBuf::from("/media/shows/Severance/S01E01.mkv"),
            file_name: "S01E01.mkv".to_string(),
            file_size: 6_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.5),
                genres: vec!["Sci-Fi".to_string(), "Thriller".to_string()],
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(1),
                ..Default::default()
            },
        },
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Severance - S02E01 - Hello Lumon".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 6000,
            file_path: PathBuf::from("/media/shows/Severance/S02E01.mkv"),
            file_name: "S02E01.mkv".to_string(),
            file_size: 6_000_000,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.9),
                genres: vec!["Sci-Fi".to_string(), "Thriller".to_string()],
                series_title: Some("Severance".to_string()),
                season: Some(2),
                episode: Some(1),
                ..Default::default()
            },
        },
    ];

    media_repo
        .upsert_batch(&items)
        .await
        .expect("upsert failed");
    (media_repo, lib_repo)
}

#[tokio::test]
async fn test_migration_003_creates_rating_index() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let conn = pool.get().await.unwrap();
    conn.interact(|c| {
        let index_exists: i32 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_media_rating'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index_exists, 1);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_find_recently_added() {
    let (media_repo, _) = setup_test_data().await;

    // 1. Without library filter (global)
    let global_recent = media_repo.find_recently_added(None, 3, 0).await.unwrap();
    assert_eq!(global_recent.len(), 3);
    assert_eq!(global_recent[0].title, "Primer"); // added_at 7000
    assert_eq!(global_recent[1].title, "Severance - S02E01 - Hello Lumon"); // added_at 6000
    assert_eq!(global_recent[2].title, "Severance - S01E02 - Half Loop"); // added_at 5000

    // Offset test
    let global_recent_page2 = media_repo.find_recently_added(None, 2, 3).await.unwrap();
    assert_eq!(global_recent_page2.len(), 2);
    assert_eq!(
        global_recent_page2[0].title,
        "Severance - S01E01 - Good News About Hell"
    ); // added_at 4900
    assert_eq!(global_recent_page2[1].title, "The Prestige"); // added_at 4000

    // 2. With library filter
    let movie_recent = media_repo
        .find_recently_added(Some("movies"), 2, 0)
        .await
        .unwrap();
    assert_eq!(movie_recent.len(), 2);
    assert_eq!(movie_recent[0].title, "Primer"); // added_at 7000
    assert_eq!(movie_recent[1].title, "The Prestige"); // added_at 4000
}

#[tokio::test]
async fn test_find_top_rated() {
    let (media_repo, _) = setup_test_data().await;

    let top = media_repo.find_top_rated(4, 0).await.unwrap();
    assert_eq!(top.len(), 4);
    assert_eq!(top[0].title, "Severance - S02E01 - Hello Lumon"); // rating 8.9
    assert_eq!(top[1].title, "Inception"); // rating 8.8
    assert_eq!(top[2].title, "Interstellar"); // rating 8.7
    assert_eq!(top[3].title, "Severance - S01E02 - Half Loop"); // rating 8.6

    // With offset
    let top_page2 = media_repo.find_top_rated(2, 4).await.unwrap();
    assert_eq!(top_page2.len(), 2);
    let page2_titles: Vec<&str> = top_page2.iter().map(|m| m.title.as_str()).collect();
    assert!(page2_titles.contains(&"The Prestige"));
    assert!(page2_titles.contains(&"Severance - S01E01 - Good News About Hell"));
}

#[tokio::test]
async fn test_find_by_genre() {
    let (media_repo, _) = setup_test_data().await;

    // 1. Query "Mystery"
    let mystery = media_repo.find_by_genre("Mystery", 10, 0).await.unwrap();
    assert_eq!(mystery.len(), 2);
    let mystery_titles: Vec<&str> = mystery.iter().map(|m| m.title.as_str()).collect();
    assert!(mystery_titles.contains(&"Memento"));
    assert!(mystery_titles.contains(&"The Prestige"));

    // 2. Query case-insensitive "action"
    let action = media_repo.find_by_genre("action", 10, 0).await.unwrap();
    assert_eq!(action.len(), 1);
    assert_eq!(action[0].title, "Inception");

    // 3. Query "Sci-Fi" with pagination
    let scifi_page1 = media_repo.find_by_genre("Sci-Fi", 2, 0).await.unwrap();
    assert_eq!(scifi_page1.len(), 2);
    let scifi_page2 = media_repo.find_by_genre("Sci-Fi", 2, 2).await.unwrap();
    assert_eq!(scifi_page2.len(), 2);
}

#[tokio::test]
async fn test_find_by_library_paginated() {
    let (media_repo, _) = setup_test_data().await;

    // 1. Default / title:asc
    let (items, total) = media_repo
        .find_by_library_paginated("movies", 2, 0, Some("title:asc"))
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Inception");
    assert_eq!(items[1].title, "Interstellar");

    // 2. release_year:desc
    let (items_by_year, total_year) = media_repo
        .find_by_library_paginated("movies", 5, 0, Some("release_year:desc"))
        .await
        .unwrap();
    assert_eq!(total_year, 5);
    assert_eq!(items_by_year[0].title, "Interstellar"); // 2014
    assert_eq!(items_by_year[1].title, "Inception"); // 2010
    assert_eq!(items_by_year[2].title, "The Prestige"); // 2006
    assert_eq!(items_by_year[3].title, "Primer"); // 2004
    assert_eq!(items_by_year[4].title, "Memento"); // 2000

    // 3. added_at:desc
    let (items_by_added, _) = media_repo
        .find_by_library_paginated("movies", 2, 0, Some("added_at:desc"))
        .await
        .unwrap();
    assert_eq!(items_by_added[0].title, "Primer"); // 7000
    assert_eq!(items_by_added[1].title, "The Prestige"); // 4000
}

#[tokio::test]
async fn test_find_spotlight_candidate() {
    let (media_repo, _) = setup_test_data().await;

    let candidate = media_repo.find_spotlight_candidate().await.unwrap();
    assert!(candidate.is_some());
    let item = candidate.unwrap();
    // Inception has backdrop_path and highest rating among items with backdrop (8.8)
    assert_eq!(item.title, "Inception");
    assert!(item.metadata.backdrop_path.is_some());
}

#[tokio::test]
async fn test_find_episodes_by_series() {
    let (media_repo, _) = setup_test_data().await;

    let episodes = media_repo
        .find_episodes_by_series("Severance")
        .await
        .unwrap();
    assert_eq!(episodes.len(), 3);
    assert_eq!(
        episodes[0].title,
        "Severance - S01E01 - Good News About Hell"
    );
    assert_eq!(episodes[1].title, "Severance - S01E02 - Half Loop");
    assert_eq!(episodes[2].title, "Severance - S02E01 - Hello Lumon");
}
