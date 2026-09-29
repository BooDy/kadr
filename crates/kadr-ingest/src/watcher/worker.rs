use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info};
use kadr_core::models::MediaItem;
use kadr_storage::repos::MediaItemRepository;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum IngestMessage {
    Upsert(MediaItem),
    Delete(PathBuf),
}

pub struct IngestWorker {
    rx: mpsc::Receiver<IngestMessage>,
    repo: MediaItemRepository,
}

impl IngestWorker {
    pub fn new(rx: mpsc::Receiver<IngestMessage>, repo: MediaItemRepository) -> Self {
        Self { rx, repo }
    }

    pub async fn run(mut self) {
        let mut batch = Vec::with_capacity(50);

        loop {
            tokio::select! {
                maybe_msg = self.rx.recv() => {
                    match maybe_msg {
                        Some(IngestMessage::Upsert(item)) => {
                            batch.push(item);
                            if batch.len() >= 50 {
                                self.flush_batch(&mut batch).await;
                            }
                        }
                        Some(IngestMessage::Delete(path)) => {
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

    async fn flush_batch(&self, batch: &mut Vec<MediaItem>) {
        if batch.is_empty() {
            return;
        }
        match self.repo.upsert_batch(batch).await {
            Ok(count) => {
                info!(count = count, "Successfully ingested media batch");
            }
            Err(e) => {
                error!(error = ?e, "Error flushing media batch to database");
            }
        }
        batch.clear();
    }
}
