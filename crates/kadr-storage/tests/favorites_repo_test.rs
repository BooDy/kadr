use std::collections::HashSet;
use std::path::PathBuf;

use kadr_core::ast::WidgetFilterConfig;
use kadr_core::filter_ast::{
    BoolOp, FilterCondition, FilterNode, LogicalOp, SetOp, WatchStateMatch,
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_core::search::SearchQuery;
use kadr_storage::pool::{create_in_memory_pool, create_pool, initialize_database};
use kadr_storage::repos::{
    build_filter_envelope, build_filter_envelope_for_user, compile_filter_node,
    compile_filter_node_for_user, FavoritesRepo, FavoritesRepository, LibraryRepository,
    MediaItemRepository, PlaybackRepository, SearchRepository, UserRepository,
};
use tempfile::TempDir;

#[allow(clippy::too_many_arguments)]
fn make_item(
    lib_id: &str,
    item_type: MediaType,
    title: &str,
    file_path: &str,
    added_at: i64,
    rating: Option<f32>,
    genres: Vec<&str>,
) -> MediaItem {
    let path = PathBuf::from(file_path);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file.mkv".to_string());
    MediaItem {
        id: None,
        library_id: lib_id.to_string(),
        item_type,
        title: title.to_string(),
        original_title: None,
        release_year: Some(2024),
        added_at,
        file_path: path,
        file_name,
        file_size: 1024,
        technical: TechnicalInfo {
            duration_seconds: 3600,
            resolution: Some("1080p".to_string()),
            ..Default::default()
        },
        metadata: MediaMetadata {
            rating,
            genres: genres.into_iter().map(str::to_string).collect(),
            backdrop_path: Some("/backdrop.jpg".to_string()),
            ..Default::default()
        },
        locked_fields: Vec::new(),
    }
}

async fn insert_and_get_id(repo: &MediaItemRepository, item: MediaItem) -> i64 {
    repo.upsert_batch(std::slice::from_ref(&item))
        .await
        .unwrap();
    repo.find_by_path(&item.file_path)
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap()
}

#[tokio::test]
async fn per_user_isolation_idempotent_add_batch_lookup_and_remove() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let fav_repo: FavoritesRepo = FavoritesRepository::new(pool.clone());

    // Verify pool() accessor is exposed
    let _shared_pool = fav_repo.pool();

    for (id, name) in [("u1", "alice"), ("u2", "bob")] {
        user_repo
            .create(&User {
                id: id.to_string(),
                username: name.to_string(),
                pin_hash: "hash".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();
    }

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 100,
        })
        .await
        .unwrap();

    let m1 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Movie One",
            "/pub/m1.mkv",
            1000,
            Some(8.0),
            vec!["Drama"],
        ),
    )
    .await;
    let m2 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Movie Two",
            "/pub/m2.mkv",
            1001,
            Some(7.5),
            vec!["Sci-Fi"],
        ),
    )
    .await;
    let m3 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Movie Three",
            "/pub/m3.mkv",
            1002,
            Some(6.5),
            vec!["Action"],
        ),
    )
    .await;

    // Empty batch lookup
    assert!(fav_repo
        .get_favorited_ids("u1", &[])
        .await
        .unwrap()
        .is_empty());
    assert!(!fav_repo.is_favorite("u1", m1).await.unwrap());

    // Add favorites for u1 and u2
    fav_repo
        .add_favorite("u1", m1, 1_700_000_100)
        .await
        .unwrap();
    fav_repo
        .add_favorite("u1", m2, 1_700_000_200)
        .await
        .unwrap();
    fav_repo
        .add_favorite("u2", m2, 1_700_000_300)
        .await
        .unwrap();

    // Idempotent re-add must preserve original favorited_at (1_700_000_100)
    fav_repo
        .add_favorite("u1", m1, 1_999_999_999)
        .await
        .unwrap();

    assert!(fav_repo.is_favorite("u1", m1).await.unwrap());
    assert!(fav_repo.is_favorite("u1", m2).await.unwrap());
    assert!(!fav_repo.is_favorite("u1", m3).await.unwrap());

    assert!(!fav_repo.is_favorite("u2", m1).await.unwrap());
    assert!(fav_repo.is_favorite("u2", m2).await.unwrap());

    let mut large_batch: Vec<i64> = (10_000..10_600).collect();
    large_batch.extend([m1, m2, m3]);
    let u1_large_set = fav_repo
        .get_favorited_ids("u1", &large_batch)
        .await
        .unwrap();
    assert_eq!(u1_large_set, HashSet::from([m1, m2]));

    let u2_set = fav_repo
        .get_favorited_ids("u2", &[m1, m2, m3])
        .await
        .unwrap();
    assert_eq!(u2_set, HashSet::from([m2]));

    // Verify original favorited_at preserved for (u1, m1)
    let (u1_list, total, locked) = fav_repo
        .list_favorites("u1", None, None, 10, 0, Some("favorited_at:asc"), &[], None)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(locked, 0);
    assert_eq!(u1_list[0].0, 1_700_000_100);
    assert_eq!(u1_list[0].1.id, Some(m1));
    assert_eq!(u1_list[1].0, 1_700_000_200);
    assert_eq!(u1_list[1].1.id, Some(m2));

    // remove_favorite returns true once, false when already absent, and does not affect u2
    assert!(fav_repo.remove_favorite("u1", m2).await.unwrap());
    assert!(!fav_repo.remove_favorite("u1", m2).await.unwrap());
    assert!(!fav_repo.is_favorite("u1", m2).await.unwrap());
    assert!(fav_repo.is_favorite("u2", m2).await.unwrap());
}

