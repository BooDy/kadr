pub mod artwork;
pub mod settings;
pub mod tmdb;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use kadr_core::events::SystemEvent;
use kadr_core::models::{MediaItem, MediaType};
use kadr_storage::repos::{MediaItemRepository, SettingsRepository};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::events::EventBus;

pub use artwork::{ArtworkDownloader, ArtworkError, ArtworkSlot, MAX_ARTWORK_BYTES};
pub use settings::{
    load_metadata_settings, update_metadata_settings, MetadataSettingsResponse,
    MetadataSettingsUpdate, StoredMetadataSettings, TmdbAuthMode,
};
pub use tmdb::{TmdbClient, TmdbEnrichedDetails, TmdbError, TmdbMediaType, TmdbSearchMatch};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrichLibraryResult {
    pub scanned: usize,
    pub matched: usize,
    pub skipped: usize,
    pub errors: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataServiceError {
    #[error("TMDb integration is disabled or API key is not configured")]
    TmdbNotConfigured,
    #[error(transparent)]
    Tmdb(#[from] TmdbError),
    #[error(transparent)]
    Artwork(#[from] ArtworkError),
    #[error("Database error: {0}")]
    Storage(String),
}

#[derive(Clone)]
pub struct MetadataService {
    settings_repo: SettingsRepository,
    media_repo: MediaItemRepository,
    tmdb_client: TmdbClient,
    artwork_downloader: ArtworkDownloader,
    config: Arc<RwLock<AppConfig>>,
}

impl MetadataService {
    pub fn new(media_repo: MediaItemRepository, config: Arc<RwLock<AppConfig>>) -> Self {
        let settings_repo = SettingsRepository::new(media_repo.pool().clone());
        let (artwork_dir, allow_private) = config
            .try_read()
            .map(|c| {
                (
                    c.server.data_dir.join("artwork"),
                    c.plugins.allow_private_proxy,
                )
            })
            .unwrap_or_else(|_| (PathBuf::from("./data/artwork"), false));
        Self {
            settings_repo,
            media_repo,
            tmdb_client: TmdbClient::new(),
            artwork_downloader: ArtworkDownloader::new(artwork_dir, allow_private),
            config,
        }
    }

    pub fn with_tmdb_client(mut self, client: TmdbClient) -> Self {
        self.tmdb_client = client;
        self
    }

    pub fn with_artwork_downloader(mut self, downloader: ArtworkDownloader) -> Self {
        self.artwork_downloader = downloader;
        self
    }

    pub fn settings_repo(&self) -> &SettingsRepository {
        &self.settings_repo
    }

    pub fn media_repo(&self) -> &MediaItemRepository {
        &self.media_repo
    }

    pub fn tmdb_client(&self) -> &TmdbClient {
        &self.tmdb_client
    }

    pub fn artwork_downloader(&self) -> &ArtworkDownloader {
        &self.artwork_downloader
    }

    pub async fn allow_private_artwork(&self) -> bool {
        self.artwork_downloader.allow_private_hosts()
            || self.config.read().await.plugins.allow_private_proxy
    }

    pub async fn download_artwork(
        &self,
        item_id: i64,
        slot: ArtworkSlot,
        url: &str,
    ) -> Result<PathBuf, ArtworkError> {
        let allow_private = self.allow_private_artwork().await;
        self.artwork_downloader
            .download_and_save(item_id, slot, url, allow_private)
            .await
    }

    pub async fn load_active_tmdb_settings(
        &self,
    ) -> Result<StoredMetadataSettings, MetadataServiceError> {
        let settings = load_metadata_settings(&self.settings_repo).await;
        if !settings.enabled || settings.api_key.trim().is_empty() {
            return Err(MetadataServiceError::TmdbNotConfigured);
        }
        Ok(settings)
    }

    pub async fn search_tmdb(
        &self,
        media_type: TmdbMediaType,
        query: &str,
        year: Option<i32>,
    ) -> Result<Vec<TmdbSearchMatch>, MetadataServiceError> {
        let settings = self.load_active_tmdb_settings().await?;
        Ok(self
            .tmdb_client
            .search(
                &settings.api_key,
                &settings.language,
                media_type,
                query,
                year,
            )
            .await?)
    }

    pub async fn apply_tmdb_to_item(
        &self,
        item_id: i64,
        tmdb_id: i64,
        media_type: TmdbMediaType,
        download_artwork: bool,
    ) -> Result<Option<MediaItem>, MetadataServiceError> {
        let settings = self.load_active_tmdb_settings().await?;
        self.apply_tmdb_with_settings(&settings, item_id, tmdb_id, media_type, download_artwork)
            .await
    }

    async fn apply_tmdb_with_settings(
        &self,
        settings: &StoredMetadataSettings,
        item_id: i64,
        tmdb_id: i64,
        media_type: TmdbMediaType,
        download_artwork: bool,
    ) -> Result<Option<MediaItem>, MetadataServiceError> {
        let mut enriched = self
            .tmdb_client
            .fetch_details(&settings.api_key, &settings.language, media_type, tmdb_id)
            .await?;

        if download_artwork {
            if let Some(ref poster_url) = enriched.poster_url {
                match self
                    .download_artwork(item_id, ArtworkSlot::Poster, poster_url)
                    .await
                {
                    Ok(path) => {
                        enriched.patch.poster_path =
                            Some(Some(path.to_string_lossy().into_owned()));
                    }
                    Err(err) => {
                        tracing::warn!(
                            item_id,
                            tmdb_id,
                            error = %err,
                            "Failed to download TMDb poster artwork"
                        );
                    }
                }
            }
            if let Some(ref backdrop_url) = enriched.backdrop_url {
                match self
                    .download_artwork(item_id, ArtworkSlot::Backdrop, backdrop_url)
                    .await
                {
                    Ok(path) => {
                        enriched.patch.backdrop_path =
                            Some(Some(path.to_string_lossy().into_owned()));
                    }
                    Err(err) => {
                        tracing::warn!(
                            item_id,
                            tmdb_id,
                            error = %err,
                            "Failed to download TMDb backdrop artwork"
                        );
                    }
                }
            }
        }

        self.media_repo
            .update_metadata(item_id, &enriched.patch)
            .await
            .map_err(|e| MetadataServiceError::Storage(e.to_string()))
    }

    pub async fn enrich_library(
        &self,
        library_id: &str,
    ) -> Result<EnrichLibraryResult, MetadataServiceError> {
        let settings = self.load_active_tmdb_settings().await?;
        let items = self
            .media_repo
            .list_all_by_library(library_id)
            .await
            .map_err(|e| MetadataServiceError::Storage(e.to_string()))?;

        let scanned = items.len();
        let mut matched = 0usize;
        let mut skipped = 0usize;
        let mut errors = 0usize;

        for item in items {
            let Some(item_id) = item.id else {
                skipped += 1;
                continue;
            };
            if !is_item_eligible_for_auto_enrich(&item) {
                skipped += 1;
                continue;
            }
            let tmdb_type = TmdbMediaType::from_media_type(item.item_type);
            match self
                .tmdb_client
                .search(
                    &settings.api_key,
                    &settings.language,
                    tmdb_type,
                    &item.title,
                    item.release_year,
                )
                .await
            {
                Ok(results) => {
                    if let Some(top) = results.first() {
                        match self
                            .apply_tmdb_with_settings(
                                &settings,
                                item_id,
                                top.tmdb_id,
                                tmdb_type,
                                true,
                            )
                            .await
                        {
                            Ok(Some(_)) => matched += 1,
                            Ok(None) => skipped += 1,
                            Err(err) => {
                                tracing::warn!(
                                    item_id,
                                    title = %item.title,
                                    error = %err,
                                    "Failed to apply TMDb match during library enrichment"
                                );
                                errors += 1;
                            }
                        }
                    } else {
                        skipped += 1;
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        item_id,
                        title = %item.title,
                        error = %err,
                        "TMDb search failed during library enrichment"
                    );
                    errors += 1;
                }
            }
        }

        Ok(EnrichLibraryResult {
            scanned,
            matched,
            skipped,
            errors,
        })
    }

    /// Background helper invoked after `IngestWorker` upserts when `tmdb.enabled && tmdb.auto_fetch_on_scan`.
    pub async fn enrich_ingested_batch(&self, raw_items: &[MediaItem], event_bus: &EventBus) {
        let settings = load_metadata_settings(&self.settings_repo).await;
        if !settings.enabled || !settings.auto_fetch_on_scan || settings.api_key.trim().is_empty() {
            return;
        }

        let path_refs: Vec<&std::path::Path> =
            raw_items.iter().map(|i| i.file_path.as_path()).collect();
        let mut candidates: Vec<MediaItem> = match self.media_repo.find_by_paths(&path_refs).await {
            Ok(map) => map.into_values().collect(),
            Err(err) => {
                tracing::warn!(error = %err, "Failed to resolve ingested items for TMDb auto-fetch");
                return;
            }
        };

        // Also include parent Show records created for episodes in this batch.
        let mut checked_shows = HashSet::new();
        for raw in raw_items {
            if let Some(ref series) = raw.metadata.series_title {
                let key = (raw.library_id.clone(), series.trim().to_lowercase());
                if !series.trim().is_empty() && checked_shows.insert(key) {
                    if let Ok(Some(show)) = self
                        .media_repo
                        .find_show_by_title(&raw.library_id, series.trim())
                        .await
                    {
                        candidates.push(show);
                    }
                }
            }
        }

        let mut updated_libs = HashSet::new();
        let mut seen_ids = HashSet::new();
        for item in candidates {
            let Some(item_id) = item.id else { continue };
            if !seen_ids.insert(item_id) || !is_item_eligible_for_auto_enrich(&item) {
                continue;
            }
            let tmdb_type = TmdbMediaType::from_media_type(item.item_type);
            if let Ok(results) = self
                .tmdb_client
                .search(
                    &settings.api_key,
                    &settings.language,
                    tmdb_type,
                    &item.title,
                    item.release_year,
                )
                .await
            {
                if let Some(top) = results.first() {
                    if let Ok(Some(_)) = self
                        .apply_tmdb_with_settings(&settings, item_id, top.tmdb_id, tmdb_type, true)
                        .await
                    {
                        updated_libs.insert(item.library_id.clone());
                    }
                }
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        for lib_id in updated_libs {
            let count = self.media_repo.count_by_library(&lib_id).await.unwrap_or(0);
            event_bus.publish(SystemEvent::LibraryUpdated {
                library_id: lib_id,
                item_count: count,
                timestamp: now,
            });
        }
    }
}

fn is_item_eligible_for_auto_enrich(item: &MediaItem) -> bool {
    if item.title.trim().is_empty() {
        return false;
    }
    if item.metadata.extra.contains_key("tmdb_id") {
        return false;
    }
    if !item.locked_fields.is_empty() {
        return false;
    }
    matches!(
        item.item_type,
        MediaType::Movie | MediaType::Show | MediaType::Anime | MediaType::Unknown
    )
}
