use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::{
    ForgeKind, PluginCapabilities, PluginInstallKind, PluginManifest, PluginPermission,
    PluginRecord, PluginStatus,
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
        install_kind: PluginInstallKind::External,
        repo_url: None,
        forge_kind: None,
        installed_tag: None,
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

    let ids: Vec<String> = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec!["hello".to_string(), "iptv".to_string()]);
}

#[tokio::test]
async fn plugin_status_and_permissions_update_and_delete() {
    let (repo, _, _) = setup().await;
    repo.insert(&record("iptv")).await.unwrap();

    assert!(repo
        .set_status("iptv", PluginStatus::Enabled)
        .await
        .unwrap());
    assert!(repo
        .set_granted("iptv", &[PluginPermission::ItemsPublish])
        .await
        .unwrap());
    let got = repo.get("iptv").await.unwrap().unwrap();
    assert_eq!(got.status, PluginStatus::Enabled);
    assert_eq!(
        got.granted_permissions,
        vec![PluginPermission::ItemsPublish]
    );

    assert!(!repo
        .set_status("nope", PluginStatus::Enabled)
        .await
        .unwrap());
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
    assert_eq!(
        libs.plugin_id_of("lib-1").await.unwrap().as_deref(),
        Some("iptv")
    );

    // Regular libraries have no plugin owner.
    libs.create("real", "Real", "/tmp/real", MediaType::Movie, false, None)
        .await
        .unwrap();
    assert_eq!(libs.plugin_id_of("real").await.unwrap(), None);
    assert_eq!(libs.plugin_id_of("absent").await.unwrap(), None);

    // The virtual library is returned by get_all and get_by_id.
    assert!(libs.get_by_id("lib-1").await.unwrap().is_some());
    assert!(libs
        .get_all()
        .await
        .unwrap()
        .iter()
        .any(|l| l.id == "lib-1"));

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

#[tokio::test]
async fn managed_plugin_metadata_base_url_and_release_updates_roundtrip() {
    let (repo, _, _) = setup().await;
    let mut rec = record("hello");
    rec.install_kind = PluginInstallKind::Managed;
    rec.repo_url = Some("https://github.com/acme/plugin-hello".to_string());
    rec.forge_kind = Some(ForgeKind::Github);
    rec.installed_tag = Some("v0.1.0".to_string());
    repo.insert(&rec).await.unwrap();

    let got = repo.get("hello").await.unwrap().unwrap();
    assert_eq!(got, rec);

    assert!(repo
        .set_base_url("hello", "http://127.0.0.1:45123")
        .await
        .unwrap());
    let mut new_manifest = got.manifest.clone();
    new_manifest.name = "Hello v2".to_string();
    new_manifest.version = "0.2.0".to_string();
    assert!(repo
        .update_release("hello", "Hello v2", &new_manifest, "v0.2.0")
        .await
        .unwrap());

    let updated = repo.get("hello").await.unwrap().unwrap();
    assert_eq!(updated.base_url, "http://127.0.0.1:45123");
    assert_eq!(updated.name, "Hello v2");
    assert_eq!(updated.manifest.version, "0.2.0");
    assert_eq!(updated.installed_tag.as_deref(), Some("v0.2.0"));
}

#[tokio::test]
async fn migration_010_upgrades_existing_external_plugins_cleanly() {
    let pool = create_in_memory_pool().unwrap();
    let conn = pool.get().await.unwrap();
    conn.interact(|c| {
        kadr_storage::migrations::migrations()
            .to_version(c, 9)
            .unwrap();
        c.execute(
            "INSERT INTO plugins (id, name, base_url, manifest, granted_permissions, status, created_at)
             VALUES ('legacy', 'Legacy', 'http://127.0.0.1:9000',
                     '{\"id\":\"legacy\",\"name\":\"Legacy\",\"version\":\"0.1.0\",\"api_version\":1}',
                     '[]', 'enabled', 1700000000)",
            [],
        )
        .unwrap();
        kadr_storage::migrations::run_migrations(c).unwrap();
    })
    .await
    .unwrap();
    drop(conn);

    let repo = PluginRepository::new(pool);
    let legacy = repo.get("legacy").await.unwrap().unwrap();
    assert_eq!(legacy.install_kind, PluginInstallKind::External);
    assert_eq!(legacy.repo_url, None);
    assert_eq!(legacy.forge_kind, None);
    assert_eq!(legacy.installed_tag, None);
    assert_eq!(legacy.status, PluginStatus::Enabled);
}
