use crate::models::MediaItem;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PLUGIN_SCHEME: &str = "plugin://";

pub const SUPPORTED_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginPermission {
    #[serde(rename = "items:publish")]
    ItemsPublish,
    #[serde(rename = "playback:resolve")]
    PlaybackResolve,
}

impl PluginPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ItemsPublish => "items:publish",
            Self::PlaybackResolve => "playback:resolve",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Pending,
    Enabled,
    Disabled,
    Unhealthy,
}

impl PluginStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Unhealthy => "unhealthy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "enabled" => Some(Self::Enabled),
            "disabled" => Some(Self::Disabled),
            "unhealthy" => Some(Self::Unhealthy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub virtual_library: bool,
    #[serde(default)]
    pub playback_resolver: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
}

impl PluginManifest {
    /// Validates id format (`[a-z0-9-]{1,32}`), name, and API version.
    pub fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 32
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err("manifest id must match [a-z0-9-]{1,32}".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("manifest name must not be empty".to_string());
        }
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(format!(
                "unsupported api_version {} (supported: {})",
                self.api_version, SUPPORTED_API_VERSION
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub manifest: PluginManifest,
    pub granted_permissions: Vec<PluginPermission>,
    pub status: PluginStatus,
    pub created_at: i64,
}

impl PluginRecord {
    pub fn has_permission(&self, perm: PluginPermission) -> bool {
        self.granted_permissions.contains(&perm)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginItemRef {
    pub plugin_id: String,
    pub remote_id: String,
}

pub fn plugin_item_path(plugin_id: &str, remote_id: &str) -> PathBuf {
    PathBuf::from(format!("{PLUGIN_SCHEME}{plugin_id}/{remote_id}"))
}

pub fn is_plugin_path(path: &Path) -> bool {
    path.to_str()
        .map(|s| s.starts_with(PLUGIN_SCHEME))
        .unwrap_or(false)
}

/// Parses `plugin://<plugin_id>/<remote_id>`. `remote_id` is everything after the first `/`.
pub fn parse_plugin_path(path: &Path) -> Option<PluginItemRef> {
    let rest = path.to_str()?.strip_prefix(PLUGIN_SCHEME)?;
    let (plugin_id, remote_id) = rest.split_once('/')?;
    if plugin_id.is_empty() || remote_id.is_empty() {
        return None;
    }
    Some(PluginItemRef {
        plugin_id: plugin_id.to_string(),
        remote_id: remote_id.to_string(),
    })
}

pub fn validate_remote_id(remote_id: &str) -> Result<(), String> {
    if remote_id.is_empty() {
        return Err("remote_id must not be empty".to_string());
    }
    if remote_id.len() > 256 {
        return Err("remote_id must be at most 256 bytes".to_string());
    }
    if remote_id.chars().any(|c| c.is_control()) {
        return Err("remote_id must not contain control characters".to_string());
    }
    Ok(())
}

impl MediaItem {
    /// The plugin reference for items published by a plugin, `None` for file-based items.
    pub fn plugin_ref(&self) -> Option<PluginItemRef> {
        parse_plugin_path(&self.file_path)
    }

    /// True when the plugin flagged the item as a live stream (`metadata.extra.live`).
    pub fn is_live(&self) -> bool {
        self.metadata
            .extra
            .get("live")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Live items are never seekable; otherwise `metadata.extra.seekable` (default true).
    pub fn is_seekable(&self) -> bool {
        if self.is_live() {
            return false;
        }
        self.metadata
            .extra
            .get("seekable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    Direct,
    Proxy,
    Transcode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackDescriptor {
    pub mode: PlaybackMode,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub expires_in_secs: Option<u64>,
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Where the transcoder reads its input from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackSource {
    File(PathBuf),
    Url {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub codecs: Vec<String>,
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub max_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub remote_id: String,
    pub user_id: String,
    #[serde(default)]
    pub client: ClientCapabilities,
}
