use serde::{Deserialize, Serialize};

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
