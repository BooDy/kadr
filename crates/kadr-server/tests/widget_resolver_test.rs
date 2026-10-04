// crates/kadr-server/tests/widget_resolver_test.rs
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use kadr_core::ast::{QueryMacro, ScreenId, WidgetNode, WidgetQueryBinding};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, PlaybackState, TechnicalInfo, User, UserRole,
    WatchState,
};
use kadr_server::layout::default_home_layout;
use kadr_server::resolver::{to_card_view_model, WidgetResolver};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[test]
fn test_card_normalization_movie_and_episode() {
    let now = now_secs();

    // 1. Movie with 4K resolution, release year, poster and backdrop
    let movie_4k = MediaItem {
        id: Some(10),
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Interstellar".to_string(),
        original_title: None,
        release_year: Some(2014),
        added_at: now - 30 * 86400, // 30 days ago (not NEW)
        file_path: PathBuf::from("/media/movies/Interstellar.mkv"),
        file_name: "Interstellar.mkv".to_string(),
        file_size: 25_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 10140,
            resolution: Some("4K UHD".to_string()),
            video_codec: Some("hevc".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(6),
            container: Some("mkv".to_string()),
        },
        metadata: MediaMetadata {
            poster_path: Some("/artwork/interstellar_poster.jpg".to_string()),
            backdrop_path: Some("/artwork/interstellar_backdrop.jpg".to_string()),
            rating: Some(8.7),
            genres: vec!["Sci-Fi".to_string(), "Adventure".to_string()],
            ..Default::default()
        },
    };

    let card_4k = to_card_view_model(&movie_4k, None);
    assert_eq!(card_4k.id, 10);
    assert_eq!(card_4k.title, "Interstellar");
    assert_eq!(card_4k.subtitle.as_deref(), Some("2014 • 4K UHD"));
    assert_eq!(card_4k.media_type, "movie");
    assert_eq!(
        card_4k.poster_url.as_deref(),
        Some("/api/v1/artwork/10/poster")
    );
    assert_eq!(
        card_4k.backdrop_url.as_deref(),
        Some("/api/v1/artwork/10/backdrop")
    );
    assert_eq!(card_4k.rating, Some(8.7));
    assert_eq!(card_4k.release_year, Some(2014));
    assert_eq!(card_4k.badge.as_deref(), Some("4K"));
    assert_eq!(card_4k.playback_progress, None);

    // 2. Movie with InProgress playback -> badge must be RESUME, progress calculated
    let playback = PlaybackState {
        user_id: "user-1".to_string(),
        media_item_id: 10,
        playback_position_seconds: 5070, // 50%
        watch_state: WatchState::InProgress,
        last_watched_at: now,
        play_count: 0,
    };
    let card_resumed = to_card_view_model(&movie_4k, Some(&playback));
    assert_eq!(card_resumed.badge.as_deref(), Some("RESUME"));
    let progress = card_resumed.playback_progress.expect("missing progress");
    assert!((progress - 0.5).abs() < 0.01);

    // 3. Movie added recently (within 14 days) -> badge NEW
    let movie_recent = MediaItem {
        id: Some(11),
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Dune: Part Two".to_string(),
        original_title: None,
        release_year: Some(2024),
        added_at: now - 3 * 86400, // 3 days ago -> NEW
        file_path: PathBuf::from("/media/movies/Dune2.mkv"),
        file_name: "Dune2.mkv".to_string(),
        file_size: 15_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 9960,
            resolution: Some("1080p".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata::default(),
    };
    let card_recent = to_card_view_model(&movie_recent, None);
    assert_eq!(card_recent.badge.as_deref(), Some("NEW"));
    assert_eq!(card_recent.subtitle.as_deref(), Some("2024 • 1080p"));

    // 4. Episode card with series_title, season, and episode
    let episode = MediaItem {
        id: Some(20),
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Good News About Hell".to_string(),
        original_title: None,
        release_year: Some(2022),
        added_at: now - 50 * 86400,
        file_path: PathBuf::from("/media/shows/Severance/S01E01.mkv"),
        file_name: "S01E01.mkv".to_string(),
        file_size: 2_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 3420,
            resolution: Some("1080p".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata {
            series_title: Some("Severance".to_string()),
            season: Some(1),
            episode: Some(1),
            ..Default::default()
        },
    };
    let card_ep = to_card_view_model(&episode, None);
    assert_eq!(card_ep.title, "Good News About Hell");
    assert_eq!(card_ep.subtitle.as_deref(), Some("S01E01 - Severance"));
    assert_eq!(card_ep.media_type, "episode");

    // 5. Episode without series_title
    let episode_no_series = MediaItem {
        id: Some(21),
        library_id: "shows".to_string(),
        item_type: MediaType::Episode,
        title: "Pilot".to_string(),
        original_title: None,
        release_year: Some(2020),
        added_at: now - 50 * 86400,
        file_path: PathBuf::from("/media/shows/Unknown/S01E02.mkv"),
        file_name: "S01E02.mkv".to_string(),
        file_size: 1_000_000_000,
        technical: TechnicalInfo::default(),
        metadata: MediaMetadata {
            season: Some(1),
            episode: Some(2),
            ..Default::default()
        },
    };
    let card_ep2 = to_card_view_model(&episode_no_series, None);
    assert_eq!(card_ep2.subtitle.as_deref(), Some("S01E02"));
}

async fn setup_test_environment() -> (
    WidgetResolver,
    MediaItemRepository,
    PlaybackRepository,
    i64,
    i64,
    i64,
) {
    let pool = create_in_memory_pool().expect("failed to create in-memory pool");
    initialize_database(&pool)
        .await
        .expect("failed to run migrations");

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "user-1".to_string(),
            username: "tester".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Standard,
            created_at: 1000,
        })
        .await
        .unwrap();

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

    let now = now_secs();

    let items = vec![
        // Item 1: Spotlight movie (4K, high rating, backdrop)
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Spotlight Movie".to_string(),
            original_title: None,
            release_year: Some(2023),
            added_at: now - 20 * 86400,
            file_path: PathBuf::from("/media/movies/spotlight.mkv"),
            file_name: "spotlight.mkv".to_string(),
            file_size: 20_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 7200,
                resolution: Some("4K".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("truehd".to_string()),
                audio_channels: Some(8),
                container: Some("mkv".to_string()),
            },
            metadata: MediaMetadata {
                overview: Some("A breathtaking spotlight film.".to_string()),
                backdrop_path: Some("/artwork/spotlight_backdrop.jpg".to_string()),
                poster_path: Some("/artwork/spotlight_poster.jpg".to_string()),
                rating: Some(9.5),
                genres: vec!["Sci-Fi".to_string(), "Action".to_string()],
                ..Default::default()
            },
        },
        // Item 2: Movie in progress for user-1
        MediaItem {
            id: None,
            library_id: "movies".to_string(),
            item_type: MediaType::Movie,
            title: "Continue Movie".to_string(),
            original_title: None,
            release_year: Some(2021),
            added_at: now - 21 * 86400,
            file_path: PathBuf::from("/media/movies/continue.mkv"),
            file_name: "continue.mkv".to_string(),
            file_size: 10_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 6000,
                resolution: Some("1080p".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                overview: Some("An engaging thriller.".to_string()),
                rating: Some(8.1),
                genres: vec!["Thriller".to_string()],
                ..Default::default()
            },
        },
        // Item 3: Show container
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Show,
            title: "Dark Matter".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: now - 22 * 86400,
            file_path: PathBuf::from("/media/shows/DarkMatter"),
            file_name: "DarkMatter".to_string(),
            file_size: 0,
            technical: TechnicalInfo::default(),
            metadata: MediaMetadata {
                overview: Some("Multiverse thriller series.".to_string()),
                rating: Some(8.9),
                genres: vec!["Sci-Fi".to_string(), "Drama".to_string()],
                ..Default::default()
            },
        },
        // Item 4: Dark Matter Episode 1
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Dark Matter - S01E01 - Are You Happy?".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: now - 23 * 86400,
            file_path: PathBuf::from("/media/shows/DarkMatter/S01E01.mkv"),
            file_name: "S01E01.mkv".to_string(),
            file_size: 3_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 3000,
                resolution: Some("4K".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                series_title: Some("Dark Matter".to_string()),
                season: Some(1),
                episode: Some(1),
                overview: Some("Jason Dessen is abducted.".to_string()),
                rating: Some(8.5),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
        },
        // Item 5: Dark Matter Episode 2
        MediaItem {
            id: None,
            library_id: "shows".to_string(),
            item_type: MediaType::Episode,
            title: "Dark Matter - S01E02 - Trip of a Lifetime".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: now - 24 * 86400,
            file_path: PathBuf::from("/media/shows/DarkMatter/S01E02.mkv"),
            file_name: "S01E02.mkv".to_string(),
            file_size: 3_000_000_000,
            technical: TechnicalInfo {
                duration_seconds: 3100,
                resolution: Some("4K".to_string()),
                ..Default::default()
            },
            metadata: MediaMetadata {
                series_title: Some("Dark Matter".to_string()),
                season: Some(1),
                episode: Some(2),
                overview: Some("Jason wakes up in a different life.".to_string()),
                rating: Some(8.7),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
        },
    ];

    media_repo
        .upsert_batch(&items)
        .await
        .expect("upsert failed");

    let spotlight = media_repo
        .find_spotlight_candidate()
        .await
        .unwrap()
        .expect("spotlight item");
    let spotlight_id = spotlight.id.unwrap();

    let cont_movie = media_repo
        .get_by_id(spotlight_id + 1)
        .await
        .unwrap()
        .expect("continue movie");
    let cont_movie_id = cont_movie.id.unwrap();

    let show = media_repo
        .get_by_id(spotlight_id + 2)
        .await
        .unwrap()
        .expect("show item");
    let show_id = show.id.unwrap();

    // Set playback in-progress for user-1 on cont_movie
    playback_repo
        .upsert_progress("user-1", cont_movie_id, 3000, WatchState::InProgress, now)
        .await
        .expect("upsert progress failed");

    let resolver = WidgetResolver::new(media_repo.clone(), playback_repo.clone());
    (
        resolver,
        media_repo,
        playback_repo,
        spotlight_id,
        cont_movie_id,
        show_id,
    )
}

