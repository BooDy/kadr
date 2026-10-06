// crates/kadr-storage/tests/widget_queries_test.rs
use kadr_core::ast::WidgetFilterConfig;
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
        ..Default::default()
    };
    let shows_lib = Library {
        id: "shows".to_string(),
        name: "Shows".to_string(),
        path: PathBuf::from("/media/shows"),
        media_type: MediaType::Show,
        created_at: 1000,
        ..Default::default()
    };

    lib_repo.insert(&movies_lib).await.unwrap();
    lib_repo.insert(&shows_lib).await.unwrap();

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
            item_type: MediaType::Show,
            title: "Severance".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: 6500,
            file_path: PathBuf::from("/media/shows/Severance"),
            file_name: "Severance".to_string(),
            file_size: 0,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                rating: Some(8.9),
                genres: vec!["Sci-Fi".to_string(), "Thriller".to_string()],
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
    let global_recent = media_repo
        .find_recently_added_paginated(None, 3, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(global_recent.len(), 3);
    assert_eq!(global_recent[0].title, "Primer"); // added_at 7000
    assert_eq!(global_recent[1].title, "Severance"); // added_at 6500 (Show, loose episodes excluded)
    assert_eq!(global_recent[2].title, "The Prestige"); // added_at 4000

    // Direct widget query helper test
    let direct_recent = media_repo.find_recently_added(3, &[]).await.unwrap();
    assert_eq!(direct_recent.len(), 3);
    assert_eq!(direct_recent[0].title, "Primer");

    // Offset test
    let global_recent_page2 = media_repo
        .find_recently_added_paginated(None, 2, 3, &[], None)
        .await
        .unwrap();
    assert_eq!(global_recent_page2.len(), 2);
    assert_eq!(global_recent_page2[0].title, "Memento"); // added_at 3000
    assert_eq!(global_recent_page2[1].title, "Interstellar"); // added_at 2000

    // 2. With library filter
    let movie_recent = media_repo
        .find_recently_added_paginated(Some("movies"), 2, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(movie_recent.len(), 2);
    assert_eq!(movie_recent[0].title, "Primer"); // added_at 7000
    assert_eq!(movie_recent[1].title, "The Prestige"); // added_at 4000
}

#[tokio::test]
async fn test_find_top_rated() {
    let (media_repo, _) = setup_test_data().await;

    let top = media_repo
        .find_top_rated_paginated(4, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(top.len(), 4);
    assert_eq!(top[0].title, "Severance"); // rating 8.9 (Show, loose episodes excluded)
    assert_eq!(top[1].title, "Inception"); // rating 8.8
    assert_eq!(top[2].title, "Interstellar"); // rating 8.7
    assert_eq!(top[3].title, "The Prestige"); // rating 8.5

    // Direct widget query helper test
    let direct_top = media_repo.find_top_rated(4, &[]).await.unwrap();
    assert_eq!(direct_top.len(), 4);

    // With offset
    let top_page2 = media_repo
        .find_top_rated_paginated(2, 4, &[], None)
        .await
        .unwrap();
    assert_eq!(top_page2.len(), 1);
    let page2_titles: Vec<&str> = top_page2.iter().map(|m| m.title.as_str()).collect();
    assert_eq!(page2_titles, vec!["Memento"]); // rating 8.4
}

#[tokio::test]
async fn test_find_by_genre() {
    let (media_repo, _) = setup_test_data().await;

    // 1. Query "Mystery"
    let mystery = media_repo
        .find_by_genre_paginated("Mystery", 10, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(mystery.len(), 2);
    let mystery_titles: Vec<&str> = mystery.iter().map(|m| m.title.as_str()).collect();
    assert!(mystery_titles.contains(&"Memento"));
    assert!(mystery_titles.contains(&"The Prestige"));

    // 2. Query case-insensitive "action"
    let action = media_repo
        .find_by_genre_paginated("action", 10, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(action.len(), 1);
    assert_eq!(action[0].title, "Inception");

    // 3. Query "Sci-Fi" with pagination
    let scifi_page1 = media_repo
        .find_by_genre_paginated("Sci-Fi", 2, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(scifi_page1.len(), 2);
    let scifi_page2 = media_repo
        .find_by_genre_paginated("Sci-Fi", 2, 2, &[], None)
        .await
        .unwrap();
    assert_eq!(scifi_page2.len(), 2);
}

#[tokio::test]
async fn test_find_by_library_paginated() {
    let (media_repo, _) = setup_test_data().await;

    // 1. Default / title:asc
    let (items, total) = media_repo
        .find_by_library_paginated("movies", 2, 0, Some("title:asc"), &[], None)
        .await
        .unwrap();
    assert_eq!(total, 5);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Inception");
    assert_eq!(items[1].title, "Interstellar");

    // 2. release_year:desc
    let (items_by_year, total_year) = media_repo
        .find_by_library_paginated("movies", 5, 0, Some("release_year:desc"), &[], None)
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
        .find_by_library_paginated("movies", 2, 0, Some("added_at:desc"), &[], None)
        .await
        .unwrap();
    assert_eq!(items_by_added[0].title, "Primer"); // 7000
    assert_eq!(items_by_added[1].title, "The Prestige"); // 4000
}

#[tokio::test]
async fn test_find_spotlight_candidate() {
    let (media_repo, _) = setup_test_data().await;

    let candidate = media_repo.find_spotlight_candidate(&[], None).await.unwrap();
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

#[tokio::test]
async fn test_filter_exclude_private() {
    let (media_repo, lib_repo) = setup_test_data().await;

    let priv_lib = Library {
        id: "private_vault".to_string(),
        name: "Private Vault".to_string(),
        path: PathBuf::from("/media/private"),
        media_type: MediaType::Movie,
        is_private: true,
        created_at: 1000,
        ..Default::default()
    };
    lib_repo.insert(&priv_lib).await.unwrap();

    let priv_item = MediaItem {
        id: None,
        library_id: "private_vault".to_string(),
        item_type: MediaType::Movie,
        title: "Confidential Project".to_string(),
        added_at: 8000,
        file_path: PathBuf::from("/media/private/confidential.mkv"),
        file_name: "confidential.mkv".to_string(),
        metadata: MediaMetadata {
            rating: Some(9.9),
            backdrop_path: Some("/media/private/backdrop.jpg".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    media_repo.upsert_batch(&[priv_item]).await.unwrap();

    let unlocked = vec!["private_vault".to_string()];

    // 1. Without exclude_private, unlocked library shows up in recently added
    let results_unlocked = media_repo
        .find_recently_added_paginated(None, 10, 0, &unlocked, None)
        .await
        .unwrap();
    assert!(results_unlocked
        .iter()
        .any(|m| m.title == "Confidential Project"));

    // 2. With exclude_private = true, private media is EXCLUDED even though library is unlocked
    let filter = WidgetFilterConfig {
        exclude_private: true,
        ..Default::default()
    };
    let results_filtered = media_repo
        .find_recently_added_paginated(None, 10, 0, &unlocked, Some(&filter))
        .await
        .unwrap();
    assert!(!results_filtered
        .iter()
        .any(|m| m.title == "Confidential Project"));

    // 3. Spotlight candidate with exclude_private = true ignores the higher-rated private item
    let spotlight_unfiltered = media_repo
        .find_spotlight_candidate(&unlocked, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(spotlight_unfiltered.title, "Confidential Project");

    let spotlight_filtered = media_repo
        .find_spotlight_candidate(&unlocked, Some(&filter))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(spotlight_filtered.title, "Inception");
}

#[tokio::test]
async fn test_filter_exclude_library_ids() {
    let (media_repo, _) = setup_test_data().await;

    // Filter out "shows" library
    let filter = WidgetFilterConfig {
        exclude_library_ids: vec!["shows".to_string()],
        ..Default::default()
    };

    let items = media_repo
        .find_recently_added_paginated(None, 20, 0, &[], Some(&filter))
        .await
        .unwrap();

    assert!(!items.is_empty());
    for item in &items {
        assert_ne!(item.library_id, "shows");
        assert_eq!(item.library_id, "movies");
    }
    assert!(!items.iter().any(|m| m.title.starts_with("Severance")));
}

#[tokio::test]
async fn test_filter_exclude_genres() {
    let (media_repo, _) = setup_test_data().await;

    // Exclude "Sci-Fi" with different casing ("sci-fi")
    let filter = WidgetFilterConfig {
        exclude_genres: vec!["sci-fi".to_string()],
        ..Default::default()
    };

    let items = media_repo
        .find_recently_added_paginated(None, 20, 0, &[], Some(&filter))
        .await
        .unwrap();

    assert!(!items.is_empty());
    for item in &items {
        let has_scifi = item
            .metadata
            .genres
            .iter()
            .any(|g| g.eq_ignore_ascii_case("sci-fi"));
        assert!(
            !has_scifi,
            "Item {} has genre sci-fi but was not excluded",
            item.title
        );
    }

    let titles: Vec<&str> = items.iter().map(|m| m.title.as_str()).collect();
    assert!(titles.contains(&"The Prestige"));
    assert!(titles.contains(&"Memento"));
}

#[tokio::test]
async fn test_filter_max_age_days() {
    let (media_repo, _) = setup_test_data().await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let recent_item = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Recent Release".to_string(),
        added_at: now - 3 * 86_400, // 3 days ago
        file_path: PathBuf::from("/media/movies/recent.mkv"),
        file_name: "recent.mkv".to_string(),
        ..Default::default()
    };
    let old_item = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Old Release".to_string(),
        added_at: now - 60 * 86_400, // 60 days ago
        file_path: PathBuf::from("/media/movies/old.mkv"),
        file_name: "old.mkv".to_string(),
        ..Default::default()
    };
    media_repo
        .upsert_batch(&[recent_item, old_item])
        .await
        .unwrap();

    let filter = WidgetFilterConfig {
        max_age_days: Some(30), // Cutoff 30 days
        ..Default::default()
    };

    let items = media_repo
        .find_recently_added_paginated(Some("movies"), 20, 0, &[], Some(&filter))
        .await
        .unwrap();

    let titles: Vec<&str> = items.iter().map(|m| m.title.as_str()).collect();
    assert!(titles.contains(&"Recent Release"));
    assert!(!titles.contains(&"Old Release"));
    assert!(!titles.contains(&"Primer"));
}

#[tokio::test]
async fn test_filter_combined_filters() {
    let (media_repo, lib_repo) = setup_test_data().await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let priv_lib = Library {
        id: "private_stream".to_string(),
        name: "Private Stream".to_string(),
        path: PathBuf::from("/media/priv_stream"),
        media_type: MediaType::Movie,
        is_private: true,
        ..Default::default()
    };
    lib_repo.insert(&priv_lib).await.unwrap();

    let items = vec![
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Fresh Drama".to_string(),
            added_at: now - 2 * 86_400,
            file_path: PathBuf::from("/media/movies/fresh_drama.mkv"),
            file_name: "fresh_drama.mkv".to_string(),
            metadata: MediaMetadata {
                genres: vec!["Drama".to_string()],
                rating: Some(9.0),
                ..Default::default()
            },
            ..Default::default()
        },
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Fresh SciFi".to_string(),
            added_at: now - 2 * 86_400,
            file_path: PathBuf::from("/media/movies/fresh_scifi.mkv"),
            file_name: "fresh_scifi.mkv".to_string(),
            metadata: MediaMetadata {
                genres: vec!["Sci-Fi".to_string()],
                rating: Some(9.2),
                ..Default::default()
            },
            ..Default::default()
        },
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Stale Drama".to_string(),
            added_at: now - 40 * 86_400,
            file_path: PathBuf::from("/media/movies/stale_drama.mkv"),
            file_name: "stale_drama.mkv".to_string(),
            metadata: MediaMetadata {
                genres: vec!["Drama".to_string()],
                rating: Some(9.1),
                ..Default::default()
            },
            ..Default::default()
        },
        MediaItem {
            id: None,
            library_id: "private_stream".to_string(),
            item_type: MediaType::Movie,
            title: "Private Fresh Drama".to_string(),
            added_at: now - 2 * 86_400,
            file_path: PathBuf::from("/media/priv_stream/priv_drama.mkv"),
            file_name: "priv_drama.mkv".to_string(),
            metadata: MediaMetadata {
                genres: vec!["Drama".to_string()],
                rating: Some(9.9),
                ..Default::default()
            },
            ..Default::default()
        },
    ];
    media_repo.upsert_batch(&items).await.unwrap();

    let unlocked = vec!["private_stream".to_string()];
    let combined_filter = WidgetFilterConfig {
        exclude_private: true,
        exclude_genres: vec!["Sci-Fi".to_string()],
        max_age_days: Some(10),
        exclude_library_ids: vec!["shows".to_string()],
    };

    let results = media_repo
        .find_recently_added_paginated(None, 20, 0, &unlocked, Some(&combined_filter))
        .await
        .unwrap();

    let titles: Vec<&str> = results.iter().map(|m| m.title.as_str()).collect();
    assert_eq!(titles, vec!["Fresh Drama"]);
}

#[test]
fn test_build_filter_clauses_logic() {
    use kadr_storage::repos::widget_queries::build_filter_clauses;

    // 1. None filter with unlocked
    let (clause, params) = build_filter_clauses(None, &["lib1".to_string()], 1_000_000);
    assert!(clause.contains("l.is_private = 0 OR l.id IN (?)"));
    assert_eq!(params.len(), 1);

    // 2. exclude_private = true overrides unlocked_ids
    let f1 = WidgetFilterConfig {
        exclude_private: true,
        ..Default::default()
    };
    let (clause, params) = build_filter_clauses(Some(&f1), &["lib1".to_string()], 1_000_000);
    assert_eq!(clause, "(l.is_private = 0)");
    assert_eq!(params.len(), 0);

    // 3. exclude_library_ids
    let f2 = WidgetFilterConfig {
        exclude_library_ids: vec!["libA".to_string(), "libB".to_string()],
        ..Default::default()
    };
    let (clause, params) = build_filter_clauses(Some(&f2), &[], 1_000_000);
    assert!(clause.contains("m.library_id NOT IN (?, ?)"));
    assert_eq!(params.len(), 2);

    // 4. exclude_genres lowercase
    let f3 = WidgetFilterConfig {
        exclude_genres: vec!["Action".to_string(), "Horror".to_string()],
        ..Default::default()
    };
    let (clause, params) = build_filter_clauses(Some(&f3), &[], 1_000_000);
    assert!(clause.contains("NOT EXISTS"));
    assert!(clause.contains("json_each(json_extract(m.metadata, '$.genres'))"));
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], rusqlite::types::Value::Text("action".to_string()));
    assert_eq!(params[1], rusqlite::types::Value::Text("horror".to_string()));

    // 5. max_age_days
    let f4 = WidgetFilterConfig {
        max_age_days: Some(7),
        ..Default::default()
    };
    let (clause, params) = build_filter_clauses(Some(&f4), &[], 1_000_000);
    assert!(clause.contains("m.added_at >= ?"));
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], rusqlite::types::Value::Integer(1_000_000 - 7 * 86_400));
}

#[tokio::test]
async fn test_catalog_queries_exclude_loose_episodes() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    let repo = MediaItemRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool);

    lib_repo
        .insert(&Library {
            id: "movies".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/media/movies"),
            media_type: MediaType::Movie,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "shows".to_string(),
            name: "Shows".to_string(),
            path: PathBuf::from("/media/shows"),
            media_type: MediaType::Show,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let movie = MediaItem {
        id: None,
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Test Movie".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 2000,
        file_path: PathBuf::from("/media/movies/movie.mkv"),
        file_name: "movie.mkv".to_string(),
        file_size: 1_000_000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            rating: Some(8.5),
            genres: vec!["Sci-Fi".to_string()],
            ..Default::default()
        },
    };

    let show = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Show,
        title: "Test Show".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 2010,
        file_path: PathBuf::from("/media/shows/show"),
        file_name: "show".to_string(),
        file_size: 0,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            rating: Some(9.0),
            genres: vec!["Sci-Fi".to_string()],
            ..Default::default()
        },
    };

    let ep1 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Test Show S01E01".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 2020,
        file_path: PathBuf::from("/media/shows/show/s01e01.mkv"),
        file_name: "s01e01.mkv".to_string(),
        file_size: 500_000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            series_title: Some("Test Show".to_string()),
            season: Some(1),
            episode: Some(1),
            rating: Some(8.8),
            genres: vec!["Sci-Fi".to_string()],
            ..Default::default()
        },
    };

    let ep2 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Test Show S01E02".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 2030,
        file_path: PathBuf::from("/media/shows/show/s01e02.mkv"),
        file_name: "s01e02.mkv".to_string(),
        file_size: 500_000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            series_title: Some("Test Show".to_string()),
            season: Some(1),
            episode: Some(2),
            rating: Some(8.9),
            genres: vec!["Sci-Fi".to_string()],
            ..Default::default()
        },
    };

    let ep3 = MediaItem {
        id: None,
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Test Show S01E03".to_string(),
        original_title: None,
        release_year: Some(2023),
        added_at: 2040,
        file_path: PathBuf::from("/media/shows/show/s01e03.mkv"),
        file_name: "s01e03.mkv".to_string(),
        file_size: 500_000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            series_title: Some("Test Show".to_string()),
            season: Some(1),
            episode: Some(3),
            rating: Some(9.1),
            genres: vec!["Sci-Fi".to_string()],
            ..Default::default()
        },
    };

    repo.upsert_batch(&[movie, show, ep1, ep2, ep3]).await.unwrap();

    // 1. find_by_library_paginated returns only Show, not Episodes
    let (shows, total) = repo
        .find_by_library_paginated("shows", 10, 0, None, &[], None)
        .await
        .unwrap();
    assert_eq!(total, 1, "Total shows count should exclude loose episodes");
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].title, "Test Show");
    assert_eq!(shows[0].item_type, MediaType::Show);

    // 2. find_recently_added_paginated excludes episodes
    let recent = repo
        .find_recently_added_paginated(None, 10, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(
        recent.len(),
        2,
        "Recently added should only contain Movie and Show"
    );
    assert!(recent.iter().all(|item| item.item_type != MediaType::Episode));

    // 3. find_top_rated_paginated excludes episodes
    let top = repo.find_top_rated_paginated(10, 0, &[], None).await.unwrap();
    assert_eq!(top.len(), 2, "Top rated should only contain Movie and Show");
    assert!(top.iter().all(|item| item.item_type != MediaType::Episode));

    // 4. find_by_genre_paginated excludes episodes
    let sci_fi = repo
        .find_by_genre_paginated("Sci-Fi", 10, 0, &[], None)
        .await
        .unwrap();
    assert_eq!(
        sci_fi.len(),
        2,
        "Genre Sci-Fi should only contain Movie and Show"
    );
    assert!(sci_fi.iter().all(|item| item.item_type != MediaType::Episode));
}



