use crate::sidecars::{DiscoveredSubtitle, SidecarScanner};
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use kadr_core::subtitles::SubtitleTrack;
use kadr_storage::repos::{MediaItemRepository, SubtitleRepository};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info};

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum IngestMessage {
    Upsert(MediaItem, Vec<DiscoveredSubtitle>),
    Delete(PathBuf),
}

pub struct IngestWorker {
    rx: mpsc::Receiver<IngestMessage>,
    repo: MediaItemRepository,
    subtitle_repo: Option<SubtitleRepository>,
    event_callback: Option<Arc<dyn Fn(kadr_core::events::SystemEvent) + Send + Sync>>,
}

impl IngestWorker {
    pub fn new(rx: mpsc::Receiver<IngestMessage>, repo: MediaItemRepository) -> Self {
        Self {
            rx,
            repo,
            subtitle_repo: None,
            event_callback: None,
        }
    }

    pub fn with_subtitles(mut self, subtitle_repo: SubtitleRepository) -> Self {
        self.subtitle_repo = Some(subtitle_repo);
        self
    }

    pub fn with_event_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(kadr_core::events::SystemEvent) + Send + Sync + 'static,
    {
        self.event_callback = Some(Arc::new(callback));
        self
    }

    pub fn new_with_subtitles(
        rx: mpsc::Receiver<IngestMessage>,
        repo: MediaItemRepository,
        subtitle_repo: SubtitleRepository,
    ) -> Self {
        Self {
            rx,
            repo,
            subtitle_repo: Some(subtitle_repo),
            event_callback: None,
        }
    }

    pub async fn run(mut self) {
        let mut batch: Vec<(MediaItem, Vec<DiscoveredSubtitle>)> = Vec::with_capacity(50);

        loop {
            tokio::select! {
                maybe_msg = self.rx.recv() => {
                    match maybe_msg {
                        Some(IngestMessage::Upsert(item, subs)) => {
                            batch.push((item, subs));
                            if batch.len() >= 50 {
                                self.flush_batch(&mut batch).await;
                            }
                        }
                        Some(IngestMessage::Delete(path)) => {
                            batch.retain(|(item, _)| item.file_path != path);
                            if let Err(e) = self.repo.delete_by_path(&path).await {
                                error!(path = ?path, error = ?e, "Failed to delete removed media file");
                            }
                        }
                        None => {
                            // Channel closed, flush remaining items and exit
                            if !batch.is_empty() {
                                self.flush_batch(&mut batch).await;
                            }
                            break;
                        }
                    }
                }
                _ = sleep(Duration::from_millis(100)), if !batch.is_empty() => {
                    self.flush_batch(&mut batch).await;
                }
            }
        }
    }

