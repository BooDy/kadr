use kadr_core::ast::{
    CardViewModel, QueryMacro, ResolvedLayoutScope, ScreenId, ScreenLayout, TargetDevice,
    TimeWindow, WidgetNode, WidgetQueryBinding,
};
use kadr_core::models::{User, UserRole};
use kadr_storage::error::StorageError;
use kadr_storage::pool::{create_in_memory_pool, create_pool, initialize_database};
use kadr_storage::repos::{
    LayoutOverrideRepo, LayoutOverrideRepository, StoredLayoutOverride, UserRepository,
};
use tempfile::TempDir;

fn make_layout(screen_id: ScreenId, title: &str, widget_id: &str) -> ScreenLayout {
    ScreenLayout::new(
        screen_id,
        title,
        vec![WidgetNode::Carousel {
            id: widget_id.to_string(),
            title: format!("{title} Rail"),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded).with_limit(12),
            time_window: None,
            items: None,
            next_cursor: None,
        }],
    )
}

async fn seed_users(user_repo: &UserRepository) {
    for (id, username) in [("u1", "alice"), ("u2", "bob")] {
        user_repo
            .create(&User {
                id: id.to_string(),
                username: username.to_string(),
                pin_hash: "pin-hash".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn global_and_per_user_upsert_idempotency_and_multi_scope_isolation() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    seed_users(&user_repo).await;

    let repo: LayoutOverrideRepo = LayoutOverrideRepository::new(pool.clone());
    let _shared_pool = repo.pool();

    // 1. Double-upsert Global (user_id = None, TargetDevice::Tv) must update in place (count == 1)
    let global_tv_v1 = make_layout(ScreenId::Home, "Living Room TV v1", "tv-rail-v1");
    let global_tv_v2 = make_layout(ScreenId::Home, "Living Room TV v2", "tv-rail-v2");

    repo.upsert("home", None, TargetDevice::Tv, &global_tv_v1, 1_700_000_100)
        .await
        .unwrap();
    repo.upsert("home", None, TargetDevice::Tv, &global_tv_v2, 1_700_000_200)
        .await
        .unwrap();

    let all_after_global = repo.list_all().await.unwrap();
    assert_eq!(
        all_after_global.len(),
        1,
        "expression unique index must deduplicate (screen_id, NULL, target_device)"
    );
    assert_eq!(
        all_after_global[0],
        StoredLayoutOverride {
            screen_id: "home".to_string(),
            user_id: None,
            target_device: TargetDevice::Tv,
            layout: global_tv_v2.clone(),
            updated_at: 1_700_000_200,
        }
    );

    // Verify user_id column is stored as true SQL NULL (never empty string "")
    {
        let conn = pool.get().await.unwrap();
        let is_sql_null: bool = conn
            .interact(|c| {
                c.query_row(
                    "SELECT user_id IS NULL FROM layout_overrides WHERE screen_id = 'home'",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert!(is_sql_null, "None user_id must bind SQL NULL");
    }

    // 2. Per-user scopes across u1 (All, Mobile) and u2 (Tv), including double-upsert on (u1, All)
    let u1_all_v1 = make_layout(ScreenId::Home, "Alice Default v1", "u1-all-v1");
    let u1_all_v2 = make_layout(ScreenId::Home, "Alice Default v2", "u1-all-v2");
    let u1_mobile = make_layout(ScreenId::Home, "Alice Phone", "u1-mobile");
    let u2_tv = make_layout(ScreenId::Home, "Bob TV", "u2-tv");

    repo.upsert("home", Some("u1"), TargetDevice::All, &u1_all_v1, 1_000)
        .await
        .unwrap();
    repo.upsert("home", Some("u1"), TargetDevice::All, &u1_all_v2, 1_050)
        .await
        .unwrap();
    repo.upsert("home", Some("u1"), TargetDevice::Mobile, &u1_mobile, 1_100)
        .await
        .unwrap();
    repo.upsert("home", Some("u2"), TargetDevice::Tv, &u2_tv, 1_200)
        .await
        .unwrap();

    assert_eq!(repo.list_all().await.unwrap().len(), 4);

    let got_global_tv = repo
        .get("home", None, TargetDevice::Tv)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_global_tv.layout, global_tv_v2);
    assert_eq!(got_global_tv.updated_at, 1_700_000_200);

    let got_u1_all = repo
        .get("home", Some("u1"), TargetDevice::All)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_u1_all.layout, u1_all_v2);
    assert_eq!(got_u1_all.updated_at, 1_050);

    let got_u1_mobile = repo
        .get("home", Some("u1"), TargetDevice::Mobile)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_u1_mobile.layout, u1_mobile);

    let got_u2_tv = repo
        .get("home", Some("u2"), TargetDevice::Tv)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_u2_tv.layout, u2_tv);

    // Unpopulated slots remain isolated (None)
    assert!(repo
        .get("home", Some("u1"), TargetDevice::Tv)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .get("home", None, TargetDevice::Mobile)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn strips_runtime_fields_and_preserves_dense_list_and_time_window() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    seed_users(&user_repo).await;

    let repo = LayoutOverrideRepository::new(pool.clone());

    let sample_card = CardViewModel {
        id: 42,
        title: "Hydrated Runtime Card".to_string(),
        media_type: "movie".to_string(),
        duration_seconds: Some(7200),
        ..Default::default()
    };

    let hydrated_layout = ScreenLayout {
        id: ScreenId::Home,
        title: "Night Cinema".to_string(),
        resolved_scope: Some(ResolvedLayoutScope {
            user_id: Some("u1".to_string()),
            target_device: TargetDevice::Web,
            is_exact_override: true,
        }),
        widgets: vec![
            WidgetNode::HeroBanner {
                id: "hero-1".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::SpotlightItem { item_id: Some(42) }),
                time_window: None,
                data: Some(sample_card.clone()),
            },
            WidgetNode::DenseList {
                id: "late-night-list".to_string(),
                title: "Late Night Picks".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::TopRated).with_limit(10),
                time_window: Some(TimeWindow {
                    start: "23:00".to_string(),
                    end: "04:00".to_string(),
                }),
                items: Some(vec![sample_card]),
                next_cursor: Some("10".to_string()),
                total_count: Some(99),
            },
        ],
    };

    repo.upsert(
        "home",
        Some("u1"),
        TargetDevice::Web,
        &hydrated_layout,
        1_750_000_000,
    )
    .await
    .unwrap();

    // Verify raw persisted JSON contains zero runtime/hydrated fields
    let raw_json: String = {
        let conn = pool.get().await.unwrap();
        conn.interact(|c| {
            c.query_row(
                "SELECT layout_json FROM layout_overrides WHERE screen_id = 'home' AND user_id = 'u1'",
                [],
                |row| row.get(0),
            )
        })
        .await
        .unwrap()
        .unwrap()
    };

    assert!(!raw_json.contains("resolved_scope"));
    assert!(!raw_json.contains("Hydrated Runtime Card"));
    assert!(!raw_json.contains("next_cursor"));
    assert!(!raw_json.contains("total_count"));
    assert!(raw_json.contains("dense_list"));
    assert!(raw_json.contains("23:00"));

    let stored = repo
        .get("home", Some("u1"), TargetDevice::Web)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.layout.resolved_scope.is_none());
    assert_eq!(stored.layout.widgets.len(), 2);
    for widget in &stored.layout.widgets {
        assert!(!widget.is_hydrated());
    }
    assert_eq!(
        stored.layout.widgets[1].time_window(),
        Some(&TimeWindow {
            start: "23:00".to_string(),
            end: "04:00".to_string(),
        })
    );
}

