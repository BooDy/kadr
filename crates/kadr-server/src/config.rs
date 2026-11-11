use kadr_core::models::MediaType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_server_name")]
    pub name: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub web_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    #[serde(default = "default_db_path")]
    pub database_path: PathBuf,
    #[serde(default = "default_max_readers")]
    pub max_readers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerSettings {
    #[serde(default = "default_debounce")]
    pub debounce_millis: u64,
    #[serde(default = "default_ffprobe")]
    pub use_ffprobe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConfig {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub media_type: MediaType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_plugin_resolve_timeout_secs")]
    pub resolve_timeout_secs: u64,
    #[serde(default)]
    pub allow_private_proxy: bool,
}

fn default_plugin_resolve_timeout_secs() -> u64 {
    5
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            resolve_timeout_secs: default_plugin_resolve_timeout_secs(),
            allow_private_proxy: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryboardSettings {
    #[serde(default = "default_storyboard_enabled")]
    pub enabled: bool,
    #[serde(default = "default_storyboard_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_storyboard_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_storyboard_tile_width")]
    pub tile_width: u32,
    #[serde(default)]
    pub generate_on_scan: bool,
}

fn default_storyboard_enabled() -> bool {
    true
}
fn default_storyboard_max_concurrent() -> usize {
    1
}
fn default_storyboard_interval_secs() -> u64 {
    10
}
fn default_storyboard_tile_width() -> u32 {
    320
}

impl Default for StoryboardSettings {
    fn default() -> Self {
        Self {
            enabled: default_storyboard_enabled(),
            max_concurrent: default_storyboard_max_concurrent(),
            interval_secs: default_storyboard_interval_secs(),
            tile_width: default_storyboard_tile_width(),
            generate_on_scan: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub storage: StorageSettings,
    #[serde(default)]
    pub scanner: ScannerSettings,
    #[serde(default)]
    pub libraries: Vec<LibraryConfig>,
    #[serde(default)]
    pub transcoding: TranscodingSettings,
    #[serde(default)]
    pub plugins: PluginSettings,
    #[serde(default)]
    pub storyboard: StoryboardSettings,
}

fn default_server_name() -> String {
    "Kadr Media Server".to_string()
}
fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8492
}
fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}
fn default_db_path() -> PathBuf {
    PathBuf::from("./data/kadr.db")
}
fn default_max_readers() -> usize {
    4
}
fn default_debounce() -> u64 {
    500
}
fn default_ffprobe() -> bool {
    true
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            name: default_server_name(),
            host: default_host(),
            port: default_port(),
            data_dir: default_data_dir(),
            web_dir: None,
        }
    }
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            database_path: default_db_path(),
            max_readers: default_max_readers(),
        }
    }
}

impl Default for ScannerSettings {
    fn default() -> Self {
        Self {
            debounce_millis: default_debounce(),
            use_ffprobe: default_ffprobe(),
        }
    }
}

fn default_transcoding_enabled() -> bool {
    true
}
fn default_ffmpeg_path() -> String {
    "ffmpeg".to_string()
}
fn default_max_concurrent_transcodes() -> usize {
    2
}
fn default_inactivity_timeout_secs() -> u64 {
    60
}
fn default_temp_dir() -> PathBuf {
    PathBuf::from("/tmp/kadr/transcodes")
}
fn default_hwaccel() -> String {
    "none".to_string()
}

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

impl AppConfig {
    pub fn load_from_file<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
