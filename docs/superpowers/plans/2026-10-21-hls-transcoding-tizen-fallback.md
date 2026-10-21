# HLS Transcoding & Universal Fallback Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement on-demand HLS remuxing and transcoding with an active session manager, smart video passthrough, and an inactivity reaper to support unsupported audio/video/container formats on Samsung Smart TVs (Tizen OS) and web players.

**Architecture:** A pure-Rust asynchronous execution pipeline wraps `ffmpeg` via `tokio::process::Command` to package media into HLS (`.m3u8` and `.ts` segments) in ephemeral session directories. A decision engine detects when video can be copied (`-c:v copy`) to fix audio or container incompatibilities at near 0% CPU, while an in-memory session manager handles segment delivery, concurrency limits, and a background reaper task that auto-kills idle processes after 60 seconds.

**Tech Stack:** Rust (axum 0.8, tokio 1, tokio-util, uuid, serde, tempfile), ffmpeg CLI, HLS standard (RFC 8216).

## Global Constraints
- Pure-Rust server baseline preserved; zero native C dependencies in Kadr crates (`ffmpeg` executed as an isolated CLI subprocess).
- Inactivity cleanup: background reaper terminates child `ffmpeg` processes and deletes ephemeral files after 60 seconds of inactivity.
- Smart copy priority: whenever the source video codec and resolution match the requested profile, the server passes video through with `-c:v copy`, only transcoding audio or remuxing containers.
- Every segment sequence URL (`/hls/sessions/{session_id}/segment_{seq}.ts`) validates that `session_id` is a valid UUID and `seq` is a `u32` to eliminate path traversal vulnerabilities.

---

### Task 1: Transcoding Decision Engine & FFmpeg Command Builder (`kadr-core`)

**Files:**
- Create: `crates/kadr-core/src/transcoding/mod.rs`
- Create: `crates/kadr-core/src/transcoding/builder.rs`
- Modify: `crates/kadr-core/src/lib.rs:1-12`
- Test: `crates/kadr-core/tests/transcoding_test.rs`

**Interfaces:**
- Consumes: `kadr_core::models::TechnicalInfo`
- Produces:
  - `pub enum VideoAction { Copy, Transcode { codec: String, max_height: Option<u32>, hwaccel: Option<String> } }`
  - `pub enum AudioAction { Copy, Transcode { codec: String, channels: u32, bitrate_kbps: u32 } }`
  - `pub struct TranscodeParameters { pub video_codec: String, pub audio_codec: String, pub max_height: Option<u32>, pub audio_track_index: u32, pub start_time: f64 }`
  - `pub struct TranscodeDecision { pub video: VideoAction, pub audio: AudioAction }`
  - `pub fn evaluate_transcode(tech: &TechnicalInfo, params: &TranscodeParameters, hwaccel: Option<&str>) -> TranscodeDecision`
  - `pub fn build_ffmpeg_hls_args(input_path: &std::path::Path, output_dir: &std::path::Path, decision: &TranscodeDecision, params: &TranscodeParameters) -> Vec<String>`

- [ ] **Step 1: Write the failing unit tests for decision engine and command builder**

