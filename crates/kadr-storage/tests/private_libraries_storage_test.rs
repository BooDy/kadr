use kadr_core::models::MediaType;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::LibraryRepo;

#[tokio::test]
async fn test_private_library_storage_and_migration() {
    let pool = create_in_memory_pool().expect("pool create failed");
    initialize_database(&pool).await.expect("db init failed");
    let repo = LibraryRepo::new(pool.clone());

    // Create public library
    let public_lib = repo
        .create(
            "lib-pub",
            "Public Movies",
            "/media/pub",
            MediaType::Movie,
            false,
            None,
        )
        .await
        .expect("create public failed");
    assert!(!public_lib.is_private);
    assert_eq!(public_lib.pin_hash, None);

    // Create private library
    let private_lib = repo
        .create(
            "lib-priv",
            "Secret Vault",
            "/media/secret",
            MediaType::Movie,
            true,
            Some("hash123"),
        )
        .await
        .expect("create private failed");
    assert!(private_lib.is_private);
    assert_eq!(private_lib.pin_hash.as_deref(), Some("hash123"));

    // Fetch by id
    let fetched = repo
        .get_by_id("lib-priv")
        .await
        .expect("fetch failed")
        .expect("missing lib");
    assert!(fetched.is_private);
    assert_eq!(fetched.pin_hash.as_deref(), Some("hash123"));

    // Update privacy
    repo.update_privacy("lib-pub", true, Some("new_pin"))
        .await
        .expect("update privacy failed");
    let updated = repo
        .get_by_id("lib-pub")
        .await
        .expect("fetch failed")
        .expect("missing lib");
    assert!(updated.is_private);
    assert_eq!(updated.pin_hash.as_deref(), Some("new_pin"));
}
