use crate::config::{AppConfig, PluginSettings};
use crate::events::EventBus;
use crate::plugins::auth::{derive_plugin_secret, PluginAuth, PluginTokenService};
use crate::plugins::client::{PluginClient, PluginError};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use kadr_core::plugin::{PlaybackDescriptor, PluginPermission, PluginRecord, PluginStatus};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PluginRepository};
use kadr_storage::{StorageError, StoragePool};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Plugins are disabled")]
    Disabled,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Forbidden(String),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Disabled => (StatusCode::NOT_FOUND, "PLUGINS_DISABLED".to_string()),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND".to_string()),
            Self::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT".to_string()),
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "INVALID_REQUEST".to_string()),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN".to_string()),
            Self::Plugin(e) => (e.status(), e.code().to_string()),
            Self::Storage(e) => {
                tracing::error!("Plugin storage error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "STORAGE_ERROR".to_string(),
                )
            }
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL".to_string()),
        };
        let message = match &self {
            Self::Storage(_) => "Database error".to_string(),
            other => other.to_string(),
        };
        (
            status,
            Json(serde_json::json!({ "error": code, "message": message })),
        )
            .into_response()
    }
}

pub(crate) struct CachedDescriptor {
    pub descriptor: PlaybackDescriptor,
    pub expires_at: Instant,
}

struct Inner {
    repo: PluginRepository,
    libs: LibraryRepository,
    media: MediaItemRepository,
    tokens: PluginTokenService,
    server_secret: Vec<u8>,
    config: Arc<RwLock<AppConfig>>,
    events: Arc<EventBus>,
    client: PluginClient,
    /// Used to fetch upstream media in `proxy` mode: never follows redirects, no total timeout.
    proxy_http: reqwest::Client,
    /// Short-lived, in-memory playback descriptor cache keyed by (item id, user id).
    pub(crate) cache: Mutex<HashMap<(i64, String), CachedDescriptor>>,
}