Create `crates/kadr-core/tests/transcoding_test.rs`:
```rust
use kadr_core::models::TechnicalInfo;
use kadr_core::transcoding::{
    build_ffmpeg_hls_args, evaluate_transcode, AudioAction, TranscodeParameters, VideoAction,
};
use std::path::Path;

#[test]
fn test_evaluate_transcode_audio_only_remux() {
    let tech = TechnicalInfo {
        container: Some("mkv".to_string()),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("dts".to_string()),
        width: Some(1920),
        height: Some(1080),
        duration_seconds: Some(3600.0),
        bitrate_kbps: Some(5000),
        has_subtitles: false,
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: Some(1080),
        audio_track_index: 0,
        start_time: 0.0,
    };

    let decision = evaluate_transcode(&tech, &params, None);
    assert_eq!(decision.video, VideoAction::Copy);
    match decision.audio {
        AudioAction::Transcode { codec, .. } => assert_eq!(codec, "aac"),
        _ => panic!("Expected audio transcode to aac"),
    }
}

#[test]
fn test_evaluate_transcode_full_video_reencode_for_hevc() {
    let tech = TechnicalInfo {
        container: Some("mkv".to_string()),
        video_codec: Some("hevc".to_string()),
        audio_codec: Some("aac".to_string()),
        width: Some(3840),
        height: Some(2160),
        duration_seconds: Some(3600.0),
        bitrate_kbps: Some(15000),
        has_subtitles: false,
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: Some(1080),
        audio_track_index: 0,
        start_time: 15.0,
    };

    let decision = evaluate_transcode(&tech, &params, None);
    match decision.video {
        VideoAction::Transcode { max_height, .. } => assert_eq!(max_height, Some(1080)),
        _ => panic!("Expected full video transcode"),
    }
    assert_eq!(decision.audio, AudioAction::Copy);
}

#[test]
fn test_build_ffmpeg_hls_args_construction() {
    let decision = kadr_core::transcoding::TranscodeDecision {
        video: VideoAction::Copy,
        audio: AudioAction::Transcode {
            codec: "aac".to_string(),
            channels: 2,
            bitrate_kbps: 192,
        },
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: None,
        audio_track_index: 1,
        start_time: 42.5,
    };

    let args = build_ffmpeg_hls_args(
        Path::new("/media/movie.mkv"),
        Path::new("/tmp/hls_test"),
        &decision,
        &params,
    );

    assert!(args.contains(&"-ss".to_string()));
    assert!(args.contains(&"42.5".to_string()));
    assert!(args.contains(&"-c:v".to_string()));
    assert!(args.contains(&"copy".to_string()));
    assert!(args.contains(&"-c:a".to_string()));
    assert!(args.contains(&"aac".to_string()));
    assert!(args.contains(&"independent_segments".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test transcoding_test`
Expected: FAIL with "unresolved import `kadr_core::transcoding`"

- [ ] **Step 3: Implement decision engine and command builder**

Create `crates/kadr-core/src/transcoding/mod.rs`:
```rust
pub mod builder;
pub use builder::*;
```

