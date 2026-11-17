use std::collections::HashMap;

use kadr_core::models::{
    MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_core::search::SearchQuery;
use kadr_storage::migrations::{migrations, run_migrations};
use kadr_storage::pool::{create_in_memory_pool, create_pool, initialize_database};
use kadr_storage::repos::{
    sanitize_fts_query, LibraryRepo, MediaItemRepo, PlaybackRepository, SearchRepository,
    UserRepository,
};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn migration_011_backfills_existing_media_items_on_upgrade() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("upgrade_test.db");

    {
        let mut conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        // Migrate up to 009_playlists.sql (version 9), before 011_search_fts.sql
        migrations().to_version(&mut conn, 9).unwrap();

        conn.execute(
            "INSERT INTO libraries (id, name, path, media_type, is_private, created_at)
             VALUES ('lib-classic', 'World Cinema', '/media/world', 'movie', 0, 1700000000)",
            [],
        )
        .unwrap();

        let meta = json!({
            "director": "Youssef Chahine",
            "writers": ["Abdel Hay Adib"],
            "actors": ["Farid Shawqi", "Hind Rostom"],
            "overview": "A newspaper vendor at Cairo main train station develops an obsession.",
            "genres": ["Drama", "Crime"],
            "tags": ["Egyptian Classic", "Restored"],
            "studio": "Gabriel Talhami Films",
            "country": "Egypt"
        });
        conn.execute(
            "INSERT INTO media_items (
                library_id, item_type, title, original_title, release_year,
                duration_seconds, added_at, file_path, file_name, file_size, metadata
             ) VALUES (
                'lib-classic', 'movie', 'Cairo Station', 'باب الحديد', 1958,
                4620, 1700000100, '/media/world/cairo_station_1958.mkv', 'cairo_station_1958.mkv', 1024, ?1
             )",
            [meta.to_string()],
        )
        .unwrap();

        // Now run latest migrations including 011_search_fts.sql
        run_migrations(&mut conn).unwrap();
    }

    let pool = create_pool(&db_path, 2).unwrap();
    let search_repo = SearchRepository::new(pool);

    for term in [
        "Cairo Station",
        "باب الحديد",
        "Chahine",
        "Hind Rostom",
        "1958",
        "Gabriel Talhami",
        "Egyptian Classic",
        "Egypt",
    ] {
        let res = search_repo
            .search(&SearchQuery::new(term), "user-1", &[])
            .await
            .unwrap_or_else(|e| panic!("failed searching '{term}': {e}"));
        assert_eq!(res.total, 1, "expected backfill match for '{term}'");
        assert_eq!(res.items[0].title, "Cairo Station");
    }
}

