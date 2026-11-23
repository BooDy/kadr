use kadr_core::ast::{QueryMacro, WidgetFilterConfig, WidgetQueryBinding};
use kadr_core::filter_ast::{
    ArrayMatch, FilterCondition, FilterNode, FloatComparison, IntComparison, LogicalOp, SetOp,
    TextMatch, TimeComparison, WatchStateMatch,
};
use kadr_core::models::{
    MediaItem, MediaMetadata, MediaType, PlaybackState, TechnicalInfo, WatchState,
};
use std::path::PathBuf;

fn sample_egyptian_classic() -> MediaItem {
    MediaItem {
        id: Some(1),
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        title: "Cairo Station".to_string(),
        original_title: Some("باب الحديد".to_string()),
        release_year: Some(1958),
        added_at: 1_700_000_000,
        file_path: PathBuf::from("/movies/cairo_station.mkv"),
        file_name: "cairo_station.mkv".to_string(),
        file_size: 4_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 4620, // 77 min
            resolution: Some("1080p".to_string()),
            video_codec: Some("hevc".to_string()),
            audio_codec: Some("flac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
            audio_tracks: vec![],
        },
        metadata: MediaMetadata {
            director: Some("Youssef Chahine".to_string()),
            writers: vec!["Abdel Hay Adib".to_string()],
            actors: vec!["Farid Shawqi".to_string(), "Hind Rostom".to_string()],
            country: Some("Egypt".to_string()),
            language: Some("ar".to_string()),
            studio: Some("Gabriel Talhami Films".to_string()),
            rating: Some(8.1),
            genres: vec!["Drama".to_string(), "Crime".to_string()],
            tags: vec!["restored".to_string(), "classic".to_string()],
            ..Default::default()
        },
        locked_fields: vec![],
    }
}

#[test]
fn test_filter_node_json_roundtrip_and_legacy_compat() {
    let legacy_json = r#"{"macro_type":"recently_added","limit":10,"filters":{"exclude_private":true,"max_age_days":30}}"#;
    let legacy_binding: WidgetQueryBinding = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(legacy_binding.macro_type, QueryMacro::RecentlyAdded);
    assert!(legacy_binding.filters.as_ref().unwrap().rule_tree.is_none());

    let custom_json = r#"{
        "macro_type": "custom_query",
        "limit": 20,
        "sort": "release_year:asc",
        "filters": {
            "rule_tree": {
                "kind": "group",
                "op": "and",
                "children": [
                    { "kind": "rule", "condition": { "field": "country", "op": "eq", "value": "Egypt" } },
                    { "kind": "rule", "condition": { "field": "duration_seconds", "op": "lte", "value": 5400 } },
                    { "kind": "rule", "negated": true, "condition": { "field": "genres", "op": "contains_any", "values": ["Musical"] } }
                ]
            }
        }
    }"#;
    let parsed: WidgetQueryBinding = serde_json::from_str(custom_json).unwrap();
    assert_eq!(parsed.macro_type, QueryMacro::CustomQuery);
    let tree = parsed.filters.as_ref().unwrap().rule_tree.as_ref().unwrap();
    assert!(tree.validate().is_ok());

    let reserialized = serde_json::to_string(&parsed).unwrap();
    let reparsed: WidgetQueryBinding = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(parsed, reparsed);
}

#[test]
fn test_validation_rejects_excess_depth_empty_groups_and_bad_ranges() {
    // Empty group rejected
    let empty_group = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![],
    };
    assert!(empty_group.validate().is_err());

    // Depth 6 rejected (max depth is 5)
    let leaf = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Country {
            matcher: TextMatch::Eq {
                value: "Egypt".to_string(),
            },
        },
    };
    let mut deep = leaf;
    for _ in 0..6 {
        deep = FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![deep],
        };
    }
    assert!(deep.validate().is_err());

    // Inverted range rejected
    let bad_range = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::ReleaseYear {
            cmp: IntComparison::Between {
                min: 1970,
                max: 1960,
            },
        },
    };
    assert!(bad_range.validate().is_err());

    // Out-of-bounds progress rejected
    let bad_progress = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::WatchState {
            matcher: WatchStateMatch::ProgressBetween { min: 0.2, max: 1.5 },
        },
    };
    assert!(bad_progress.validate().is_err());
}

