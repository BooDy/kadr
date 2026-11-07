use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::{
    PluginCapabilities, PluginManifest, PluginPermission, PluginRecord, PluginStatus,
};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PluginRepository};
use kadr_storage::{create_in_memory_pool, initialize_database};

fn record(id: &str) -> PluginRecord {
    PluginRecord {
        id: id.to_string(),
        name: format!("Plugin {id}"),
        base_url: "http://127.0.0.1:9000".to_string(),
        manifest: PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {id}"),
            version: "0.1.0".to_string(),
            api_version: 1,
            permissions: vec![PluginPermission::ItemsPublish],
            capabilities: PluginCapabilities::default(),
        },
        granted_permissions: vec![],
        status: PluginStatus::Pending,
        created_at: 1_700_000_000,
    }
}

async fn setup() -> (PluginRepository, LibraryRepository, MediaItemRepository) {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    (
        PluginRepository::new(pool.clone()),
        LibraryRepository::new(pool.clone()),
        MediaItemRepository::new(pool),
    )
}

#[tokio::test]
async fn plugin_insert_get_list_roundtrip() {
    let (repo, _, _) = setup().await;
    repo.insert(&record("iptv")).await.unwrap();
    repo.insert(&record("hello")).await.unwrap();

    let got = repo.get("iptv").await.unwrap().unwrap();
    assert_eq!(got, record("iptv"));
    assert!(repo.get("missing").await.unwrap().is_none());

    let ids: Vec<String> = repo.list().await.unwrap().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, vec!["hello".to_string(), "iptv".to_string()]);
}

#[tokio::test]
async fn plugin_status_and_permissions_update_and_delete() {
    let (repo, _, _) = setup().await;
    repo.insert(&record("iptv")).await.unwrap();

    assert!(repo.set_status("iptv", PluginStatus::Enabled).await.unwrap());
    assert!(repo
        .set_granted("iptv", &[PluginPermission::ItemsPublish])
        .await
        .unwrap());
    let got = repo.get("iptv").await.unwrap().unwrap();
    assert_eq!(got.status, PluginStatus::Enabled);
    assert_eq!(got.granted_permissions, vec![PluginPermission::ItemsPublish]);

    assert!(!repo.set_status("nope", PluginStatus::Enabled).await.unwrap());
    assert!(repo.delete("iptv").await.unwrap());
    assert!(!repo.delete("iptv").await.unwrap());
}

#[tokio::test]
async fn virtual_library_lifecycle_and_purge() {
    let (_, libs, media) = setup().await;

    let lib = libs
        .create_virtual("iptv", "lib-1", "Live TV", MediaType::LiveChannel)
        .await
        .unwrap();
    assert_eq!(lib.path.to_string_lossy(), "plugin://iptv/library/lib-1");
    assert_eq!(libs.plugin_id_of("lib-1").await.unwrap().as_deref(), Some("iptv"));

    // Regular libraries have no plugin owner.
    libs.create("real", "Real", "/tmp/real", MediaType::Movie, false, None)
        .await
        .unwrap();
    assert_eq!(libs.plugin_id_of("real").await.unwrap(), None);
    assert_eq!(libs.plugin_id_of("absent").await.unwrap(), None);

    // The virtual library is returned by get_all and get_by_id.
    assert!(libs.get_by_id("lib-1").await.unwrap().is_some());
    assert!(libs.get_all().await.unwrap().iter().any(|l| l.id == "lib-1"));

    let item = MediaItem {
        library_id: "lib-1".to_string(),
        item_type: MediaType::LiveChannel,
        title: "Channel 1".to_string(),
        file_path: "plugin://iptv/ch1".into(),
        file_name: "ch1".to_string(),
        added_at: 1,
        ..Default::default()
    };
    media.upsert_batch(&[item]).await.unwrap();
    assert_eq!(media.count_by_library("lib-1").await.unwrap(), 1);

    let removed = libs.delete_by_plugin("iptv").await.unwrap();
    assert_eq!(removed, 1);
    assert!(libs.get_by_id("lib-1").await.unwrap().is_none());
    assert_eq!(media.count_by_library("lib-1").await.unwrap(), 0);
    // Regular library untouched.
    assert!(libs.get_by_id("real").await.unwrap().is_some());
}
