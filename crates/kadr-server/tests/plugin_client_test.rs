mod plugin_support;

use kadr_core::plugin::{ClientCapabilities, PlaybackMode, ResolveRequest};
use kadr_server::plugins::client::{CircuitBreaker, PluginClient, PluginError};
use plugin_support::spawn_mock;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn req() -> ResolveRequest {
    ResolveRequest {
        remote_id: "ch1".into(),
        user_id: "u1".into(),
        client: ClientCapabilities::default(),
    }
}

#[test]
fn breaker_opens_after_three_failures_and_half_opens_after_cooldown() {
    let mut b = CircuitBreaker::default();
    let t0 = Instant::now();
    assert!(b.allow(t0));
    b.record_failure(t0);
    b.record_failure(t0);
    assert!(b.allow(t0), "two failures keep it closed");
    b.record_failure(t0);
    assert!(!b.allow(t0), "third failure opens it");
    assert!(!b.allow(t0 + Duration::from_secs(29)));
    assert!(
        b.allow(t0 + Duration::from_secs(31)),
        "half-open after cooldown"
    );
    b.record_failure(t0 + Duration::from_secs(31));
    assert!(
        !b.allow(t0 + Duration::from_secs(32)),
        "one failure while half-open re-opens"
    );
    assert!(b.allow(t0 + Duration::from_secs(62)));
    b.record_success();
    b.record_failure(t0 + Duration::from_secs(63));
    assert!(
        b.allow(t0 + Duration::from_secs(63)),
        "success fully resets the counter"
    );
}

#[test]
fn breaker_half_open_allows_single_probe_and_expires_lease() {
    let mut b = CircuitBreaker::default();
    let t0 = Instant::now();

    // Trip the breaker to open state
    b.record_failure(t0);
    b.record_failure(t0);
    b.record_failure(t0);
    assert!(!b.allow(t0));

    // Wait until cooldown elapses -> half-open
    let t_half_open = t0 + Duration::from_secs(31);
    assert!(b.allow(t_half_open), "first probe admitted");

    // Second allow() during an in-flight probe is false
    assert!(
        !b.allow(t_half_open),
        "second allow during in-flight probe is false"
    );
    assert!(
        !b.allow(t_half_open + Duration::from_secs(10)),
        "probe still in-flight"
    );

    // After the probe reports failure it is open again
    b.record_failure(t_half_open + Duration::from_secs(10));
    assert!(
        !b.allow(t_half_open + Duration::from_secs(11)),
        "failure re-opens the breaker"
    );

    // Cooldown again to half-open
    let t_half_open_2 = t_half_open + Duration::from_secs(10) + Duration::from_secs(31);
    assert!(
        b.allow(t_half_open_2),
        "probe admitted again after second cooldown"
    );

    // After success it is closed
    b.record_success();
    assert!(b.allow(t_half_open_2), "breaker is closed after success");
    assert!(
        b.allow(t_half_open_2),
        "subsequent requests are allowed when closed"
    );

    // Trip the breaker again to test lease expiration
    b.record_failure(t_half_open_2);
    b.record_failure(t_half_open_2);
    b.record_failure(t_half_open_2);
    assert!(!b.allow(t_half_open_2));

    let t_half_open_3 = t_half_open_2 + Duration::from_secs(31);
    assert!(b.allow(t_half_open_3), "probe admitted");
    assert!(!b.allow(t_half_open_3), "second allow blocked");

    // An unreported probe lease expires after 30s (Duration::from_secs(30))
    let t_expired = t_half_open_3 + Duration::from_secs(31);
    assert!(
        b.allow(t_expired),
        "unreported probe lease expired after 30s so a new probe is allowed"
    );
}

