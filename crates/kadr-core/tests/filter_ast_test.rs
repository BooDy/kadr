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
