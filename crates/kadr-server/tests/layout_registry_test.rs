use kadr_core::ast::{QueryMacro, ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding};
use kadr_server::layout::registry::LayoutRegistry;
use tempfile::tempdir;

#[test]
fn test_default_registry_initialization() {
    let registry = LayoutRegistry::new();
    let screens = registry.list_screens();

    assert_eq!(screens.len(), 3);
    assert_eq!(screens[0], (ScreenId::Home, "Home".to_string()));
    assert_eq!(screens[1], (ScreenId::Movies, "Movies".to_string()));
    assert_eq!(screens[2], (ScreenId::Shows, "TV Shows".to_string()));
}

#[test]
fn test_home_screen_layout_and_hierarchy() {
    let registry = LayoutRegistry::new();
    let home = registry
        .get_screen(&ScreenId::Home)
        .expect("Home screen should exist");

    assert_eq!(home.id, ScreenId::Home);
    assert_eq!(home.title, "Home");
    assert_eq!(home.widgets.len(), 6);

    // Widget 0: HeroBanner spotlight
    match &home.widgets[0] {
        WidgetNode::HeroBanner { id, binding, data } => {
            assert_eq!(id, "home_spotlight");
            assert_eq!(
                binding.macro_type,
                QueryMacro::SpotlightItem { item_id: None }
            );
            assert!(data.is_none());
        }
        other => panic!("Expected HeroBanner, got {other:?}"),
    }

    // Widget 1: Carousel continue_watching
    match &home.widgets[1] {
        WidgetNode::Carousel {
            id,
            title,
            binding,
            items,
            next_cursor,
        } => {
            assert_eq!(id, "continue_watching");
            assert_eq!(title, "Continue Watching");
            assert_eq!(binding.macro_type, QueryMacro::ContinueWatching);
            assert!(items.is_none());
            assert!(next_cursor.is_none());
        }
        other => panic!("Expected Carousel continue_watching, got {other:?}"),
    }

    // Widget 2: Carousel recently_added
    match &home.widgets[2] {
        WidgetNode::Carousel {
            id,
            title,
            binding,
            items,
            next_cursor,
        } => {
            assert_eq!(id, "recently_added");
            assert_eq!(title, "Recently Added");
            assert_eq!(binding.macro_type, QueryMacro::RecentlyAdded);
            assert!(items.is_none());
            assert!(next_cursor.is_none());
        }
        other => panic!("Expected Carousel recently_added, got {other:?}"),
    }

    // Widget 3: Carousel top_rated
    match &home.widgets[3] {
        WidgetNode::Carousel {
            id,
            title,
            binding,
            items,
            next_cursor,
        } => {
            assert_eq!(id, "top_rated");
            assert_eq!(title, "Top Rated");
            assert_eq!(binding.macro_type, QueryMacro::TopRated);
            assert!(items.is_none());
            assert!(next_cursor.is_none());
        }
        other => panic!("Expected Carousel top_rated, got {other:?}"),
    }

    // Widget 4: Carousel genre_action
    match &home.widgets[4] {
        WidgetNode::Carousel {
            id,
            title,
            binding,
            items,
            next_cursor,
        } => {
            assert_eq!(id, "genre_action");
            assert_eq!(title, "Action & Adventure");
            assert_eq!(
                binding.macro_type,
                QueryMacro::GenreShelf {
                    genre: "Action".to_string()
                }
            );
            assert!(items.is_none());
            assert!(next_cursor.is_none());
        }
        other => panic!("Expected Carousel genre_action, got {other:?}"),
    }

    // Widget 5: Carousel genre_scifi
    match &home.widgets[5] {
        WidgetNode::Carousel {
            id,
            title,
            binding,
            items,
            next_cursor,
        } => {
            assert_eq!(id, "genre_scifi");
            assert_eq!(title, "Sci-Fi & Fantasy");
            assert_eq!(
                binding.macro_type,
                QueryMacro::GenreShelf {
                    genre: "Sci-Fi".to_string()
                }
            );
            assert!(items.is_none());
            assert!(next_cursor.is_none());
        }
        other => panic!("Expected Carousel genre_scifi, got {other:?}"),
    }
}

#[test]
fn test_movies_screen_layout() {
    let registry = LayoutRegistry::new();
    let movies = registry
        .get_screen(&ScreenId::Movies)
        .expect("Movies screen should exist");

    assert_eq!(movies.id, ScreenId::Movies);
    assert_eq!(movies.title, "Movies");
    assert_eq!(movies.widgets.len(), 1);

    match &movies.widgets[0] {
        WidgetNode::Grid {
            id,
            title,
            binding,
            columns,
            items,
            next_cursor,
            total_count,
        } => {
            assert_eq!(id, "all_movies");
            assert_eq!(title, "All Movies");
            assert_eq!(columns, &6);
            assert_eq!(binding.macro_type, QueryMacro::RecentlyAdded);
            assert_eq!(binding.limit, 30);
            assert_eq!(binding.sort.as_deref(), Some("title:asc"));
            assert!(items.is_none());
            assert!(next_cursor.is_none());
            assert!(total_count.is_none());
        }
        other => panic!("Expected Grid all_movies, got {other:?}"),
    }
}

