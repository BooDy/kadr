use crate::config::PluginSettings;
use kadr_storage::repos::SettingsRepository;

pub const KEY_ENABLED: &str = "plugins.enabled";
pub const KEY_RESOLVE_TIMEOUT_SECS: &str = "plugins.resolve_timeout_secs";
pub const KEY_ALLOW_PRIVATE_PROXY: &str = "plugins.allow_private_proxy";

pub const MIN_TIMEOUT_SECS: u64 = 1;
pub const MAX_TIMEOUT_SECS: u64 = 60;

/// Partial update of the plugin settings (every field optional).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PluginSettingsUpdate {
    pub enabled: Option<bool>,
    pub resolve_timeout_secs: Option<u64>,
    pub allow_private_proxy: Option<bool>,
}

impl PluginSettingsUpdate {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(secs) = self.resolve_timeout_secs {
            if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&secs) {
                return Err(format!(
                    "resolve_timeout_secs must be between {MIN_TIMEOUT_SECS} and {MAX_TIMEOUT_SECS}"
                ));
            }
        }
        Ok(())
    }

    /// Key/value pairs to persist for the supplied fields only.
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(v) = self.enabled {
            out.push((KEY_ENABLED.to_string(), v.to_string()));
        }
        if let Some(v) = self.resolve_timeout_secs {
            out.push((KEY_RESOLVE_TIMEOUT_SECS.to_string(), v.to_string()));
        }
        if let Some(v) = self.allow_private_proxy {
            out.push((KEY_ALLOW_PRIVATE_PROXY.to_string(), v.to_string()));
        }
        out
    }

    pub fn apply_to(&self, settings: &mut PluginSettings) {
        if let Some(v) = self.enabled {
            settings.enabled = v;
        }
        if let Some(v) = self.resolve_timeout_secs {
            settings.resolve_timeout_secs = v;
        }
        if let Some(v) = self.allow_private_proxy {
            settings.allow_private_proxy = v;
        }
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Starts from `base` (the `kadr.toml` / default values) and applies every valid stored
/// override. Unparseable or out-of-range stored values are ignored with a warning, and a
/// storage failure falls back to `base`.
pub async fn load_plugin_settings(
    repo: &SettingsRepository,
    base: PluginSettings,
) -> PluginSettings {
    let stored = match repo.get_all().await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!("Could not read stored plugin settings, using defaults: {e}");
            return base;
        }
    };
    let mut settings = base;

    if let Some(raw) = stored.get(KEY_ENABLED) {
        match parse_bool(raw) {
            Some(v) => settings.enabled = v,
            None => {
                tracing::warn!(key = KEY_ENABLED, value = %raw, "Ignoring invalid stored setting")
            }
        }
    }
    if let Some(raw) = stored.get(KEY_RESOLVE_TIMEOUT_SECS) {
        match raw.parse::<u64>() {
            Ok(v) if (MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&v) => {
                settings.resolve_timeout_secs = v
            }
            _ => tracing::warn!(
                key = KEY_RESOLVE_TIMEOUT_SECS, value = %raw, "Ignoring invalid stored setting"
            ),
        }
    }
    if let Some(raw) = stored.get(KEY_ALLOW_PRIVATE_PROXY) {
        match parse_bool(raw) {
            Some(v) => settings.allow_private_proxy = v,
            None => tracing::warn!(
                key = KEY_ALLOW_PRIVATE_PROXY, value = %raw, "Ignoring invalid stored setting"
            ),
        }
    }
    settings
}
