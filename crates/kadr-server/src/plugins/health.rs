use crate::plugins::service::{PluginService, ServiceError};
use kadr_core::events::SystemEvent;
use kadr_core::plugin::PluginStatus;
use std::time::Duration;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl PluginService {
    /// Probes every `enabled` and `unhealthy` plugin once. A failing enabled plugin becomes
    /// `unhealthy` (and a `plugin:error` event is published); a recovering one becomes `enabled`.
    /// Disabled and pending plugins are never contacted. Does nothing when plugins are off.
    pub async fn check_all(&self) -> Result<(), ServiceError> {
        if self.ensure_enabled().await.is_err() {
            return Ok(());
        }
        self.sync_client_timeout().await;
        for record in self.repo().list().await? {
            if !matches!(
                record.status,
                PluginStatus::Enabled | PluginStatus::Unhealthy
            ) {
                continue;
            }
            let result = self
                .client()
                .health(&record.base_url, &self.secret_for(&record.id))
                .await;
            match (record.status, result) {
                (PluginStatus::Enabled, Err(e)) => {
                    if let Err(err) = self
                        .repo()
                        .set_status(&record.id, PluginStatus::Unhealthy)
                        .await
                    {
                        tracing::error!(plugin = %record.id, "Failed to update plugin status to unhealthy: {err}");
                        continue;
                    }
                    tracing::warn!(plugin = %record.id, "Plugin became unhealthy: {e}");
                    self.events().publish(SystemEvent::PluginError {
                        plugin_id: record.id.clone(),
                        message: e.to_string(),
                        timestamp: now_secs(),
                    });
                }
                (PluginStatus::Unhealthy, Ok(())) => {
                    if let Err(err) = self
                        .repo()
                        .set_status(&record.id, PluginStatus::Enabled)
                        .await
                    {
                        tracing::error!(plugin = %record.id, "Failed to update plugin status to enabled: {err}");
                        continue;
                    }
                    tracing::info!(plugin = %record.id, "Plugin recovered");
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Runs `check_all` every `interval`. The first probe happens after one full interval.
pub fn spawn_health_monitor(svc: PluginService, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = svc.check_all().await {
                tracing::error!("Plugin health check failed: {e}");
            }
        }
    })
}