#[test]
fn test_rust_matches_nested_boolean_null_negation_and_watch_state() {
    let item = sample_egyptian_classic();
    let now = 1_700_086_400; // +1 day

    let tree = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Country {
                    matcher: TextMatch::Eq {
                        value: "egypt".to_string(),
                    },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Genres {
                    matcher: ArrayMatch::ContainsAll {
                        values: vec!["drama".to_string(), "crime".to_string()],
                    },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::AddedAt {
                    cmp: TimeComparison::WithinLastDays { days: 7 },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::WatchState {
                    matcher: WatchStateMatch::ProgressBetween { min: 0.1, max: 0.9 },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::ItemType {
                    op: SetOp::In,
                    values: vec![MediaType::Movie],
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Rating {
                    cmp: FloatComparison::Gte { value: 7.5 },
                },
            },
        ],
    };

    assert!(tree.references_item_type());
    assert!(tree.needs_playback_state());
    // Unwatched (None playback) has 0.0 progress -> should not match ProgressBetween(0.1, 0.9)
    assert!(!tree.matches(&item, None, now));

    let in_progress = PlaybackState {
        user_id: "u1".to_string(),
        media_item_id: 1,
        playback_position_seconds: 2310, // 50%
        watch_state: WatchState::InProgress,
        last_watched_at: now - 60,
        play_count: 1,
    };
    assert!(tree.matches(&item, Some(&in_progress), now));

    // Negated text rule on NULL field must evaluate to TRUE ("NULL country is NOT France")
    let mut no_country_item = item.clone();
    no_country_item.metadata.country = None;
    let not_france = FilterNode::Rule {
        negated: true,
        condition: FilterCondition::Country {
            matcher: TextMatch::Eq {
                value: "France".to_string(),
            },
        },
    };
    assert!(not_france.matches(&no_country_item, None, now));
    assert_eq!(WidgetFilterConfig::default().rule_tree, None);
}

#[test]
fn test_validation_boundary_limits_and_error_variants() {
    use kadr_core::filter_ast::{
        FilterValidationError, MAX_FILTER_AST_DEPTH, MAX_FILTER_AST_RULES, MAX_FILTER_DAYS,
        MAX_FILTER_LIST_LEN, MAX_FILTER_TEXT_LEN,
    };

    // Exact max depth (5) is valid
    let mut exact_depth = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Title {
            matcher: TextMatch::Contains {
                value: "Cairo".to_string(),
            },
        },
    };
    for _ in 0..MAX_FILTER_AST_DEPTH {
        exact_depth = FilterNode::Group {
            op: LogicalOp::Or,
            negated: false,
            children: vec![exact_depth],
        };
    }
    assert!(exact_depth.validate().is_ok());

    // Rule count > 32 rejected, == 32 accepted
    let rules_32: Vec<FilterNode> = (0..MAX_FILTER_AST_RULES)
        .map(|_| FilterNode::Rule {
            negated: false,
            condition: FilterCondition::Title {
                matcher: TextMatch::IsEmpty,
            },
        })
        .collect();
    let ok_32 = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: rules_32.clone(),
    };
    assert!(ok_32.validate().is_ok());

    let mut rules_33 = rules_32;
    rules_33.push(FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Title {
            matcher: TextMatch::IsEmpty,
        },
    });
    let err_33 = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: rules_33,
    };
    assert_eq!(
        err_33.validate(),
        Err(FilterValidationError::RuleCountOutOfBounds(
            MAX_FILTER_AST_RULES + 1
        ))
    );

    // Empty / whitespace text and > MAX_FILTER_TEXT_LEN rejected
    let blank_text = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Director {
            matcher: TextMatch::StartsWith {
                value: "   ".to_string(),
            },
        },
    };
    assert_eq!(
        blank_text.validate(),
        Err(FilterValidationError::InvalidTextValue)
    );

    let too_long_text = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Title {
            matcher: TextMatch::Eq {
                value: "a".repeat(MAX_FILTER_TEXT_LEN + 1),
            },
        },
    };
    assert_eq!(
        too_long_text.validate(),
        Err(FilterValidationError::InvalidTextValue)
    );

    // Invalid list lengths (0 and > MAX_FILTER_LIST_LEN)
    let empty_list = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::LibraryId {
            op: SetOp::In,
            values: vec![],
        },
    };
    assert_eq!(
        empty_list.validate(),
        Err(FilterValidationError::InvalidListLength)
    );

    let oversized_list = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Tags {
            matcher: ArrayMatch::ContainsAny {
                values: (0..=MAX_FILTER_LIST_LEN)
                    .map(|i| format!("tag-{i}"))
                    .collect(),
            },
        },
    };
    assert_eq!(
        oversized_list.validate(),
        Err(FilterValidationError::InvalidListLength)
    );

    // Negative duration/play_count rejected, negative days/0 days rejected, NaN rating rejected
    let neg_duration = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::DurationSeconds {
            cmp: IntComparison::Gte { value: -1 },
        },
    };
    assert_eq!(
        neg_duration.validate(),
        Err(FilterValidationError::NegativeDuration)
    );

    let zero_days = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::AddedAt {
            cmp: TimeComparison::WithinLastDays { days: 0 },
        },
    };
    assert_eq!(
        zero_days.validate(),
        Err(FilterValidationError::InvalidDaysBounds)
    );

    let excess_days = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::LastWatchedAt {
            cmp: TimeComparison::OlderThanDays {
                days: MAX_FILTER_DAYS + 1,
            },
        },
    };
    assert_eq!(
        excess_days.validate(),
        Err(FilterValidationError::InvalidDaysBounds)
    );

    let nan_rating = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Rating {
            cmp: FloatComparison::Lte { value: f32::NAN },
        },
    };
    assert_eq!(
        nan_rating.validate(),
        Err(FilterValidationError::InvalidFloatBounds)
    );
}

