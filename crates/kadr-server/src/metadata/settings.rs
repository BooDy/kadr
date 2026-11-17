use kadr_storage::repos::SettingsRepository;
use serde::{Deserialize, Serialize};

pub const KEY_TMDB_ENABLED: &str = "tmdb.enabled";
pub const KEY_TMDB_API_KEY: &str = "tmdb.api_key";
pub const KEY_TMDB_LANGUAGE: &str = "tmdb.language";
pub const KEY_TMDB_AUTO_FETCH_ON_SCAN: &str = "tmdb.auto_fetch_on_scan";

pub const DEFAULT_TMDB_LANGUAGE: &str = "en-US";

/// Full TMDb settings loaded from SQLite (`SettingsRepository`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMetadataSettings {
    pub enabled: bool,
    pub auto_fetch_on_scan: bool,
    pub language: String,
    pub api_key: String,
}

impl Default for StoredMetadataSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_fetch_on_scan: false,
            language: DEFAULT_TMDB_LANGUAGE.to_string(),
            api_key: String::new(),
        }
    }
}

impl StoredMetadataSettings {
    pub fn to_response(&self) -> MetadataSettingsResponse {
        let trimmed = self.api_key.trim();
        let api_key_configured = !trimmed.is_empty();
        let api_key_hint = if api_key_configured {
            Some(mask_api_key(trimmed))
        } else {
            None
        };
        MetadataSettingsResponse {
            enabled: self.enabled,
            auto_fetch_on_scan: self.auto_fetch_on_scan,
            language: self.language.clone(),
            api_key_configured,
            api_key_hint,
        }
    }
}

/// Public response for `GET / `PUT /api/v1/admin/settings/metadata` (never exposes raw API key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataSettingsResponse {
    pub enabled: bool,
    pub auto_fetch_on_scan: bool,
    pub language: String,
    pub api_key_configured: bool,
    pub api_key_hint: Option<String>,
}

/// Request body for `PUT /api/v1/admin/settings/metadata`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetadataSettingsUpdate {
    pub enabled: Option<bool>,
    pub auto_fetch_on_scan: Option<bool>,
    pub language: Option<String>,
    pub api_key: Option<String>,
}

/// TMDb authentication credential format (v3 32-hex query key vs v4 JWT Bearer Read Access Token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmdbAuthMode {
    V3ApiKey(String),
    V4Bearer(String),
}

impl TmdbAuthMode {
    pub fn from_raw(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(rest) = trimmed
            .strip_prefix("Bearer ")
            .or_else(|| trimmed.strip_prefix("bearer "))
        {
            let tok = rest.trim();
            if tok.is_empty() {
                return None;
            }
            return Some(Self::V4Bearer(tok.to_string()));
        }
        // TMDb v4 Read Access Tokens are JWTs (`header.payload.signature`, starting with `eyJ`).
        if trimmed.starts_with("eyJ") || (trimmed.split('.').count() == 3 && trimmed.len() > 32) {
            Some(Self::V4Bearer(trimmed.to_string()))
        } else {
            Some(Self::V3ApiKey(trimmed.to_string()))
        }
    }
}

fn mask_api_key(key: &str) -> String {
    let clean = key
        .strip_prefix("Bearer ")
        .or_else(|| key.strip_prefix("bearer "))
        .map(str::trim)
        .unwrap_or(key);
    let chars: Vec<char> = clean.chars().collect();
    if chars.len() >= 8 {
        let last4: String = chars[chars.len() - 4..].iter().collect();
        format!("••••{last4}")
    } else {
        "••••".to_string()
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

pub async fn load_metadata_settings(repo: &SettingsRepository) -> StoredMetadataSettings {
    let mut out = StoredMetadataSettings::default();
    let stored = match repo.get_all().await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!("Could not read stored metadata settings, using defaults: {e}");
            return out;
        }
    };

    if let Some(v) = stored.get(KEY_TMDB_ENABLED).and_then(|s| parse_bool(s)) {
        out.enabled = v;
    }
    if let Some(v) = stored
        .get(KEY_TMDB_AUTO_FETCH_ON_SCAN)
        .and_then(|s| parse_bool(s))
    {
        out.auto_fetch_on_scan = v;
    }
    if let Some(lang) = stored.get(KEY_TMDB_LANGUAGE) {
        let trimmed = lang.trim();
        if !trimmed.is_empty() {
            out.language = trimmed.to_string();
        }
    }
    if let Some(key) = stored.get(KEY_TMDB_API_KEY) {
        out.api_key = key.trim().to_string();
    }
    out
}

pub async fn update_metadata_settings(
    repo: &SettingsRepository,
    update: MetadataSettingsUpdate,
) -> Result<StoredMetadataSettings, String> {
    let mut entries = Vec::new();

    if let Some(enabled) = update.enabled {
        entries.push((KEY_TMDB_ENABLED.to_string(), enabled.to_string()));
    }
    if let Some(auto_fetch) = update.auto_fetch_on_scan {
        entries.push((
            KEY_TMDB_AUTO_FETCH_ON_SCAN.to_string(),
            auto_fetch.to_string(),
        ));
    }
    if let Some(lang) = update.language {
        let trimmed = lang.trim();
        if trimmed.chars().any(|c| c.is_control()) || trimmed.len() > 32 {
            return Err("Invalid TMDb language code".to_string());
        }
        let effective = if trimmed.is_empty() {
            DEFAULT_TMDB_LANGUAGE.to_string()
        } else {
            trimmed.to_string()
        };
        entries.push((KEY_TMDB_LANGUAGE.to_string(), effective));
    }
    if let Some(api_key) = update.api_key {
        let trimmed = api_key.trim();
        if trimmed.chars().any(|c| c == '\r' || c == '\n' || c == '\0') {
            return Err("Invalid characters in API key".to_string());
        }
        entries.push((KEY_TMDB_API_KEY.to_string(), trimmed.to_string()));
    }

    if !entries.is_empty() {
        repo.set_many(&entries)
            .await
            .map_err(|e| format!("Failed to persist metadata settings: {e}"))?;
    }

    Ok(load_metadata_settings(repo).await)
}
