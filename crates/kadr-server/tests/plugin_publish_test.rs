mod plugin_support;

use axum::http::StatusCode;
use kadr_core::events::SystemEvent;
use kadr_core::models::MediaType;
use kadr_core::plugin::plugin_item_path;
use plugin_support::*;
use serde_json::json;
use std::time::Duration;

async fn setup() -> (Harness, MockPlugin, String) {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;
    let (token, _) = register_and_approve(&h, &mock).await;
    (h, mock, token)
}

async fn create_lib(h: &Harness, token: &str, id: &str) {
    let (status, body) = call(
        &h.app, "POST", "/api/v1/plugin/libraries", Some(token),
        Some(json!({ "id": id, "name": "Hello", "media_type": "live_channel" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn publish_update_and_delete_items() {
    let (h, _mock, token) = setup().await;
    create_lib(&h, &token, "lib-1").await;
    assert_eq!(h.lib_repo.plugin_id_of("lib-1").await.unwrap().as_deref(), Some("hello"));

    let (status, body) = call(
        &h.app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token),
        Some(json!({ "items": [
            { "remote_id": "ch1", "title": "Channel 1", "live": true, "genres": ["news"] },
            { "remote_id": "group/ch2", "title": "Channel 2", "duration_seconds": 90, "poster_url": "https://img/x.png" }
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["upserted"], 2);

    let items = h.media_repo.list_by_library("lib-1", 100, 0).await.unwrap();
    assert_eq!(items.len(), 2);
    let ch1 = h.media_repo.find_by_path(plugin_item_path("hello", "ch1")).await.unwrap().unwrap();
    assert!(ch1.is_live());
    assert_eq!(ch1.item_type, MediaType::LiveChannel, "defaults to the library media type");
    assert_eq!(ch1.metadata.genres, vec!["news".to_string()]);
    let ch2 = h.media_repo.find_by_path(plugin_item_path("hello", "group/ch2")).await.unwrap().unwrap();
    assert_eq!(ch2.technical.duration_seconds, 90);
    assert_eq!(ch2.metadata.extra["poster_url"], "https://img/x.png");

    // Re-upserting updates in place instead of duplicating.
    call(
        &h.app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token),
        Some(json!({ "items": [{ "remote_id": "ch1", "title": "Renamed", "live": true }] })),
    )
    .await;
    assert_eq!(h.media_repo.count_by_library("lib-1").await.unwrap(), 2);
    let ch1 = h.media_repo.find_by_path(plugin_item_path("hello", "ch1")).await.unwrap().unwrap();
    assert_eq!(ch1.title, "Renamed");

    let (status, _) = call(&h.app, "DELETE", "/api/v1/plugin/libraries/lib-1/items/ch1", Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(h.media_repo.count_by_library("lib-1").await.unwrap(), 1);
    let (status, _) = call(&h.app, "DELETE", "/api/v1/plugin/libraries/lib-1/items/ch1", Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // remote ids containing '/' are addressed with percent-encoding.
    let (status, _) = call(&h.app, "DELETE", "/api/v1/plugin/libraries/lib-1/items/group%2Fch2", Some(&token), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn publishing_emits_a_library_updated_event() {
    let (h, _mock, token) = setup().await;
    create_lib(&h, &token, "lib-1").await;
    let mut rx = h.events.subscribe();
    call(
        &h.app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token),
        Some(json!({ "items": [{ "remote_id": "ch1", "title": "C" }] })),
    )
    .await;
    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
    match ev {
        SystemEvent::LibraryUpdated { library_id, item_count, .. } => {
            assert_eq!(library_id, "lib-1");
            assert_eq!(item_count, 1);
        }
        other => panic!("unexpected event {other:?}"),
    }
}

#[tokio::test]
async fn tokens_are_not_interchangeable() {
    let (h, _mock, plugin_token) = setup().await;
    let body = Some(json!({ "name": "X" }));

    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", None, body.clone()).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&h.user_token), body.clone()).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&h.admin_token), body).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (s, _) = call(&h.app, "GET", "/api/v1/auth/me", Some(&plugin_token), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "plugin token must not authenticate as a user");
    let (s, _) = call(&h.app, "GET", "/api/v1/admin/plugins", Some(&plugin_token), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "plugin token must not reach admin routes");
}

#[tokio::test]
async fn missing_permission_and_disabled_plugin_are_forbidden() {
    let h = build(enabled_settings()).await;
    let mock = spawn_mock("resolver-only", &["playback:resolve"]).await;
    let (token, _) = register_and_approve(&h, &mock).await;
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "X" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (h, _mock, token) = setup().await;
    call(&h.app, "POST", "/api/v1/admin/plugins/hello/disable", Some(&h.admin_token), None).await;
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "X" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn plugins_cannot_touch_other_libraries() {
    let (h, _mock, token) = setup().await;
    h.lib_repo.create_virtual("other", "other-lib", "Other", MediaType::Movie).await.unwrap();
    h.lib_repo.create("real", "Real", "/tmp/real", MediaType::Movie, false, None).await.unwrap();
    let items = Some(json!({ "items": [{ "remote_id": "x", "title": "X" }] }));

    for lib in ["other-lib", "real", "does-not-exist"] {
        let (s, _) = call(&h.app, "PUT", &format!("/api/v1/plugin/libraries/{lib}/items"), Some(&token), items.clone()).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "library {lib}");
        let (s, _) = call(&h.app, "DELETE", &format!("/api/v1/plugin/libraries/{lib}/items/x"), Some(&token), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "library {lib}");
    }
    assert_eq!(h.media_repo.count_by_library("other-lib").await.unwrap(), 0);
    assert_eq!(h.media_repo.count_by_library("real").await.unwrap(), 0);
}

#[tokio::test]
async fn validation_errors() {
    let (h, _mock, token) = setup().await;
    create_lib(&h, &token, "lib-1").await;

    // duplicate library id, bad characters, empty name
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "id": "lib-1", "name": "Again" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "id": "bad id/..", "name": "X" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "  " }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // id omitted -> generated
    let (s, body) = call(&h.app, "POST", "/api/v1/plugin/libraries", Some(&token), Some(json!({ "name": "Auto" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(!body["id"].as_str().unwrap().is_empty());

    // empty remote id, empty title, oversized batch
    let put = |items: serde_json::Value| {
        let app = h.app.clone();
        let token = token.clone();
        async move {
            call(&app, "PUT", "/api/v1/plugin/libraries/lib-1/items", Some(&token), Some(json!({ "items": items }))).await
        }
    };
    assert_eq!(put(json!([{ "remote_id": "", "title": "X" }])).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(put(json!([{ "remote_id": "a", "title": " " }])).await.0, StatusCode::BAD_REQUEST);
    let many: Vec<_> = (0..1001).map(|i| json!({ "remote_id": format!("r{i}"), "title": "T" })).collect();
    assert_eq!(put(json!(many)).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(h.media_repo.count_by_library("lib-1").await.unwrap(), 0, "rejected batches write nothing");
}
