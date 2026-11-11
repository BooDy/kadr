mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::plugin_item_path;
use kadr_server::config::PluginSettings;
use kadr_server::plugins::settings::load_plugin_settings;
use kadr_storage::repos::SettingsRepository;
use plugin_support::*;
use serde_json::json;
use std::sync::atomic::Ordering;

const SETTINGS: &str = "/api/v1/admin/plugins/settings";

#[tokio::test]
async fn settings_get_works_while_plugins_are_disabled() {
    let h = build(PluginSettings::default()).await;
    let (status, body) = call(&h.app, "GET", SETTINGS, Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "enabled": false, "resolve_timeout_secs": 5, "allow_private_proxy": false })
    );

    // The other plugin admin routes are still blocked while disabled.
    let (status, body) = call(
        &h.app,
        "GET",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "PLUGINS_DISABLED");
}

#[tokio::test]
async fn settings_routes_require_admin() {
    let h = build(PluginSettings::default()).await;
    for method in ["GET", "PUT"] {
        let body = if method == "PUT" {
            Some(json!({ "enabled": true }))
        } else {
            None
        };
        let (s, _) = call(&h.app, method, SETTINGS, None, body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = call(&h.app, method, SETTINGS, Some(&h.user_token), body).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn enabling_from_the_api_takes_effect_immediately() {
    let h = build(PluginSettings::default()).await;
    let (status, body) = call(
        &h.app,
        "PUT",
        SETTINGS,
        Some(&h.admin_token),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert_eq!(
        body["resolve_timeout_secs"], 5,
        "untouched keys keep their value"
    );

    let (status, body) = call(
        &h.app,
        "GET",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!([]));

    let (_, body) = call(
        &h.app,
        "PUT",
        SETTINGS,
        Some(&h.admin_token),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(body["enabled"], false);
    let (status, _) = call(
        &h.app,
        "GET",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn partial_updates_only_change_supplied_keys() {
    let h = build(PluginSettings::default()).await;
    let (_, body) = call(
        &h.app,
        "PUT",
        SETTINGS,
        Some(&h.admin_token),
        Some(json!({ "allow_private_proxy": true })),
    )
    .await;
    assert_eq!(
        body,
        json!({ "enabled": false, "resolve_timeout_secs": 5, "allow_private_proxy": true })
    );
    let (_, body) = call(
        &h.app,
        "PUT",
        SETTINGS,
        Some(&h.admin_token),
        Some(json!({ "resolve_timeout_secs": 12 })),
    )
    .await;
    assert_eq!(
        body,
        json!({ "enabled": false, "resolve_timeout_secs": 12, "allow_private_proxy": true })
    );
}

#[tokio::test]
async fn invalid_timeouts_are_rejected_and_change_nothing() {
    let h = build(PluginSettings::default()).await;
    for bad in [0u64, 61, 1000] {
        let (status, body) = call(
            &h.app,
            "PUT",
            SETTINGS,
            Some(&h.admin_token),
            Some(json!({ "enabled": true, "resolve_timeout_secs": bad })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "timeout {bad}");
        assert_eq!(body["error"], "INVALID_REQUEST");
    }
    let (_, body) = call(&h.app, "GET", SETTINGS, Some(&h.admin_token), None).await;
    assert_eq!(
        body["enabled"], false,
        "a rejected update must not apply any of its keys"
    );
    assert_eq!(body["resolve_timeout_secs"], 5);

    for ok in [1u64, 60] {
        let (status, _) = call(
            &h.app,
            "PUT",
            SETTINGS,
            Some(&h.admin_token),
            Some(json!({ "resolve_timeout_secs": ok })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "timeout {ok}");
    }
}

#[tokio::test]
async fn settings_are_persisted_and_override_the_config_default_on_load() {
    let h = build(PluginSettings::default()).await;
    let (_, put) = call(
        &h.app,
        "PUT",
        SETTINGS,
        Some(&h.admin_token),
        Some(json!({ "enabled": true, "resolve_timeout_secs": 9, "allow_private_proxy": true })),
    )
    .await;
    assert_eq!(put["resolve_timeout_secs"], 9);

    let repo = SettingsRepository::new(h.media_repo.pool().clone());
    let loaded = load_plugin_settings(&repo, PluginSettings::default()).await;
    assert_eq!(
        loaded,
        PluginSettings {
            enabled: true,
            resolve_timeout_secs: 9,
            allow_private_proxy: true
        }
    );
}

#[tokio::test]
async fn load_falls_back_to_the_base_for_missing_and_unparseable_values() {
    let h = build(PluginSettings::default()).await;
    let repo = SettingsRepository::new(h.media_repo.pool().clone());
    let base = PluginSettings {
        enabled: true,
        resolve_timeout_secs: 7,
        allow_private_proxy: false,
    };

    // Nothing stored: the base is returned untouched.
    assert_eq!(load_plugin_settings(&repo, base.clone()).await, base);

    // Garbage and out-of-range values are ignored; valid ones still apply.
    repo.set("plugins.enabled", "yes").await.unwrap();
    repo.set("plugins.resolve_timeout_secs", "abc")
        .await
        .unwrap();
    repo.set("plugins.allow_private_proxy", "true")
        .await
        .unwrap();
    let loaded = load_plugin_settings(&repo, base.clone()).await;
    assert_eq!(
        loaded,
        PluginSettings {
            allow_private_proxy: true,
            ..base.clone()
        }
    );

    repo.set("plugins.resolve_timeout_secs", "999")
        .await
        .unwrap();
    assert_eq!(
        load_plugin_settings(&repo, base.clone())
            .await
            .resolve_timeout_secs,
        7
    );
}

#[tokio::test]
async fn the_settings_path_is_not_captured_as_a_plugin_id() {
    let h = build(enabled_settings()).await;
    let (status, _) = call(&h.app, "DELETE", SETTINGS, Some(&h.admin_token), None).await;
    assert_eq!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "must not reach the remove-plugin handler"
    );

    // And a plugin cannot register under the reserved id.
    let mock = spawn_mock("settings", &["items:publish"]).await;
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn a_changed_timeout_applies_to_the_next_resolve_without_rebuilding() {
    let h = build(PluginSettings {
        resolve_timeout_secs: 3,
        ..enabled_settings()
    })
    .await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;
    register_and_approve(&h, &mock).await;
    h.lib_repo
        .create_virtual("hello", "lib-1", "Lib", MediaType::Movie)
        .await
        .unwrap();
    h.media_repo
        .upsert_batch(&[MediaItem {
            library_id: "lib-1".into(),
            item_type: MediaType::Movie,
            title: "m1".into(),
            file_path: plugin_item_path("hello", "m1"),
            file_name: "m1".into(),
            added_at: 1,
            ..Default::default()
        }])
        .await
        .unwrap();
    let item = h
        .media_repo
        .find_by_path(plugin_item_path("hello", "m1"))
        .await
        .unwrap()
        .unwrap();
    let uri = format!("/api/v1/stream/{}", item.id.unwrap());

    mock.resolve_delay_ms.store(1500, Ordering::SeqCst);
    let (status, _, _) = call_raw(&h.app, "GET", &uri, Some(&h.user_token), &[], None).await;
    assert_eq!(
        status,
        StatusCode::TEMPORARY_REDIRECT,
        "1.5s is inside the 3s timeout"
    );

    call(
        &h.app,
        "PUT",
        SETTINGS,
        Some(&h.admin_token),
        Some(json!({ "resolve_timeout_secs": 1 })),
    )
    .await;
    let (status, _, _) = call_raw(&h.app, "GET", &uri, Some(&h.user_token), &[], None).await;
    assert_eq!(
        status,
        StatusCode::GATEWAY_TIMEOUT,
        "1.5s now exceeds the new 1s timeout"
    );
}