Create `crates/kadr-core/src/transcoding/builder.rs`:
```rust
use crate::models::TechnicalInfo;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoAction {
    Copy,
    Transcode {
        codec: String,
        max_height: Option<u32>,
        hwaccel: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioAction {
    Copy,
    Transcode {
        codec: String,
        channels: u32,
        bitrate_kbps: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscodeParameters {
    pub video_codec: String,
    pub audio_codec: String,
    pub max_height: Option<u32>,
    pub audio_track_index: u32,
    pub start_time: f64,
}

impl Default for TranscodeParameters {
    fn default() -> Self {
        Self {
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            max_height: None,
            audio_track_index: 0,
            start_time: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscodeDecision {
    pub video: VideoAction,
    pub audio: AudioAction,
}

pub fn evaluate_transcode(
    tech: &TechnicalInfo,
    params: &TranscodeParameters,
    hwaccel: Option<&str>,
) -> TranscodeDecision {
    let source_video_codec = tech
        .video_codec
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let target_video_codec = params.video_codec.to_lowercase();

    let height_ok = match (tech.height, params.max_height) {
        (Some(src_h), Some(max_h)) => src_h <= max_h,
        _ => true,
    };

    let video_action = if source_video_codec == target_video_codec && height_ok {
        VideoAction::Copy
    } else {
        VideoAction::Transcode {
            codec: target_video_codec,
            max_height: params.max_height,
            hwaccel: hwaccel.map(|s| s.to_string()),
        }
    };

    let source_audio_codec = tech
        .audio_codec
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let target_audio_codec = params.audio_codec.to_lowercase();

    let audio_action = if source_audio_codec == target_audio_codec {
        AudioAction::Copy
    } else {
        AudioAction::Transcode {
            codec: target_audio_codec,
            channels: 2,
            bitrate_kbps: 192,
        }
    };

    TranscodeDecision {
        video: video_action,
        audio: audio_action,
    }
}

pub fn build_ffmpeg_hls_args(
    input_path: &Path,
    output_dir: &Path,
    decision: &TranscodeDecision,
    params: &TranscodeParameters,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];

    if params.start_time > 0.0 {
        args.push("-ss".into());
        args.push(format!("{:.3}", params.start_time));
    }

    args.push("-i".into());
    args.push(input_path.to_string_lossy().to_string());

    // Stream mapping
    args.push("-map".into());
    args.push("0:v:0".into());
    args.push("-map".into());
    args.push(format!("0:a:{}", params.audio_track_index));

    // Video codec flags
    match &decision.video {
        VideoAction::Copy => {
            args.push("-c:v".into());
            args.push("copy".into());
        }
        VideoAction::Transcode {
            max_height,
            hwaccel,
            ..
        } => {
            if let Some(scale_h) = max_height {
                args.push("-vf".into());
                args.push(format!("scale=-2:'min(ih,{})'", scale_h));
            }
            match hwaccel.as_deref() {
                Some("vaapi") => {
                    args.push("-c:v".into());
                    args.push("h264_vaapi".into());
                }
                Some("nvenc") => {
                    args.push("-c:v".into());
                    args.push("h264_nvenc".into());
                }
                _ => {
                    args.push("-c:v".into());
                    args.push("libx264".into());
                    args.push("-preset".into());
                    args.push("veryfast".into());
                    args.push("-crf".into());
                    args.push("23".into());
                    args.push("-pix_fmt".into());
                    args.push("yuv420p".into());
                }
            }
        }
    }

    // Audio codec flags
    match &decision.audio {
        AudioAction::Copy => {
            args.push("-c:a".into());
            args.push("copy".into());
        }
        AudioAction::Transcode {
            codec,
            channels,
            bitrate_kbps,
        } => {
            args.push("-c:a".into());
            args.push(codec.clone());
            args.push("-b:a".into());
            args.push(format!("{}k", bitrate_kbps));
            args.push("-ac".into());
            args.push(channels.to_string());
        }
    }

    // Disable embedded subtitles
    args.push("-sn".into());

    // HLS packaging options
    args.push("-f".into());
    args.push("hls".into());
    args.push("-hls_time".into());
    args.push("4".into());
    args.push("-hls_playlist_type".into());
    args.push("event".into());
    args.push("-hls_flags".into());
    args.push("independent_segments".into());
    args.push("-hls_segment_filename".into());
    args.push(
        output_dir
            .join("segment_%04d.ts")
            .to_string_lossy()
            .to_string(),
    );
    args.push(
        output_dir
            .join("playlist.m3u8")
            .to_string_lossy()
            .to_string(),
    );

    args
}
```

Update `crates/kadr-core/src/lib.rs` to expose `pub mod transcoding; pub use transcoding::*;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test transcoding_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-core/src/transcoding/ crates/kadr-core/src/lib.rs crates/kadr-core/tests/transcoding_test.rs
git commit -m "feat(core): implement transcoding decision engine and ffmpeg command builder"
```

---

### Task 2: Transcoding Configuration & HLS Session Data Models (`kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/config.rs:1-119`
- Create: `crates/kadr-server/src/streaming/hls/mod.rs`
- Create: `crates/kadr-server/src/streaming/hls/session.rs`
- Modify: `crates/kadr-server/src/streaming/mod.rs`
- Test: `crates/kadr-server/tests/hls_session_model_test.rs`

**Interfaces:**
- Consumes: `AppConfig`, `TranscodeParameters`, `TranscodeDecision`
- Produces:
  - `pub struct TranscodingSettings { pub enabled: bool, pub ffmpeg_path: String, pub max_concurrent_transcodes: usize, pub inactivity_timeout_secs: u64, pub temp_dir: PathBuf, pub hwaccel: String }`
  - `pub struct TranscodeSession { pub session_id: String, pub media_id: i64, pub temp_dir: PathBuf, pub is_full_video_transcode: bool, pub last_accessed_at: Arc<AtomicU64>, ... }`
  - `pub struct HlsSessionManager { pub config: TranscodingSettings, ... }`

- [ ] **Step 1: Write the failing unit test for configuration and session manager initialization**

