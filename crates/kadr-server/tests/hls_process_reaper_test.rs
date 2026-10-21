use kadr_core::models::TechnicalInfo;
use kadr_core::transcoding::TranscodeParameters;
use kadr_server::config::TranscodingSettings;
use kadr_server::streaming::hls::session::{HlsError, HlsSessionManager};
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[tokio::test]
async fn test_hls_reaper_cleans_expired_session() {
    let temp_root = tempfile::tempdir().unwrap();
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 2,
        inactivity_timeout_secs: 1, // 1 second for fast test
        temp_dir: temp_root.path().to_path_buf(),
        hwaccel: "none".to_string(),
    };

    let manager = Arc::new(HlsSessionManager::new(settings));
    let session_dir = temp_root.path().join("test_session_123");
    tokio::fs::create_dir_all(&session_dir).await.unwrap();

    let session = manager
        .register_test_session("test_session_123", 1, session_dir.clone(), false)
        .await;

    // Simulate session accessed 5 seconds ago (exceeding 1s timeout)
    session.last_accessed_at.store(0, Ordering::Relaxed);

    let reaper_ran = manager.sweep_expired_sessions().await;
    assert_eq!(reaper_ran, 1);
    assert_eq!(manager.active_session_count().await, 0);
    assert!(
        !session_dir.exists(),
        "Expired session temp directory should be removed"
    );
}

#[tokio::test]
async fn test_hls_reaper_preserves_active_sessions() {
    let temp_root = tempfile::tempdir().unwrap();
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 2,
        inactivity_timeout_secs: 60,
        temp_dir: temp_root.path().to_path_buf(),
        hwaccel: "none".to_string(),
    };

    let manager = Arc::new(HlsSessionManager::new(settings));
    let session_dir = temp_root.path().join("active_session_456");
    tokio::fs::create_dir_all(&session_dir).await.unwrap();

    let session = manager
        .register_test_session("active_session_456", 2, session_dir.clone(), false)
        .await;

    // Freshly touched
    session.touch();

    let reaper_ran = manager.sweep_expired_sessions().await;
    assert_eq!(reaper_ran, 0);
    assert_eq!(manager.active_session_count().await, 1);
    assert!(session_dir.exists());
}

#[tokio::test]
async fn test_hls_terminate_session() {
    let temp_root = tempfile::tempdir().unwrap();
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 2,
        inactivity_timeout_secs: 60,
        temp_dir: temp_root.path().to_path_buf(),
        hwaccel: "none".to_string(),
    };

    let manager = Arc::new(HlsSessionManager::new(settings));
    let session_dir = temp_root.path().join("term_session_789");
    tokio::fs::create_dir_all(&session_dir).await.unwrap();

    let _session = manager
        .register_test_session("term_session_789", 3, session_dir.clone(), false)
        .await;

    assert_eq!(manager.active_session_count().await, 1);
    let terminated = manager.terminate_session("term_session_789").await;
    assert!(terminated);
    assert_eq!(manager.active_session_count().await, 0);
    assert!(
        !session_dir.exists(),
        "Terminated session dir should be deleted"
    );

    // Terminating non-existent returns false
    let terminated_again = manager.terminate_session("term_session_789").await;
    assert!(!terminated_again);
}