#[tokio::test]
async fn fts_triggers_sync_insert_update_delete_and_cascade() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());
    let search_repo = SearchRepository::new(pool.clone());

    lib_repo
        .create(
            "lib-1",
            "Sci-Fi",
            "/media/scifi",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();

    // 1. AFTER INSERT via upsert_batch
    let mut item = MediaItem {
        library_id: "lib-1".into(),
        item_type: MediaType::Movie,
        title: "Project Hail Mary".into(),
        release_year: Some(2026),
        added_at: 1_700_000_000,
        file_path: "/media/scifi/hail_mary.mkv".into(),
        file_name: "hail_mary.mkv".into(),
        technical: TechnicalInfo {
            duration_seconds: 7200,
            ..Default::default()
        },
        metadata: MediaMetadata {
            director: Some("Phil Lord".into()),
            actors: vec!["Ryan Gosling".into()],
            genres: vec!["Sci-Fi".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    item_repo
        .upsert_batch(std::slice::from_ref(&item))
        .await
        .unwrap();

    let found = search_repo
        .search(&SearchQuery::new("Gosling"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(found.total, 1);
    let item_id = found.items[0].id;

    // 2. AFTER UPDATE via upsert_batch (rescan / metadata update)
    item.title = "Project Hail Mary (Director's Cut)".into();
    item.metadata.director = Some("Christopher Miller".into());
    item.metadata.actors = vec!["Sandra Hüller".into()];
    item_repo
        .upsert_batch(std::slice::from_ref(&item))
        .await
        .unwrap();

    // Old actor/director must no longer match!
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("Gosling"), "u1", &[])
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("Phil Lord"), "u1", &[])
            .await
            .unwrap()
            .total,
        0
    );

    // New diacritic-normalized actor ("Huller" matches "Hüller") & director match immediately
    let updated = search_repo
        .search(&SearchQuery::new("Huller"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(updated.total, 1);
    assert_eq!(updated.items[0].id, item_id);
    assert_eq!(
        updated.items[0].director.as_deref(),
        Some("Christopher Miller")
    );

    // 3. Direct SQL UPDATE on media_items (simulating inline metadata editor)
    {
        let conn = pool.get().await.unwrap();
        conn.interact(move |c| {
            let new_meta = json!({
                "director": "Denis Villeneuve",
                "actors": ["Timothée Chalamet"],
                "genres": ["Adventure", "Sci-Fi"]
            });
            c.execute(
                "UPDATE media_items SET title = 'Rendezvous with Rama', metadata = ?1 WHERE id = ?2",
                rusqlite::params![new_meta.to_string(), item_id],
            )
        })
        .await
        .unwrap()
        .unwrap();
    }

    // Old title word ("Project") is gone, while file_name ("hail_mary.mkv") remains searchable!
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("Project"), "u1", &[])
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("hail_mary"), "u1", &[])
            .await
            .unwrap()
            .total,
        1
    );
    let inline_edited = search_repo
        .search(&SearchQuery::new("Timothee Villeneuve"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(inline_edited.total, 1);
    assert_eq!(inline_edited.items[0].title, "Rendezvous with Rama");

    // 4. AFTER DELETE via delete_by_path
    item_repo
        .delete_by_path("/media/scifi/hail_mary.mkv")
        .await
        .unwrap();
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("Rendezvous"), "u1", &[])
            .await
            .unwrap()
            .total,
        0
    );

    // 5. Library ON DELETE CASCADE cleans up media_items_fts via AFTER DELETE trigger
    item_repo
        .upsert_batch(std::slice::from_ref(&item))
        .await
        .unwrap();
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("Miller"), "u1", &[])
            .await
            .unwrap()
            .total,
        1
    );
    lib_repo.delete("lib-1").await.unwrap();
    assert_eq!(
        search_repo
            .search(&SearchQuery::new("Miller"), "u1", &[])
            .await
            .unwrap()
            .total,
        0
    );

    let conn = pool.get().await.unwrap();
    let remaining_fts_rows: i64 = conn
        .interact(|c| c.query_row("SELECT COUNT(*) FROM media_items_fts", [], |r| r.get(0)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(remaining_fts_rows, 0);
}

#[tokio::test]
async fn fts_query_sanitizer_never_throws_syntax_errors_on_hostile_or_unicode_inputs() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());
    let search_repo = SearchRepository::new(pool.clone());

    lib_repo
        .create("lib-1", "Movies", "/movies", MediaType::Movie, false, None)
        .await
        .unwrap();

    item_repo
        .upsert_batch(&[
            MediaItem {
                library_id: "lib-1".into(),
                item_type: MediaType::Movie,
                title: "Spider-Man: Into the Spider-Verse".into(),
                original_title: Some("Spider-Man".into()),
                release_year: Some(2018),
                file_path: "/movies/spiderman.mkv".into(),
                file_name: "Spider-Man.Into.the.Spider-Verse.2018.mkv".into(),
                ..Default::default()
            },
            MediaItem {
                library_id: "lib-1".into(),
                item_type: MediaType::Movie,
                title: "باب الحديد".into(),
                original_title: Some("Cairo Station".into()),
                release_year: Some(1958),
                file_path: "/movies/bab_el_hadid.mkv".into(),
                file_name: "bab_el_hadid.mkv".into(),
                metadata: MediaMetadata {
                    overview: Some("تحفة يوسف شاهين الكلاسيكية".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "lib-1".into(),
                item_type: MediaType::Movie,
                title: "Le Fabuleux Destin d'Amélie Poulain".into(),
                release_year: Some(2001),
                file_path: "/movies/amelie.mkv".into(),
                file_name: "C++_for_Cinema_foo_bar.mkv".into(),
                ..Default::default()
            },
            MediaItem {
                library_id: "lib-1".into(),
                item_type: MediaType::Show,
                title: "Andor".into(),
                release_year: Some(2022),
                file_path: "/movies/andor".into(),
                file_name: "andor".into(),
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    let hostile_inputs = [
        "",
        "    \t\n  ",
        "\"",
        "\"\"\"",
        "*",
        "***",
        "^",
        ":",
        ":::",
        "()",
        "((()))",
        "-",
        "---",
        "+",
        "+++",
        "\"*^:()-+",
        "Spider-Man: Into",
        "C++",
        "foo\"bar",
        "باب الحديد",
        "بَابُ الْحَدِيدِ",
        "Amelie",
        "Amélie",
        "Ame\u{0301}lie",
        "2018",
        "1958",
        "AND",
        "OR",
        "NOT",
        "NEAR",
        "NEAR/5",
        "NEAR(spider, verse)",
        "title:spider",
        "nonexistent_column:spider",
        "spider OR (NOT verse) ^ 5 *",
        "' OR 1=1; DROP TABLE media_items; --",
        "{title original_title} : \"spider\"*",
        "Ocean’s 11 — Director’s Cut!",
        "🎬🍿🔥",
        &"word ".repeat(200),
    ];

    for input in hostile_inputs {
        let res = search_repo
            .search(&SearchQuery::new(input), "u1", &[])
            .await
            .unwrap_or_else(|e| panic!("FTS5 failed on input {input:?}: {e}"));
        assert!(res.items.len() <= 30);
    }

    // Specific positive assertions
    let spider = search_repo
        .search(&SearchQuery::new("Spider-Man: Into"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(spider.total, 1);
    assert_eq!(spider.items[0].title, "Spider-Man: Into the Spider-Verse");

    let arabic_with_diacritics = search_repo
        .search(&SearchQuery::new("بَابُ الْحَدِيدِ"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(arabic_with_diacritics.total, 1);
    assert_eq!(arabic_with_diacritics.items[0].title, "باب الحديد");

    let amelie = search_repo
        .search(&SearchQuery::new("Amelie"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(amelie.total, 1);
    assert!(amelie.items[0].title.contains("Amélie"));

    // Typing "AND" while searching for "Andor" matches Andor without FTS syntax error!
    let andor = search_repo
        .search(&SearchQuery::new("AND"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(andor.total, 1);
    assert_eq!(andor.items[0].title, "Andor");

    assert!(sanitize_fts_query("\"*^:()-+").is_none());
}

#[tokio::test]
async fn bm25_ranking_media_types_playback_progress_and_artwork_hydration() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let search_repo = SearchRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "alice".into(),
            username: "alice".into(),
            pin_hash: "hash".into(),
            role: UserRole::Standard,
            created_at: 100,
        })
        .await
        .unwrap();

    lib_repo
        .create(
            "lib-movies",
            "Cinema",
            "/movies",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();
    lib_repo
        .create("lib-tv", "Series", "/tv", MediaType::Show, false, None)
        .await
        .unwrap();
    lib_repo
        .create(
            "lib-home",
            "Family Archive",
            "/homevids",
            MediaType::HomeVideos,
            false,
            None,
        )
        .await
        .unwrap();
    lib_repo
        .create(
            "lib-live",
            "IPTV Live",
            "plugin://iptv",
            MediaType::LiveChannel,
            false,
            None,
        )
        .await
        .unwrap();

    let mut plugin_extra = HashMap::new();
    plugin_extra.insert(
        "poster_url".into(),
        json!("https://cdn.example.com/ch1.png"),
    );

    item_repo
        .upsert_batch(&[
            // Overview-only mention of "Matrix" (should rank BELOW title match)
            MediaItem {
                library_id: "lib-movies".into(),
                item_type: MediaType::Movie,
                title: "Documentary About Cyber Cinema".into(),
                release_year: Some(2020),
                added_at: 100,
                file_path: "/movies/doc.mkv".into(),
                file_name: "doc.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 3600,
                    resolution: Some("3840x2160".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    overview: Some(
                        "Explores many sci-fi classics including a brief mention of Matrix effects."
                            .into(),
                    ),
                    poster_path: Some("/local/posters/doc.jpg".into()),
                    backdrop_path: Some("/local/backdrops/doc.jpg".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            // Exact title match on "Matrix" (should rank FIRST via BM25 title weight 10.0)
            MediaItem {
                library_id: "lib-movies".into(),
                item_type: MediaType::Movie,
                title: "The Matrix".into(),
                release_year: Some(1999),
                added_at: 90,
                file_path: "/movies/matrix.mkv".into(),
                file_name: "matrix.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 8160,
                    resolution: Some("1080p".into()),
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    director: Some("Lana Wachowski".into()),
                    writers: vec!["Lilly Wachowski".into()],
                    actors: vec!["Keanu Reeves".into(), "Carrie-Anne Moss".into()],
                    genres: vec!["Action".into(), "Sci-Fi".into()],
                    studio: Some("Warner Bros".into()),
                    tags: vec!["Cyberpunk".into()],
                    rating: Some(8.7),
                    poster_path: Some("https://image.tmdb.org/t/p/w500/matrix.jpg".into()),
                    backdrop_path: Some("https://image.tmdb.org/t/p/original/matrix_bg.jpg".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            // Episode with S01E02 metadata
            MediaItem {
                library_id: "lib-tv".into(),
                item_type: MediaType::Episode,
                title: "Severance Pilot Part 2".into(),
                release_year: Some(2022),
                added_at: 200,
                file_path: "/tv/severance/ep2.mkv".into(),
                file_name: "ep2.mkv".into(),
                technical: TechnicalInfo {
                    duration_seconds: 3000,
                    ..Default::default()
                },
                metadata: MediaMetadata {
                    series_title: Some("Severance".into()),
                    season: Some(1),
                    episode: Some(2),
                    director: Some("Ben Stiller".into()),
                    genres: vec!["Thriller".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
            // Home Video
            MediaItem {
                library_id: "lib-home".into(),
                item_type: MediaType::HomeVideos,
                title: "Summer Trip Alexandria 2024".into(),
                release_year: Some(2024),
                added_at: 300,
                file_path: "/homevids/alexandria_2024.mp4".into(),
                file_name: "alexandria_2024.mp4".into(),
                technical: TechnicalInfo {
                    duration_seconds: 900,
                    ..Default::default()
                },
                ..Default::default()
            },
            // Live Channel from plugin
            MediaItem {
                library_id: "lib-live".into(),
                item_type: MediaType::LiveChannel,
                title: "Al Jazeera Documentary HD".into(),
                added_at: 400,
                file_path: "plugin://iptv/ch1".into(),
                file_name: "ch1".into(),
                metadata: MediaMetadata {
                    extra: plugin_extra,
                    ..Default::default()
                },
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    let matrix = item_repo
        .find_by_path("/movies/matrix.mkv")
        .await
        .unwrap()
        .unwrap();
    let matrix_id = matrix.id.unwrap();

    playback_repo
        .upsert_progress(
            "alice",
            matrix_id,
            4080,
            WatchState::InProgress,
            1_700_000_500,
        )
        .await
        .unwrap();

    // BM25 ranking check: "The Matrix" beats overview-only match
    let ranked = search_repo
        .search(&SearchQuery::new("Matrix"), "alice", &[])
        .await
        .unwrap();
    assert_eq!(ranked.total, 2);
    assert_eq!(ranked.items[0].title, "The Matrix");
    assert_eq!(ranked.items[1].title, "Documentary About Cyber Cinema");

    // Verify remote URL in poster_path vs local /api/v1/artwork/{id}/poster
    assert_eq!(
        ranked.items[0].poster_url,
        "https://image.tmdb.org/t/p/w500/matrix.jpg"
    );
    assert_eq!(
        ranked.items[0].backdrop_url.as_deref(),
        Some("https://image.tmdb.org/t/p/original/matrix_bg.jpg")
    );
    assert!((ranked.items[0].playback_progress.unwrap() - 0.5).abs() < 0.01);
    assert_eq!(ranked.items[0].badge.as_deref(), Some("RESUME"));

    let doc_id = ranked.items[1].id;
    assert_eq!(
        ranked.items[1].poster_url,
        format!("/api/v1/artwork/{doc_id}/poster")
    );
    assert_eq!(
        ranked.items[1].backdrop_url.as_deref(),
        Some(format!("/api/v1/artwork/{doc_id}/backdrop").as_str())
    );
    assert_eq!(ranked.items[1].badge.as_deref(), Some("4K"));

    // Episode S01E02 + series_title search
    let ep_res = search_repo
        .search(&SearchQuery::new("Severance S01E02"), "alice", &[])
        .await
        .unwrap();
    assert_eq!(ep_res.total, 1);
    assert_eq!(ep_res.items[0].season, Some(1));
    assert_eq!(ep_res.items[0].episode, Some(2));
    assert_eq!(ep_res.items[0].series_title.as_deref(), Some("Severance"));

    // Filter by item_type: HomeVideos
    let hv_res = search_repo
        .search(
            &SearchQuery {
                q: "2024".into(),
                item_type: Some(MediaType::HomeVideos),
                ..Default::default()
            },
            "alice",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(hv_res.total, 1);
    assert_eq!(hv_res.items[0].title, "Summer Trip Alexandria 2024");

    // Live Channel + plugin extra.poster_url hydration
    let live_res = search_repo
        .search(&SearchQuery::new("Jazeera"), "alice", &[])
        .await
        .unwrap();
    assert_eq!(live_res.total, 1);
    assert_eq!(live_res.items[0].item_type, MediaType::LiveChannel);
    assert_eq!(
        live_res.items[0].poster_url,
        "https://cdn.example.com/ch1.png"
    );
}

#[tokio::test]
async fn private_library_zero_leak_when_locked_and_visible_when_unlocked() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepo::new(pool.clone());
    let item_repo = MediaItemRepo::new(pool.clone());
    let search_repo = SearchRepository::new(pool.clone());

    lib_repo
        .create(
            "pub-lib",
            "Public Cinema",
            "/pub",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .unwrap();
    lib_repo
        .create(
            "priv-vault",
            "Secret Vault",
            "/vault",
            MediaType::Movie,
            true,
            Some("argon2-pin-hash"),
        )
        .await
        .unwrap();

    item_repo
        .upsert_batch(&[
            MediaItem {
                library_id: "pub-lib".into(),
                item_type: MediaType::Movie,
                title: "Shared Keyword Public Film".into(),
                release_year: Some(2023),
                file_path: "/pub/public_film.mkv".into(),
                file_name: "public_film.mkv".into(),
                metadata: MediaMetadata {
                    director: Some("Shared Director".into()),
                    genres: vec!["Mystery".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
            MediaItem {
                library_id: "priv-vault".into(),
                item_type: MediaType::Movie,
                title: "Shared Keyword Classified Vault Film".into(),
                release_year: Some(2023),
                file_path: "/vault/classified.mkv".into(),
                file_name: "classified.mkv".into(),
                metadata: MediaMetadata {
                    director: Some("Shared Director".into()),
                    actors: vec!["UltraSecretActorName".into()],
                    overview: Some("UltraSecretPlotKeyword".into()),
                    genres: vec!["Mystery".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
        ])
        .await
        .unwrap();

    // 1. Locked: private-only terms return ZERO items and total == 0
    for secret_term in [
        "Classified",
        "UltraSecretActorName",
        "UltraSecretPlotKeyword",
    ] {
        let locked = search_repo
            .search(&SearchQuery::new(secret_term), "u1", &[])
            .await
            .unwrap();
        assert_eq!(locked.total, 0, "leaked total count for {secret_term}");
        assert!(locked.items.is_empty(), "leaked item for {secret_term}");
    }

    // 2. Locked + shared keyword: ONLY public item appears, total == 1 (not 2!)
    let shared_locked = search_repo
        .search(&SearchQuery::new("Shared Keyword"), "u1", &[])
        .await
        .unwrap();
    assert_eq!(shared_locked.total, 1);
    assert_eq!(shared_locked.items.len(), 1);
    assert_eq!(shared_locked.items[0].library_id, "pub-lib");

    // 3. Locked + explicit library_id targeting private library: STILL zero leak!
    let targeted_locked = search_repo
        .search(
            &SearchQuery {
                q: "Shared".into(),
                library_id: Some("priv-vault".into()),
                ..Default::default()
            },
            "u1",
            &["unrelated-unlocked-lib".to_string()],
        )
        .await
        .unwrap();
    assert_eq!(targeted_locked.total, 0);
    assert!(targeted_locked.items.is_empty());

    // 4. Unlocked: private library items & counts become visible
    let unlocked_ids = vec!["priv-vault".to_string()];
    let shared_unlocked = search_repo
        .search(&SearchQuery::new("Shared Keyword"), "u1", &unlocked_ids)
        .await
        .unwrap();
    assert_eq!(shared_unlocked.total, 2);
    assert_eq!(shared_unlocked.items.len(), 2);

    let secret_unlocked = search_repo
        .search(
            &SearchQuery::new("UltraSecretActorName"),
            "u1",
            &unlocked_ids,
        )
        .await
        .unwrap();
    assert_eq!(secret_unlocked.total, 1);
    assert_eq!(
        secret_unlocked.items[0].title,
        "Shared Keyword Classified Vault Film"
    );
}