Create `crates/kadr-server/tests/hls_session_model_test.rs`:
```rust
use kadr_server::config::TranscodingSettings;
use kadr_server::streaming::hls::session::HlsSessionManager;
use std::path::PathBuf;

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

    let manager = HlsSessionManager::new(settings);
    assert_eq!(manager.active_session_count().await, 0);
    assert_eq!(manager.active_full_transcode_count().await, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test hls_session_model_test`
Expected: FAIL with "unresolved import `TranscodingSettings`"

- [ ] **Step 3: Implement TranscodingSettings and HlsSessionManager scaffolding**

In `crates/kadr-server/src/config.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodingSettings {
    #[serde(default = "default_transcoding_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ffmpeg_path")]
    pub ffmpeg_path: String,
    #[serde(default = "default_max_concurrent_transcodes")]
    pub max_concurrent_transcodes: usize,
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
    #[serde(default = "default_temp_dir")]
    pub temp_dir: PathBuf,
    #[serde(default = "default_hwaccel")]
    pub hwaccel: String,
}

fn default_transcoding_enabled() -> bool { true }
fn default_ffmpeg_path() -> String { "ffmpeg".to_string() }
fn default_max_concurrent_transcodes() -> usize { 2 }
fn default_inactivity_timeout_secs() -> u64 { 60 }
fn default_temp_dir() -> PathBuf { PathBuf::from("/tmp/kadr/transcodes") }
fn default_hwaccel() -> String { "none".to_string() }

impl Default for TranscodingSettings {
    fn default() -> Self {
        Self {
            enabled: default_transcoding_enabled(),
            ffmpeg_path: default_ffmpeg_path(),
            max_concurrent_transcodes: default_max_concurrent_transcodes(),
            inactivity_timeout_secs: default_inactivity_timeout_secs(),
            temp_dir: default_temp_dir(),
            hwaccel: default_hwaccel(),
        }
    }
}
```
Add `pub transcoding: TranscodingSettings` to `AppConfig`.

Create `crates/kadr-server/src/streaming/hls/session.rs`:
```rust
use crate::config::TranscodingSettings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};

pub struct TranscodeSession {
    pub session_id: String,
    pub media_id: i64,
    pub temp_dir: PathBuf,
    pub is_full_video_transcode: bool,
    pub last_accessed_at: Arc<AtomicU64>,
    pub child_process: Arc<Mutex<Option<tokio::process::Child>>>,
}

impl TranscodeSession {
    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_accessed_at.store(now, Ordering::Relaxed);
    }
}

pub struct HlsSessionManager {
    settings: TranscodingSettings,
    sessions: Arc<RwLock<HashMap<String, Arc<TranscodeSession>>>>,
}

impl HlsSessionManager {
    pub fn new(settings: TranscodingSettings) -> Self {
        Self {
            settings,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn settings(&self) -> &TranscodingSettings {
        &self.settings
    }

    pub async fn active_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub async fn active_full_transcode_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.values().filter(|s| s.is_full_video_transcode).count()
    }
}
```

Create `crates/kadr-server/src/streaming/hls/mod.rs` and update `crates/kadr-server/src/streaming/mod.rs` to expose `pub mod hls;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test hls_session_model_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/config.rs crates/kadr-server/src/streaming/ crates/kadr-server/tests/hls_session_model_test.rs
git commit -m "feat(streaming): add transcoding configuration and HlsSessionManager scaffolding"
```

---

### Task 3: Process Execution, Segment Polling & Inactivity Reaper (`kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/streaming/hls/session.rs`
- Test: `crates/kadr-server/tests/hls_process_reaper_test.rs`

**Interfaces:**
- Produces:
  - `impl HlsSessionManager`:
    - `pub async fn get_or_create_session(&self, media_id: i64, media_path: &Path, tech: &TechnicalInfo, params: &TranscodeParameters) -> Result<Arc<TranscodeSession>, HlsError>`
    - `pub async fn wait_for_playlist(&self, session: &TranscodeSession, timeout_secs: u64) -> Result<PathBuf, HlsError>`
    - `pub async fn wait_for_segment(&self, session: &TranscodeSession, seq: u32, timeout_secs: u64) -> Result<PathBuf, HlsError>`
    - `pub async fn terminate_session(&self, session_id: &str) -> bool`
    - `pub fn start_reaper(self: Arc<Self>) -> tokio::task::JoinHandle<()>`

