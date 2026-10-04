// crates/kadr-core/tests/ast_test.rs
use kadr_core::ast::{
    default_widget_limit, CardViewModel, ItemDetailsPayload, QueryMacro, ScreenId, ScreenLayout,
    WidgetNode, WidgetQueryBinding,
};
use kadr_core::models::TechnicalInfo;

#[test]
fn test_card_view_model_serialization_roundtrip() {
    let card = CardViewModel {
        id: 42,
        title: "Cairo Station".to_string(),
        subtitle: Some("Bab El Hadid".to_string()),
        poster_url: Some("/api/v1/artwork/42/poster".to_string()),
        backdrop_url: Some("/api/v1/artwork/42/backdrop".to_string()),
        media_type: "movie".to_string(),
        playback_progress: Some(0.45),
        rating: Some(8.2),
        release_year: Some(1958),
        badge: Some("4K".to_string()),
    };

    let serialized = serde_json::to_string(&card).expect("serialization failed");
    let deserialized: CardViewModel =
        serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(card, deserialized);
}

#[test]
fn test_card_view_model_with_nones() {
    let card = CardViewModel {
        id: 101,
        title: "Minimal Movie".to_string(),
        subtitle: None,
        poster_url: None,
        backdrop_url: None,
        media_type: "movie".to_string(),
        playback_progress: None,
        rating: None,
        release_year: None,
        badge: None,
    };

    let serialized = serde_json::to_string(&card).expect("serialization failed");
    let deserialized: CardViewModel =
        serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(card, deserialized);
}

#[test]
fn test_screen_id_variants() {
    let home = ScreenId::Home;
    let movies = ScreenId::Movies;
    let shows = ScreenId::Shows;
    let custom = ScreenId::Custom("favorites".to_string());

    assert_eq!(serde_json::to_string(&home).unwrap(), "\"home\"");
    assert_eq!(serde_json::to_string(&movies).unwrap(), "\"movies\"");
    assert_eq!(serde_json::to_string(&shows).unwrap(), "\"shows\"");
    assert_eq!(
        serde_json::to_string(&custom).unwrap(),
        "\"favorites\""
    );

    assert_eq!(
        serde_json::from_str::<ScreenId>("\"home\"").unwrap(),
        ScreenId::Home
    );
    assert_eq!(
        serde_json::from_str::<ScreenId>("\"movies\"").unwrap(),
        ScreenId::Movies
    );
    assert_eq!(
        serde_json::from_str::<ScreenId>("\"shows\"").unwrap(),
        ScreenId::Shows
    );
    assert_eq!(
        serde_json::from_str::<ScreenId>("\"favorites\"").unwrap(),
        ScreenId::Custom("favorites".to_string())
    );
    assert_eq!(
        serde_json::from_str::<ScreenId>("{\"custom\":\"favorites\"}").unwrap(),
        ScreenId::Custom("favorites".to_string())
    );
}

#[test]
fn test_query_macro_variants() {
    let macros = vec![
        (QueryMacro::ContinueWatching, "\"continue_watching\""),
        (QueryMacro::RecentlyAdded, "\"recently_added\""),
        (QueryMacro::TopRated, "\"top_rated\""),
        (
            QueryMacro::LibraryItems {
                library_id: "lib-1".to_string(),
            },
            "{\"library_items\":{\"library_id\":\"lib-1\"}}",
        ),
        (
            QueryMacro::GenreShelf {
                genre: "Action".to_string(),
            },
            "{\"genre_shelf\":{\"genre\":\"Action\"}}",
        ),
        (
            QueryMacro::SpotlightItem { item_id: Some(10) },
            "{\"spotlight_item\":{\"item_id\":10}}",
        ),
        (
            QueryMacro::SpotlightItem { item_id: None },
            "{\"spotlight_item\":{\"item_id\":null}}",
        ),
        (
            QueryMacro::ItemDetails { item_id: 42 },
            "{\"item_details\":{\"item_id\":42}}",
        ),
    ];

    for (qm, expected_json) in macros {
        let serialized = serde_json::to_string(&qm).expect("serialization failed");
        assert_eq!(serialized, expected_json);
        let deserialized: QueryMacro =
            serde_json::from_str(&serialized).expect("deserialization failed");
        assert_eq!(qm, deserialized);
    }
}

