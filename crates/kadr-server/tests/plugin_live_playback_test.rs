mod plugin_support;

use axum::http::StatusCode;
use kadr_core::models::{MediaItem, MediaType};
use kadr_core::plugin::plugin_item_path;
use plugin_support::*;
use serde_json::json;

async fn seed(h: &Harness, live: bool) -> MediaItem {
    h.lib_repo.create_virtual("hello", "lib-1", "Lib", MediaType::LiveChannel).await.unwrap();
    let mut item = MediaItem {
        library_id: "lib-1".into(),
        item_type: MediaType::LiveChannel,
        title: "Ch".into(),
        file_path: plugin_item_path("hello", "ch1"),
        file_name: "ch1".into(),
        added_at: 1,
        ..Default::default()
    };
    if live {
        item.metadata.extra.insert("live".into(), json!(true));
    }
    h.media_repo.upsert_batch(&[item]).await.unwrap();
    h.media_repo.find_by_path(plugin_item_path("hello", "ch1")).await.unwrap().unwrap()
}

#[tokio::test]
async fn live_items_never_resume_and_never_persist_progress() {
    let h = build(enabled_settings()).await;
    let item = seed(&h, true).await;
    let id = item.id.unwrap();

    // Pre-existing state must not be used for resume, and must not be overwritten.
    h.playback_repo
        .upsert_progress("viewer", id, 400, kadr_core::models::WatchState::InProgress, 1)
        .await
        .unwrap();

    let (status, body) = call(&h.app, "POST", "/api/v1/playback/sessions", Some(&h.user_token), Some(json!({ "media_item_id": id }))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["resume_position_seconds"], 0);
    let session_id = body["session_id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &h.app, "POST", &format!("/api/v1/playback/{session_id}/progress"),
        Some(&h.user_token), Some(json!({ "position_seconds": 500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let state = h.playback_repo.get_state("viewer", id).await.unwrap().unwrap();
    assert_eq!(state.playback_position_seconds, 400, "live heartbeats must not persist progress");
}

#[tokio::test]
async fn non_live_plugin_items_persist_progress_like_files() {
    let h = build(enabled_settings()).await;
    let item = seed(&h, false).await;
    let id = item.id.unwrap();

    let (_, body) = call(&h.app, "POST", "/api/v1/playback/sessions", Some(&h.user_token), Some(json!({ "media_item_id": id }))).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();
    call(
        &h.app, "POST", &format!("/api/v1/playback/{session_id}/progress"),
        Some(&h.user_token), Some(json!({ "position_seconds": 30 })),
    )
    .await;
    let state = h.playback_repo.get_state("viewer", id).await.unwrap().unwrap();
    assert_eq!(state.playback_position_seconds, 30);
}

#[tokio::test]
async fn live_item_with_nonzero_duration_never_resumes_and_never_persists_progress() {
    let h = build(enabled_settings()).await;
    h.lib_repo.create_virtual("hello", "lib-2", "Lib", MediaType::LiveChannel).await.unwrap();
    let mut item = MediaItem {
        library_id: "lib-2".into(),
        item_type: MediaType::LiveChannel,
        title: "Ch2".into(),
        file_path: plugin_item_path("hello", "ch2"),
        file_name: "ch2".into(),
        added_at: 1,
        technical: kadr_core::models::TechnicalInfo {
            duration_seconds: 3600,
            ..Default::default()
        },
        ..Default::default()
    };
    item.metadata.extra.insert("live".into(), json!(true));
    h.media_repo.upsert_batch(&[item]).await.unwrap();
    let seeded = h.media_repo.find_by_path(plugin_item_path("hello", "ch2")).await.unwrap().unwrap();
    let id = seeded.id.unwrap();

    h.playback_repo
        .upsert_progress("viewer", id, 400, kadr_core::models::WatchState::InProgress, 1)
        .await
        .unwrap();

    let (status, body) = call(&h.app, "POST", "/api/v1/playback/sessions", Some(&h.user_token), Some(json!({ "media_item_id": id }))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["resume_position_seconds"], 0);
    let session_id = body["session_id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &h.app, "POST", &format!("/api/v1/playback/{session_id}/progress"),
        Some(&h.user_token), Some(json!({ "position_seconds": 500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let state = h.playback_repo.get_state("viewer", id).await.unwrap().unwrap();
    assert_eq!(state.playback_position_seconds, 400, "live heartbeats must not persist progress even with duration > 0");
}

#[tokio::test]
async fn non_live_item_with_zero_duration_persists_progress() {
    let h = build(enabled_settings()).await;
    let item = seed(&h, false).await;
    let id = item.id.unwrap();
    assert_eq!(item.technical.duration_seconds, 0);

    let (_, body) = call(&h.app, "POST", "/api/v1/playback/sessions", Some(&h.user_token), Some(json!({ "media_item_id": id }))).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();
    let (status, _) = call(
        &h.app, "POST", &format!("/api/v1/playback/{session_id}/progress"),
        Some(&h.user_token), Some(json!({ "position_seconds": 45 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let state = h.playback_repo.get_state("viewer", id).await.unwrap().unwrap();
    assert_eq!(state.playback_position_seconds, 45, "non-live 0-duration item must persist progress");
}
