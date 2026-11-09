use kadr_storage::repos::SettingsRepository;
use kadr_storage::{create_in_memory_pool, initialize_database};

async fn repo() -> SettingsRepository {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    SettingsRepository::new(pool)
}

#[tokio::test]
async fn get_returns_none_for_missing_keys() {
    let repo = repo().await;
    assert_eq!(repo.get("plugins.enabled").await.unwrap(), None);
    assert!(repo.get_all().await.unwrap().is_empty());
}

#[tokio::test]
async fn set_inserts_then_overwrites() {
    let repo = repo().await;
    repo.set("plugins.enabled", "true").await.unwrap();
    assert_eq!(repo.get("plugins.enabled").await.unwrap().as_deref(), Some("true"));
    repo.set("plugins.enabled", "false").await.unwrap();
    assert_eq!(repo.get("plugins.enabled").await.unwrap().as_deref(), Some("false"));
}

#[tokio::test]
async fn set_many_writes_all_entries_and_get_all_returns_them() {
    let repo = repo().await;
    repo.set_many(&[
        ("a".to_string(), "1".to_string()),
        ("b".to_string(), "2".to_string()),
    ])
    .await
    .unwrap();
    let all = repo.get_all().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all["a"], "1");
    assert_eq!(all["b"], "2");

    // Empty input is a no-op.
    repo.set_many(&[]).await.unwrap();
    assert_eq!(repo.get_all().await.unwrap().len(), 2);
}

#[tokio::test]
async fn set_many_upserts_existing_keys() {
    let repo = repo().await;
    repo.set("a", "old").await.unwrap();
    repo.set_many(&[("a".to_string(), "new".to_string())]).await.unwrap();
    assert_eq!(repo.get("a").await.unwrap().as_deref(), Some("new"));
}