#[test]
fn test_evaluator_or_groups_unwatched_defaults_and_zero_duration() {
    let mut item = sample_egyptian_classic();
    let now = 1_700_086_400;

    // Group with OR + negation, PlayCount default 0, WatchState::StateIn default Unwatched
    let unwatched_zero_plays = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::WatchState {
                    matcher: WatchStateMatch::StateIn {
                        values: vec![WatchState::Unwatched],
                    },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::PlayCount {
                    cmp: IntComparison::Eq { value: 0 },
                },
            },
            FilterNode::Group {
                op: LogicalOp::Or,
                negated: true,
                children: vec![
                    FilterNode::Rule {
                        negated: false,
                        condition: FilterCondition::Language {
                            matcher: TextMatch::In {
                                values: vec!["fr".to_string(), "de".to_string()],
                            },
                        },
                    },
                    FilterNode::Rule {
                        negated: false,
                        condition: FilterCondition::SeriesTitle {
                            matcher: TextMatch::Contains {
                                value: "Anthology".to_string(),
                            },
                        },
                    },
                ],
            },
        ],
    };

    assert!(!unwatched_zero_plays.references_item_type());
    assert!(unwatched_zero_plays.needs_playback_state());
    assert!(unwatched_zero_plays.matches(&item, None, now));

    // Zero duration item never matches ProgressBetween (avoids division by zero)
    item.technical.duration_seconds = 0;
    let progress_any = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::WatchState {
            matcher: WatchStateMatch::ProgressBetween { min: 0.0, max: 1.0 },
        },
    };
    let playback = PlaybackState {
        user_id: "u1".to_string(),
        media_item_id: 1,
        playback_position_seconds: 100,
        watch_state: WatchState::InProgress,
        last_watched_at: now,
        play_count: 1,
    };
    assert!(!progress_any.matches(&item, Some(&playback), now));
}

#[test]
fn test_library_id_trim_parity_and_time_cmp_saturation() {
    let item = sample_egyptian_classic();
    let padded_lib_rule = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::LibraryId {
            op: SetOp::In,
            values: vec![format!("  {}  ", item.library_id)],
        },
    };
    assert!(padded_lib_rule.matches(&item, None, 1_700_086_400));

    // Extreme days/epoch values must saturate rather than panic on i64 underflow/overflow
    let extreme_within = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::AddedAt {
            cmp: TimeComparison::WithinLastDays { days: u32::MAX },
        },
    };
    assert!(extreme_within.matches(&item, None, i64::MIN + 10));

    let extreme_older = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::AddedAt {
            cmp: TimeComparison::OlderThanDays { days: u32::MAX },
        },
    };
    assert!(!extreme_older.matches(&item, None, i64::MIN + 10));
}

fn sample_movie() -> MediaItem {
    sample_egyptian_classic()
}

