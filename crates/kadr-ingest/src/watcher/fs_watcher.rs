use crate::watcher::debouncer::DebounceQueue;
use crate::watcher::pipeline::IngestPipeline;
use crate::watcher::worker::IngestMessage;
use kadr_core::models::Library;
use notify::event::{ModifyKind, RenameMode};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

pub fn scan_directory_recursive<P: AsRef<Path>>(dir: P) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut visited_dirs = std::collections::HashSet::new();
    let mut seen_files = std::collections::HashSet::new();
    scan_directory_recursive_inner(dir.as_ref(), &mut visited_dirs, &mut seen_files, &mut files);
    files
}

fn scan_directory_recursive_inner(
    dir: &Path,
    visited_dirs: &mut std::collections::HashSet<PathBuf>,
    seen_files: &mut std::collections::HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) {
    let canon_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited_dirs.insert(canon_dir) {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory_recursive_inner(&path, visited_dirs, seen_files, files);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ["mkv", "mp4", "webm", "avi"].contains(&ext.to_lowercase().as_str()) {
                        let canon_file =
                            std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                        if seen_files.insert(canon_file.clone()) {
                            files.push(canon_file);
                        }
                    }
                }
            }
        }
    }
}

pub async fn start_library_watcher(
    library: Library,
    pipeline: Arc<IngestPipeline>,
    tx: mpsc::Sender<IngestMessage>,
    debounce_duration: Duration,
) -> notify::Result<RecommendedWatcher> {
    let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = notify_tx.send(event);
            }
        },
        Config::default(),
    )?;

    let paths = if library.paths.is_empty() {
        vec![library.path.clone()]
    } else {
        library.paths.clone()
    };

    // Reactive watcher task with debouncing spawned first so notify_rx is actively drained
    let lib_for_debouncer = library.clone();
    let pipe_for_debouncer = pipeline.clone();
    let tx_for_debouncer = tx.clone();
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
                                                let _ = tx_for_debouncer.send(IngestMessage::Delete(path)).await;
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
                                            let _ = tx_for_debouncer.send(IngestMessage::Delete(from)).await;
                                            debouncer.record_event(to);
                                        }
                                        _ => {
                                            for path in event.paths {
                                                if !path.exists() {
                                                    debouncer.remove(&path);
                                                    let _ = tx_for_debouncer.send(IngestMessage::Delete(path)).await;
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
                                        let _ = tx_for_debouncer.send(IngestMessage::Delete(path)).await;
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
                            match pipe_for_debouncer.process_file(&lib_for_debouncer, &path).await {
                                Ok(Some((item, subs))) => {
                                    let _ = tx_for_debouncer.send(IngestMessage::Upsert(item, subs)).await;
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

    for p in &paths {
        if p.exists() {
            let _ = watcher.watch(p, RecursiveMode::Recursive);
        }
    }

    // Initial reconciliation scan
    let lib_clone = library.clone();
    let pipe_clone = pipeline.clone();
    let tx_clone = tx.clone();
    let paths_clone = paths.clone();
    tokio::spawn(async move {
        for p in &paths_clone {
            if p.exists() {
                let initial_files = scan_directory_recursive(p);
                info!(
                    library = %lib_clone.name,
                    path = ?p,
                    count = initial_files.len(),
                    "Running startup library scan"
                );
                for file in initial_files {
                    if let Ok(Some((item, subs))) = pipe_clone.process_file(&lib_clone, &file).await
                    {
                        let _ = tx_clone.send(IngestMessage::Upsert(item, subs)).await;
                    }
                }
            }
        }
    });

    Ok(watcher)
}