#[test]
fn test_widget_query_binding_defaults() {
    let binding_json = r#"{"macro_type":"recently_added"}"#;
    let binding: WidgetQueryBinding = serde_json::from_str(binding_json).unwrap();
    assert_eq!(binding.macro_type, QueryMacro::RecentlyAdded);
    assert_eq!(binding.limit, default_widget_limit());
    assert_eq!(binding.limit, 20);
    assert_eq!(binding.sort, None);

    let custom_binding = WidgetQueryBinding {
        macro_type: QueryMacro::TopRated,
        limit: 50,
        sort: Some("rating_desc".to_string()),
    };
    let serialized = serde_json::to_string(&custom_binding).unwrap();
    let deserialized: WidgetQueryBinding = serde_json::from_str(&serialized).unwrap();
    assert_eq!(custom_binding, deserialized);
}

#[test]
fn test_widget_node_hero_banner() {
    let binding = WidgetQueryBinding {
        macro_type: QueryMacro::SpotlightItem { item_id: None },
        limit: 1,
        sort: None,
    };

    // Unhydrated HeroBanner
    let unhydrated = WidgetNode::HeroBanner {
        id: "hero".to_string(),
        binding: binding.clone(),
        data: None,
    };
    let unhydrated_json = serde_json::to_string(&unhydrated).unwrap();
    assert!(
        !unhydrated_json.contains("\"data\""),
        "None data should be omitted"
    );
    assert!(unhydrated_json.contains("\"type\":\"hero_banner\""));

    let deserialized: WidgetNode = serde_json::from_str(&unhydrated_json).unwrap();
    assert_eq!(unhydrated, deserialized);

    // Hydrated HeroBanner
    let card = CardViewModel {
        id: 7,
        title: "The Mummy".to_string(),
        subtitle: Some("Al-Mummia".to_string()),
        poster_url: Some("/api/v1/artwork/7/poster".to_string()),
        backdrop_url: Some("/api/v1/artwork/7/backdrop".to_string()),
        media_type: "movie".to_string(),
        playback_progress: None,
        rating: Some(8.5),
        release_year: Some(1969),
        badge: Some("FEATURED".to_string()),
    };

    let hydrated = WidgetNode::HeroBanner {
        id: "hero".to_string(),
        binding,
        data: Some(card),
    };
    let hydrated_json = serde_json::to_string(&hydrated).unwrap();
    assert!(hydrated_json.contains("\"data\""));
    let deserialized_hydrated: WidgetNode = serde_json::from_str(&hydrated_json).unwrap();
    assert_eq!(hydrated, deserialized_hydrated);
}

#[test]
fn test_widget_node_carousel() {
    let binding = WidgetQueryBinding {
        macro_type: QueryMacro::ContinueWatching,
        limit: 10,
        sort: None,
    };

    // Unhydrated Carousel
    let unhydrated = WidgetNode::Carousel {
        id: "continue_watching".to_string(),
        title: "Continue Watching".to_string(),
        binding: binding.clone(),
        items: None,
        next_cursor: None,
    };
    let json = serde_json::to_string(&unhydrated).unwrap();
    assert!(!json.contains("\"items\""));
    assert!(!json.contains("\"next_cursor\""));
    assert!(json.contains("\"type\":\"carousel\""));
    let deserialized: WidgetNode = serde_json::from_str(&json).unwrap();
    assert_eq!(unhydrated, deserialized);

    // Hydrated Carousel with items and cursor
    let item = CardViewModel {
        id: 1,
        title: "Movie 1".to_string(),
        subtitle: None,
        poster_url: None,
        backdrop_url: None,
        media_type: "movie".to_string(),
        playback_progress: Some(0.8),
        rating: None,
        release_year: Some(2020),
        badge: Some("RESUME".to_string()),
    };
    let hydrated = WidgetNode::Carousel {
        id: "continue_watching".to_string(),
        title: "Continue Watching".to_string(),
        binding,
        items: Some(vec![item]),
        next_cursor: Some("cursor_10".to_string()),
    };
    let hydrated_json = serde_json::to_string(&hydrated).unwrap();
    assert!(hydrated_json.contains("\"items\""));
    assert!(hydrated_json.contains("\"next_cursor\":\"cursor_10\""));
    let deserialized_hydrated: WidgetNode = serde_json::from_str(&hydrated_json).unwrap();
    assert_eq!(hydrated, deserialized_hydrated);
}