#[tokio::test]
async fn test_concurrent_home_screen_resolution() {
    let (resolver, _, _, spotlight_id, cont_movie_id, _) = setup_test_environment().await;

    let home_layout = default_home_layout();
    let start_time = std::time::Instant::now();
    let resolved = resolver.resolve_screen(home_layout, "user-1").await;
    let elapsed = start_time.elapsed();

    // Verify response time budget < 50ms
    assert!(
        elapsed.as_millis() < 50,
        "Screen resolution took {}ms, budget is 50ms",
        elapsed.as_millis()
    );

    assert_eq!(resolved.id, ScreenId::Home);
    assert_eq!(resolved.title, "Home");
    assert!(!resolved.widgets.is_empty());

    // 1. Verify HeroBanner spotlight
    let hero = resolved.widgets.iter().find(|w| w.id() == "home_spotlight");
    assert!(hero.is_some(), "Hero banner missing");
    if let Some(WidgetNode::HeroBanner { data, .. }) = hero {
        let card = data.as_ref().expect("hero card not populated");
        assert_eq!(card.id, spotlight_id);
        assert_eq!(card.title, "Spotlight Movie");
        assert_eq!(card.badge.as_deref(), Some("4K"));
    } else {
        panic!("home_spotlight is not HeroBanner");
    }

    // 2. Verify Continue Watching carousel
    let cont = resolved
        .widgets
        .iter()
        .find(|w| w.id() == "continue_watching");
    assert!(cont.is_some(), "Continue watching carousel missing");
    if let Some(WidgetNode::Carousel { items, .. }) = cont {
        let cards = items
            .as_ref()
            .expect("continue watching items not populated");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, cont_movie_id);
        assert_eq!(cards[0].title, "Continue Movie");
        assert_eq!(cards[0].badge.as_deref(), Some("RESUME"));
        let progress = cards[0].playback_progress.expect("missing progress");
        assert!((progress - 0.5).abs() < 0.01);
    } else {
        panic!("continue_watching is not Carousel");
    }

    // 3. Verify Recently Added carousel
    let recent = resolved.widgets.iter().find(|w| w.id() == "recently_added");
    assert!(recent.is_some(), "Recently added carousel missing");
    if let Some(WidgetNode::Carousel { items, .. }) = recent {
        let cards = items.as_ref().expect("recently added items not populated");
        assert!(!cards.is_empty());
        // Verify spotlight movie is first (highest added_at)
        assert_eq!(cards[0].id, spotlight_id);
    } else {
        panic!("recently_added is not Carousel");
    }

    // 4. Verify Top Rated carousel
    let top_rated = resolved.widgets.iter().find(|w| w.id() == "top_rated");
    assert!(top_rated.is_some(), "Top rated carousel missing");
    if let Some(WidgetNode::Carousel { items, .. }) = top_rated {
        let cards = items.as_ref().expect("top rated items not populated");
        assert!(!cards.is_empty());
        // Highest rating is spotlight movie (9.5)
        assert_eq!(cards[0].id, spotlight_id);
        assert_eq!(cards[0].rating, Some(9.5));
    } else {
        panic!("top_rated is not Carousel");
    }
}

