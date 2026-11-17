use crate::config::{AppConfig, PluginSettings};
use crate::events::EventBus;
use crate::plugins::auth::{derive_plugin_secret, PluginAuth, PluginTokenService};
use crate::plugins::client::{PluginClient, PluginError};
use crate::plugins::forge::{ForgeClient, ForgeError};
use crate::plugins::installer::{plugin_paths, remove_plugin_files, unpack_bundle};
use crate::plugins::settings::PluginSettingsUpdate;
use crate::plugins::supervisor::{
    kadr_local_url, OnPortChanged, PluginSupervisor, SpawnSpec, StartOutcome,
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use kadr_core::events::SystemEvent;
use kadr_core::plugin::{
    ClientCapabilities, ForgeKind, PlaybackDescriptor, PluginInstallKind, PluginPermission,
    PluginRecord, PluginStatus,
};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PluginRepository, SettingsRepository,
};
use kadr_storage::{StorageError, StoragePool};
use std::collections::HashMap;
use std::path::PathBuf;
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
    #[error("{0}")]
    ChecksumMismatch(String),
    #[error("{0}")]
    ForgeError(String),
    #[error("Forge request timed out")]
    ForgeTimeout,
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Internal(String),
}

impl From<ForgeError> for ServiceError {
    fn from(e: ForgeError) -> Self {
        match e {
            ForgeError::InvalidRequest(m) => Self::Invalid(m),
            ForgeError::ChecksumMismatch(m) => Self::ChecksumMismatch(m),
            ForgeError::ForgeError(m) => Self::ForgeError(m),
            ForgeError::Timeout => Self::ForgeTimeout,
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Disabled => (StatusCode::NOT_FOUND, "PLUGINS_DISABLED".to_string()),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND".to_string()),
            Self::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT".to_string()),
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "INVALID_REQUEST".to_string()),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN".to_string()),
            Self::ChecksumMismatch(_) => (StatusCode::BAD_GATEWAY, "CHECKSUM_MISMATCH".to_string()),
            Self::ForgeError(_) => (StatusCode::BAD_GATEWAY, "FORGE_ERROR".to_string()),
            Self::ForgeTimeout => (StatusCode::GATEWAY_TIMEOUT, "FORGE_TIMEOUT".to_string()),
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateOutcome {
    pub updated: bool,
    pub previous_version: String,
    pub requires_approval: bool,
    pub plugin: PluginRecord,
}

pub(crate) struct CachedDescriptor {
    pub descriptor: PlaybackDescriptor,
    pub expires_at: Instant,
}

struct Inner {
    repo: PluginRepository,
    libs: LibraryRepository,
    settings_repo: SettingsRepository,
    media: MediaItemRepository,
    tokens: PluginTokenService,
    server_secret: Vec<u8>,
    config: Arc<RwLock<AppConfig>>,
    events: Arc<EventBus>,
    client: PluginClient,
    forge: ForgeClient,
    supervisor: PluginSupervisor,
    op_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Used to fetch upstream media in `proxy` mode: never follows redirects, no total timeout.
    proxy_http: reqwest::Client,
    /// Short-lived, in-memory playback descriptor cache keyed by (item id, user id, client capabilities).
    pub(crate) cache: Mutex<HashMap<(i64, String, ClientCapabilities), CachedDescriptor>>,
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
        let timeout = Duration::from_secs(settings.resolve_timeout_secs.max(1));
        Self {
            inner: Arc::new(Inner {
                repo: PluginRepository::new(pool.clone()),
                libs: LibraryRepository::new(pool.clone()),
                settings_repo: SettingsRepository::new(pool.clone()),
                media: MediaItemRepository::new(pool),
                tokens: PluginTokenService::new(server_secret),
                server_secret: server_secret.to_vec(),
                config,
                events,
                client: PluginClient::new(timeout),
                forge: ForgeClient::new(timeout),
                supervisor: PluginSupervisor::new(),
                op_locks: Mutex::new(HashMap::new()),
                proxy_http: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(Duration::from_secs(10))
                    .build()
                    .expect("reqwest client builds with static configuration"),
                cache: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn with_forge_client(mut self, forge: ForgeClient) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_forge_client must be called before cloning PluginService")
            .forge = forge;
        self
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
    pub fn supervisor(&self) -> &PluginSupervisor {
        &self.inner.supervisor
    }
    pub fn proxy_client(&self) -> &reqwest::Client {
        &self.inner.proxy_http
    }
    pub(crate) fn cache(
        &self,
    ) -> &Mutex<HashMap<(i64, String, ClientCapabilities), CachedDescriptor>> {
        &self.inner.cache
    }

    pub async fn plugins_dir(&self) -> PathBuf {
        self.inner
            .config
            .read()
            .await
            .server
            .data_dir
            .join("plugins")
    }

    pub fn secret_for(&self, plugin_id: &str) -> String {
        derive_plugin_secret(&self.inner.server_secret, plugin_id)
    }

    async fn lock_for(&self, id: &str) -> Arc<Mutex<()>> {
        let mut map = self.inner.op_locks.lock().await;
        map.entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
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

    pub async fn get_settings(&self) -> PluginSettings {
        self.settings().await
    }

    /// Persists the supplied keys first, then applies them to the live config. A failed
    /// write leaves the in-memory config unchanged.
    pub async fn update_settings(
        &self,
        update: PluginSettingsUpdate,
    ) -> Result<PluginSettings, ServiceError> {
        update.validate().map_err(ServiceError::Invalid)?;
        let was_enabled = self.settings().await.enabled;
        self.inner.settings_repo.set_many(&update.entries()).await?;
        let applied = {
            let mut cfg = self.inner.config.write().await;
            update.apply_to(&mut cfg.plugins);
            cfg.plugins.clone()
        };
        self.apply_timeout(&applied);
        if was_enabled && !applied.enabled {
            self.shutdown_managed_plugins().await;
        } else if !was_enabled && applied.enabled {
            self.start_managed_plugins().await;
        }
        Ok(applied)
    }

    fn apply_timeout(&self, settings: &PluginSettings) {
        let timeout = Duration::from_secs(settings.resolve_timeout_secs.max(1));
        self.inner.client.set_timeout(timeout);
        self.inner.forge.set_timeout(timeout);
    }

    /// Re-reads `resolve_timeout_secs` from the live config and pushes it into the client.
    /// Called before every outbound plugin call so every service instance stays current.
    pub(crate) async fn sync_client_timeout(&self) {
        let settings = self.settings().await;
        self.apply_timeout(&settings);
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

    async fn spawn_managed(&self, record: &PluginRecord) -> Result<(), ServiceError> {
        let (plugins_dir, kadr_url, timeout_secs) = {
            let cfg = self.inner.config.read().await;
            (
                cfg.server.data_dir.join("plugins"),
                kadr_local_url(&cfg.server.host, cfg.server.port),
                cfg.plugins.resolve_timeout_secs.max(1),
            )
        };
        let paths = plugin_paths(&plugins_dir, &record.id);
        let token = self
            .inner
            .tokens
            .issue(&record.id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))?;
        let secret = self.secret_for(&record.id);
        let repo = self.inner.repo.clone();
        let on_port: OnPortChanged = Arc::new(move |id, url| {
            let repo = repo.clone();
            tokio::spawn(async move {
                let _ = repo.set_base_url(&id, &url).await;
            });
        });

        let outcome = self
            .inner
            .supervisor
            .start(
                SpawnSpec {
                    plugin_id: record.id.clone(),
                    bin_path: paths.bin_path,
                    data_dir: paths.data_dir,
                    kadr_url,
                    secret,
                    token,
                },
                Duration::from_secs(timeout_secs),
                Some(on_port),
            )
            .await;

        match outcome {
            StartOutcome::Healthy { base_url } => {
                self.inner.repo.set_base_url(&record.id, &base_url).await?;
                self.inner
                    .repo
                    .set_status(&record.id, PluginStatus::Enabled)
                    .await?;
            }
            StartOutcome::Unhealthy { base_url, reason } => {
                self.inner.repo.set_base_url(&record.id, &base_url).await?;
                self.inner
                    .repo
                    .set_status(&record.id, PluginStatus::Unhealthy)
                    .await?;
                self.inner.events.publish(SystemEvent::PluginError {
                    plugin_id: record.id.clone(),
                    message: reason,
                    timestamp: now_secs(),
                });
            }
        }
        Ok(())
    }

    /// Fetches and validates the manifest, then stores the plugin as `pending`.
    /// Returns the record and the derived server-to-plugin secret.
    pub async fn register(&self, base_url: &str) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        self.sync_client_timeout().await;
        let base_url = normalize_base_url(base_url)?;
        let manifest = self.inner.client.fetch_manifest(&base_url).await?;
        manifest.validate().map_err(ServiceError::Invalid)?;

        let lock = self.lock_for(&manifest.id).await;
        let _guard = lock.lock().await;

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
            install_kind: PluginInstallKind::External,
            repo_url: None,
            forge_kind: None,
            installed_tag: None,
        };
        self.inner.repo.insert(&record).await?;
        let secret = self.secret_for(&record.id);
        Ok((record, secret))
    }

    /// Downloads a prebuilt release bundle from GitHub, GitLab, or Gitea, unpacks it into
    /// `<data_dir>/plugins/<id>/`, and registers the plugin as `pending` WITHOUT executing the binary.
    pub async fn install_from_git(
        &self,
        repo_url: &str,
        tag: Option<&str>,
        forge: Option<ForgeKind>,
    ) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        self.sync_client_timeout().await;
        let settings = self.settings().await;
        let plugins_dir = self.plugins_dir().await;

        let bundle = self
            .inner
            .forge
            .fetch_release_bundle(repo_url, tag, forge, settings.allow_private_proxy)
            .await?;
        let staged =
            unpack_bundle(&plugins_dir, &bundle.archive_bytes).map_err(ServiceError::Invalid)?;
        let manifest = staged.manifest().clone();

        let lock = self.lock_for(&manifest.id).await;
        let _guard = lock.lock().await;

        if self.inner.repo.get(&manifest.id).await?.is_some() {
            return Err(ServiceError::Conflict(format!(
                "plugin '{}' is already registered",
                manifest.id
            )));
        }
        staged
            .commit_fresh(&plugins_dir)
            .map_err(ServiceError::Internal)?;

        let record = PluginRecord {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            base_url: "http://127.0.0.1:0".to_string(),
            manifest,
            granted_permissions: vec![],
            status: PluginStatus::Pending,
            created_at: now_secs(),
            install_kind: PluginInstallKind::Managed,
            repo_url: Some(bundle.canonical_repo_url),
            forge_kind: Some(bundle.forge_kind),
            installed_tag: Some(bundle.tag_name),
        };
        self.inner.repo.insert(&record).await?;
        Ok(record)
    }

    pub async fn update_managed(
        &self,
        id: &str,
        tag: Option<&str>,
    ) -> Result<UpdateOutcome, ServiceError> {
        self.ensure_enabled().await?;
        self.sync_client_timeout().await;
        let lock = self.lock_for(id).await;
        let _guard = lock.lock().await;

        let existing = self.get_or_404(id).await?;
        if existing.install_kind != PluginInstallKind::Managed {
            return Err(ServiceError::Invalid(
                "only Git-installed managed plugins can be updated".to_string(),
            ));
        }
        let repo_url = existing.repo_url.clone().ok_or_else(|| {
            ServiceError::Invalid("managed plugin has no repository URL".to_string())
        })?;
        let settings = self.settings().await;
        let bundle = self
            .inner
            .forge
            .fetch_release_bundle(
                &repo_url,
                tag,
                existing.forge_kind,
                settings.allow_private_proxy,
            )
            .await?;

        let previous_version = existing.manifest.version.clone();
        let explicit_tag = tag.map(str::trim).filter(|t| !t.is_empty());
        if explicit_tag.is_none()
            && existing.installed_tag.as_deref() == Some(bundle.tag_name.as_str())
        {
            return Ok(UpdateOutcome {
                updated: false,
                previous_version,
                requires_approval: false,
                plugin: existing,
            });
        }

        let plugins_dir = self.plugins_dir().await;
        let staged =
            unpack_bundle(&plugins_dir, &bundle.archive_bytes).map_err(ServiceError::Invalid)?;
        let new_manifest = staged.manifest().clone();
        if new_manifest.id != existing.id {
            return Err(ServiceError::Invalid(format!(
                "updated manifest id '{}' does not match installed plugin '{}'",
                new_manifest.id, existing.id
            )));
        }

        self.inner.supervisor.stop(id).await;
        staged
            .commit_update(&plugins_dir, id)
            .map_err(ServiceError::Internal)?;
        self.inner
            .repo
            .update_release(id, &new_manifest.name, &new_manifest, &bundle.tag_name)
            .await?;

        let needs_new_perms = existing.status == PluginStatus::Pending
            || new_manifest
                .permissions
                .iter()
                .any(|p| !existing.granted_permissions.contains(p));

        if needs_new_perms {
            self.inner
                .repo
                .set_status(id, PluginStatus::Pending)
                .await?;
            return Ok(UpdateOutcome {
                updated: true,
                previous_version,
                requires_approval: true,
                plugin: self.get_or_404(id).await?,
            });
        }

        let refreshed = self.get_or_404(id).await?;
        if matches!(
            existing.status,
            PluginStatus::Enabled | PluginStatus::Unhealthy
        ) {
            self.spawn_managed(&refreshed).await?;
        }
        Ok(UpdateOutcome {
            updated: true,
            previous_version,
            requires_approval: false,
            plugin: self.get_or_404(id).await?,
        })
    }

    pub async fn start_managed_plugins(&self) {
        if self.ensure_enabled().await.is_err() {
            return;
        }
        let Ok(records) = self.inner.repo.list().await else {
            return;
        };
        for rec in records {
            if rec.install_kind == PluginInstallKind::Managed
                && matches!(rec.status, PluginStatus::Enabled | PluginStatus::Unhealthy)
            {
                let lock = self.lock_for(&rec.id).await;
                let _guard = lock.lock().await;
                if let Err(e) = self.spawn_managed(&rec).await {
                    tracing::error!(plugin = %rec.id, "Failed to start managed plugin: {e}");
                }
            }
        }
    }

    pub async fn shutdown_managed_plugins(&self) {
        self.inner.supervisor.stop_all().await;
    }

    /// Grants permissions (default: everything the manifest requested), enables the plugin
    /// and issues its plugin-to-server token.
    pub async fn approve(
        &self,
        id: &str,
        requested: Option<Vec<PluginPermission>>,
    ) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let lock = self.lock_for(id).await;
        let _guard = lock.lock().await;

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

        if record.install_kind == PluginInstallKind::Managed {
            let refreshed = self.get_or_404(id).await?;
            self.spawn_managed(&refreshed).await?;
        }

        let token = self
            .inner
            .tokens
            .issue(id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))?;
        Ok((self.get_or_404(id).await?, token))
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<PluginRecord, ServiceError> {
        self.ensure_enabled().await?;
        let lock = self.lock_for(id).await;
        let _guard = lock.lock().await;

        let record = self.get_or_404(id).await?;
        if record.status == PluginStatus::Pending {
            return Err(ServiceError::Invalid(
                "plugin has not been approved yet".to_string(),
            ));
        }
        if record.install_kind == PluginInstallKind::Managed {
            if enabled {
                self.spawn_managed(&record).await?;
            } else {
                self.inner.supervisor.stop(id).await;
                self.inner
                    .repo
                    .set_status(id, PluginStatus::Disabled)
                    .await?;
            }
        } else {
            let status = if enabled {
                PluginStatus::Enabled
            } else {
                PluginStatus::Disabled
            };
            self.inner.repo.set_status(id, status).await?;
        }
        self.get_or_404(id).await
    }

    /// Connection details an admin copies into the plugin process. The secret is derived, so it
    /// can be shown again at any time (including before approval).
    pub async fn connection(&self, id: &str) -> Result<(PluginRecord, String), ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        let secret = self.secret_for(&record.id);
        Ok((record, secret))
    }

    /// Issues a fresh plugin-to-server token for an approved plugin.
    pub async fn issue_token(&self, id: &str) -> Result<String, ServiceError> {
        self.ensure_enabled().await?;
        let record = self.get_or_404(id).await?;
        if record.status == PluginStatus::Pending {
            return Err(ServiceError::Invalid(
                "plugin has not been approved yet".to_string(),
            ));
        }
        self.inner
            .tokens
            .issue(&record.id)
            .map_err(|e| ServiceError::Internal(format!("failed to issue token: {e}")))
    }

    pub async fn remove(&self, id: &str, purge: bool) -> Result<(), ServiceError> {
        self.ensure_enabled().await?;
        let lock = self.lock_for(id).await;
        let _guard = lock.lock().await;

        let record = self.get_or_404(id).await?;
        if record.install_kind == PluginInstallKind::Managed {
            self.inner.supervisor.stop(id).await;
        }
        if purge {
            self.inner.libs.delete_by_plugin(id).await?;
        }
        self.inner.repo.delete(id).await?;
        if record.install_kind == PluginInstallKind::Managed {
            let _ = remove_plugin_files(&self.plugins_dir().await, id, purge);
        }
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
