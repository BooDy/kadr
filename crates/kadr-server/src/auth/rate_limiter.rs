use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitStatus {
    Allowed,
    LockedOut { retry_after_secs: u64 },
}

#[derive(Debug, Clone)]
struct AttemptRecord {
    failed_count: u32,
    first_failed_at: Instant,
    lockout_until: Option<Instant>,
}

#[derive(Clone)]
pub struct RateLimiter {
    max_attempts: u32,
    window_duration: Duration,
    lockout_duration: Duration,
    attempts: Arc<RwLock<HashMap<IpAddr, AttemptRecord>>>,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_duration: Duration, lockout_duration: Duration) -> Self {
        Self {
            max_attempts,
            window_duration,
            lockout_duration,
            attempts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn check_attempt(&self, ip: &IpAddr) -> RateLimitStatus {
        let now = Instant::now();
        let read = self.attempts.read().await;
        if let Some(record) = read.get(ip) {
            if let Some(lockout) = record.lockout_until {
                if now < lockout {
                    let remaining = lockout
                        .duration_since(now)
                        .as_secs()
                        .saturating_add(1)
                        .min(self.lockout_duration.as_secs())
                        .max(1);
                    return RateLimitStatus::LockedOut {
                        retry_after_secs: remaining,
                    };
                }
            }
        }
        RateLimitStatus::Allowed
    }

    pub async fn record_failure(&self, ip: &IpAddr) {
        let now = Instant::now();
        let mut write = self.attempts.write().await;
        let record = write.entry(*ip).or_insert(AttemptRecord {
            failed_count: 0,
            first_failed_at: now,
            lockout_until: None,
        });

        if now.duration_since(record.first_failed_at) > self.window_duration {
            record.failed_count = 1;
            record.first_failed_at = now;
            record.lockout_until = None;
        } else {
            record.failed_count += 1;
        }

        if record.failed_count >= self.max_attempts {
            record.lockout_until = Some(now + self.lockout_duration);
        }
    }

    pub async fn record_success(&self, ip: &IpAddr) {
        let mut write = self.attempts.write().await;
        write.remove(ip);
    }

    pub async fn prune_stale(&self) -> usize {
        let now = Instant::now();
        let mut write = self.attempts.write().await;
        let initial_len = write.len();
        write.retain(|_, record| {
            let is_locked_out = match record.lockout_until {
                Some(lockout) => now < lockout,
                None => false,
            };
            is_locked_out
                || now.saturating_duration_since(record.first_failed_at) <= self.window_duration
        });
        initial_len - write.len()
    }
}
