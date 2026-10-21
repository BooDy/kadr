use kadr_core::transcoding::TranscodeParameters;
use kadr_server::config::{AppConfig, TranscodingSettings};
use kadr_server::streaming::hls::session::{HlsSessionManager, TranscodeSession};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_hls_session_manager_init_and_limits() {
    let settings = TranscodingSettings {
        enabled: true,
        ffmpeg_path: "ffmpeg".to_string(),
        max_concurrent_transcodes: 2,
        inactivity_timeout_secs: 60,
        temp_dir: PathBuf::from("/tmp/kadr_test_transcodes"),
        hwaccel: "none".to_string(),
    };

    let manager = HlsSessionManager::new(settings.clone());
    assert!(manager.settings().enabled);
    assert_eq!(manager.settings().max_concurrent_transcodes, 2);
    assert_eq!(manager.active_session_count().await, 0);
    assert_eq!(manager.active_full_transcode_count().await, 0);
}

#[test]
fn test_transcoding_settings_default_and_toml() {
    let default_settings = TranscodingSettings::default();
    assert!(default_settings.enabled);
    assert_eq!(default_settings.ffmpeg_path, "ffmpeg");
    assert_eq!(default_settings.max_concurrent_transcodes, 2);
    assert_eq!(default_settings.inactivity_timeout_secs, 60);
    assert_eq!(
        default_settings.temp_dir,
        PathBuf::from("/tmp/kadr/transcodes")
    );
    assert_eq!(default_settings.hwaccel, "none");

    let toml_str = r#"
        [transcoding]
        enabled = false
        ffmpeg_path = "/usr/local/bin/ffmpeg"
        max_concurrent_transcodes = 4
        inactivity_timeout_secs = 120
        temp_dir = "/var/transcode"
        hwaccel = "vaapi"
    "#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.transcoding.enabled);
    assert_eq!(config.transcoding.ffmpeg_path, "/usr/local/bin/ffmpeg");
    assert_eq!(config.transcoding.max_concurrent_transcodes, 4);
    assert_eq!(config.transcoding.inactivity_timeout_secs, 120);
    assert_eq!(config.transcoding.temp_dir, PathBuf::from("/var/transcode"));
    assert_eq!(config.transcoding.hwaccel, "vaapi");
}

#[tokio::test]
async fn test_transcode_session_touch() {
    let session = TranscodeSession {
        session_id: "test-sess-1".to_string(),
        media_id: 42,
        temp_dir: PathBuf::from("/tmp/session_42"),
        is_full_video_transcode: true,
        last_accessed_at: Arc::new(AtomicU64::new(100)),
        child_process: Arc::new(Mutex::new(None)),
        params: TranscodeParameters::default(),
        last_stderr: Arc::new(Mutex::new(Vec::new())),
    };

    assert_eq!(session.last_accessed_at.load(Ordering::Relaxed), 100);
    session.touch();
    let updated = session.last_accessed_at.load(Ordering::Relaxed);
    assert!(updated > 100);
}
