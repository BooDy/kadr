mod plugin_support;

use axum::http::StatusCode;
use kadr_server::config::PluginSettings;
use plugin_support::*;
use serde_json::json;

async fn registered(h: &Harness, id: &str) -> (MockPlugin, String) {
    let mock = spawn_mock(id, &["items:publish", "playback:resolve"]).await;
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        Some(json!({ "base_url": mock.base_url })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    (mock, body["secret"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn connection_returns_the_same_secret_as_registration() {
    let h = build(enabled_settings()).await;
    let (mock, secret) = registered(&h, "hello").await;

    let (status, body) = call(
        &h.app,
        "GET",
        "/api/v1/admin/plugins/hello/connection",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["secret"], secret.as_str());
    assert_eq!(body["base_url"], mock.base_url);
}

#[tokio::test]
async fn connection_is_available_before_approval_too() {
    // The secret is derived, so the admin can configure the plugin before approving it.
    let h = build(enabled_settings()).await;
    registered(&h, "hello").await;
    let (status, _) = call(
        &h.app,
        "GET",
        "/api/v1/admin/plugins/hello/connection",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn issued_tokens_work_on_plugin_routes() {
    let h = build(enabled_settings()).await;
    registered(&h, "hello").await;
    call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/hello/approve",
        Some(&h.admin_token),
        None,
    )
    .await;

    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/hello/token",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token = body["token"].as_str().unwrap().to_string();
    assert!(!token.is_empty());

    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/plugin/libraries",
        Some(&token),
        Some(json!({ "id": "lib-1", "name": "Hello" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn tokens_cannot_be_issued_for_pending_or_unknown_plugins() {
    let h = build(enabled_settings()).await;
    registered(&h, "hello").await;

    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/hello/token",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "INVALID_REQUEST");

    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/nope/token",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &h.app,
        "GET",
        "/api/v1/admin/plugins/nope/connection",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn both_routes_are_blocked_while_plugins_are_disabled() {
    let h = build(PluginSettings::default()).await;
    for (method, uri) in [
        ("GET", "/api/v1/admin/plugins/hello/connection"),
        ("POST", "/api/v1/admin/plugins/hello/token"),
    ] {
        let (status, body) = call(&h.app, method, uri, Some(&h.admin_token), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "PLUGINS_DISABLED");
    }
}

#[tokio::test]
async fn both_routes_require_an_admin_and_reject_plugin_tokens() {
    let h = build(enabled_settings()).await;
    let (mock, _) = registered(&h, "hello").await;
    let (plugin_token, _) = {
        let (_, body) = call(
            &h.app,
            "POST",
            "/api/v1/admin/plugins/hello/approve",
            Some(&h.admin_token),
            None,
        )
        .await;
        (body["token"].as_str().unwrap().to_string(), mock)
    };

    for (method, uri) in [
        ("GET", "/api/v1/admin/plugins/hello/connection"),
        ("POST", "/api/v1/admin/plugins/hello/token"),
    ] {
        let (s, _) = call(&h.app, method, uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = call(&h.app, method, uri, Some(&h.user_token), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = call(&h.app, method, uri, Some(&plugin_token), None).await;
        assert_eq!(
            s,
            StatusCode::UNAUTHORIZED,
            "a plugin token must not reach admin routes"
        );
    }
}

#[tokio::test]
async fn the_secret_never_appears_in_the_plugin_list() {
    let h = build(enabled_settings()).await;
    let (_, secret) = registered(&h, "hello").await;
    let (status, _headers, bytes) = call_raw(
        &h.app,
        "GET",
        "/api/v1/admin/plugins",
        Some(&h.admin_token),
        &[],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let parsed: serde_json::Value =
        serde_json::from_slice(&bytes).expect("list response must be valid JSON");
    let list = parsed
        .as_array()
        .expect("list response must be a JSON array");
    assert!(
        list.iter()
            .any(|p| p.get("id").and_then(|id| id.as_str()) == Some("hello")),
        "list response must contain the registered plugin 'hello'"
    );
    let raw = std::str::from_utf8(&bytes).expect("list response must be valid UTF-8");
    assert!(
        !raw.contains(&secret),
        "list response must not leak the derived secret"
    );
}
