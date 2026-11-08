mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::{Library, MediaItem, MediaType};
use kadr_core::plugin::plugin_item_path;
use kadr_server::config::{PluginSettings, TranscodingSettings};
use kadr_server::HlsSessionManager;
use plugin_support::*;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;

async fn seed_item(
    h: &Harness,
    library_id: &str,
    plugin_id: &str,
    remote_id: &str,
    live: bool,
) -> MediaItem {
    if h.lib_repo.get_by_id(library_id).await.unwrap().is_none() {
        h.lib_repo
            .create_virtual(plugin_id, library_id, "Lib", MediaType::Movie)
            .await
            .unwrap();
    }
    let mut item = MediaItem {
        library_id: library_id.to_string(),
        item_type: MediaType::Movie,
        title: remote_id.to_string(),
        file_path: plugin_item_path(plugin_id, remote_id),
        file_name: remote_id.to_string(),
        added_at: 1,
        ..Default::default()
    };
    if live {
        item.metadata.extra.insert("live".into(), json!(true));
    }
    h.media_repo.upsert_batch(&[item]).await.unwrap();
    h.media_repo
        .find_by_path(plugin_item_path(plugin_id, remote_id))
        .await
        .unwrap()
        .unwrap()
}

async fn setup(settings: PluginSettings) -> (Harness, MockPlugin) {
    let h = build(settings).await;
    let mock = spawn_mock("hello", &["items:publish", "playback:resolve"]).await;
    register_and_approve(&h, &mock).await;
    (h, mock)
}

fn stream_uri(item: &MediaItem) -> String {
    format!("/api/v1/stream/{}", item.id.unwrap())
}

async fn get(
    h: &Harness,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    call_raw(&h.app, "GET", uri, Some(&h.user_token), headers, None).await
}

fn json_body(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn direct_mode_redirects_to_the_descriptor_url() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, headers, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(headers["location"], "https://cdn.example.com/video.mp4");
    assert_eq!(
        mock.last_resolve_body.lock().unwrap().as_ref().unwrap()["remote_id"],
        "m1"
    );
    assert_eq!(
        mock.last_resolve_body.lock().unwrap().as_ref().unwrap()["user_id"],
        "viewer"
    );
}

#[tokio::test]
async fn proxy_mode_streams_the_upstream_body_and_forwards_ranges() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({ "mode": "proxy", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, headers, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"0123456789");
    assert_eq!(headers["content-type"], "video/mp4");

    let (status, headers, body) = get(&h, &stream_uri(&item), &[("range", "bytes=2-5")]).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, b"2345");
    assert_eq!(headers["content-range"], "bytes 2-5/10");
}

#[tokio::test]
async fn proxy_mode_ignores_ranges_for_live_items() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({ "mode": "proxy", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "live1", true).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[("range", "bytes=2-5")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"0123456789");
}

#[tokio::test]
async fn transcode_mode_points_clients_at_the_hls_endpoint() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({ "mode": "transcode", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let v = json_body(&body);
    assert_eq!(v["error"], "PLAYBACK_REQUIRES_TRANSCODE");
    assert_eq!(
        v["hls_url"],
        format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap())
    );
}

#[tokio::test]
async fn plugin_failures_map_to_503_504_502() {
    let (h, mock) = setup(PluginSettings {
        resolve_timeout_secs: 1,
        ..enabled_settings()
    })
    .await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    mock.resolve_status.store(500, Ordering::SeqCst);
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(&body)["error"], "PLUGIN_UNAVAILABLE");

    mock.resolve_status.store(0, Ordering::SeqCst);
    mock.resolve_delay_ms.store(2500, Ordering::SeqCst);
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(json_body(&body)["error"], "PLUGIN_TIMEOUT");

    mock.resolve_delay_ms.store(0, Ordering::SeqCst);
    mock.set_descriptor(json!({ "nope": 1 }));
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json_body(&body)["error"], "PLUGIN_BAD_RESPONSE");
}

