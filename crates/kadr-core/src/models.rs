use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    #[serde(alias = "Anime", alias = "ANIME")]
    Anime,
    #[serde(alias = "Music", alias = "MUSIC")]
    Music,
    #[serde(alias = "HomeVideos", alias = "home_videos", alias = "HOME_VIDEOS")]
    HomeVideos,
    #[serde(alias = "Audiobook", alias = "AUDIOBOOK")]
    Audiobook,
    #[serde(alias = "LiveChannel", alias = "LIVE_CHANNEL")]
    LiveChannel,
    #[default]
    #[serde(alias = "Unknown", alias = "UNKNOWN")]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    pub media_type: MediaType,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default, skip_serializing)]
    pub pin_hash: Option<String>,
    pub created_at: i64,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            path: PathBuf::new(),
            paths: Vec::new(),
            media_type: MediaType::Unknown,
            is_private: false,
            pin_hash: None,
            created_at: 0,
        }
    }
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

impl TechnicalInfo {
    pub fn height(&self) -> Option<u32> {
        let res = self.resolution.as_deref()?.to_lowercase();
        if let Some((_, h)) = res.split_once('x') {
            if let Ok(val) = h.trim_end_matches('p').parse::<u32>() {
                return Some(val);
            }
        }
        if res.contains("4k") || res.contains("2160") {
            return Some(2160);
        }
        if res.contains("1080") {
            return Some(1080);
        }
        if res.contains("720") {
            return Some(720);
        }
        if res.contains("480") {
            return Some(480);
        }
        if let Some(val) = res.strip_suffix('p') {
            if let Ok(val) = val.parse::<u32>() {
                return Some(val);
            }
        }
        res.parse::<u32>().ok()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_title: Option<String>,
    #[serde(
        default,
        alias = "season_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub season: Option<u32>,
    #[serde(
        default,
        alias = "episode_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub episode: Option<u32>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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
    #[serde(default)]
    pub locked_fields: Vec<String>,
}

fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Partial update payload for persisted inline metadata & artwork overrides.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub original_title: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub release_year: Option<Option<i32>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub overview: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub director: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genres: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub studio: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub country: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub language: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub rating: Option<Option<f32>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub poster_path: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub backdrop_path: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub series_title: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub season: Option<Option<u32>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub episode: Option<Option<u32>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub tmdb_id: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub tmdb_media_type: Option<Option<String>>,
    #[serde(default)]
    pub reset_locks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    #[serde(alias = "Admin", alias = "ADMIN")]
    Admin,
    #[serde(alias = "Standard", alias = "STANDARD")]
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub pin_hash: String,
    pub role: UserRole,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    Unwatched,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackState {
    pub user_id: String,
    pub media_item_id: i64,
    pub playback_position_seconds: i64,
    pub watch_state: WatchState,
    pub last_watched_at: i64,
    pub play_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub session_id: String,
    pub user_id: String,
    pub media_item_id: i64,
    pub duration_seconds: i64,
    pub current_position_seconds: i64,
    pub started_at: i64,
    pub last_heartbeat_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub username: String,
    pub role: UserRole,
    pub exp: usize,
    pub iat: usize,
}
