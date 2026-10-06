use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use kadr_core::models::{Library, User, UserRole};
use kadr_ingest::watcher::{start_library_watcher, IngestPipeline, IngestWorker};
use kadr_server::api::{create_router_with_ingest, mount_web_serving};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::pin::hash_pin;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::config::AppConfig;
use kadr_server::events::EventBus;
use kadr_server::identity::{load_or_create_server_id, ServerIdentity};
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::session::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
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
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "kadr=info,kadr_server=info,kadr_ingest=info,kadr_storage=info".into()
            }),
        )
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
            let env_config = std::env::var("KADR_CONFIG").ok().map(PathBuf::from);
            let local_toml = std::path::Path::new("kadr.toml");
            let sys_toml = std::path::Path::new("/etc/kadr/kadr.toml");

            if let Some(ref path) = env_config.filter(|p| p.exists()) {
                info!(path = ?path, "Loading configuration from KADR_CONFIG environment variable");
                AppConfig::load_from_file(path)?
            } else if local_toml.exists() {
                info!(path = ?local_toml, "Loading configuration from local kadr.toml");
                AppConfig::load_from_file(local_toml)?
            } else if sys_toml.exists() {
                info!(path = ?sys_toml, "Loading configuration from /etc/kadr/kadr.toml");
                AppConfig::load_from_file(sys_toml)?
            } else {
                info!("Config file not found (checked KADR_CONFIG, ./kadr.toml, /etc/kadr/kadr.toml), using defaults");
                AppConfig::default()
            }
        }
    };

    let mut config = config;
    if let Ok(port_str) = std::env::var("KADR_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            config.server.port = port;
        }
    }
    if let Ok(host) = std::env::var("KADR_HOST") {
        config.server.host = host;
    }
    if let Ok(data_dir) = std::env::var("KADR_DATA_DIR") {
        config.server.data_dir = PathBuf::from(data_dir);
    }

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
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    if let Ok(removed) = media_repo.deduplicate_media_items().await {
        if removed > 0 {
            info!(removed, "Cleaned up duplicate media items from storage");
        }
    }

    // Sync bootstrap libraries
    for lib_cfg in &config.libraries {
        let lib = Library {
            id: lib_cfg.id.clone(),
            name: lib_cfg.name.clone(),
            path: lib_cfg.path.clone(),
            media_type: lib_cfg.media_type,
            created_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs() as i64,
            ..Default::default()
        };
        lib_repo.insert(&lib).await?;
    }

    let active_libraries = lib_repo.get_all().await?;
    info!(
        count = active_libraries.len(),
        "Loaded registered libraries"
    );

    let event_bus = Arc::new(EventBus::default_bus());

    let (ingest_tx, ingest_rx) = tokio::sync::mpsc::channel(200);
    let bus_for_worker = event_bus.clone();
    let worker = IngestWorker::new(ingest_rx, media_repo.clone())
        .with_subtitles(subtitle_repo.clone())
        .with_event_callback(move |ev| {
            bus_for_worker.publish(ev);
        });
    let worker_handle = tokio::spawn(worker.run());

    let thumbnails_dir = config.server.data_dir.join("thumbnails");
    let pipeline = Arc::new(IngestPipeline::new(
        config.scanner.use_ffprobe,
        Some(thumbnails_dir),
    ));
    let mut watchers = Vec::new();

    for lib in &active_libraries {
        let has_existing_path = lib.paths.iter().any(|p| p.exists()) || lib.path.exists();
        if has_existing_path {
            info!(library = %lib.name, path = ?lib.path, paths = ?lib.paths, "Starting filesystem watcher");
            let watcher = start_library_watcher(
                lib.clone(),
                pipeline.clone(),
                ingest_tx.clone(),
                Duration::from_millis(config.scanner.debounce_millis),
            )
            .await?;
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
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;
        let admin = User {
            id: admin_id,
            username: "admin".to_string(),
            pin_hash,
            role: UserRole::Admin,
            created_at: now,
        };
        user_repo.create(&admin).await?;
        info!(
            "Seeded default admin user 'admin' with initial PIN '{}'",
            default_pin
        );
    }

    // Initialize JWT service with persisted secret or auto-generate
    let jwt_secret_path = config.server.data_dir.join("jwt.secret");
    let existing_secret = if jwt_secret_path.exists() {
        let content = std::fs::read_to_string(&jwt_secret_path)?
            .trim()
            .to_string();
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    } else {
        None
    };

    let jwt_secret = match existing_secret {
        Some(secret) => secret,
        None => {
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
        }
    };
    let jwt_svc = JwtService::new(&jwt_secret, 86400 * 7);

    // Initialize rate limiter (5 attempts, 300s window, 300s lockout)
    let rate_limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));

    // Initialize session registry and spawn periodic stale session and rate limiter pruning task
    let session_registry = Arc::new(SessionRegistry::new());
    let sessions_cleanup = session_registry.clone();
    let rate_limiter_cleanup = rate_limiter.clone();
    let prune_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let pruned = sessions_cleanup.prune_stale(Duration::from_secs(60)).await;
            if pruned > 0 {
                info!(pruned, "Pruned stale playback sessions");
            }
            let pruned_ips = rate_limiter_cleanup.prune_stale().await;
            if pruned_ips > 0 {
                info!(pruned = pruned_ips, "Pruned stale rate limiter entries");
            }
        }
    });

    // Initialize layout registry and widget resolver
    let layout_registry = LayoutRegistry::new();
    let config_screens_dir = std::path::Path::new("config/screens");
    if config_screens_dir.exists() {
        if let Err(e) = layout_registry.load_overrides_from_dir(config_screens_dir) {
            warn!(error = %e, "Failed to load layout overrides from config/screens");
        }
    }
    let screens_dir = config.server.data_dir.join("screens");
    if screens_dir.exists() {
        if let Err(e) = layout_registry.load_overrides_from_dir(&screens_dir) {
            warn!(error = %e, "Failed to load layout overrides from disk");
        }
    }

    // Register default layouts for all active libraries if not already overridden
    for lib in &active_libraries {
        if layout_registry.find_screen_for_library(&lib.id, &lib.name).is_none() {
            layout_registry.register_screen(kadr_server::layout::default_library_layout(&lib.id, &lib.name));
        }
    }

    let widget_resolver = Arc::new(WidgetResolver::new(
        media_repo.clone(),
        playback_repo.clone(),
    ));

    // Initialize subtitle delivery service and OpenSubtitles client
    let subtitle_cache_dir = config.server.data_dir.join("subtitles_cache");
    if !subtitle_cache_dir.exists() {
        std::fs::create_dir_all(&subtitle_cache_dir)?;
    }
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        subtitle_cache_dir,
        subtitle_repo.clone(),
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));

    // Initialize telemetry collector and spawn periodic broadcaster
    let telemetry_collector = Arc::new(TelemetryCollector::new(
        config.storage.database_path.clone(),
        session_registry.clone(),
        event_bus.clone(),
    ));
    let telemetry_handle = telemetry_collector
        .clone()
        .spawn_periodic_broadcaster(Duration::from_secs(5));

    // Initialize server identity
    let server_id = match load_or_create_server_id(&config.server.data_dir) {
        Ok(id) => id,
        Err(e) => {
            warn!(
                "Failed to load or create server ID in {:?}: {}, generating ephemeral UUID",
                config.server.data_dir, e
            );
            uuid::Uuid::new_v4().to_string()
        }
    };
    let server_identity = Arc::new(ServerIdentity { id: server_id });

    // Assemble Axum HTTP router
    let mut app = create_router_with_ingest(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        rate_limiter,
        session_registry,
        layout_registry,
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus,
        telemetry_collector,
        ingest_tx.clone(),
        pipeline.clone(),
        Arc::new(tokio::sync::RwLock::new(config.clone())),
        server_identity,
    );

    // Mount static web SPA serving if assets exist
    let env_web = std::env::var("KADR_WEB_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    let config_web = config.server.web_dir.clone();
    let web_dist = std::path::Path::new("web/dist");
    let data_web = config.server.data_dir.join("web");
    let sys_web = std::path::Path::new("/usr/share/kadr/web");

    let web_dir: Option<std::path::PathBuf> = if let Some(ref path) = env_web.filter(|p| p.exists())
    {
        Some(path.clone())
    } else if let Some(ref path) = config_web.filter(|p| p.exists()) {
        Some(path.clone())
    } else if web_dist.exists() {
        Some(web_dist.to_path_buf())
    } else if data_web.exists() {
        Some(data_web)
    } else if sys_web.exists() {
        Some(sys_web.to_path_buf())
    } else {
        None
    };

    if let Some(ref dir) = web_dir {
        info!(path = ?dir, "Static web assets directory found; mounting SPA static file serving");
        app = mount_web_serving(app, dir);
    } else {
        info!("Static web assets directory not found (checked KADR_WEB_DIR, server.web_dir, 'web/dist', data_dir/'web', and '/usr/share/kadr/web'); running in API-only headless mode");
    }

    // Bind TCP listener and serve Axum router
    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!(listen = %addr, "Binding Axum HTTP listener");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Kadr Milestone 5A HTTP server running at http://{}", addr);

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal());

    if let Err(e) = server.await {
        error!(error = ?e, "Axum server terminated with error");
    }

    info!("Shutdown signal received. Stopping maintenance, watchers, and flushing storage...");
    telemetry_handle.abort();
    prune_handle.abort();
    drop(watchers);
    drop(ingest_tx);
    if let Err(e) = worker_handle.await {
        error!(error = ?e, "Ingest worker task panicked or failed during execution");
    }

    info!("Kadr server terminated cleanly.");
    Ok(())
}