#[tokio::test]
async fn test_hls_wait_for_playlist_and_segment() {
    let temp_root = tempfile::tempdir().unwrap();
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 2,
        inactivity_timeout_secs: 60,
        temp_dir: temp_root.path().to_path_buf(),
        hwaccel: "none".to_string(),
    };

    let manager = Arc::new(HlsSessionManager::new(settings));
    let session_dir = temp_root.path().join("wait_session");
    tokio::fs::create_dir_all(&session_dir).await.unwrap();

    let session = manager
        .register_test_session("wait_session", 4, session_dir.clone(), false)
        .await;

    // Timeout test for playlist
    let timeout_res = manager.wait_for_playlist(&session, 1).await;
    assert!(matches!(timeout_res, Err(HlsError::Timeout(_))));

    // Write playlist file and test success
    let playlist_path = session_dir.join("playlist.m3u8");
    tokio::fs::write(&playlist_path, "#EXTM3U\n").await.unwrap();

    let playlist_res = manager.wait_for_playlist(&session, 1).await;
    assert!(playlist_res.is_ok());
    assert_eq!(playlist_res.unwrap(), playlist_path);

    // Timeout test for segment
    let seg_timeout_res = manager.wait_for_segment(&session, 0, 1).await;
    assert!(matches!(seg_timeout_res, Err(HlsError::Timeout(_))));

    // Write segment file and test success
    let segment_path = session_dir.join("segment_0000.ts");
    tokio::fs::write(&segment_path, b"test-ts-bytes")
        .await
        .unwrap();

    let seg_res = manager.wait_for_segment(&session, 0, 1).await;
    assert!(seg_res.is_ok());
    assert_eq!(seg_res.unwrap(), segment_path);
}

#[tokio::test]
async fn test_hls_session_concurrency_and_deduplication() {
    let temp_root = tempfile::tempdir().unwrap();
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 1, // Max 1 full transcode
        inactivity_timeout_secs: 60,
        temp_dir: temp_root.path().to_path_buf(),
        hwaccel: "none".to_string(),
    };

    let manager = Arc::new(HlsSessionManager::new(settings));
    let session_dir = temp_root.path().join("existing_transcode");
    tokio::fs::create_dir_all(&session_dir).await.unwrap();

    // Register active full video transcode session
    let existing_session = manager
        .register_test_session("existing_transcode", 100, session_dir.clone(), true)
        .await;

    // Same media_id and params should deduplicate and return existing session
    let dummy_video = temp_root.path().join("dummy.mkv");
    tokio::fs::write(&dummy_video, b"video").await.unwrap();

    let tech = TechnicalInfo {
        video_codec: Some("hevc".to_string()), // triggers video transcode since target is h264
        audio_codec: Some("aac".to_string()),
        ..Default::default()
    };
    let params = TranscodeParameters::default();

    // 1. Deduplication: if media_id matches existing_session.media_id and params match, reuse!
    let dedup = manager
        .get_or_create_session(100, &dummy_video, &tech, &params)
        .await
        .unwrap();
    assert_eq!(dedup.session_id, existing_session.session_id);

    // 2. Concurrency gating: requesting a new media_id (101) with full video transcode should exceed limit (1)
    let err = manager
        .get_or_create_session(101, &dummy_video, &tech, &params)
        .await
        .unwrap_err();
    assert!(matches!(err, HlsError::CapacityExceeded));

    // Audio-only transcode / copy (video is h264, target is h264) bypasses video concurrency limit!
    let tech_copy = TechnicalInfo {
        video_codec: Some("h264".to_string()),
        audio_codec: Some("flac".to_string()), // audio transcode to aac, but video copy
        ..Default::default()
    };
    // Non-existent ffmpeg binary will fail at spawn, but won't fail at capacity check!
    // Since ffmpeg may or may not be available on system path, check that error is not CapacityExceeded
    let copy_res = manager
        .get_or_create_session(102, &dummy_video, &tech_copy, &params)
        .await;
    match copy_res {
        Ok(s) => assert_eq!(s.media_id, 102),
        Err(HlsError::CapacityExceeded) => {
            panic!("Video copy should not be gated by CapacityExceeded")
        }
        Err(_) => {} // Spawn error is expected if ffmpeg is missing or invalid input
    }
}

#[tokio::test]
async fn test_hls_start_reaper_task_handle() {
    let temp_root = tempfile::tempdir().unwrap();
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 2,
        inactivity_timeout_secs: 1,
        temp_dir: temp_root.path().to_path_buf(),
        hwaccel: "none".to_string(),
    };

    let manager = Arc::new(HlsSessionManager::new(settings));
    let handle = manager.clone().start_reaper();

    // Verify handle is active and can be cleanly aborted
    assert!(!handle.is_finished());
    handle.abort();
}