#[tokio::test]
async fn empty_user_id_is_rejected_and_never_collides_with_global_null() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let repo = LayoutOverrideRepository::new(pool.clone());
    let global_tv = make_layout(ScreenId::Home, "Global TV", "global-tv");

    repo.upsert("home", None, TargetDevice::Tv, &global_tv, 500)
        .await
        .unwrap();

    assert!(matches!(
        repo.upsert("home", Some(""), TargetDevice::Tv, &global_tv, 600)
            .await,
        Err(StorageError::InvalidInput(_))
    ));
    assert!(matches!(
        repo.get("home", Some(""), TargetDevice::Tv).await,
        Err(StorageError::InvalidInput(_))
    ));
    assert!(matches!(
        repo.delete("home", Some(""), TargetDevice::Tv).await,
        Err(StorageError::InvalidInput(_))
    ));

    // Global override must still exist untouched
    assert!(repo
        .get("home", None, TargetDevice::Tv)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn targeted_delete_and_delete_all_for_screen() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    seed_users(&user_repo).await;

    let repo = LayoutOverrideRepository::new(pool.clone());

    repo.upsert(
        "home",
        None,
        TargetDevice::Tv,
        &make_layout(ScreenId::Home, "Global TV", "g-tv"),
        100,
    )
    .await
    .unwrap();
    repo.upsert(
        "home",
        Some("u1"),
        TargetDevice::All,
        &make_layout(ScreenId::Home, "U1 All", "u1-all"),
        101,
    )
    .await
    .unwrap();
    repo.upsert(
        "home",
        Some("u1"),
        TargetDevice::Mobile,
        &make_layout(ScreenId::Home, "U1 Mobile", "u1-mob"),
        102,
    )
    .await
    .unwrap();

    let custom_id = ScreenId::Custom("custom_scifi".to_string());
    repo.upsert(
        "custom_scifi",
        None,
        TargetDevice::Tv,
        &make_layout(custom_id.clone(), "Sci-Fi TV", "sf-tv"),
        200,
    )
    .await
    .unwrap();
    repo.upsert(
        "custom_scifi",
        Some("u1"),
        TargetDevice::All,
        &make_layout(custom_id.clone(), "Sci-Fi U1", "sf-u1"),
        201,
    )
    .await
    .unwrap();
    repo.upsert(
        "custom_scifi",
        Some("u2"),
        TargetDevice::Mobile,
        &make_layout(custom_id, "Sci-Fi U2", "sf-u2"),
        202,
    )
    .await
    .unwrap();

    // Delete single scoped slot ("home", Some("u1"), Mobile)
    assert!(repo
        .delete("home", Some("u1"), TargetDevice::Mobile)
        .await
        .unwrap());
    assert!(!repo
        .delete("home", Some("u1"), TargetDevice::Mobile)
        .await
        .unwrap());

    assert!(repo
        .get("home", Some("u1"), TargetDevice::Mobile)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .get("home", Some("u1"), TargetDevice::All)
        .await
        .unwrap()
        .is_some());
    assert!(repo
        .get("home", None, TargetDevice::Tv)
        .await
        .unwrap()
        .is_some());

    // Delete all overrides for "custom_scifi" while preserving "home"
    let removed = repo.delete_all_for_screen("custom_scifi").await.unwrap();
    assert_eq!(removed, 3);
    assert_eq!(repo.delete_all_for_screen("custom_scifi").await.unwrap(), 0);

    let remaining = repo.list_all().await.unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().all(|entry| entry.screen_id == "home"));
}

