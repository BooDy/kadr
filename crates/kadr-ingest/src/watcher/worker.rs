use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info};
use kadr_core::models::MediaItem;
use kadr_core::subtitles::SubtitleTrack;
use kadr_storage::repos::{MediaItemRepository, SubtitleRepository};
use crate::sidecars::DiscoveredSubtitle;

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
}

impl IngestWorker {
    pub fn new(rx: mpsc::Receiver<IngestMessage>, repo: MediaItemRepository) -> Self {
        Self {
            rx,
            repo,
            subtitle_repo: None,
        }
    }

    pub fn with_subtitles(mut self, subtitle_repo: SubtitleRepository) -> Self {
        self.subtitle_repo = Some(subtitle_repo);
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
        let items: Vec<MediaItem> = batch.iter().map(|(item, _)| item.clone()).collect();
        match self.repo.upsert_batch(&items).await {
            Ok(count) => {
                info!(count = count, "Successfully ingested media batch");
                if let Some(ref sub_repo) = self.subtitle_repo {
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    for (item, subs) in batch.iter() {
                        if subs.is_empty() {
                            continue;
                        }
                        match self.repo.find_by_path(&item.file_path).await {
                            Ok(Some(persisted)) => {
                                if let Some(media_id) = persisted.id {
                                    let tracks: Vec<SubtitleTrack> = subs
                                        .iter()
                                        .map(|s| s.clone().into_subtitle_track(media_id, now))
                                        .collect();

                                    let existing = sub_repo.find_by_media_item(media_id).await.unwrap_or_default();
                                    let to_insert: Vec<SubtitleTrack> = tracks
                                        .into_iter()
                                        .filter(|t| !existing.iter().any(|e| e.file_path == t.file_path && e.source == t.source))
                                        .collect();

                                    if !to_insert.is_empty() {
                                        if let Err(e) = sub_repo.batch_insert(&to_insert).await {
                                            error!(path = ?item.file_path, error = ?e, "Failed to insert subtitle tracks");
                                        }
                                    }
                                }
                            }
                            Ok(None) => {
                                error!(path = ?item.file_path, "Could not find persisted media item for subtitles");
                            }
                            Err(e) => {
                                error!(path = ?item.file_path, error = ?e, "Error finding media item for subtitles");
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