- [ ] **Step 1: Write the failing unit test for session lifecycle and reaper expiry**

Create `crates/kadr-server/tests/hls_process_reaper_test.rs`:
```rust
use kadr_server::config::TranscodingSettings;
use kadr_server::streaming::hls::session::HlsSessionManager;
use std::path::PathBuf;
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
    assert!(!session_dir.exists(), "Expired session temp directory should be removed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test hls_process_reaper_test`
Expected: FAIL with "no method named `sweep_expired_sessions`"

- [ ] **Step 3: Implement process execution, segment waiting, and reaper**

In `crates/kadr-server/src/streaming/hls/session.rs`:
- Implement `get_or_create_session`:
  - Check capacity limit: if `is_full_video_transcode` and `active_full_transcode_count() >= max_concurrent_transcodes`, return `HlsError::CapacityExceeded`.
  - Create session folder: `<temp_dir>/<session_id>/`.
  - Build args using `kadr_core::transcoding::build_ffmpeg_hls_args`.
  - Spawn child using `tokio::process::Command::new(&self.settings.ffmpeg_path).args(&args).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()`.
  - Return `Arc<TranscodeSession>`.
- Implement `wait_for_playlist` and `wait_for_segment`:
  - Polls `tokio::fs::try_exists(&target_path)` with `tokio::time::sleep(Duration::from_millis(200))` up to `timeout_secs`.
- Implement `sweep_expired_sessions`:
  - Iterates active sessions; if `now - last_accessed > inactivity_timeout_secs`, locks `child_process`, sends `start_kill()`, removes temp directory with `tokio::fs::remove_dir_all(&temp_dir)`, and removes from `sessions` map.
- Implement `start_reaper`:
  - Spawns background loop every 15 seconds invoking `sweep_expired_sessions()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test hls_process_reaper_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/streaming/hls/session.rs crates/kadr-server/tests/hls_process_reaper_test.rs
git commit -m "feat(streaming): implement HlsSessionManager process spawning and inactivity reaper"
```

---

### Task 4: HLS API Endpoints & Route Mounting (`kadr-server`)

**Files:**
- Create: `crates/kadr-server/src/api/hls_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs`
- Modify: `crates/kadr-server/src/lib.rs`
- Test: `crates/kadr-server/tests/hls_routes_test.rs`

**Interfaces:**
- Produces:
  - `pub fn hls_routes() -> axum::Router`
  - Handler `master_playlist`: `GET /api/v1/media/:item_id/hls/master.m3u8`
  - Handler `stream_segment`: `GET /api/v1/media/:item_id/hls/sessions/:session_id/segment_:seq.ts`
  - Handler `delete_session`: `DELETE /api/v1/media/:item_id/hls/sessions/:session_id`

- [ ] **Step 1: Write the failing integration test for HLS route registration and response headers**

Create `crates/kadr-server/tests/hls_routes_test.rs`:
```rust
use axum::http::{Request, StatusCode};
use kadr_server::streaming::hls::session::HlsSessionManager;
use tower::ServiceExt;

#[tokio::test]
async fn test_hls_invalid_uuid_segment_path_returns_bad_request() {
    let (app, _state) = test_helpers::create_test_app().await;

    let req = Request::builder()
        .uri("/api/v1/media/1/hls/sessions/not-a-valid-uuid/segment_0000.ts")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test hls_routes_test`
Expected: FAIL with "route not found (404) or endpoint not registered"

- [ ] **Step 3: Implement HLS API routes and attach to router**

Create `crates/kadr-server/src/api/hls_routes.rs`:
- Define `HlsQuery` struct for deserializing `video_codec`, `audio_codec`, `max_height`, `start_time`, `audio_track`.
- Implement `master_playlist`:
  - Fetch `media_item` and verify `UnlockedLibraries`.
  - Invoke `session_mgr.get_or_create_session(...)`.
  - Await `session_mgr.wait_for_playlist(...)`.
  - Rewrite relative segment URLs in the M3U8 to point to `/api/v1/media/{item_id}/hls/sessions/{session_id}/segment_XXXX.ts`.
  - Return `200 OK` with `Content-Type: application/vnd.apple.mpegurl`.
