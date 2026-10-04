use deadpool_sqlite::{Config, Runtime};
use kadr_core::models::MediaType;
use kadr_storage::migrations::run_migrations;
use kadr_storage::repos::LibraryRepository;
use std::path::PathBuf;

async fn setup_test_db() -> deadpool_sqlite::Pool {
    let cfg = Config::new("");
    let pool = cfg.create_pool(Runtime::Tokio1).unwrap();
    let conn = pool.get().await.unwrap();
    conn.interact(run_migrations).await.unwrap().unwrap();
    pool
}

#[tokio::test]
async fn test_create_and_manage_multiple_library_paths() {
    let pool = setup_test_db().await;
    let repo = LibraryRepository::new(pool);

    let paths = vec![
        PathBuf::from("/media/movies1"),
        PathBuf::from("/media/movies2"),
    ];

    let lib = repo
        .create_with_paths(
            "lib-multi",
            "Multi Movies",
            &paths,
            MediaType::Movie,
            false,
            None,
        )
        .await
        .expect("Failed to create library with multiple paths");

    assert_eq!(lib.id, "lib-multi");
    assert_eq!(lib.path, PathBuf::from("/media/movies1"));
    assert_eq!(lib.paths.len(), 2);
    assert_eq!(lib.paths[0], PathBuf::from("/media/movies1"));
    assert_eq!(lib.paths[1], PathBuf::from("/media/movies2"));

    // Fetch by id
    let fetched = repo.get_by_id("lib-multi").await.unwrap().expect("Library should exist");
    assert_eq!(fetched.paths.len(), 2);

    // Add a 3rd path
    repo.add_path("lib-multi", "/media/movies3")
        .await
        .expect("Failed to add 3rd path");

    let updated = repo.get_by_id("lib-multi").await.unwrap().unwrap();
    assert_eq!(updated.paths.len(), 3);
    assert!(updated.paths.contains(&PathBuf::from("/media/movies3")));

    // Remove 2nd path
    repo.remove_path("lib-multi", "/media/movies2")
        .await
        .expect("Failed to remove path");

    let after_remove = repo.get_by_id("lib-multi").await.unwrap().unwrap();
    assert_eq!(after_remove.paths.len(), 2);
    assert!(!after_remove.paths.contains(&PathBuf::from("/media/movies2")));

    // Removing non-existent path errors or succeeds idempotently
    repo.remove_path("lib-multi", "/media/movies3").await.unwrap();
    let final_lib = repo.get_by_id("lib-multi").await.unwrap().unwrap();
    assert_eq!(final_lib.paths.len(), 1);

    // Attempting to remove the last path must fail
    let err = repo.remove_path("lib-multi", "/media/movies1").await;
    assert!(err.is_err(), "Removing last path must fail");
}