#[test]
fn test_shows_screen_layout() {
    let registry = LayoutRegistry::new();
    let shows = registry
        .get_screen(&ScreenId::Shows)
        .expect("Shows screen should exist");

    assert_eq!(shows.id, ScreenId::Shows);
    assert_eq!(shows.title, "TV Shows");
    assert_eq!(shows.widgets.len(), 1);

    match &shows.widgets[0] {
        WidgetNode::Grid {
            id,
            title,
            binding,
            columns,
            items,
            next_cursor,
            total_count,
        } => {
            assert_eq!(id, "all_shows");
            assert_eq!(title, "All TV Shows");
            assert_eq!(columns, &6);
            assert_eq!(binding.macro_type, QueryMacro::RecentlyAdded);
            assert_eq!(binding.limit, 30);
            assert_eq!(binding.sort.as_deref(), Some("title:asc"));
            assert!(items.is_none());
            assert!(next_cursor.is_none());
            assert!(total_count.is_none());
        }
        other => panic!("Expected Grid all_shows, got {other:?}"),
    }
}

#[test]
fn test_register_screen_override_and_custom() {
    let mut registry = LayoutRegistry::new();

    // 1. Override existing Home screen
    let custom_home = ScreenLayout::new(
        ScreenId::Home,
        "Customized Home",
        vec![WidgetNode::Carousel {
            id: "custom_shelf".to_string(),
            title: "Custom Shelf".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::TopRated),
            items: None,
            next_cursor: None,
        }],
    );
    registry.register_screen(custom_home);

    let fetched_home = registry
        .get_screen(&ScreenId::Home)
        .expect("Home should exist");
    assert_eq!(fetched_home.title, "Customized Home");
    assert_eq!(fetched_home.widgets.len(), 1);
    assert_eq!(fetched_home.widgets[0].id(), "custom_shelf");

    // Order and count should remain 3 (no duplicate entry for Home)
    let screens = registry.list_screens();
    assert_eq!(screens.len(), 3);
    assert_eq!(screens[0], (ScreenId::Home, "Customized Home".to_string()));

    // 2. Register a brand new custom screen
    let custom_screen =
        ScreenLayout::new(ScreenId::Custom("anime".to_string()), "Anime Zone", vec![]);
    registry.register_screen(custom_screen);

    let fetched_custom = registry
        .get_screen(&ScreenId::Custom("anime".to_string()))
        .expect("Custom anime screen should exist");
    assert_eq!(fetched_custom.title, "Anime Zone");

    let screens = registry.list_screens();
    assert_eq!(screens.len(), 4);
    assert_eq!(
        screens[3],
        (ScreenId::Custom("anime".to_string()), "Anime Zone".to_string())
    );

    // Non-existent screen returns None
    assert!(registry
        .get_screen(&ScreenId::Custom("nonexistent".to_string()))
        .is_none());
}

#[test]
fn test_load_overrides_from_dir() {
    let mut registry = LayoutRegistry::new();
    let temp_dir = tempdir().expect("Failed to create temp dir");

    // Non-existent dir returns Ok(())
    let non_existent = temp_dir.path().join("does_not_exist");
    assert!(registry.load_overrides_from_dir(&non_existent).is_ok());

    // Write a valid JSON screen override for custom screen
    let custom_layout = ScreenLayout::new(
        ScreenId::Custom("kids".to_string()),
        "Kids & Family",
        vec![WidgetNode::Carousel {
            id: "kids_cartoons".to_string(),
            title: "Cartoons".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::GenreShelf {
                genre: "Animation".to_string(),
            })
            .with_limit(15),
            items: None,
            next_cursor: None,
        }],
    );
    let kids_json = serde_json::to_string_pretty(&custom_layout).unwrap();
    std::fs::write(temp_dir.path().join("kids.json"), kids_json)
        .expect("Failed to write kids.json");

    // Write a valid TOML screen override for home screen
    let home_override = ScreenLayout::new(ScreenId::Home, "Welcome Home", vec![]);
    let home_toml = toml::to_string(&home_override).expect("Failed to serialize to TOML");
    std::fs::write(temp_dir.path().join("home.toml"), home_toml).expect("Failed to write home.toml");

    // Write an invalid JSON file to verify it logs warning and continues without failing
    std::fs::write(temp_dir.path().join("broken.json"), "invalid json content")
        .expect("Failed to write broken.json");

    assert!(registry.load_overrides_from_dir(temp_dir.path()).is_ok());

    let kids_screen = registry
        .get_screen(&ScreenId::Custom("kids".to_string()))
        .expect("Kids screen should be loaded from override");
    assert_eq!(kids_screen.title, "Kids & Family");
    assert_eq!(kids_screen.widgets.len(), 1);
    assert_eq!(kids_screen.widgets[0].id(), "kids_cartoons");

    let overridden_home = registry
        .get_screen(&ScreenId::Home)
        .expect("Home screen should be overridden");
    assert_eq!(overridden_home.title, "Welcome Home");
    assert_eq!(overridden_home.widgets.len(), 0);
}