#[derive(Clone)]
pub struct PluginService {
    inner: Arc<Inner>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn normalize_base_url(raw: &str) -> Result<String, ServiceError> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|e| ServiceError::Invalid(format!("invalid base_url: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ServiceError::Invalid(
            "base_url must use http or https".to_string(),
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

impl PluginService {
    pub fn new(
        pool: StoragePool,
        server_secret: &[u8],
        config: Arc<RwLock<AppConfig>>,
        events: Arc<EventBus>,
    ) -> Self {
        let settings = config
            .try_read()
            .map(|c| c.plugins.clone())
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                repo: PluginRepository::new(pool.clone()),
                libs: LibraryRepository::new(pool.clone()),
                media: MediaItemRepository::new(pool),
                tokens: PluginTokenService::new(server_secret),
                server_secret: server_secret.to_vec(),
                config,
                events,
                client: PluginClient::new(Duration::from_secs(
                    settings.resolve_timeout_secs.max(1),
                )),
                proxy_http: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(Duration::from_secs(10))
                    .build()
                    .expect("reqwest client builds with static configuration"),
                cache: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn repo(&self) -> &PluginRepository {
        &self.inner.repo
    }
    pub fn libs(&self) -> &LibraryRepository {
        &self.inner.libs
    }
    pub fn media(&self) -> &MediaItemRepository {
        &self.inner.media
    }
    pub fn events(&self) -> &Arc<EventBus> {
        &self.inner.events
    }
    pub fn client(&self) -> &PluginClient {
        &self.inner.client
    }
    pub fn proxy_client(&self) -> &reqwest::Client {
        &self.inner.proxy_http
    }
    pub(crate) fn cache(&self) -> &Mutex<HashMap<(i64, String), CachedDescriptor>> {
        &self.inner.cache
    }

    pub fn secret_for(&self, plugin_id: &str) -> String {
        derive_plugin_secret(&self.inner.server_secret, plugin_id)
    }

    pub async fn settings(&self) -> PluginSettings {
        self.inner.config.read().await.plugins.clone()
    }

    pub async fn ensure_enabled(&self) -> Result<(), ServiceError> {
        if self.settings().await.enabled {
            Ok(())
        } else {
            Err(ServiceError::Disabled)
        }
    }

    pub async fn list(&self) -> Result<Vec<PluginRecord>, ServiceError> {
        self.ensure_enabled().await?;
        Ok(self.inner.repo.list().await?)
    }

    async fn get_or_404(&self, id: &str) -> Result<PluginRecord, ServiceError> {
        self.inner
            .repo
            .get(id)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("plugin '{id}' not found")))
    }

    /// Fetches and validates the manifest, then stores the plugin as `pending`.
    /// Returns the record and the derived server-to-plugin secret.
    pub async fn register(&self, base_url: &str) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let base_url = normalize_base_url(base_url)?;
        let manifest = self.inner.client.fetch_manifest(&base_url).await?;
        manifest.validate().map_err(ServiceError::Invalid)?;
        if self.inner.repo.get(&manifest.id).await?.is_some() {
            return Err(ServiceError::Conflict(format!(
                "plugin '{}' is already registered",
                manifest.id
            )));
        }
        let record = PluginRecord {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            base_url,
            manifest,
            granted_permissions: vec![],
            status: PluginStatus::Pending,
            created_at: now_secs(),
        };
        self.inner.repo.insert(&record).await?;
        let secret = self.secret_for(&record.id);
        Ok((record, secret))
    }

    /// Grants permissions (default: everything the manifest requested), enables the plugin
    /// and issues its plugin-to-server token.
    pub async fn approve(
        &self,
        id: &str,
        requested: Option<Vec<PluginPermission>>,
    ) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        let mut granted = match requested {
            None => record.manifest.permissions.clone(),
            Some(perms) => {
                if let Some(bad) = perms
                    .iter()
                    .find(|p| !record.manifest.permissions.contains(p))
                {
                    return Err(ServiceError::Invalid(format!(
                        "permission '{}' was not requested in the manifest",
                        bad.as_str()
                    )));
                }
                perms
            }
        };
        granted.sort_by_key(|p| p.as_str());
        granted.dedup();

        self.inner.repo.set_granted(id, &granted).await?;
        self.inner
            .repo
            .set_status(id, PluginStatus::Enabled)
            .await?;
        let token = self
            .inner
            .tokens
            .issue(id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))?;
        Ok((self.get_or_404(id).await?, token))
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        if record.status == PluginStatus::Pending {
            return Err(ServiceError::Invalid(
                "plugin has not been approved yet".to_string(),
            ));
        }
        let status = if enabled {
            PluginStatus::Enabled
        } else {
            PluginStatus::Disabled
        };
        self.inner.repo.set_status(id, status).await?;
        self.get_or_404(id).await
    }

    pub async fn remove(&self, id: &str, purge: bool) -> Result<(), ServiceError> {
        self.ensure_enabled().await?;
        self.get_or_404(id).await?;
        if purge {
            self.inner.libs.delete_by_plugin(id).await?;
        }
        self.inner.repo.delete(id).await?;
        Ok(())
    }

    /// Authorizes a plugin-initiated call: the plugin must exist, be enabled (or unhealthy)
    /// and hold `perm`.
    pub async fn authorize(
        &self,
        auth: &PluginAuth,
        perm: PluginPermission,
    ) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        let record = self
            .inner
            .repo
            .get(&auth.plugin_id)
            .await?
            .ok_or_else(|| ServiceError::Forbidden("plugin is not registered".to_string()))?;
        if !matches!(
            record.status,
            PluginStatus::Enabled | PluginStatus::Unhealthy
        ) {
            return Err(ServiceError::Forbidden("plugin is not enabled".to_string()));
        }
        if !record.has_permission(perm) {
            return Err(ServiceError::Forbidden(format!(
                "missing permission '{}'",
                perm.as_str()
            )));
        }
        Ok(record)
    }
}