#[test]
fn test_widget_node_grid() {
    let binding = WidgetQueryBinding {
        macro_type: QueryMacro::LibraryItems {
            library_id: "lib-movies".to_string(),
        },
        limit: 24,
        sort: Some("title_asc".to_string()),
    };

    let grid = WidgetNode::Grid {
        id: "movies_grid".to_string(),
        title: "All Movies".to_string(),
        binding,
        columns: 6,
        items: Some(vec![]),
        next_cursor: Some("next_page".to_string()),
        total_count: Some(150),
    };

    let json = serde_json::to_string(&grid).unwrap();
    assert!(json.contains("\"type\":\"grid\""));
    assert!(json.contains("\"columns\":6"));
    assert!(json.contains("\"total_count\":150"));
    let deserialized: WidgetNode = serde_json::from_str(&json).unwrap();
    assert_eq!(grid, deserialized);
}

#[test]
fn test_widget_node_item_details() {
    let payload = ItemDetailsPayload {
        card: CardViewModel {
            id: 42,
            title: "Cairo Station".to_string(),
            subtitle: Some("Bab El Hadid".to_string()),
            poster_url: Some("/api/v1/artwork/42/poster".to_string()),
            backdrop_url: Some("/api/v1/artwork/42/backdrop".to_string()),
            media_type: "movie".to_string(),
            playback_progress: Some(0.3),
            rating: Some(8.2),
            release_year: Some(1958),
            badge: None,
        },
        overview: Some("Classic Egyptian film directed by Youssef Chahine.".to_string()),
        genres: vec!["Drama".to_string(), "Thriller".to_string()],
        duration_seconds: Some(4620),
        technical: Some(TechnicalInfo {
            duration_seconds: 4620,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
        }),
        stream_url: "/api/v1/stream/42".to_string(),
        resume_position_seconds: Some(1386),
        episodes: None,
    };

    let node = WidgetNode::ItemDetails {
        id: "details_42".to_string(),
        item_id: 42,
        details: Some(payload),
    };

    let json = serde_json::to_string(&node).unwrap();
    assert!(json.contains("\"type\":\"item_details\""));
    assert!(json.contains("\"item_id\":42"));
    assert!(json.contains("\"technical\""));
    assert!(json.contains("\"genres\":[\"Drama\",\"Thriller\"]"));

    let deserialized: WidgetNode = serde_json::from_str(&json).unwrap();
    assert_eq!(node, deserialized);
}

#[test]
fn test_screen_layout_serialization_roundtrip() {
    let hero = WidgetNode::HeroBanner {
        id: "hero".to_string(),
        binding: WidgetQueryBinding {
            macro_type: QueryMacro::SpotlightItem { item_id: None },
            limit: 1,
            sort: None,
        },
        data: None,
    };

    let continue_watching = WidgetNode::Carousel {
        id: "continue_watching".to_string(),
        title: "Continue Watching".to_string(),
        binding: WidgetQueryBinding {
            macro_type: QueryMacro::ContinueWatching,
            limit: 10,
            sort: None,
        },
        items: None,
        next_cursor: None,
    };

    let recently_added = WidgetNode::Carousel {
        id: "recently_added".to_string(),
        title: "Recently Added".to_string(),
        binding: WidgetQueryBinding {
            macro_type: QueryMacro::RecentlyAdded,
            limit: 20,
            sort: None,
        },
        items: None,
        next_cursor: None,
    };

    let layout = ScreenLayout {
        id: ScreenId::Home,
        title: "Home".to_string(),
        widgets: vec![hero, continue_watching, recently_added],
    };

    let serialized = serde_json::to_string(&layout).expect("serialization failed");
    assert!(serialized.contains("\"id\":\"home\""));
    assert!(serialized.contains("\"title\":\"Home\""));
    assert!(serialized.contains("\"widgets\":["));

    let deserialized: ScreenLayout =
        serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(layout, deserialized);
}