#[tokio::test]
async fn foreign_key_cascade_on_user_delete_and_corrupt_json_skip_in_list_all() {
    let dir = TempDir::new().unwrap();
    let pool = create_pool(dir.path().join("scoped_layouts.db"), 2).unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    seed_users(&user_repo).await;

    let repo = LayoutOverrideRepository::new(pool.clone());

    repo.upsert(
        "home",
        None,
        TargetDevice::Tv,
        &make_layout(ScreenId::Home, "Global TV", "g-tv"),
        100,
    )
    .await
    .unwrap();
    repo.upsert(
        "home",
        Some("u1"),
        TargetDevice::All,
        &make_layout(ScreenId::Home, "U1 Home All", "u1-home-all"),
        101,
    )
    .await
    .unwrap();
    repo.upsert(
        "movies",
        Some("u1"),
        TargetDevice::Tv,
        &make_layout(ScreenId::Movies, "U1 Movies TV", "u1-movies-tv"),
        102,
    )
    .await
    .unwrap();
    repo.upsert(
        "home",
        Some("u2"),
        TargetDevice::Web,
        &make_layout(ScreenId::Home, "U2 Home Web", "u2-home-web"),
        103,
    )
    .await
    .unwrap();

    // Deleting u1 cascades via ON DELETE CASCADE while preserving u2 and Global rows
    assert!(user_repo.delete("u1").await.unwrap());

    assert!(repo
        .get("home", Some("u1"), TargetDevice::All)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .get("movies", Some("u1"), TargetDevice::Tv)
        .await
        .unwrap()
        .is_none());
    assert!(repo
        .get("home", Some("u2"), TargetDevice::Web)
        .await
        .unwrap()
        .is_some());
    assert!(repo
        .get("home", None, TargetDevice::Tv)
        .await
        .unwrap()
        .is_some());

    // Inject a manually corrupted JSON row and verify list_all() skips it without failing startup
    {
        let conn = pool.get().await.unwrap();
        conn.interact(|c| {
            c.execute(
                "INSERT INTO layout_overrides (screen_id, user_id, target_device, layout_json, updated_at)
                 VALUES ('shows', NULL, 'web', '{not valid json', 999)",
                [],
            )
        })
        .await
        .unwrap()
        .unwrap();
    }

    let valid_entries = repo.list_all().await.unwrap();
    assert_eq!(valid_entries.len(), 2);
}