#[tokio::test]
async fn foreign_key_cascade_on_user_and_media_item_delete() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let fav_repo = FavoritesRepository::new(pool.clone());

    for (id, name) in [("u1", "alice"), ("u2", "bob")] {
        user_repo
            .create(&User {
                id: id.to_string(),
                username: name.to_string(),
                pin_hash: "hash".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();
    }

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 100,
        })
        .await
        .unwrap();

    let m1 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Cascade Movie 1",
            "/pub/c1.mkv",
            1000,
            None,
            vec![],
        ),
    )
    .await;
    let m2 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Cascade Movie 2",
            "/pub/c2.mkv",
            1001,
            None,
            vec![],
        ),
    )
    .await;

    fav_repo.add_favorite("u1", m1, 100).await.unwrap();
    fav_repo.add_favorite("u1", m2, 200).await.unwrap();
    fav_repo.add_favorite("u2", m1, 300).await.unwrap();
    fav_repo.add_favorite("u2", m2, 400).await.unwrap();

    // Deleting MediaItem m1 cascades to both u1 and u2 favorites for m1
    assert!(media_repo.delete_by_path("/pub/c1.mkv").await.unwrap());
    assert!(!fav_repo.is_favorite("u1", m1).await.unwrap());
    assert!(!fav_repo.is_favorite("u2", m1).await.unwrap());
    assert!(fav_repo.is_favorite("u1", m2).await.unwrap());
    assert!(fav_repo.is_favorite("u2", m2).await.unwrap());

    // Deleting User u1 cascades to u1's remaining favorites while keeping u2's intact
    assert!(user_repo.delete("u1").await.unwrap());
    assert!(!fav_repo.is_favorite("u1", m2).await.unwrap());
    assert!(fav_repo.is_favorite("u2", m2).await.unwrap());

    let conn = pool.get().await.unwrap();
    let remaining_rows: i64 = conn
        .interact(|c| c.query_row("SELECT COUNT(*) FROM user_favorites", [], |r| r.get(0)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(remaining_rows, 1);
}

#[tokio::test]
async fn list_favorites_sorting_item_types_episodes_and_private_library_isolation() {
    let dir = TempDir::new().unwrap();
    let pool = create_pool(dir.path().join("fav_list.db"), 2).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let fav_repo = FavoritesRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "u1".to_string(),
            username: "alice".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Standard,
            created_at: 100,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public Cinema".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 100,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-priv".to_string(),
            name: "Secret Vault".to_string(),
            path: PathBuf::from("/priv"),
            paths: vec![PathBuf::from("/priv")],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("secret-pin".to_string()),
            created_at: 100,
        })
        .await
        .unwrap();

    let pub_movie = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Arrival",
            "/pub/arrival.mkv",
            1000,
            Some(8.0),
            vec!["Sci-Fi"],
        ),
    )
    .await;
    let pub_show = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Show,
            "Severance",
            "/pub/severance",
            1100,
            Some(9.2),
            vec!["Drama", "Sci-Fi"],
        ),
    )
    .await;
    let pub_episode = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Episode,
            "Severance S01E01",
            "/pub/severance_s01e01.mkv",
            1200,
            Some(8.7),
            vec!["Drama"],
        ),
    )
    .await;
    let priv_movie = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-priv",
            MediaType::Movie,
            "Vault Film",
            "/priv/vault.mkv",
            1300,
            Some(9.9),
            vec!["Mystery"],
        ),
    )
    .await;
    let priv_episode = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-priv",
            MediaType::Episode,
            "Vault Episode",
            "/priv/vault_ep.mkv",
            1400,
            Some(9.5),
            vec!["Mystery"],
        ),
    )
    .await;

    fav_repo.add_favorite("u1", pub_movie, 100).await.unwrap();
    fav_repo.add_favorite("u1", pub_show, 200).await.unwrap();
    fav_repo.add_favorite("u1", pub_episode, 300).await.unwrap();
    fav_repo.add_favorite("u1", priv_movie, 400).await.unwrap();
    fav_repo
        .add_favorite("u1", priv_episode, 500)
        .await
        .unwrap();

    playback_repo
        .upsert_progress("u1", pub_movie, 600, WatchState::InProgress, 5_000)
        .await
        .unwrap();
    playback_repo
        .upsert_progress("u1", pub_episode, 1320, WatchState::Completed, 9_000)
        .await
        .unwrap();

    // 1. Default sort (favorited_at:desc) with locked private library:
    //    - Episodes and Shows ARE included
    //    - Zero private items leaked, locked_count == 2
    let (items_desc, visible_total, locked_count) = fav_repo
        .list_favorites("u1", None, None, 10, 0, None, &[], None)
        .await
        .unwrap();
    assert_eq!(visible_total, 3);
    assert_eq!(locked_count, 2);
    let desc_ids: Vec<i64> = items_desc.iter().map(|(_, m)| m.id.unwrap()).collect();
    assert_eq!(desc_ids, vec![pub_episode, pub_show, pub_movie]);
    let desc_ts: Vec<i64> = items_desc.iter().map(|(ts, _)| *ts).collect();
    assert_eq!(desc_ts, vec![300, 200, 100]);

    // 2. Pagination preserves visible_total & locked_count
    let (page_items, page_total, page_locked) = fav_repo
        .list_favorites("u1", None, None, 2, 1, Some("favorited_at:desc"), &[], None)
        .await
        .unwrap();
    assert_eq!(page_total, 3);
    assert_eq!(page_locked, 2);
    assert_eq!(
        page_items
            .iter()
            .map(|(_, m)| m.id.unwrap())
            .collect::<Vec<_>>(),
        vec![pub_show, pub_movie]
    );

    // 3. Sort: favorited_at:asc
    let (items_asc, _, _) = fav_repo
        .list_favorites("u1", None, None, 10, 0, Some("favorited_at:asc"), &[], None)
        .await
        .unwrap();
    assert_eq!(
        items_asc
            .iter()
            .map(|(_, m)| m.id.unwrap())
            .collect::<Vec<_>>(),
        vec![pub_movie, pub_show, pub_episode]
    );

    // 4. Sort: rating:desc (delegates to resolve_sort_clause)
    let (items_rating, _, _) = fav_repo
        .list_favorites("u1", None, None, 10, 0, Some("rating:desc"), &[], None)
        .await
        .unwrap();
    assert_eq!(
        items_rating
            .iter()
            .map(|(_, m)| m.id.unwrap())
            .collect::<Vec<_>>(),
        vec![pub_show, pub_episode, pub_movie]
    );

    // 5. Sort: last_watched_at:desc (exercises playback_join parameter ordering in list_favorites!)
    let (items_watched, _, _) = fav_repo
        .list_favorites(
            "u1",
            Some("lib-pub"),
            None,
            10,
            0,
            Some("last_watched_at:desc"),
            &[],
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        items_watched
            .iter()
            .map(|(_, m)| m.id.unwrap())
            .collect::<Vec<_>>(),
        vec![pub_episode, pub_movie, pub_show]
    );

    // 6. Filter by MediaType::Episode and MediaType::Show (and scoped locked_count!)
    let (ep_items, ep_total, ep_locked) = fav_repo
        .list_favorites("u1", None, Some(MediaType::Episode), 10, 0, None, &[], None)
        .await
        .unwrap();
    assert_eq!(ep_total, 1);
    assert_eq!(ep_locked, 1); // priv_episode is locked!
    assert_eq!(ep_items[0].1.id, Some(pub_episode));

    let (show_items, show_total, show_locked) = fav_repo
        .list_favorites("u1", None, Some(MediaType::Show), 10, 0, None, &[], None)
        .await
        .unwrap();
    assert_eq!(show_total, 1);
    assert_eq!(show_locked, 0);
    assert_eq!(show_items[0].1.id, Some(pub_show));

    // Scoped to locked private library directly when locked -> 0 visible, 2 locked
    let (priv_locked_items, priv_locked_total, priv_locked_cnt) = fav_repo
        .list_favorites("u1", Some("lib-priv"), None, 10, 0, None, &[], None)
        .await
        .unwrap();
    assert!(priv_locked_items.is_empty());
    assert_eq!(priv_locked_total, 0);
    assert_eq!(priv_locked_cnt, 2);

    // 7. Unlocking lib-priv reveals private items and drops locked_count to 0
    let unlocked = vec!["lib-priv".to_string()];
    let (all_items, all_total, all_locked) = fav_repo
        .list_favorites("u1", None, None, 10, 0, None, &unlocked, None)
        .await
        .unwrap();
    assert_eq!(all_total, 5);
    assert_eq!(all_locked, 0);
    assert_eq!(
        all_items
            .iter()
            .map(|(_, m)| m.id.unwrap())
            .collect::<Vec<_>>(),
        vec![priv_episode, priv_movie, pub_episode, pub_show, pub_movie]
    );

    // 8. Combined parameter ordering: unlocked_ids + library_id + item_type + playback sort + rule_tree (WatchState + IsFavorite)
    let combo_cfg = WidgetFilterConfig {
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
                    condition: FilterCondition::IsFavorite {
                        op: BoolOp::Eq,
                        value: true,
                    },
                },
            ],
        }),
        ..Default::default()
    };
    let (combo_items, combo_total, combo_locked) = fav_repo
        .list_favorites(
            "u1",
            Some("lib-pub"),
            Some(MediaType::Episode),
            10,
            0,
            Some("last_watched_at:desc"),
            &unlocked,
            Some(&combo_cfg),
        )
        .await
        .unwrap();
    assert_eq!(combo_total, 1);
    assert_eq!(combo_locked, 0);
    assert_eq!(combo_items[0].1.id, Some(pub_episode));
}

