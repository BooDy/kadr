use kadr_server::auth::pin::{hash_pin, validate_pin, verify_pin};
use kadr_server::auth::rate_limiter::{RateLimitStatus, RateLimiter};
use std::net::IpAddr;
use std::time::Duration;

#[test]
fn test_pin_validation_and_argon2_hashing() {
    assert!(validate_pin("1234").is_ok());
    assert!(validate_pin("0000").is_ok());
    assert!(validate_pin("123").is_err());
    assert!(validate_pin("12345").is_err());
    assert!(validate_pin("abcd").is_err());

    let hash = hash_pin("1234").unwrap();
    assert!(verify_pin("1234", &hash).unwrap());
    assert!(!verify_pin("9999", &hash).unwrap());
}

#[tokio::test]
async fn test_rate_limiter_lockout_after_five_attempts() {
    let limiter = RateLimiter::new(5, Duration::from_secs(300), Duration::from_secs(300));
    let ip: IpAddr = "192.168.1.100".parse().unwrap();

    for _ in 0..4 {
        assert_eq!(limiter.check_attempt(&ip).await, RateLimitStatus::Allowed);
        limiter.record_failure(&ip).await;
    }

    assert_eq!(limiter.check_attempt(&ip).await, RateLimitStatus::Allowed);
    limiter.record_failure(&ip).await;

    // 5th failure triggers lockout
    match limiter.check_attempt(&ip).await {
        RateLimitStatus::LockedOut { retry_after_secs } => {
            assert!(retry_after_secs > 0 && retry_after_secs <= 300);
        }
        RateLimitStatus::Allowed => panic!("should be locked out"),
    }

    // Success on different IP works
    let other_ip: IpAddr = "192.168.1.200".parse().unwrap();
    assert_eq!(
        limiter.check_attempt(&other_ip).await,
        RateLimitStatus::Allowed
    );
}

#[tokio::test]
async fn test_rate_limiter_prune_stale() {
    let limiter = RateLimiter::new(3, Duration::from_millis(50), Duration::from_millis(120));
    let ip1: IpAddr = "192.168.1.10".parse().unwrap();
    let ip2: IpAddr = "192.168.1.20".parse().unwrap();

    limiter.record_failure(&ip1).await;
    // ip2 fails 3 times to get locked out
    for _ in 0..3 {
        limiter.record_failure(&ip2).await;
    }

    // Immediately after failure, neither is stale
    assert_eq!(limiter.prune_stale().await, 0);

    // Sleep past window (60ms) but before lockout (120ms)
    tokio::time::sleep(Duration::from_millis(60)).await;

    // ip1 is not locked out and window expired -> pruned
    // ip2 is still locked out -> kept
    assert_eq!(limiter.prune_stale().await, 1);
    assert_eq!(limiter.check_attempt(&ip1).await, RateLimitStatus::Allowed);
    match limiter.check_attempt(&ip2).await {
        RateLimitStatus::LockedOut { .. } => {}
        _ => panic!("ip2 should still be locked out"),
    }

    // Sleep past lockout duration (another 70ms)
    tokio::time::sleep(Duration::from_millis(70)).await;

    // ip2 lockout expired and window expired -> pruned
    assert_eq!(limiter.prune_stale().await, 1);
    assert_eq!(limiter.check_attempt(&ip2).await, RateLimitStatus::Allowed);
}