#[test]
fn favorites_ast_and_wire_types_serde_and_eval() {
    use kadr_core::{
        BoolOp, CardViewModel, FavoriteStatusResponse, FilterCondition, FilterNode, LogicalOp,
        QueryMacro, SystemEvent,
    };

    // QueryMacro::Favorites
    let macro_json = serde_json::to_string(&QueryMacro::Favorites).unwrap();
    assert_eq!(macro_json, "\"favorites\"");
    let parsed_macro: QueryMacro = serde_json::from_str(&macro_json).unwrap();
    assert_eq!(parsed_macro, QueryMacro::Favorites);

    // CardViewModel omits is_favorite when false, includes when true
    let default_card = CardViewModel {
        id: 1,
        title: "Film".to_string(),
        media_type: "movie".to_string(),
        ..Default::default()
    };
    let card_false_json = serde_json::to_string(&default_card).unwrap();
    assert!(!card_false_json.contains("is_favorite"));

    let fav_card = CardViewModel {
        is_favorite: true,
        ..default_card
    };
    let card_true_json = serde_json::to_string(&fav_card).unwrap();
    assert!(card_true_json.contains("\"is_favorite\":true"));

    // FilterCondition::IsFavorite accepts both explicit op="eq" and omitted op
    let with_op: FilterCondition =
        serde_json::from_str(r#"{"field":"is_favorite","op":"eq","value":true}"#).unwrap();
    let without_op: FilterCondition =
        serde_json::from_str(r#"{"field":"is_favorite","value":true}"#).unwrap();
    assert_eq!(
        with_op,
        FilterCondition::IsFavorite {
            op: BoolOp::Eq,
            value: true
        }
    );
    assert_eq!(with_op, without_op);

    let node = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![FilterNode::Rule {
            negated: false,
            condition: with_op,
        }],
    };
    node.validate().unwrap();
    assert!(node.needs_favorite_state());

    let item = sample_movie();
    assert!(!node.matches(&item, None, 1_700_000_000));
    assert!(node.matches_with_context(&item, None, true, 1_700_000_000));
    assert!(!node.matches_with_context(&item, None, false, 1_700_000_000));

    // SystemEvent::FavoriteUpdated
    let evt = SystemEvent::FavoriteUpdated {
        user_id: "u1".to_string(),
        item_id: 42,
        is_favorite: true,
        timestamp: 12345,
    };
    assert_eq!(evt.event_type(), "favorite:updated");
    let status = FavoriteStatusResponse {
        item_id: 42,
        is_favorite: true,
    };
    assert!(status.is_favorite);
}

#[test]
fn favorites_negation_nested_groups_and_wire_models_edge_cases() {
    use kadr_core::favorites::{FavoriteItemView, FavoriteStatusResponse, FavoritesListResponse};
    use kadr_core::{
        BoolOp, CardViewModel, FilterCondition, FilterNode, LogicalOp, MediaType, SearchResultItem,
        SystemEvent, TextMatch,
    };

    let item = sample_movie();
    let now = 1_700_000_000;

    // Non-favorite tree does not report needs_favorite_state
    let plain_rule = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Country {
            matcher: TextMatch::Eq {
                value: "Egypt".to_string(),
            },
        },
    };
    assert!(!plain_rule.needs_favorite_state());

    // Negated Rule on IsFavorite { value: true }: matches when is_favorite == false
    let negated_fav_rule = FilterNode::Rule {
        negated: true,
        condition: FilterCondition::IsFavorite {
            op: BoolOp::Eq,
            value: true,
        },
    };
    negated_fav_rule.validate().unwrap();
    assert!(negated_fav_rule.needs_favorite_state());
    assert!(!negated_fav_rule.needs_playback_state());
    assert!(negated_fav_rule.matches(&item, None, now));
    assert!(negated_fav_rule.matches_with_context(&item, None, false, now));
    assert!(!negated_fav_rule.matches_with_context(&item, None, true, now));

    // IsFavorite { value: false } + negated Group threading is_favorite down nested children
    let nested_negated_group = FilterNode::Group {
        op: LogicalOp::Or,
        negated: true,
        children: vec![
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Country {
                    matcher: TextMatch::Eq {
                        value: "France".to_string(),
                    },
                },
            },
            FilterNode::Group {
                op: LogicalOp::And,
                negated: false,
                children: vec![FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::IsFavorite {
                        op: BoolOp::Eq,
                        value: false,
                    },
                }],
            },
        ],
    };
    nested_negated_group.validate().unwrap();
    assert!(nested_negated_group.needs_favorite_state());
    // When is_favorite=false: inner IsFavorite(false) is true -> Or is true -> negated Group is false
    assert!(!nested_negated_group.matches(&item, None, now));
    assert!(!nested_negated_group.matches_with_context(&item, None, false, now));
    // When is_favorite=true: Country==France is false, IsFavorite(false) is false -> Or is false -> negated Group is true
    assert!(nested_negated_group.matches_with_context(&item, None, true, now));

    // SearchResultItem omits is_favorite when false, includes when true, defaults to false on deserialize
    let search_item = SearchResultItem {
        id: 7,
        library_id: "lib-movies".to_string(),
        library_name: "Movies".to_string(),
        item_type: MediaType::Movie,
        title: "Cairo Station".to_string(),
        original_title: None,
        release_year: Some(1958),
        overview: None,
        poster_url: "/api/v1/artwork/7/poster".to_string(),
        backdrop_url: None,
        badge: None,
        rating: Some(8.1),
        duration_seconds: 4620,
        playback_progress: None,
        series_title: None,
        season: None,
        episode: None,
        genres: vec!["Drama".to_string()],
        director: Some("Youssef Chahine".to_string()),
        is_favorite: false,
    };
    let search_false_json = serde_json::to_string(&search_item).unwrap();
    assert!(!search_false_json.contains("is_favorite"));
    let parsed_search_false: SearchResultItem = serde_json::from_str(&search_false_json).unwrap();
    assert!(!parsed_search_false.is_favorite);

    let search_true = SearchResultItem {
        is_favorite: true,
        ..search_item
    };
    let search_true_json = serde_json::to_string(&search_true).unwrap();
    assert!(search_true_json.contains("\"is_favorite\":true"));
    let parsed_search_true: SearchResultItem = serde_json::from_str(&search_true_json).unwrap();
    assert!(parsed_search_true.is_favorite);

    // FavoriteItemView, FavoritesListResponse, FavoriteStatusResponse serde
    let list_default = FavoritesListResponse::default();
    assert_eq!(list_default.total_count, 0);
    assert_eq!(list_default.locked_item_count, 0);
    assert!(list_default.items.is_empty());

    let fav_view = FavoriteItemView {
        favorited_at: 1_700_000_123,
        duration_seconds: 4620,
        resume_position_seconds: None,
        card: CardViewModel {
            id: 1,
            title: "Cairo Station".to_string(),
            media_type: "movie".to_string(),
            is_favorite: true,
            ..Default::default()
        },
    };
    let view_json = serde_json::to_string(&fav_view).unwrap();
    assert!(!view_json.contains("resume_position_seconds"));
    let list_resp = FavoritesListResponse {
        items: vec![FavoriteItemView {
            resume_position_seconds: Some(300),
            ..fav_view
        }],
        total_count: 1,
        locked_item_count: 2,
    };
    let list_json = serde_json::to_string(&list_resp).unwrap();
    assert!(list_json.contains("\"resume_position_seconds\":300"));
    let list_roundtrip: FavoritesListResponse = serde_json::from_str(&list_json).unwrap();
    assert_eq!(list_roundtrip, list_resp);

    let status_resp = FavoriteStatusResponse {
        item_id: 1,
        is_favorite: false,
    };
    let status_json = serde_json::to_string(&status_resp).unwrap();
    let status_roundtrip: FavoriteStatusResponse = serde_json::from_str(&status_json).unwrap();
    assert_eq!(status_roundtrip, status_resp);

    // SystemEvent::FavoriteUpdated tagged JSON wire format roundtrip
    let evt = SystemEvent::FavoriteUpdated {
        user_id: "u1".to_string(),
        item_id: 42,
        is_favorite: false,
        timestamp: 99_999,
    };
    let evt_val = serde_json::to_value(&evt).unwrap();
    assert_eq!(evt_val["type"], "favorite:updated");
    assert_eq!(evt_val["payload"]["user_id"], "u1");
    assert_eq!(evt_val["payload"]["item_id"], 42);
    assert_eq!(evt_val["payload"]["is_favorite"], false);
    assert_eq!(evt_val["payload"]["timestamp"], 99_999);
    let evt_roundtrip: SystemEvent = serde_json::from_value(evt_val).unwrap();
    assert_eq!(evt_roundtrip, evt);
}