#[tokio::test]
async fn is_favorite_filter_sql_vs_rust_parity_and_privacy_isolation() {
    let dir = TempDir::new().unwrap();
    let pool = create_pool(dir.path().join("fav_parity.db"), 2).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let fav_repo = FavoritesRepository::new(pool.clone());

    for (uid, uname) in [("u1", "alice"), ("u2", "bob")] {
        user_repo
            .create(&User {
                id: uid.to_string(),
                username: uname.to_string(),
                pin_hash: "hash".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();
    }

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 100,
        })
        .await
        .unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-priv".to_string(),
            name: "Private".to_string(),
            path: PathBuf::from("/priv"),
            paths: vec![PathBuf::from("/priv")],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("pin".to_string()),
            created_at: 100,
        })
        .await
        .unwrap();

    let now = 1_750_000_000i64;
    let m1 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Alpha",
            "/pub/alpha.mkv",
            now - 100,
            Some(8.5),
            vec!["Sci-Fi"],
        ),
    )
    .await;
    let m2 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Beta",
            "/pub/beta.mkv",
            now - 200,
            Some(7.2),
            vec!["Sci-Fi", "Drama"],
        ),
    )
    .await;
    let ep1 = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Episode,
            "Gamma Ep",
            "/pub/gamma_ep.mkv",
            now - 300,
            Some(9.0),
            vec!["Drama"],
        ),
    )
    .await;
    let priv_m = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-priv",
            MediaType::Movie,
            "Private Omega",
            "/priv/omega.mkv",
            now - 50,
            Some(9.8),
            vec!["Sci-Fi"],
        ),
    )
    .await;

    // u1 favorites m1, ep1, priv_m; u2 favorites m2
    fav_repo.add_favorite("u1", m1, now - 10).await.unwrap();
    fav_repo.add_favorite("u1", ep1, now - 20).await.unwrap();
    fav_repo.add_favorite("u1", priv_m, now - 5).await.unwrap();
    fav_repo.add_favorite("u2", m2, now - 15).await.unwrap();

    playback_repo
        .upsert_progress("u1", m1, 1800, WatchState::InProgress, now - 50)
        .await
        .unwrap();
    playback_repo
        .upsert_progress("u1", m2, 1800, WatchState::InProgress, now - 60)
        .await
        .unwrap();

    let all_ids = [m1, m2, ep1, priv_m];
    let all_items_map = media_repo.get_by_ids(&all_ids).await.unwrap();
    let all_items: Vec<MediaItem> = all_ids
        .iter()
        .map(|id| all_items_map.get(id).unwrap().clone())
        .collect();

    let trees = [
        // 1. IsFavorite == true
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::IsFavorite {
                op: BoolOp::Eq,
                value: true,
            },
        },
        // 2. IsFavorite == false
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::IsFavorite {
                op: BoolOp::Eq,
                value: false,
            },
        },
        // 3. Negated IsFavorite == true
        FilterNode::Rule {
            negated: true,
            condition: FilterCondition::IsFavorite {
                op: BoolOp::Eq,
                value: true,
            },
        },
        // 4. Negated IsFavorite == false
        FilterNode::Rule {
            negated: true,
            condition: FilterCondition::IsFavorite {
                op: BoolOp::Eq,
                value: false,
            },
        },
        // 5. Compound Group combining WatchState (needs_playback_join) + ItemType (includes Episode) + IsFavorite
        FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::ItemType {
                        op: SetOp::In,
                        values: vec![MediaType::Movie, MediaType::Episode],
                    },
                },
                FilterNode::Group {
                    op: LogicalOp::Or,
                    negated: false,
                    children: vec![
                        FilterNode::Rule {
                            negated: false,
                            condition: FilterCondition::WatchState {
                                matcher: WatchStateMatch::StateIn {
                                    values: vec![WatchState::InProgress],
                                },
                            },
                        },
                        FilterNode::Rule {
                            negated: false,
                            condition: FilterCondition::ItemType {
                                op: SetOp::In,
                                values: vec![MediaType::Episode],
                            },
                        },
                    ],
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::IsFavorite {
                        op: BoolOp::Eq,
                        value: true,
                    },
                },
            ],
        },
    ];

    for unlocked in [Vec::<String>::new(), vec!["lib-priv".to_string()]] {
        for uid in ["u1", "u2"] {
            let fav_set = fav_repo.get_favorited_ids(uid, &all_ids).await.unwrap();
            let pb_map = playback_repo
                .get_states_for_items(uid, &all_ids)
                .await
                .unwrap();

            for (idx, tree) in trees.iter().enumerate() {
                tree.validate().unwrap();
                let cfg = WidgetFilterConfig {
                    rule_tree: Some(tree.clone()),
                    ..Default::default()
                };

                let (sql_items, sql_count) = media_repo
                    .find_custom_paginated(
                        None,
                        uid,
                        50,
                        0,
                        Some("title:asc"),
                        &unlocked,
                        Some(&cfg),
                    )
                    .await
                    .unwrap();
                assert_eq!(sql_items.len() as u64, sql_count);

                let mut expected_items: Vec<&MediaItem> = all_items
                    .iter()
                    .filter(|item| {
                        let lib_visible =
                            item.library_id != "lib-priv" || unlocked.contains(&item.library_id);
                        if !lib_visible {
                            return false;
                        }
                        if !tree.references_item_type() && item.item_type == MediaType::Episode {
                            return false;
                        }
                        let id = item.id.unwrap();
                        let pb = pb_map.get(&id);
                        let is_fav = fav_set.contains(&id);
                        tree.matches_with_context(item, pb, is_fav, now)
                    })
                    .collect();
                expected_items.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));

                let sql_ids: Vec<i64> = sql_items.iter().map(|i| i.id.unwrap()).collect();
                let rust_ids: Vec<i64> = expected_items.iter().map(|i| i.id.unwrap()).collect();
                assert_eq!(
                    sql_ids, rust_ids,
                    "SQL vs Rust mismatch on tree #{idx} for user={uid}, unlocked={unlocked:?}"
                );
            }
        }
    }

    // Verify WidgetQueries *_for_user methods honor IsFavorite for user u1
    let fav_only_cfg = WidgetFilterConfig {
        rule_tree: Some(FilterNode::Rule {
            negated: false,
            condition: FilterCondition::IsFavorite {
                op: BoolOp::Eq,
                value: true,
            },
        }),
        ..Default::default()
    };

    let recent = media_repo
        .find_recently_added_paginated_for_user(None, Some("u1"), 10, 0, &[], Some(&fav_only_cfg))
        .await
        .unwrap();
    assert_eq!(
        recent.iter().map(|m| m.id.unwrap()).collect::<Vec<_>>(),
        vec![m1]
    );

    let top_rated = media_repo
        .find_top_rated_paginated_for_user(None, Some("u1"), 10, 0, &[], Some(&fav_only_cfg))
        .await
        .unwrap();
    assert_eq!(
        top_rated.iter().map(|m| m.id.unwrap()).collect::<Vec<_>>(),
        vec![m1]
    );

    let by_genre = media_repo
        .find_by_genre_paginated_for_user(
            "Sci-Fi",
            None,
            Some("u1"),
            10,
            0,
            &[],
            Some(&fav_only_cfg),
        )
        .await
        .unwrap();
    assert_eq!(
        by_genre.iter().map(|m| m.id.unwrap()).collect::<Vec<_>>(),
        vec![m1]
    );

    let (by_lib, by_lib_total) = media_repo
        .find_by_library_paginated_for_user(
            "lib-pub",
            Some("u1"),
            10,
            0,
            None,
            &[],
            Some(&fav_only_cfg),
        )
        .await
        .unwrap();
    assert_eq!(by_lib_total, 1);
    assert_eq!(by_lib[0].id, Some(m1));

    let spotlight = media_repo
        .find_spotlight_candidate_for_user(None, Some("u1"), &[], Some(&fav_only_cfg))
        .await
        .unwrap();
    assert_eq!(spotlight.and_then(|m| m.id), Some(m1));

    // Also verify compile_filter_node / compile_filter_node_for_user and build_filter_envelope(_for_user)
    let anon_compiled = compile_filter_node(fav_only_cfg.rule_tree.as_ref().unwrap(), now);
    assert_eq!(anon_compiled.params.len(), 1);
    let u1_compiled =
        compile_filter_node_for_user(fav_only_cfg.rule_tree.as_ref().unwrap(), Some("u1"), now);
    assert_eq!(u1_compiled.params.len(), 1);
    let _env_anon = build_filter_envelope(Some(&fav_only_cfg), &[], now);
    let _env_u1 = build_filter_envelope_for_user(Some(&fav_only_cfg), Some("u1"), &[], now);
}