#[tokio::test]
async fn private_proxy_targets_are_rejected_unless_allowed() {
    let (h, mock) = setup(PluginSettings {
        allow_private_proxy: false,
        ..enabled_settings()
    })
    .await;
    mock.set_descriptor(json!({ "mode": "proxy", "url": mock.media_url() }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(json_body(&body)["error"], "PLUGIN_BAD_RESPONSE");
}

#[tokio::test]
async fn proxy_does_not_follow_upstream_redirects() {
    let (h, mock) = setup(enabled_settings()).await;
    // The mock's /health answers 200, so point at a path that does not exist: 404 -> 502.
    mock.set_descriptor(json!({ "mode": "proxy", "url": format!("{}/missing", mock.base_url) }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;
    let (status, _, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn descriptors_are_cached_only_when_the_plugin_sets_expires_in_secs() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    get(&h, &stream_uri(&item), &[]).await;
    get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(mock.calls(), 2, "no expires_in_secs -> never cached");

    mock.set_descriptor(
        json!({ "mode": "direct", "url": "https://cdn.example.com/a.mp4", "expires_in_secs": 60 }),
    );
    get(&h, &stream_uri(&item), &[]).await;
    get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(mock.calls(), 3, "second request served from cache");

    // A different user has their own cache entry.
    call_raw(
        &h.app,
        "GET",
        &stream_uri(&item),
        Some(&h.admin_token),
        &[],
        None,
    )
    .await;
    assert_eq!(mock.calls(), 4);
}

#[tokio::test]
async fn unavailable_plugins_are_never_called() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/hello/disable",
        Some(&h.admin_token),
        None,
    )
    .await;
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(&body)["error"], "PLUGIN_UNAVAILABLE");
    assert_eq!(mock.calls(), 0);

    // Plugin removed entirely (no purge): items remain but cannot be played.
    call(
        &h.app,
        "DELETE",
        "/api/v1/admin/plugins/hello",
        Some(&h.admin_token),
        None,
    )
    .await;
    let (status, _, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(mock.calls(), 0);
}

#[tokio::test]
async fn locked_libraries_never_trigger_a_resolve_call() {
    let (h, mock) = setup(enabled_settings()).await;
    h.lib_repo
        .insert(&Library {
            id: "priv".into(),
            name: "Private".into(),
            path: "plugin://hello/library/priv".into(),
            paths: vec![],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some(kadr_server::auth::pin::hash_pin("1234").unwrap()),
            created_at: 0,
        })
        .await
        .unwrap();
    let item = seed_item(&h, "priv", "hello", "secret", false).await;

    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json_body(&body)["error"], "LIBRARY_LOCKED");
    assert_eq!(mock.calls(), 0);
}

#[tokio::test]
async fn hls_locked_libraries_never_trigger_a_resolve_call() {
    let (h, mock) = setup(enabled_settings()).await;
    h.lib_repo
        .insert(&Library {
            id: "priv".into(),
            name: "Private".into(),
            path: "plugin://hello/library/priv".into(),
            paths: vec![],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some(kadr_server::auth::pin::hash_pin("1234").unwrap()),
            created_at: 0,
        })
        .await
        .unwrap();
    let item = seed_item(&h, "priv", "hello", "secret", false).await;

    let hls = format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap());
    let (status, _, body) = call_raw(&h.app, "GET", &hls, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json_body(&body)["error"], "LIBRARY_LOCKED");
    assert_eq!(mock.calls(), 0);
}

#[tokio::test]
async fn descriptor_cache_distinguishes_client_capabilities() {
    let (h, mock) = setup(enabled_settings()).await;
    mock.set_descriptor(json!({
        "mode": "transcode",
        "url": "http://8.8.8.8/live.ts",
        "expires_in_secs": 60,
    }));
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let dir = tempfile::tempdir().unwrap();
    let (script, _log) = fake_ffmpeg(dir.path());
    let mgr = Arc::new(HlsSessionManager::new(TranscodingSettings {
        ffmpeg_path: script.to_string_lossy().to_string(),
        temp_dir: dir.path().join("sessions"),
        ..Default::default()
    }));
    let app = h.app.clone().layer(axum::Extension(mgr));

    let hls_base = format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap());
    let hls_480 = format!("{hls_base}?max_height=480");
    let hls_720 = format!("{hls_base}?max_height=720");

    // 1. Direct-play endpoint uses default capabilities -> reaches plugin.
    let (status, _, body) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json_body(&body)["error"], "PLAYBACK_REQUIRES_TRANSCODE");
    assert_eq!(mock.calls(), 1);

    // 2. Repeat direct-play request -> served from cache.
    let (status, _, _) = get(&h, &stream_uri(&item), &[]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(mock.calls(), 1);

    // 3. Different capabilities (?max_height=480) -> reaches plugin.
    let (status, _, _) = call_raw(&app, "GET", &hls_480, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.calls(), 2);

    // 4. Identical request with ?max_height=480 -> served from cache.
    let (status, _, _) = call_raw(&app, "GET", &hls_480, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.calls(), 2);

    // 5. Another different capability (?max_height=720) -> reaches plugin.
    let (status, _, _) = call_raw(&app, "GET", &hls_720, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.calls(), 3);

    // 6. Repeat ?max_height=720 request -> served from cache.
    let (status, _, _) = call_raw(&app, "GET", &hls_720, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mock.calls(), 3);
}

