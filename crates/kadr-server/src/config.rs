use kadr_core::models::MediaType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
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
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8096
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
            host: default_host(),
            port: default_port(),
            data_dir: default_data_dir(),
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

impl AppConfig {
    pub fn load_from_file<P: AsRef<std::path::Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