#[tokio::test]
async fn search_repository_populates_per_user_is_favorite() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let fav_repo = FavoritesRepository::new(pool.clone());
    let search_repo = SearchRepository::new(pool.clone());

    for (uid, uname) in [("u1", "alice"), ("u2", "bob")] {
        user_repo
            .create(&User {
                id: uid.to_string(),
                username: uname.to_string(),
                pin_hash: "hash".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();
    }

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Sci-Fi Vault".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 100,
        })
        .await
        .unwrap();

    let item_id = insert_and_get_id(
        &media_repo,
        make_item(
            "lib-pub",
            MediaType::Movie,
            "Interstellar Odyssey",
            "/pub/interstellar.mkv",
            1_700_000_000,
            Some(8.8),
            vec!["Sci-Fi"],
        ),
    )
    .await;

    fav_repo
        .add_favorite("u1", item_id, 1_700_000_500)
        .await
        .unwrap();
    playback_repo
        .upsert_progress("u1", item_id, 1800, WatchState::InProgress, 1_700_000_600)
        .await
        .unwrap();

    let u1_res = search_repo
        .search(&SearchQuery::new("Interstellar"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(u1_res.total, 1);
    assert_eq!(u1_res.items[0].id, item_id);
    assert!(u1_res.items[0].is_favorite);
    assert_eq!(u1_res.items[0].playback_progress, Some(0.5));

    let u2_res = search_repo
        .search(&SearchQuery::new("Interstellar"), "u2", &[])
        .await
        .unwrap();
    assert_eq!(u2_res.total, 1);
    assert_eq!(u2_res.items[0].id, item_id);
    assert!(!u2_res.items[0].is_favorite);
    assert_eq!(u2_res.items[0].playback_progress, None);

    // Removing favorite flips search result back to false
    fav_repo.remove_favorite("u1", item_id).await.unwrap();
    let u1_after = search_repo
        .search(&SearchQuery::new("Interstellar"), "u1", &[])
        .await
        .unwrap();
    assert!(!u1_after.items[0].is_favorite);
}
