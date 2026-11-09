use axum::http::StatusCode;
use kadr_core::plugin::{PlaybackDescriptor, PluginManifest, ResolveRequest};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FAILURE_THRESHOLD: u32 = 3;
const COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin unavailable: {0}")]
    Unavailable(String),
    #[error("Plugin timed out")]
    Timeout,
    #[error("Plugin returned an invalid response: {0}")]
    BadResponse(String),
}

impl PluginError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::BadResponse(_) => StatusCode::BAD_GATEWAY,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "PLUGIN_UNAVAILABLE",
            Self::Timeout => "PLUGIN_TIMEOUT",
            Self::BadResponse(_) => "PLUGIN_BAD_RESPONSE",
        }
    }
}

/// Opens after `FAILURE_THRESHOLD` consecutive failures, half-opens after `COOLDOWN`
/// (a single failure in the half-open state re-opens it; a success resets it).
#[derive(Debug, Default)]
pub struct CircuitBreaker {
    failures: u32,
    open_until: Option<Instant>,
    probing: Option<Instant>,
}

impl CircuitBreaker {
    pub fn allow(&mut self, now: Instant) -> bool {
        match self.open_until {
            Some(until) if now < until => false,
            Some(_) => {
                if let Some(probe_expiry) = self.probing {
                    if now < probe_expiry {
                        return false;
                    }
                }
                self.probing = Some(now + COOLDOWN);
                true
            }
            None => true,
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.open_until = None;
        self.probing = None;
    }

    pub fn record_failure(&mut self, now: Instant) {
        if self.open_until.is_some() || self.probing.is_some() {
            self.probing = None;
            self.open_until = Some(now + COOLDOWN);
            self.failures = FAILURE_THRESHOLD;
        } else {
            self.failures += 1;
            if self.failures >= FAILURE_THRESHOLD {
                self.open_until = Some(now + COOLDOWN);
            }
        }
    }
}

const CONNECT_ERROR_PREFIX: &str = "connection failed";

fn map_reqwest(e: reqwest::Error) -> PluginError {
    if e.is_timeout() {
        PluginError::Timeout
    } else if e.is_connect() {
        PluginError::Unavailable(format!("{CONNECT_ERROR_PREFIX}: {e}"))
    } else {
        PluginError::Unavailable(e.to_string())
    }
}

#[derive(Clone)]
pub struct PluginClient {
    http: reqwest::Client,
    timeout_ms: Arc<AtomicU64>,
    breakers: Arc<Mutex<HashMap<String, CircuitBreaker>>>,
}

impl PluginClient {
    pub fn new(timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client builds with static configuration");
        Self {
            http,
            timeout_ms: Arc::new(AtomicU64::new(timeout.as_millis() as u64)),
            breakers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Updates the request timeout used by every clone of this client from now on.
    pub fn set_timeout(&self, timeout: Duration) {
        self.timeout_ms
            .store(timeout.as_millis().max(1) as u64, Ordering::Relaxed);
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.load(Ordering::Relaxed))
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        rb: reqwest::RequestBuilder,
    ) -> Result<T, PluginError> {
        let resp = rb.timeout(self.timeout()).send().await.map_err(map_reqwest)?;
        let status = resp.status();
        if status.is_server_error() {
            return Err(PluginError::Unavailable(format!(
                "plugin returned {status}"
            )));
        }
        if !status.is_success() {
            return Err(PluginError::BadResponse(format!(
                "plugin returned {status}"
            )));
        }
        resp.json::<T>().await.map_err(|e| {
            if e.is_timeout() {
                PluginError::Timeout
            } else {
                PluginError::BadResponse(e.to_string())
            }
        })
    }

    /// `GET <base_url>/manifest.json` (public, unauthenticated; not subject to the breaker).
    pub async fn fetch_manifest(&self, base_url: &str) -> Result<PluginManifest, PluginError> {
        let url = format!("{}/manifest.json", base_url.trim_end_matches('/'));
        self.send_json(self.http.get(url)).await
    }

    /// `GET <base_url>/health` (not subject to the breaker; used to detect recovery).
    pub async fn health(&self, base_url: &str, secret: &str) -> Result<(), PluginError> {
        let url = format!("{}/health", base_url.trim_end_matches('/'));
        let resp = self
            .http
            .get(url)
            .bearer_auth(secret)
            .timeout(self.timeout())
            .send()
            .await
            .map_err(map_reqwest)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(PluginError::Unavailable(format!(
                "health check returned {}",
                resp.status()
            )))
        }
    }

    /// `POST <base_url>/playback/resolve`. Subject to the per-plugin circuit breaker and
    /// retried once on connection errors (never on timeouts).
    pub async fn resolve(
        &self,
        plugin_id: &str,
        base_url: &str,
        secret: &str,
        req: &ResolveRequest,
    ) -> Result<PlaybackDescriptor, PluginError> {
        {
            let mut breakers = self.breakers.lock().unwrap();
            let breaker = breakers.entry(plugin_id.to_string()).or_default();
            if !breaker.allow(Instant::now()) {
                return Err(PluginError::Unavailable(
                    "circuit open after repeated failures".to_string(),
                ));
            }
        }

        let url = format!("{}/playback/resolve", base_url.trim_end_matches('/'));
        let mut result = Err(PluginError::Unavailable("no attempt made".to_string()));
        for _attempt in 0..2 {
            let rb = self.http.post(&url).bearer_auth(secret).json(req);
            result = self.send_json::<PlaybackDescriptor>(rb).await;
            match &result {
                Err(PluginError::Unavailable(msg)) if msg.starts_with(CONNECT_ERROR_PREFIX) => {
                    continue;
                }
                _ => break,
            }
        }

        let mut breakers = self.breakers.lock().unwrap();
        let breaker = breakers.entry(plugin_id.to_string()).or_default();
        match &result {
            Ok(_) => breaker.record_success(),
            Err(_) => breaker.record_failure(Instant::now()),
        }
        result
    }
}
