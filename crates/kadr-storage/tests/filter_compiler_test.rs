use kadr_core::ast::WidgetFilterConfig;
use kadr_core::filter_ast::{
    ArrayMatch, FilterCondition, FilterNode, FloatComparison, IntComparison, LogicalOp, SetOp,
    TextMatch, TimeComparison, WatchStateMatch,
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    build_filter_envelope, compile_filter_node, LibraryRepository, MediaItemRepository,
    PlaybackRepository, UserRepository,
};
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_sql_compiler_parity_with_rust_evaluator_and_private_library_isolation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_filter_ast.db");
    let pool = create_pool(&db_path, 4).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "u1".to_string(),
            username: "viewer".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Admin,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "pub-movies".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "priv-vault".to_string(),
            name: "Private Vault".to_string(),
            path: PathBuf::from("/priv"),
            paths: vec![PathBuf::from("/priv")],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("secret".to_string()),
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let seed_items = vec![
        MediaItem {
            id: None,
            library_id: "pub-movies".to_string(),
            item_type: MediaType::Movie,
            title: "Cairo Station 100%".to_string(),
            original_title: Some("باب الحديد".to_string()),
            release_year: Some(1958),
            added_at: now - 3 * 86_400,
            file_path: PathBuf::from("/pub/cairo.mkv"),
            file_name: "cairo.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 4620,
                resolution: Some("1080p".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("flac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                director: Some("Youssef Chahine".to_string()),
                country: Some("Egypt".to_string()),
                language: Some("ar".to_string()),
                studio: Some("Studio Misr".to_string()),
                rating: Some(8.2),
                genres: vec!["Drama".to_string(), "Crime".to_string()],
                tags: vec!["restored".to_string()],
                ..Default::default()
            },
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "pub-movies".to_string(),
            item_type: MediaType::Movie,
            title: "Dune Part Two".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: now - 60 * 86_400,
            file_path: PathBuf::from("/pub/dune2.mkv"),
            file_name: "dune2.mkv".to_string(),
            file_size: 2000,
            technical: TechnicalInfo {
                duration_seconds: 9960,
                resolution: Some("4k".to_string()),
                video_codec: Some("av1".to_string()),
                audio_codec: Some("eac3".to_string()),
                audio_channels: Some(6),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                director: Some("Denis Villeneuve".to_string()),
                country: None, // NULL country to test NOT NULL edge cases
                language: Some("en".to_string()),
                rating: Some(8.6),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "pub-movies".to_string(),
            item_type: MediaType::Episode,
            title: "Short Sitcom Pilot".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now - 86_400,
            file_path: PathBuf::from("/pub/ep1.mp4"),
            file_name: "ep1.mp4".to_string(),
            file_size: 500,
            technical: TechnicalInfo {
                duration_seconds: 1320,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mp4".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata::default(),
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "priv-vault".to_string(),
            item_type: MediaType::Movie,
            title: "Secret Archive Film".to_string(),
            original_title: None,
            release_year: Some(1962),
            added_at: now - 86_400,
            file_path: PathBuf::from("/priv/secret.mkv"),
            file_name: "secret.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 4000,
                resolution: Some("4k".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                director: Some("Secret Director".to_string()),
                country: Some("Egypt".to_string()),
                genres: vec!["SecretGenre".to_string()],
                tags: vec!["topsecret".to_string()],
                rating: Some(9.5),
                ..Default::default()
            },
            locked_fields: vec![],
        },
    ];

    media_repo.upsert_batch(&seed_items).await.unwrap();
    let pub_items = media_repo.list_all_by_library("pub-movies").await.unwrap();
    let cairo = pub_items
        .iter()
        .find(|i| i.title.starts_with("Cairo"))
        .unwrap();

    // Mark Cairo Station 50% watched
    playback_repo
        .upsert_progress(
            "u1",
            cairo.id.unwrap(),
            2310,
            WatchState::InProgress,
            now - 120,
        )
        .await
        .unwrap();

    // 1. Root OR query attempting to match private item -> MUST return 0 private items when locked!
    let or_tree = FilterNode::Group {
        op: LogicalOp::Or,
        negated: false,
        children: vec![
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Country {
                    matcher: TextMatch::Eq {
                        value: "Egypt".to_string(),
                    },
                },
            },
            FilterNode::Rule {
                negated: true,
                condition: FilterCondition::Title {
                    matcher: TextMatch::Eq {
                        value: "Nonexistent".to_string(),
                    },
                },
            },
        ],
    };
    let filters = WidgetFilterConfig {
        rule_tree: Some(or_tree),
        ..Default::default()
    };

    let (locked_res, locked_total) = media_repo
        .find_custom_paginated(
            None,
            "u1",
            50,
            0,
            Some("duration_seconds:asc"),
            &[],
            Some(&filters),
        )
        .await
        .unwrap();
    // Only Cairo + Dune (episode excluded because rule_tree didn't reference item_type; private item excluded!)
    assert_eq!(locked_total, 2);
    assert!(locked_res.iter().all(|i| i.library_id == "pub-movies"));
    assert_eq!(locked_res[0].title, "Cairo Station 100%");

    // Unlocking priv-vault reveals Secret Archive Film
    let unlocked = vec!["priv-vault".to_string()];
    let (unlocked_res, unlocked_total) = media_repo
        .find_custom_paginated(
            None,
            "u1",
            50,
            0,
            Some("duration_seconds:asc"),
            &unlocked,
            Some(&filters),
        )
        .await
        .unwrap();
    assert_eq!(unlocked_total, 3);
    assert_eq!(unlocked_res[0].title, "Secret Archive Film");

    // 2. Facets NEVER leak SecretGenre / Secret Director / topsecret when locked
    let locked_facets = media_repo.get_catalog_facets(None, &[]).await.unwrap();
    assert!(!locked_facets.genres.iter().any(|g| g == "SecretGenre"));
    assert!(!locked_facets
        .directors
        .iter()
        .any(|d| d == "Secret Director"));
    assert!(!locked_facets.tags.iter().any(|t| t == "topsecret"));
    assert!(locked_facets.countries.contains(&"Egypt".to_string()));
    assert_eq!(locked_facets.year_min, Some(1958));
    assert_eq!(locked_facets.year_max, Some(2024));

    let unlocked_facets = media_repo
        .get_catalog_facets(None, &unlocked)
        .await
        .unwrap();
    assert!(unlocked_facets.genres.contains(&"SecretGenre".to_string()));

    let scoped_facets = media_repo
        .get_catalog_facets(Some("priv-vault"), &unlocked)
        .await
        .unwrap();
    assert_eq!(scoped_facets.genres, vec!["SecretGenre".to_string()]);
    assert_eq!(scoped_facets.year_min, Some(1962));
    assert_eq!(scoped_facets.year_max, Some(1962));

    // 3. Parity test across NULL negation, LIKE '%' escaping, progress_between, and explicit episode item_type
    let parity_trees = vec![
        // NOT (country = Egypt) -> matches Dune (country IS NULL)
        FilterNode::Rule {
            negated: true,
            condition: FilterCondition::Country {
                matcher: TextMatch::Eq {
                    value: "Egypt".to_string(),
                },
            },
        },
        // Literal '%' in title contains ("100%") -> matches ONLY Cairo Station 100%, not Dune!
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::Title {
                matcher: TextMatch::Contains {
                    value: "100%".to_string(),
                },
            },
        },
        // Explicit Episode item_type rule -> includes episodes!
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::ItemType {
                op: SetOp::In,
                values: vec![MediaType::Episode],
            },
        },
        // Watch progress between 0.4 and 0.6 -> matches only Cairo Station
        FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::WatchState {
                        matcher: WatchStateMatch::ProgressBetween { min: 0.4, max: 0.6 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Genres {
                        matcher: ArrayMatch::ContainsAll {
                            values: vec!["Drama".to_string(), "Crime".to_string()],
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Rating {
                        cmp: FloatComparison::Between { min: 8.0, max: 9.0 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::ReleaseYear {
                        cmp: IntComparison::Lte { value: 1960 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::AddedAt {
                        cmp: TimeComparison::WithinLastDays { days: 7 },
                    },
                },
            ],
        },
    ];

    for tree in parity_trees {
        let cfg = WidgetFilterConfig {
            rule_tree: Some(tree.clone()),
            ..Default::default()
        };
        let (sql_items, _) = media_repo
            .find_custom_paginated(None, "u1", 50, 0, None, &[], Some(&cfg))
            .await
            .unwrap();
        let sql_ids: Vec<i64> = sql_items.iter().filter_map(|i| i.id).collect();

        let mut rust_ids = Vec::new();
        for item in &pub_items {
            if !tree.references_item_type() && item.item_type == MediaType::Episode {
                continue;
            }
            let st = playback_repo
                .get_state("u1", item.id.unwrap())
                .await
                .unwrap();
            if tree.matches(item, st.as_ref(), now) {
                rust_ids.push(item.id.unwrap());
            }
        }
        let mut sql_sorted = sql_ids.clone();
        sql_sorted.sort();
        rust_ids.sort();
        assert_eq!(sql_sorted, rust_ids, "Mismatch on tree: {tree:?}");
    }
}

#[tokio::test]
async fn test_preset_for_user_queries_sort_options_indexes_and_full_operator_parity() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_filter_ast_full.db");
    let pool = create_pool(&db_path, 4).unwrap();
    initialize_database(&pool).await.unwrap();

    // Verify migration 014 expression indexes exist
    {
        let conn = pool.get().await.unwrap();
        conn.interact(|c| {
            let indexes: Vec<String> = c
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='media_items'",
                )
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            for expected in [
                "idx_media_release_year",
                "idx_media_duration",
                "idx_media_resolution_lower",
                "idx_media_video_codec_lower",
                "idx_media_country_lower",
                "idx_media_language_lower",
                "idx_media_director_lower",
                "idx_media_studio_lower",
            ] {
                assert!(
                    indexes.contains(&expected.to_string()),
                    "Missing index {expected}"
                );
            }
        })
        .await
        .unwrap();
    }

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "u1".to_string(),
            username: "viewer".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Admin,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-a".to_string(),
            name: "Lib A".to_string(),
            path: PathBuf::from("/lib-a"),
            paths: vec![PathBuf::from("/lib-a")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let items = vec![
        MediaItem {
            id: None,
            library_id: "lib-a".to_string(),
            item_type: MediaType::Movie,
            title: "Alpha_1\\Test".to_string(),
            original_title: Some("Orig Alpha".to_string()),
            release_year: Some(1999),
            added_at: now - 10 * 86_400,
            file_path: PathBuf::from("/lib-a/alpha.mkv"),
            file_name: "alpha.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 5400,
                resolution: Some("1080p".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                series_title: Some("Anthology".to_string()),
                director: Some("Director One".to_string()),
                studio: Some("Studio One".to_string()),
                country: Some("Japan".to_string()),
                language: Some("ja".to_string()),
                rating: Some(7.8),
                backdrop_path: Some("/backdrop/alpha.jpg".to_string()),
                genres: vec!["Sci-Fi".to_string(), "Mystery".to_string()],
                tags: vec!["cult".to_string()],
                actors: vec!["Actor A".to_string()],
                writers: vec!["Writer A".to_string()],
                ..Default::default()
            },
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "lib-a".to_string(),
            item_type: MediaType::Movie,
            title: "AlphaX1ZTest".to_string(),
            original_title: None,
            release_year: Some(2015),
            added_at: now - 100 * 86_400,
            file_path: PathBuf::from("/lib-a/beta.mp4"),
            file_name: "beta.mp4".to_string(),
            file_size: 2000,
            technical: TechnicalInfo {
                duration_seconds: 7200,
                resolution: Some("4k".to_string()),
                video_codec: Some("av1".to_string()),
                audio_codec: Some("eac3".to_string()),
                audio_channels: Some(6),
                container: Some("mp4".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                rating: Some(9.1),
                backdrop_path: Some("/backdrop/beta.jpg".to_string()),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
            locked_fields: vec![],
        },
    ];

    media_repo.upsert_batch(&items).await.unwrap();
    let stored = media_repo.list_all_by_library("lib-a").await.unwrap();
    let alpha = stored.iter().find(|i| i.title == "Alpha_1\\Test").unwrap();
    let beta = stored.iter().find(|i| i.title == "AlphaX1ZTest").unwrap();

    playback_repo
        .upsert_progress(
            "u1",
            alpha.id.unwrap(),
            5400,
            WatchState::Completed,
            now - 3600,
        )
        .await
        .unwrap();

    // Test LIKE escaping for '_' and '\' in StartsWith ("Alpha_1\\") -> MUST match only Alpha_1\Test, not AlphaX1ZTest!
    let starts_with_escaped = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Title {
            matcher: TextMatch::StartsWith {
                value: "Alpha_1\\".to_string(),
            },
        },
    };
    let (res, total) = media_repo
        .find_custom_paginated(
            Some("lib-a"),
            "u1",
            20,
            0,
            Some("last_watched_at:desc"),
            &[],
            Some(&WidgetFilterConfig {
                rule_tree: Some(starts_with_escaped),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(res[0].id, alpha.id);

    // Sort by last_watched_at:desc without playback rule in filter -> alpha (watched) before beta (unwatched)
    let (sorted_watched, _) = media_repo
        .find_custom_paginated(
            Some("lib-a"),
            "u1",
            20,
            0,
            Some("last_watched_at:desc"),
            &[],
            None,
        )
        .await
        .unwrap();
    assert_eq!(sorted_watched[0].id, alpha.id);
    assert_eq!(sorted_watched[1].id, beta.id);

    // Test preset *_for_user methods with WatchState::Completed rule + library_id/genre bind order!
    let completed_filter = WidgetFilterConfig {
        rule_tree: Some(FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::WatchState {
                        matcher: WatchStateMatch::StateIn {
                            values: vec![WatchState::Completed],
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::PlayCount {
                        cmp: IntComparison::Gte { value: 1 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::LastWatchedAt {
                        cmp: TimeComparison::AfterEpoch {
                            timestamp: now - 7200,
                        },
                    },
                },
            ],
        }),
        ..Default::default()
    };

    let recent = media_repo
        .find_recently_added_paginated_for_user(
            Some("lib-a"),
            Some("u1"),
            10,
            0,
            &[],
            Some(&completed_filter),
        )
        .await
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, alpha.id);

    let top_rated = media_repo
        .find_top_rated_paginated_for_user(
            Some("lib-a"),
            Some("u1"),
            10,
            0,
            &[],
            Some(&completed_filter),
        )
        .await
        .unwrap();
    assert_eq!(top_rated.len(), 1);
    assert_eq!(top_rated[0].id, alpha.id);

    let by_genre = media_repo
        .find_by_genre_paginated_for_user(
            "Sci-Fi",
            Some("lib-a"),
            Some("u1"),
            10,
            0,
            &[],
            Some(&completed_filter),
        )
        .await
        .unwrap();
    assert_eq!(by_genre.len(), 1);
    assert_eq!(by_genre[0].id, alpha.id);

    let (by_lib, by_lib_total) = media_repo
        .find_by_library_paginated_for_user(
            "lib-a",
            Some("u1"),
            10,
            0,
            Some("last_watched_at:desc"),
            &[],
            Some(&completed_filter),
        )
        .await
        .unwrap();
    assert_eq!(by_lib_total, 1);
    assert_eq!(by_lib[0].id, alpha.id);

    let spotlight = media_repo
        .find_spotlight_candidate_for_user(Some("lib-a"), Some("u1"), &[], Some(&completed_filter))
        .await
        .unwrap();
    assert_eq!(spotlight.unwrap().id, alpha.id);

    // Exhaustive parity check across remaining operators & fields
    let extra_trees = vec![
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::LibraryId {
                op: SetOp::In,
                values: vec!["lib-a".to_string()],
            },
        },
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::OriginalTitle {
                matcher: TextMatch::IsEmpty,
            },
        },
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::SeriesTitle {
                matcher: TextMatch::In {
                    values: vec!["anthology".to_string(), "other".to_string()],
                },
            },
        },
        FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Director {
                        matcher: TextMatch::Eq {
                            value: "director one".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Studio {
                        matcher: TextMatch::Contains {
                            value: "studio".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Language {
                        matcher: TextMatch::Eq {
                            value: "ja".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Resolution {
                        matcher: TextMatch::Eq {
                            value: "1080p".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::VideoCodec {
                        matcher: TextMatch::Eq {
                            value: "hevc".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::AudioCodec {
                        matcher: TextMatch::Eq {
                            value: "aac".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Container {
                        matcher: TextMatch::Eq {
                            value: "mkv".to_string(),
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Tags {
                        matcher: ArrayMatch::ContainsAny {
                            values: vec!["cult".to_string()],
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Actors {
                        matcher: ArrayMatch::ContainsAny {
                            values: vec!["actor a".to_string()],
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Writers {
                        matcher: ArrayMatch::ContainsAny {
                            values: vec!["writer a".to_string()],
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::DurationSeconds {
                        cmp: IntComparison::Between {
                            min: 5000,
                            max: 6000,
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::AudioChannels {
                        cmp: IntComparison::Eq { value: 2 },
                    },
                },
            ],
        },
        FilterNode::Group {
            op: LogicalOp::And,
            negated: true,
            children: vec![FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Actors {
                    matcher: ArrayMatch::IsEmpty,
                },
            }],
        },
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
            condition: FilterCondition::AddedAt {
                cmp: TimeComparison::OlderThanDays { days: 30 },
            },
        },
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::AddedAt {
                cmp: TimeComparison::BetweenEpochs {
                    min: now - 20 * 86_400,
                    max: now,
                },
            },
        },
    ];

    for tree in extra_trees {
        let cfg = WidgetFilterConfig {
            rule_tree: Some(tree.clone()),
            ..Default::default()
        };
        let (sql_items, _) = media_repo
            .find_custom_paginated(None, "u1", 50, 0, None, &[], Some(&cfg))
            .await
            .unwrap();
        let mut sql_ids: Vec<i64> = sql_items.iter().filter_map(|i| i.id).collect();
        sql_ids.sort();

        let mut rust_ids = Vec::new();
        for item in &stored {
            let st = playback_repo
                .get_state("u1", item.id.unwrap())
                .await
                .unwrap();
            if tree.matches(item, st.as_ref(), now) {
                rust_ids.push(item.id.unwrap());
            }
        }
        rust_ids.sort();
        assert_eq!(sql_ids, rust_ids, "Mismatch on extra tree: {tree:?}");
    }
}

#[tokio::test]
async fn test_explain_query_plan_uses_filter_ast_indexes_and_negated_null_groups_match_rust() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_filter_ast_eqp.db");
    let pool = create_pool(&db_path, 4).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "u1".to_string(),
            username: "viewer".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Admin,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-main".to_string(),
            name: "Main Library".to_string(),
            path: PathBuf::from("/main"),
            paths: vec![PathBuf::from("/main")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    let now = 1_700_000_000_i64;
    let seed_items = vec![
        MediaItem {
            id: None,
            library_id: "lib-main".to_string(),
            item_type: MediaType::Movie,
            title: "The Mummy (Al-Mummia)".to_string(),
            original_title: None,
            release_year: Some(1969),
            added_at: now - 86_400,
            file_path: PathBuf::from("/main/mummy.mkv"),
            file_name: "mummy.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 6120,
                resolution: Some("1080p".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("flac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                country: Some("Egypt".to_string()),
                ..Default::default()
            },
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "lib-main".to_string(),
            item_type: MediaType::Movie,
            title: "Mystery Null Metadata Film".to_string(),
            original_title: None,
            release_year: None,
            added_at: now - 172_800,
            file_path: PathBuf::from("/main/mystery.mkv"),
            file_name: "mystery.mkv".to_string(),
            file_size: 800,
            technical: TechnicalInfo {
                duration_seconds: 4800,
                resolution: None,
                video_codec: None,
                audio_codec: None,
                audio_channels: None,
                container: None,
                audio_tracks: vec![],
            },
            metadata: MediaMetadata::default(),
            locked_fields: vec![],
        },
    ];
    media_repo.upsert_batch(&seed_items).await.unwrap();

    let country_rule = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Country {
            matcher: TextMatch::Eq {
                value: "Egypt".to_string(),
            },
        },
    };
    let year_rule = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::ReleaseYear {
            cmp: IntComparison::Gte { value: 1960 },
        },
    };

    // Assert EXPLAIN QUERY PLAN uses idx_media_country_lower and idx_media_release_year
    {
        let conn = pool.get().await.unwrap();
        let country_tree = country_rule.clone();
        let year_tree = year_rule.clone();
        conn.interact(move |c| {
            let explain_plan = |node: &FilterNode| -> String {
                let compiled = compile_filter_node(node, now);
                let sql = format!(
                    "EXPLAIN QUERY PLAN SELECT m.id FROM media_items m \
                     JOIN libraries l ON m.library_id = l.id \
                     WHERE {}",
                    compiled.sql
                );
                let mut stmt = c.prepare(&sql).unwrap();
                let details: Vec<String> = stmt
                    .query_map(rusqlite::params_from_iter(compiled.params), |r| {
                        r.get::<_, String>(3)
                    })
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                details.join(" | ")
            };

            let country_plan = explain_plan(&country_tree);
            assert!(
                country_plan.contains("SEARCH m USING INDEX idx_media_country_lower"),
                "Expected idx_media_country_lower in EXPLAIN QUERY PLAN, got: {country_plan}"
            );

            let year_plan = explain_plan(&year_tree);
            assert!(
                year_plan.contains("SEARCH m USING INDEX idx_media_release_year"),
                "Expected idx_media_release_year in EXPLAIN QUERY PLAN, got: {year_plan}"
            );

            // Also verify full envelope query for Country = 'Egypt' uses idx_media_country_lower
            let env = build_filter_envelope(
                Some(&WidgetFilterConfig {
                    rule_tree: Some(country_tree),
                    ..Default::default()
                }),
                &[],
                now,
            );
            let env_sql = format!(
                "EXPLAIN QUERY PLAN SELECT m.id FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE m.item_type != 'episode' AND {}",
                env.sql
            );
            let env_details: Vec<String> = c
                .prepare(&env_sql)
                .unwrap()
                .query_map(rusqlite::params_from_iter(env.params), |r| {
                    r.get::<_, String>(3)
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            let env_plan = env_details.join(" | ");
            assert!(
                env_plan.contains("SEARCH m USING INDEX idx_media_country_lower"),
                "Expected idx_media_country_lower in envelope EXPLAIN QUERY PLAN, got: {env_plan}"
            );
        })
        .await
        .unwrap();
    }

    // Verify negated Group over NULL-producing rules retains 100% parity with Rust evaluator
    let stored = media_repo.list_all_by_library("lib-main").await.unwrap();
    let negated_null_trees = vec![
        FilterNode::Group {
            op: LogicalOp::Or,
            negated: true,
            children: vec![country_rule.clone(), year_rule.clone()],
        },
        FilterNode::Group {
            op: LogicalOp::And,
            negated: true,
            children: vec![country_rule, year_rule],
        },
    ];
    for tree in negated_null_trees {
        let cfg = WidgetFilterConfig {
            rule_tree: Some(tree.clone()),
            ..Default::default()
        };
        let (sql_items, _) = media_repo
            .find_custom_paginated(None, "u1", 50, 0, None, &[], Some(&cfg))
            .await
            .unwrap();
        let mut sql_ids: Vec<i64> = sql_items.iter().filter_map(|i| i.id).collect();
        sql_ids.sort();

        let mut rust_ids: Vec<i64> = stored
            .iter()
            .filter(|i| tree.matches(i, None, now))
            .filter_map(|i| i.id)
            .collect();
        rust_ids.sort();
        assert_eq!(sql_ids, rust_ids, "Negated NULL parity mismatch: {tree:?}");
    }
}