#[tokio::test]
async fn hls_feeds_proxy_and_transcode_urls_to_ffmpeg_and_refuses_direct() {
    let (h, mock) = setup(enabled_settings()).await;
    let item = seed_item(&h, "lib-1", "hello", "m1", false).await;

    let dir = tempfile::tempdir().unwrap();
    let (script, log) = fake_ffmpeg(dir.path());
    let mgr = Arc::new(HlsSessionManager::new(TranscodingSettings {
        ffmpeg_path: script.to_string_lossy().to_string(),
        temp_dir: dir.path().join("sessions"),
        ..Default::default()
    }));
    let app = h.app.clone().layer(axum::Extension(mgr));
    let hls = format!("/api/v1/media/{}/hls/master.m3u8", item.id.unwrap());

    // direct -> refuse (direct URLs are not SSRF-validated, so they are never given to ffmpeg)
    let (status, _, body) = call_raw(&app, "GET", &hls, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json_body(&body)["error"], "PLAYBACK_DIRECT_ONLY");
    assert!(!log.exists());

    mock.set_descriptor(json!({ "mode": "transcode", "url": "http://8.8.8.8/live.ts", "headers": { "X-Token": "abc" } }));
    let (status, _, _) = call_raw(&app, "GET", &hls, Some(&h.user_token), &[], None).await;
    assert_eq!(status, StatusCode::OK);
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("http://8.8.8.8/live.ts"), "{logged}");
    assert!(logged.contains("-protocol_whitelist"));
    assert!(logged.contains("X-Token: abc"));
}

#[tokio::test]
async fn file_items_are_unaffected() {
    let (h, _mock) = setup(enabled_settings()).await;
    h.lib_repo
        .create("real", "Real", "/tmp/real", MediaType::Movie, false, None)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("movie.mp4");
    std::fs::write(&file, b"abcdefghij").unwrap();
    h.media_repo
        .upsert_batch(&[MediaItem {
            library_id: "real".into(),
            item_type: MediaType::Movie,
            title: "Movie".into(),
            file_path: file.clone(),
            file_name: "movie.mp4".into(),
            file_size: 10,
            added_at: 1,
            ..Default::default()
        }])
        .await
        .unwrap();
    let item = h.media_repo.find_by_path(&file).await.unwrap().unwrap();

    let (status, _, body) = get(&h, &stream_uri(&item), &[("range", "bytes=0-3")]).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, b"abcd");
}