    async fn flush_batch(&self, batch: &mut Vec<(MediaItem, Vec<DiscoveredSubtitle>)>) {
        if batch.is_empty() {
            return;
        }

        // Collect distinct series from episodes in this batch
        // Key by (library_id, lowercase series_title) to normalize case differences
        let mut distinct_series: HashMap<(String, String), (String, MediaItem)> = HashMap::new();
        for (item, _) in batch.iter() {
            if item.item_type == MediaType::Episode || item.metadata.series_title.is_some() {
                if let Some(ref series_title) = item.metadata.series_title {
                    let trimmed = series_title.trim();
                    if !trimmed.is_empty() {
                        distinct_series
                            .entry((item.library_id.clone(), trimmed.to_lowercase()))
                            .or_insert_with(|| (trimmed.to_string(), item.clone()));
                    }
                }
            }
        }

        // For each series_title, ensure a MediaType::Show record exists in that library
        let mut queued_series_titles: HashSet<(String, String)> = HashSet::new();
        let mut parent_shows_to_create = Vec::new();
        for ((lib_id, lower_series), (series_title, ep)) in distinct_series {
            if !queued_series_titles.insert((lib_id.clone(), lower_series)) {
                continue;
            }

            match self.repo.find_show_by_title(&lib_id, &series_title).await {
                Ok(Some(_)) => {
                    // Show record already exists
                }
                Ok(None) => {
                    let series_folder = determine_series_folder(&ep.file_path, &series_title);
                    if !series_folder.as_os_str().is_empty() {
                        if let Ok(Some(_)) = self.repo.find_by_path(&series_folder).await {
                            continue;
                        }
                    }

                    let (poster_path, backdrop_path) =
                        find_series_artwork(&series_folder, &ep.metadata);
                    let meta = MediaMetadata {
                        poster_path,
                        backdrop_path,
                        ..Default::default()
                    };

                    let show_item = MediaItem {
                        id: None,
                        library_id: lib_id,
                        item_type: MediaType::Show,
                        title: series_title,
                        original_title: None,
                        release_year: ep.release_year,
                        added_at: ep.added_at,
                        file_path: series_folder,
                        file_name: String::new(),
                        file_size: 0,
                        technical: TechnicalInfo::default(),
                        metadata: meta,
                    };
                    parent_shows_to_create.push(show_item);
                }
                Err(e) => {
                    error!(
                        library_id = %lib_id,
                        series = %series_title,
                        error = ?e,
                        "Failed to check existing show record"
                    );
                }
            }
        }

        if !parent_shows_to_create.is_empty() {
            if let Err(e) = self.repo.upsert_batch(&parent_shows_to_create).await {
                error!(error = ?e, "Failed to insert parent show records");
            }
        }

        let items: Vec<MediaItem> = batch.iter().map(|(item, _)| item.clone()).collect();
        match self.repo.upsert_batch(&items).await {
            Ok(count) => {
                info!(count = count, "Successfully ingested media batch");
                if let Some(ref cb) = self.event_callback {
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let mut counts_by_lib: HashMap<&str, usize> = HashMap::new();
                    for item in &items {
                        *counts_by_lib.entry(&item.library_id).or_insert(0) += 1;
                    }
                    for (lib_id, item_count) in counts_by_lib {
                        cb(kadr_core::events::SystemEvent::LibraryUpdated {
                            library_id: lib_id.to_string(),
                            item_count,
                            timestamp: now,
                        });
                    }
                }
                if let Some(ref sub_repo) = self.subtitle_repo {
                    let paths_with_subs: Vec<&std::path::Path> = batch
                        .iter()
                        .filter(|(_, subs)| !subs.is_empty())
                        .map(|(item, _)| item.file_path.as_path())
                        .collect();

                    if !paths_with_subs.is_empty() {
                        let now = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;

                        match self.repo.find_by_paths(&paths_with_subs).await {
                            Ok(persisted_map) => {
                                for (item, subs) in batch.iter() {
                                    if subs.is_empty() {
                                        continue;
                                    }
                                    match persisted_map.get(&item.file_path) {
                                        Some(persisted) => {
                                            if let Some(media_id) = persisted.id {
                                                let tracks: Vec<SubtitleTrack> = subs
                                                    .iter()
                                                    .map(|s| {
                                                        s.clone().into_subtitle_track(media_id, now)
                                                    })
                                                    .collect();

                                                let existing = sub_repo
                                                    .find_by_media_item(media_id)
                                                    .await
                                                    .unwrap_or_default();
                                                let to_insert: Vec<SubtitleTrack> = tracks
                                                    .into_iter()
                                                    .filter(|t| {
                                                        !existing.iter().any(|e| {
                                                            e.file_path == t.file_path
                                                                && e.source == t.source
                                                        })
                                                    })
                                                    .collect();

                                                if !to_insert.is_empty() {
                                                    if let Err(e) =
                                                        sub_repo.batch_insert(&to_insert).await
                                                    {
                                                        error!(path = ?item.file_path, error = ?e, "Failed to insert subtitle tracks");
                                                    }
                                                }
                                            }
                                        }
                                        None => {
                                            error!(path = ?item.file_path, "Could not find persisted media item for subtitles");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error = ?e, "Error finding media items for subtitles batch");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!(error = ?e, "Error flushing media batch to database");
            }
        }
        batch.clear();
    }
}

fn determine_series_folder(episode_path: &std::path::Path, series_title: &str) -> PathBuf {
    let parent = match episode_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => return PathBuf::new(),
    };

    let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if is_season_folder(parent_name) {
        if let Some(grandparent) = parent.parent() {
            if !grandparent.as_os_str().is_empty() {
                return grandparent.to_path_buf();
            }
        }
    }

    if parent_name.eq_ignore_ascii_case(series_title) {
        return parent.to_path_buf();
    }

    let candidate = parent.join(series_title);
    if candidate.exists() && candidate.is_dir() {
        return candidate;
    }

    parent.join(series_title)
}

fn is_season_folder(name: &str) -> bool {
    let lower = name.to_lowercase();
    let stripped = if let Some(rest) = lower.strip_prefix("season") {
        rest
    } else if let Some(rest) = lower.strip_prefix('s') {
        rest
    } else {
        return false;
    };
    let digits =
        stripped.trim_matches(|c: char| c == '.' || c == '_' || c == '-' || c.is_whitespace());
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn find_series_artwork(
    series_folder: &std::path::Path,
    ep_meta: &MediaMetadata,
) -> (Option<String>, Option<String>) {
    let mut poster = None;
    let mut backdrop = None;

    if series_folder.exists() && series_folder.is_dir() {
        let dummy = series_folder.join("dummy.mkv");
        let artwork = SidecarScanner::new().find_artwork(&dummy);
        if let Some(p) = artwork.poster {
            poster = Some(p.to_string_lossy().to_string());
        }
        if let Some(b) = artwork.backdrop {
            backdrop = Some(b.to_string_lossy().to_string());
        }
    }

    if poster.is_none() {
        poster = ep_meta.poster_path.clone();
    }
    if backdrop.is_none() {
        backdrop = ep_meta.backdrop_path.clone();
    }

    (poster, backdrop)
}