#[test]
fn error_status_and_code_mapping() {
    assert_eq!(PluginError::Unavailable("x".into()).status().as_u16(), 503);
    assert_eq!(
        PluginError::Unavailable("x".into()).code(),
        "PLUGIN_UNAVAILABLE"
    );
    assert_eq!(PluginError::Timeout.status().as_u16(), 504);
    assert_eq!(PluginError::Timeout.code(), "PLUGIN_TIMEOUT");
    assert_eq!(PluginError::BadResponse("x".into()).status().as_u16(), 502);
    assert_eq!(
        PluginError::BadResponse("x".into()).code(),
        "PLUGIN_BAD_RESPONSE"
    );
}

#[tokio::test]
async fn fetch_manifest_and_health() {
    let mock = spawn_mock("hello", &["items:publish"]).await;
    let client = PluginClient::new(Duration::from_secs(2));

    let m = client.fetch_manifest(&mock.base_url).await.unwrap();
    assert_eq!(m.id, "hello");

    client.health(&mock.base_url, "any").await.unwrap();
    mock.healthy.store(false, Ordering::SeqCst);
    assert!(matches!(
        client.health(&mock.base_url, "any").await,
        Err(PluginError::Unavailable(_))
    ));
}

#[tokio::test]
async fn unreachable_plugin_is_unavailable() {
    let client = PluginClient::new(Duration::from_secs(1));
    assert!(matches!(
        client.fetch_manifest("http://127.0.0.1:1").await,
        Err(PluginError::Unavailable(_))
    ));
}

#[tokio::test]
async fn resolve_sends_bearer_secret_and_parses_descriptor() {
    let mock = spawn_mock("hello", &["playback:resolve"]).await;
    mock.set_secret("s3cret");
    let client = PluginClient::new(Duration::from_secs(2));

    let d = client
        .resolve("hello", &mock.base_url, "s3cret", &req())
        .await
        .unwrap();
    assert_eq!(d.mode, PlaybackMode::Direct);
    assert_eq!(
        mock.last_resolve_body.lock().unwrap().as_ref().unwrap()["remote_id"],
        "ch1"
    );

    // Wrong secret -> plugin answers 401 -> treated as a bad response, not an outage.
    let err = client
        .resolve("hello", &mock.base_url, "wrong", &req())
        .await
        .unwrap_err();
    assert!(matches!(err, PluginError::BadResponse(_)), "got {err:?}");
}

#[tokio::test]
async fn resolve_times_out() {
    let mock = spawn_mock("slow", &["playback:resolve"]).await;
    mock.resolve_delay_ms.store(1500, Ordering::SeqCst);
    let client = PluginClient::new(Duration::from_millis(300));
    assert!(matches!(
        client.resolve("slow", &mock.base_url, "s", &req()).await,
        Err(PluginError::Timeout)
    ));
}

#[tokio::test]
async fn resolve_maps_server_errors_and_bad_json() {
    let mock = spawn_mock("bad", &["playback:resolve"]).await;
    let client = PluginClient::new(Duration::from_secs(2));

    mock.set_descriptor(json!({ "nope": 1 }));
    assert!(matches!(
        client.resolve("bad", &mock.base_url, "s", &req()).await,
        Err(PluginError::BadResponse(_))
    ));

    mock.resolve_status.store(500, Ordering::SeqCst);
    assert!(matches!(
        client.resolve("bad", &mock.base_url, "s", &req()).await,
        Err(PluginError::Unavailable(_))
    ));
}

#[tokio::test]
async fn circuit_opens_after_repeated_failures_and_stops_calling_the_plugin() {
    let mock = spawn_mock("flaky", &["playback:resolve"]).await;
    mock.resolve_status.store(500, Ordering::SeqCst);
    let client = PluginClient::new(Duration::from_secs(2));

    for _ in 0..3 {
        assert!(client
            .resolve("flaky", &mock.base_url, "s", &req())
            .await
            .is_err());
    }
    assert_eq!(mock.calls(), 3);

    let err = client
        .resolve("flaky", &mock.base_url, "s", &req())
        .await
        .unwrap_err();
    assert!(matches!(err, PluginError::Unavailable(_)));
    assert_eq!(mock.calls(), 3, "open circuit must not reach the plugin");
}
