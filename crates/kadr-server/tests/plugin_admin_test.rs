mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::MediaType;
use kadr_server::config::PluginSettings;
use plugin_support::*;
use serde_json::json;

#[tokio::test]
async fn plugins_disabled_by_default() {
    let h = build(PluginSettings::default()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "PLUGINS_DISABLED");
}

#[tokio::test]
async fn admin_routes_require_admin() {
    let h = build(enabled_settings()).await;
    let (s, _) = call(&h.app, "GET", "/api/v1/admin/plugins", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.user_token), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn register_then_approve_flow() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": format!("{}/", mock.base_url) })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["plugin"]["id"], "hello");
    assert_eq!(body["plugin"]["status"], "pending");
    assert_eq!(body["plugin"]["base_url"], mock.base_url, "trailing slash is trimmed");
    assert!(!body["secret"].as_str().unwrap().is_empty());
    assert!(body["plugin"].get("secret").is_none());

    // Duplicate registration conflicts.
    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token), None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["plugin"]["status"], "enabled");
    assert_eq!(body["plugin"]["granted_permissions"], json!(["items:publish", "playback:resolve"]));
    assert!(!body["token"].as_str().unwrap().is_empty());

    let (status, body) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&h.admin_token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn approve_can_grant_a_subset_but_not_unrequested_permissions() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    call(&h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
         Some(json!({ "base_url": mock.base_url }))).await;

    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token),
        Some(json!({ "permissions": ["playback:resolve"] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins/hello/approve", Some(&h.admin_token),
        Some(json!({ "permissions": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["plugin"]["granted_permissions"], json!([]));
}

#[tokio::test]
async fn register_rejects_unreachable_plugin_and_bad_manifests() {
    let h = build(enabled_settings()).await;

    let (status, body) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": "http://127.0.0.1:1" })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "PLUGIN_UNAVAILABLE");

    let mock = spawn_mock("hello", &[]).await;
    let mut m = manifest_json("hello", &[]);
    m["api_version"] = json!(2);
    mock.set_manifest(m);
    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(
        &h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
        Some(json!({ "base_url": "ftp://example.com" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn enable_disable_and_remove_with_and_without_purge() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    register_and_approve(&h, &mock).await;
    h.lib_repo
        .create_virtual("hello", "lib-1", "Hello", MediaType::LiveChannel)
        .await
        .unwrap();

    let (s, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/disable", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "disabled");
    let (s, body) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/enable", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "enabled");

    // Remove without purge keeps the library.
    let (s, _) = call(&h.app, "DELETE", "/api/v1/admin/plugins/hello", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(h.plugin_repo.get("hello").await.unwrap().is_none());
    assert!(h.lib_repo.get_by_id("lib-1").await.unwrap().is_some());

    // Re-register and remove with purge deletes it.
    register_and_approve(&h, &mock).await;
    let (s, _) = call(&h.app, "DELETE", "/api/v1/admin/plugins/hello?purge=true", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(h.lib_repo.get_by_id("lib-1").await.unwrap().is_none());

    let (s, _) = call(&h.app, "DELETE", "/api/v1/admin/plugins/hello", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pending_plugins_cannot_be_enabled_before_approval() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish"]).await;
    call(&h.app, "POST", "/api/v1/admin/plugins", Some(&h.admin_token),
         Some(json!({ "base_url": mock.base_url }))).await;
    let (s, _) = call(&h.app, "POST", "/api/v1/admin/plugins/hello/enable", Some(&h.admin_token), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
