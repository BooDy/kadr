use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};

const SAFE_OS_ENV_KEYS: &[&str] = &[
    "PATH",
    "LANG",
    "LC_ALL",
    "TZ",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "SYSTEMROOT",
];

pub fn kadr_local_url(host: &str, port: u16) -> String {
    let trimmed = host.trim();
    if matches!(trimmed, "0.0.0.0" | "::" | "[::]" | "") {
        format!("http://127.0.0.1:{port}")
    } else if trimmed.contains(':') && !trimmed.starts_with('[') {
        format!("http://[{trimmed}]:{port}")
    } else {
        format!("http://{trimmed}:{port}")
    }
}

#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub plugin_id: String,
    pub bin_path: PathBuf,
    pub data_dir: PathBuf,
    pub kadr_url: String,
    pub secret: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub enum StartOutcome {
    Healthy { base_url: String },
    Unhealthy { base_url: String, reason: String },
}

pub type OnPortChanged = Arc<dyn Fn(String, String) + Send + Sync>;

struct ManagedEntry {
    stop_flag: Arc<AtomicBool>,
    stop_notify: Arc<Notify>,
    alive: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct PluginSupervisor {
    entries: Arc<Mutex<HashMap<String, ManagedEntry>>>,
    http: reqwest::Client,
}

fn reserve_loopback_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to allocate loopback port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to inspect loopback port: {e}"))?
        .port();
    Ok(port)
}

fn spawn_child(spec: &SpawnSpec, port: u16) -> Result<Child, String> {
    std::fs::create_dir_all(&spec.data_dir)
        .map_err(|e| format!("failed to create plugin data dir: {e}"))?;

    let working_dir = spec
        .bin_path
        .parent()
        .and_then(|p| {
            if p.file_name().and_then(|n| n.to_str()) == Some("bin") {
                p.parent()
            } else {
                Some(p)
            }
        })
        .filter(|p| p.exists())
        .unwrap_or(&spec.data_dir);

    let mut cmd = Command::new(&spec.bin_path);
    cmd.env_clear();
    for key in SAFE_OS_ENV_KEYS {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    cmd.env("PLUGIN_BIND", format!("127.0.0.1:{port}"))
        .env("KADR_URL", &spec.kadr_url)
        .env("KADR_PLUGIN_SECRET", &spec.secret)
        .env("KADR_PLUGIN_TOKEN", &spec.token)
        .env("PLUGIN_DATA_DIR", &spec.data_dir)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn plugin '{}': {e}", spec.plugin_id))?;

    if let Some(stdout) = child.stdout.take() {
        let id = spec.plugin_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::info!(target: "plugin", plugin = %id, "{line}");
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let id = spec.plugin_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(target: "plugin", plugin = %id, "{line}");
            }
        });
    }

    Ok(child)
}

async fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: Sending SIGTERM to a direct child PID owned by this process before waiting.
        unsafe {
            let _ = libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        if tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .is_ok()
        {
            return;
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

impl PluginSupervisor {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .build()
            .expect("reqwest client builds");
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            http,
        }
    }

    async fn wait_for_health(
        &self,
        child: &mut Child,
        base_url: &str,
        secret: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout.max(Duration::from_millis(300));
        let health_url = format!("{base_url}/health");
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("plugin exited prematurely with {status}"));
            }
            if let Ok(resp) = self
                .http
                .get(&health_url)
                .bearer_auth(secret)
                .timeout(Duration::from_millis(500))
                .send()
                .await
            {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "plugin did not become healthy within {}s",
                    timeout.as_secs().max(1)
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn start(
        &self,
        spec: SpawnSpec,
        ready_timeout: Duration,
        on_port_changed: Option<OnPortChanged>,
    ) -> StartOutcome {
        self.stop(&spec.plugin_id).await;

        let mut last_err = String::new();
        let mut started: Option<(Child, String, bool)> = None;

        for _attempt in 0..3 {
            let port = match reserve_loopback_port() {
                Ok(p) => p,
                Err(e) => {
                    return StartOutcome::Unhealthy {
                        base_url: "http://127.0.0.1:0".to_string(),
                        reason: e,
                    }
                }
            };
            let base_url = format!("http://127.0.0.1:{port}");
            if let Some(ref cb) = on_port_changed {
                cb(spec.plugin_id.clone(), base_url.clone());
            }
            let mut child = match spawn_child(&spec, port) {
                Ok(c) => c,
                Err(e) => {
                    return StartOutcome::Unhealthy {
                        base_url,
                        reason: e,
                    };
                }
            };
            match self
                .wait_for_health(&mut child, &base_url, &spec.secret, ready_timeout)
                .await
            {
                Ok(()) => {
                    started = Some((child, base_url, true));
                    break;
                }
                Err(e) => {
                    last_err = e;
                    // Retry another port only if the process exited immediately (e.g., port race).
                    if child.try_wait().ok().flatten().is_some() {
                        continue;
                    }
                    started = Some((child, base_url, false));
                    break;
                }
            }
        }

        let (initial_child, base_url, is_healthy) = match started {
            Some((c, url, ok)) => (Some(c), url, ok),
            None => (None, "http://127.0.0.1:0".to_string(), false),
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_notify = Arc::new(Notify::new());
        let alive = Arc::new(AtomicBool::new(initial_child.is_some()));

        let flag_bg = stop_flag.clone();
        let notify_bg = stop_notify.clone();
        let alive_bg = alive.clone();
        let spec_bg = spec.clone();
        let cb_bg = on_port_changed;

        let task = tokio::spawn(async move {
            let mut current_child = initial_child;
            let mut backoff_secs = 1u64;
            let mut spawned_at = Instant::now();
            loop {
                if flag_bg.load(Ordering::SeqCst) {
                    break;
                }
                let exited = match current_child.as_mut() {
                    Some(ch) => {
                        tokio::select! {
                            _ = notify_bg.notified() => false,
                            _ = ch.wait() => true,
                        }
                    }
                    None => true,
                };
                if flag_bg.load(Ordering::SeqCst) || !exited {
                    break;
                }
                alive_bg.store(false, Ordering::SeqCst);
                current_child = None;

                if spawned_at.elapsed() >= Duration::from_secs(60) {
                    backoff_secs = 1;
                }
                tracing::warn!(
                    plugin = %spec_bg.plugin_id,
                    backoff_secs,
                    "Managed plugin exited unexpectedly; scheduling restart"
                );
                tokio::select! {
                    _ = notify_bg.notified() => break,
                    _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                }
                if flag_bg.load(Ordering::SeqCst) {
                    break;
                }
                backoff_secs = (backoff_secs * 2).min(30);

                if let Ok(port) = reserve_loopback_port() {
                    let url = format!("http://127.0.0.1:{port}");
                    if let Ok(new_child) = spawn_child(&spec_bg, port) {
                        spawned_at = Instant::now();
                        alive_bg.store(true, Ordering::SeqCst);
                        if let Some(ref cb) = cb_bg {
                            cb(spec_bg.plugin_id.clone(), url);
                        }
                        current_child = Some(new_child);
                    }
                }
            }
            if let Some(mut ch) = current_child.take() {
                terminate_child(&mut ch).await;
            }
            alive_bg.store(false, Ordering::SeqCst);
        });

        self.entries.lock().await.insert(
            spec.plugin_id.clone(),
            ManagedEntry {
                stop_flag,
                stop_notify,
                alive,
                task,
            },
        );

        if is_healthy {
            StartOutcome::Healthy { base_url }
        } else {
            StartOutcome::Unhealthy {
                base_url,
                reason: last_err,
            }
        }
    }

    pub async fn stop(&self, plugin_id: &str) {
        let entry = self.entries.lock().await.remove(plugin_id);
        if let Some(entry) = entry {
            entry.stop_flag.store(true, Ordering::SeqCst);
            entry.stop_notify.notify_one();
            let _ = entry.task.await;
        }
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self.entries.lock().await.keys().cloned().collect();
        for id in ids {
            self.stop(&id).await;
        }
    }

    pub async fn is_running(&self, plugin_id: &str) -> bool {
        let entries = self.entries.lock().await;
        entries
            .get(plugin_id)
            .map(|e| e.alive.load(Ordering::SeqCst))
            .unwrap_or(false)
    }
}
