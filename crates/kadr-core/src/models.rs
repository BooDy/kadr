use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    #[serde(alias = "Movie", alias = "MOVIE")]
    Movie,
    #[serde(alias = "Show", alias = "SHOW")]
    Show,
    #[serde(alias = "Season", alias = "SEASON")]
    Season,
    #[serde(alias = "Episode", alias = "EPISODE")]
    Episode,
    #[serde(alias = "Unknown", alias = "UNKNOWN")]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub media_type: MediaType,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechnicalInfo {
    pub duration_seconds: i64,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u8>,
    pub container: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub director: Option<String>,
    pub writers: Vec<String>,
    pub actors: Vec<String>,
    pub overview: Option<String>,
    pub country: Option<String>,
    pub language: Option<String>,
    pub tags: Vec<String>,
    pub studio: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub release_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: Option<i64>,
    pub library_id: String,
    pub item_type: MediaType,
    pub title: String,
    pub original_title: Option<String>,
    pub release_year: Option<i32>,
    pub added_at: i64,
    pub file_path: PathBuf,
    pub file_name: String,
    pub file_size: u64,
    pub technical: TechnicalInfo,
    pub metadata: MediaMetadata,
}
