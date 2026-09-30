use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use clap::Parser;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use kadr_core::models::{Library, User, UserRole};
use kadr_ingest::watcher::{start_library_watcher, IngestPipeline, IngestWorker};
use kadr_server::api::create_router;
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::playback::session::SessionRegistry;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository,
};

#[derive(Parser, Debug)]
#[command(name = "kadr", about = "Kadr Media Server")]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                error!(error = %e, "Failed to install SIGTERM signal handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kadr=info,kadr_server=info,kadr_ingest=info,kadr_storage=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    info!(config = ?args.config, "Starting Kadr media server");

    let config = match &args.config {
        Some(path) => {
            if !path.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Config file not found: {}", path.display()),
                )
                .into());
            }
            AppConfig::load_from_file(path)?
        }
        None => {
            let default_path = std::path::Path::new("kadr.toml");
            if default_path.exists() {
                AppConfig::load_from_file(default_path)?
            } else {
                info!("Config file not found, using defaults");
                AppConfig::default()
            }
        }
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
    let user_repo = UserRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());

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
    let worker = IngestWorker::new(ingest_rx, media_repo.clone());
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

    // Auto-bootstrap initial admin user if no users exist
    let user_count = user_repo.count().await?;
    if user_count == 0 {
        let admin_id = uuid::Uuid::new_v4().to_string();
        let default_pin = "1234";
        let pin_hash = hash_pin(default_pin)?;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() as i64;
        let admin = User {
            id: admin_id,
            username: "admin".to_string(),
            pin_hash,
            role: UserRole::Admin,
            created_at: now,
        };
        user_repo.create(&admin).await?;
        info!("Seeded default admin user 'admin' with initial PIN '{}'", default_pin);
    }

    // Initialize JWT service with persisted secret or auto-generate
    let jwt_secret_path = config.server.data_dir.join("jwt.secret");
    let jwt_secret = if jwt_secret_path.exists() {
        std::fs::read_to_string(&jwt_secret_path)?.trim().to_string()
    } else {
        use rand::distributions::Alphanumeric;
        use rand::Rng;
        let generated: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect();
        if let Some(parent) = jwt_secret_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        if let Err(e) = std::fs::write(&jwt_secret_path, &generated) {
            warn!(error = %e, "Could not persist JWT secret to disk; using in-memory secret");
        } else {
            info!(path = ?jwt_secret_path, "Generated and saved new JWT secret");
        }
        generated
    };
    let jwt_svc = JwtService::new(&jwt_secret, 86400 * 7);

    // Initialize rate limiter (5 attempts, 300s window, 300s lockout)
    let rate_limiter = RateLimiter::new(
        5,
        Duration::from_secs(300),
        Duration::from_secs(300),
    );

    // Initialize session registry and spawn periodic stale session pruning task (prunes sessions idle >60s every 30s)
    let session_registry = Arc::new(SessionRegistry::new());
    let sessions_cleanup = session_registry.clone();
    let prune_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let pruned = sessions_cleanup.prune_stale(Duration::from_secs(60)).await;
            if pruned > 0 {
                info!(pruned, "Pruned stale playback sessions");
            }
        }
    });

    // Assemble Axum HTTP router
    let app = create_router(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        rate_limiter,
        session_registry,
    );

    // Bind TCP listener and serve Axum router
    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!(listen = %addr, "Binding Axum HTTP listener");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Kadr Milestone 2 HTTP server running at http://{}", addr);

    let server = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal());

    if let Err(e) = server.await {
        error!(error = ?e, "Axum server terminated with error");
    }

    info!("Shutdown signal received. Stopping maintenance, watchers, and flushing storage...");
    prune_handle.abort();
    drop(watchers);
    drop(ingest_tx);
    if let Err(e) = worker_handle.await {
        error!(error = ?e, "Ingest worker task panicked or failed during execution");
    }

    info!("Kadr server terminated cleanly.");
    Ok(())
}
