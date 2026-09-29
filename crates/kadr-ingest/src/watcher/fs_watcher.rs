use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use notify::event::{ModifyKind, RenameMode};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{info, warn};
use kadr_core::models::Library;
use crate::watcher::debouncer::DebounceQueue;
use crate::watcher::pipeline::IngestPipeline;
use crate::watcher::worker::IngestMessage;

pub fn scan_directory_recursive<P: AsRef<Path>>(dir: P) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(scan_directory_recursive(path));
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ["mkv", "mp4", "webm", "avi"].contains(&ext.to_lowercase().as_str()) {
                        files.push(path);
                    }
                }
            }
        }
    }
    files
}

pub async fn start_library_watcher(
    library: Library,
    pipeline: Arc<IngestPipeline>,
    tx: mpsc::Sender<IngestMessage>,
    debounce_duration: Duration,
) -> notify::Result<RecommendedWatcher> {
    let (notify_tx, mut notify_rx) = mpsc::channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = notify_tx.blocking_send(event);
            }
        },
        Config::default(),
    )?;

    watcher.watch(&library.path, RecursiveMode::Recursive)?;

    // Initial reconciliation scan
    let lib_clone = library.clone();
    let pipe_clone = pipeline.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let initial_files = scan_directory_recursive(&lib_clone.path);
        info!(library = %lib_clone.name, count = initial_files.len(), "Running startup library scan");
        for file in initial_files {
            if let Ok(Some(item)) = pipe_clone.process_file(&lib_clone, &file).await {
                let _ = tx_clone.send(IngestMessage::Upsert(item)).await;
            }
        }
    });

    // Reactive watcher task with debouncing
    tokio::spawn(async move {
        let mut debouncer = DebounceQueue::new(debounce_duration);
        let mut interval = tokio::time::interval(Duration::from_millis(200));

        loop {
            tokio::select! {
                maybe_event = notify_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            match event.kind {
                                EventKind::Create(_) => {
                                    for path in event.paths {
                                        debouncer.record_event(path);
                                    }
                                }
                                EventKind::Modify(ModifyKind::Name(mode)) => {
                                    match mode {
                                        RenameMode::From => {
                                            for path in event.paths {
                                                debouncer.remove(&path);
                                                let _ = tx.send(IngestMessage::Delete(path)).await;
                                            }
                                        }
                                        RenameMode::To => {
                                            for path in event.paths {
                                                debouncer.record_event(path);
                                            }
                                        }
                                        RenameMode::Both if event.paths.len() >= 2 => {
                                            let from = event.paths[0].clone();
                                            let to = event.paths[1].clone();
                                            debouncer.remove(&from);
                                            let _ = tx.send(IngestMessage::Delete(from)).await;
                                            debouncer.record_event(to);
                                        }
                                        _ => {
                                            for path in event.paths {
                                                if !path.exists() {
                                                    debouncer.remove(&path);
                                                    let _ = tx.send(IngestMessage::Delete(path)).await;
                                                } else {
                                                    debouncer.record_event(path);
                                                }
                                            }
                                        }
                                    }
                                }
                                EventKind::Modify(_) => {
                                    for path in event.paths {
                                        debouncer.record_event(path);
                                    }
                                }
                                EventKind::Remove(_) => {
                                    for path in event.paths {
                                        debouncer.remove(&path);
                                        let _ = tx.send(IngestMessage::Delete(path)).await;
                                    }
                                }
                                _ => {}
                            }
                        }
                        None => break,
                    }
                }
                _ = interval.tick() => {
                    let settled = debouncer.extract_settled();
                    for path in settled {
                        if path.is_file() {
                            match pipeline.process_file(&library, &path).await {
                                Ok(Some(item)) => {
                                    let _ = tx.send(IngestMessage::Upsert(item)).await;
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    warn!(path = ?path, error = ?e, "Failed to ingest media file");
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(watcher)
}
