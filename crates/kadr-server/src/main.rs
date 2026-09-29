use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use kadr_core::models::Library;
use kadr_ingest::watcher::{start_library_watcher, IngestPipeline, IngestWorker};
use kadr_server::config::AppConfig;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository};

#[derive(Parser, Debug)]
#[command(name = "kadr", about = "Kadr Media Server")]
struct Args {
    #[arg(short, long, default_value = "kadr.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kadr=info,kadr_server=info,kadr_ingest=info,kadr_storage=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    info!(config = ?args.config, "Starting Kadr media server");

    let config = if args.config.exists() {
        AppConfig::load_from_file(&args.config)?
    } else {
        info!("Config file not found, using defaults");
        AppConfig::default()
    };

    if let Some(parent) = config.storage.database_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    info!(db = ?config.storage.database_path, "Initializing SQLite storage");
    let pool = create_pool(&config.storage.database_path, config.storage.max_readers)?;
    initialize_database(&pool).await?;

    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());

    // Sync bootstrap libraries
    for lib_cfg in &config.libraries {
        let lib = Library {
            id: lib_cfg.id.clone(),
            name: lib_cfg.name.clone(),
            path: lib_cfg.path.clone(),
            media_type: lib_cfg.media_type,
            created_at: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() as i64,
        };
        lib_repo.create(&lib).await?;
    }

    let active_libraries = lib_repo.get_all().await?;
    info!(count = active_libraries.len(), "Loaded registered libraries");

    let (ingest_tx, ingest_rx) = tokio::sync::mpsc::channel(200);
    let worker = IngestWorker::new(ingest_rx, media_repo);
    let worker_handle = tokio::spawn(worker.run());

    let pipeline = Arc::new(IngestPipeline::new(config.scanner.use_ffprobe));
    let mut watchers = Vec::new();

    for lib in active_libraries {
        if lib.path.exists() {
            info!(library = %lib.name, path = ?lib.path, "Starting filesystem watcher");
            let watcher = start_library_watcher(
                lib,
                pipeline.clone(),
                ingest_tx.clone(),
                Duration::from_millis(config.scanner.debounce_millis),
            ).await?;
            watchers.push(watcher);
        } else {
            error!(library = %lib.name, path = ?lib.path, "Library path does not exist, skipping watcher");
        }
    }

    info!("Kadr Milestone 1 Core & Storage ready. Press Ctrl+C to terminate.");
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received. Stopping watchers and flushing storage...");

    drop(watchers);
    drop(ingest_tx);
    let _ = worker_handle.await;

    info!("Kadr server terminated cleanly.");
    Ok(())
}
