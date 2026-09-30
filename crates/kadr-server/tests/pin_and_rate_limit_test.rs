use std::net::IpAddr;
use std::time::Duration;
use kadr_server::auth::pin::{hash_pin, validate_pin, verify_pin};
use kadr_server::auth::rate_limiter::{RateLimitStatus, RateLimiter};

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
    assert_eq!(limiter.check_attempt(&other_ip).await, RateLimitStatus::Allowed);
}