#[tokio::test]
async fn test_resolve_widget_data_pagination_and_genres() {
    let (resolver, _, _, _, _, _) = setup_test_environment().await;

    // Test GenreShelf for Action
    let action_binding = WidgetQueryBinding::new(QueryMacro::GenreShelf {
        genre: "Action".to_string(),
    })
    .with_limit(5);

    let (cards, next_cursor, _) = resolver
        .resolve_widget_data(&action_binding, "user-1", 0)
        .await
        .expect("resolve widget data failed");

    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].title, "Spotlight Movie");
    assert_eq!(next_cursor, None); // count < limit

    // Test LibraryItems
    let lib_binding = WidgetQueryBinding::new(QueryMacro::LibraryItems {
        library_id: "movies".to_string(),
    })
    .with_limit(1);

    let (cards, next_cursor, total_count) = resolver
        .resolve_widget_data(&lib_binding, "user-1", 0)
        .await
        .expect("resolve library items failed");

    assert_eq!(cards.len(), 1);
    assert_eq!(total_count, Some(2));
    assert_eq!(next_cursor, Some("1".to_string()));
}

#[tokio::test]
async fn test_resolve_item_details_movie_and_show() {
    let (resolver, _, _, spotlight_id, _, show_id) = setup_test_environment().await;

    // 1. Resolve Movie item details
    let movie_details = resolver
        .resolve_item_details(spotlight_id, "user-1")
        .await
        .expect("resolve failed")
        .expect("movie details not found");

    assert_eq!(movie_details.card.id, spotlight_id);
    assert_eq!(movie_details.card.title, "Spotlight Movie");
    assert_eq!(
        movie_details.overview.as_deref(),
        Some("A breathtaking spotlight film.")
    );
    assert_eq!(movie_details.genres, vec!["Sci-Fi", "Action"]);
    assert_eq!(movie_details.duration_seconds, Some(7200));
    assert_eq!(
        movie_details.stream_url,
        format!("/api/v1/stream/{spotlight_id}")
    );
    assert_eq!(movie_details.episodes, None);
    assert!(movie_details.technical.is_some());
    assert_eq!(
        movie_details
            .technical
            .as_ref()
            .unwrap()
            .resolution
            .as_deref(),
        Some("4K")
    );

    // 2. Resolve Series item details with child episodes
    let show_details = resolver
        .resolve_item_details(show_id, "user-1")
        .await
        .expect("resolve failed")
        .expect("show details not found");

    assert_eq!(show_details.card.id, show_id);
    assert_eq!(show_details.card.title, "Dark Matter");
    assert_eq!(show_details.stream_url, format!("/api/v1/stream/{show_id}"));

    let episodes = show_details.episodes.expect("missing episodes");
    assert_eq!(episodes.len(), 2);
    assert_eq!(
        episodes[0].subtitle.as_deref(),
        Some("S01E01 - Dark Matter")
    );
    assert_eq!(
        episodes[1].subtitle.as_deref(),
        Some("S01E02 - Dark Matter")
    );

    // 3. Resolve non-existent item -> None
    let missing = resolver
        .resolve_item_details(99999, "user-1")
        .await
        .expect("resolve missing item returned error");
    assert!(missing.is_none());
}
