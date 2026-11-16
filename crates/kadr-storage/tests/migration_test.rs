// crates/kadr-storage/tests/migration_test.rs
use kadr_storage::migrations::run_migrations;
use kadr_storage::pool::create_in_memory_pool;

#[tokio::test]
async fn test_in_memory_pool_migration_and_pragmas() {
    let pool = create_in_memory_pool().expect("failed to create pool");
    let conn = pool.get().await.expect("failed to get connection");

    conn.interact(|c| {
        run_migrations(c).expect("failed to run migrations");

        // Verify WAL and foreign keys
        let fk_enabled: i32 = c
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk_enabled, 1);

        // Verify tables exist
        let tables: Vec<String> = c
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"libraries".to_string()));
        assert!(tables.contains(&"media_items".to_string()));
        assert!(tables.contains(&"users".to_string()));
    })
    .await
    .expect("interact failed");
}

#[tokio::test]
async fn test_initialize_database() {
    let pool = create_in_memory_pool().expect("failed to create pool");
    kadr_storage::pool::initialize_database(&pool)
        .await
        .expect("failed to initialize database");

    let conn = pool.get().await.expect("failed to get connection");
    conn.interact(|c| {
        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('libraries', 'media_items', 'users')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    })
    .await
    .expect("interact failed");
}

#[tokio::test]
async fn test_file_pool_pragmas_and_wal() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!(
        "kadr_test_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pool = kadr_storage::pool::create_pool(&db_path, 2).expect("failed to create pool");
    let conn = pool.get().await.expect("failed to get connection");

    conn.interact(|c| {
        let journal_mode: String = c
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let synchronous: i32 = c.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
        assert_eq!(synchronous, 1); // 1 = NORMAL

        let busy_timeout: i32 = c
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);

        let fk_enabled: i32 = c
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk_enabled, 1);
    })
    .await
    .expect("interact failed");

    // Verify a second simultaneously open pooled connection also sees WAL and per-connection pragmas
    let conn2 = pool.get().await.expect("failed to get second connection");
    conn2
        .interact(|c| {
            let journal_mode: String = c
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(journal_mode.to_lowercase(), "wal");

            let synchronous: i32 = c.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
            assert_eq!(synchronous, 1);

            let busy_timeout: i32 = c
                .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
                .unwrap();
            assert_eq!(busy_timeout, 5000);

            let fk_enabled: i32 = c
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk_enabled, 1);
        })
        .await
        .expect("second connection interact failed");

    drop(conn);
    drop(conn2);
    drop(pool);

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_file_pool_concurrent_connection_creation_under_wal() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("concurrent_wal.db");
    let pool = kadr_storage::pool::create_pool(&db_path, 8).expect("failed to create pool");
    kadr_storage::pool::initialize_database(&pool)
        .await
        .expect("failed to initialize database");

    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool_clone = pool.clone();
        handles.push(tokio::spawn(async move {
            let conn = pool_clone.get().await.expect("pool.get failed");
            conn.interact(|c| {
                let journal_mode: String = c
                    .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(journal_mode.to_lowercase(), "wal");
                let fk_enabled: i32 = c
                    .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(fk_enabled, 1);
                let count: i64 = c
                    .query_row("SELECT count(*) FROM media_items", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(count, 0);
            })
            .await
            .expect("interact failed");
            // Hold connection briefly so all 8 slots open concurrently
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            drop(conn);
        }));
    }

    for handle in handles {
        handle.await.expect("task panicked");
    }
    assert_eq!(pool.status().size, 8);
}
