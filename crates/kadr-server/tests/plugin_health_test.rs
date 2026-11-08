mod plugin_support;

use kadr_core::events::SystemEvent;
use kadr_core::plugin::PluginStatus;
use kadr_server::config::AppConfig;
use kadr_server::plugins::PluginService;
use plugin_support::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn health_check_flips_status_and_publishes_an_event() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    register_and_approve(&h, &mock).await;

    let config = AppConfig {
        plugins: enabled_settings(),
        ..Default::default()
    };
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        Arc::new(tokio::sync::RwLock::new(config)),
        h.events.clone(),
    );
    let mut rx = h.events.subscribe();

    svc.check_all().await.unwrap();
    assert_eq!(
        h.plugin_repo.get("hello").await.unwrap().unwrap().status,
        PluginStatus::Enabled
    );

    mock.healthy.store(false, Ordering::SeqCst);
    svc.check_all().await.unwrap();
    assert_eq!(
        h.plugin_repo.get("hello").await.unwrap().unwrap().status,
        PluginStatus::Unhealthy
    );
    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        SystemEvent::PluginError { plugin_id, .. } => assert_eq!(plugin_id, "hello"),
        other => panic!("unexpected event {other:?}"),
    }

    mock.healthy.store(true, Ordering::SeqCst);
    svc.check_all().await.unwrap();
    assert_eq!(
        h.plugin_repo.get("hello").await.unwrap().unwrap().status,
        PluginStatus::Enabled
    );
}

#[tokio::test]
async fn disabled_and_pending_plugins_are_not_probed() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    register_and_approve(&h, &mock).await;
    call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/hello/disable",
        Some(&h.admin_token),
        None,
    )
    .await;
    mock.healthy.store(false, Ordering::SeqCst);

    let config = AppConfig {
        plugins: enabled_settings(),
        ..Default::default()
    };
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        Arc::new(tokio::sync::RwLock::new(config)),
        h.events.clone(),
    );
    svc.check_all().await.unwrap();
    assert_eq!(
        h.plugin_repo.get("hello").await.unwrap().unwrap().status,
        PluginStatus::Disabled
    );
}

#[tokio::test]
async fn check_all_continues_probing_other_plugins_on_failure() {
    let h = build(enabled_settings()).await;
    let mock1 = spawn_mock("plug1", &["items:publish"]).await;
    let mock2 = spawn_mock("plug2", &["items:publish"]).await;
    register_and_approve(&h, &mock1).await;
    register_and_approve(&h, &mock2).await;

    let config = AppConfig {
        plugins: enabled_settings(),
        ..Default::default()
    };
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        Arc::new(tokio::sync::RwLock::new(config)),
        h.events.clone(),
    );

    mock1.healthy.store(false, Ordering::SeqCst);
    mock2.healthy.store(true, Ordering::SeqCst);

    svc.check_all().await.unwrap();
    assert_eq!(
        h.plugin_repo.get("plug1").await.unwrap().unwrap().status,
        PluginStatus::Unhealthy
    );
    assert_eq!(
        h.plugin_repo.get("plug2").await.unwrap().unwrap().status,
        PluginStatus::Enabled
    );
}