- Implement `stream_segment`:
  - Validate `uuid::Uuid::parse_str(&session_id)` (return 400 Bad Request on failure).
  - Find session; verify `session.media_id == item_id`.
  - Touch session: `session.touch()`.
  - Await segment readiness via `session_mgr.wait_for_segment(...)`.
  - Stream using `ReaderStream` with 64 KB capacity.
  - Return `200 OK` with `Content-Type: video/mp2t` and `Cache-Control: public, max-age=3600`.
- Implement `delete_session`:
  - Validate `session_id`.
  - Invoke `session_mgr.terminate_session(&session_id)`.
  - Return `204 No Content`.

Register routes in `crates/kadr-server/src/api/mod.rs` and `crates/kadr-server/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test hls_routes_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-server/src/api/hls_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/src/lib.rs crates/kadr-server/tests/hls_routes_test.rs
git commit -m "feat(api): implement HLS master playlist, segment streaming, and teardown endpoints"
```

---

### Task 5: End-to-End Transcoding & Playback Fallback Verification

**Files:**
- Create: `tests/e2e_hls_transcoding_test.rs`

**Interfaces:**
- Consumes: Complete Kadr server stack, test media generator using CLI `ffmpeg`
- Produces: Verified E2E test suite running real HLS transcoding, segment streaming, and session teardown.

- [ ] **Step 1: Write the end-to-end integration test**

Create `tests/e2e_hls_transcoding_test.rs`:
```rust
use axum::http::{header, Request, StatusCode};
use std::process::Command;
use tower::ServiceExt;

#[tokio::test]
async fn test_e2e_hls_transcoding_and_segment_delivery() {
    // 1. Check if ffmpeg is available on the host
    let ffmpeg_available = Command::new("ffmpeg").arg("-version").output().is_ok();
    if !ffmpeg_available {
        eprintln!("Skipping test_e2e_hls_transcoding_and_segment_delivery: ffmpeg not available");
        return;
    }

    // 2. Generate a 6-second synthetic test video (H.264 + AAC)
    let temp_dir = tempfile::tempdir().unwrap();
    let video_path = temp_dir.path().join("sample.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y", "-f", "lavfi", "-i", "testsrc=duration=6:size=640x360:rate=30",
            "-f", "lavfi", "-i", "sine=frequency=1000:duration=6",
            "-c:v", "libx264", "-c:a", "aac",
            video_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute ffmpeg generator");
    assert!(status.status.success());

    // 3. Initialize test server and seed item in database
    let (app, media_id) = test_helpers::setup_e2e_with_media(&video_path).await;

    // 4. Request master playlist
    let req = Request::builder()
        .uri(format!("/api/v1/media/{media_id}/hls/master.m3u8?video_codec=h264&audio_codec=aac"))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/vnd.apple.mpegurl"
    );

    let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let playlist_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(playlist_str.contains("#EXTM3U"));
    assert!(playlist_str.contains(".ts"));

    // 5. Extract first segment URL and fetch it
    let segment_line = playlist_str
        .lines()
        .find(|l| l.contains("segment_0000.ts"))
        .expect("Should contain segment_0000.ts");

    let seg_req = Request::builder()
        .uri(segment_line)
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let seg_resp = app.clone().oneshot(seg_req).await.unwrap();
    assert_eq!(seg_resp.status(), StatusCode::OK);
    assert_eq!(
        seg_resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "video/mp2t"
    );

    let seg_bytes = axum::body::to_bytes(seg_resp.into_body(), 10 * 1024 * 1024).await.unwrap();
    assert!(!seg_bytes.is_empty());
    // MPEG-TS sync byte is 0x47
    assert_eq!(seg_bytes[0], 0x47);
}
```

- [ ] **Step 2: Run test to verify it compiles and runs**

Run: `cargo test --test e2e_hls_transcoding_test`
Expected: PASS (or skipped if running in an environment without ffmpeg)

- [ ] **Step 3: Run full workspace test suite**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add tests/e2e_hls_transcoding_test.rs
git commit -m "test(e2e): add end-to-end integration test for HLS transcoding and segment delivery"
```
