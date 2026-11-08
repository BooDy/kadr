use kadr_core::plugin::PlaybackDescriptor;
use kadr_server::plugins::url_guard::{is_private_ip, validate_descriptor};
use std::net::IpAddr;

fn desc(json: &str) -> PlaybackDescriptor {
    serde_json::from_str(json).unwrap()
}

#[test]
fn private_and_special_ranges_are_detected() {
    for ip in [
        "127.0.0.1", "10.0.0.1", "192.168.1.1", "172.16.0.1", "169.254.1.1", "0.0.0.0",
        "0.1.2.3", "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1",
        "::127.0.0.1", "::10.0.0.1",
    ] {
        assert!(is_private_ip(ip.parse::<IpAddr>().unwrap()), "{ip} should be private");
    }
    for ip in ["8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
        assert!(!is_private_ip(ip.parse::<IpAddr>().unwrap()), "{ip} should be public");
    }
}

#[tokio::test]
async fn rejects_non_http_schemes_and_garbage() {
    assert!(validate_descriptor(&desc(r#"{"mode":"direct","url":"ftp://x/y"}"#), false).await.is_err());
    assert!(validate_descriptor(&desc(r#"{"mode":"direct","url":"file:///etc/passwd"}"#), true).await.is_err());
    assert!(validate_descriptor(&desc(r#"{"mode":"direct","url":"not a url"}"#), false).await.is_err());
}

#[tokio::test]
async fn direct_descriptors_cannot_carry_headers() {
    let d = desc(r#"{"mode":"direct","url":"https://cdn.example.com/a.mp4","headers":{"X-A":"b"}}"#);
    assert!(validate_descriptor(&d, false).await.is_err());
}

#[tokio::test]
async fn header_injection_is_rejected() {
    let d = desc(r#"{"mode":"proxy","url":"http://8.8.8.8/a","headers":{"X-A":"b\r\nEvil: 1"}}"#);
    assert!(validate_descriptor(&d, true).await.is_err());
    let d = desc(r#"{"mode":"proxy","url":"http://8.8.8.8/a","headers":{"Bad:Name":"b"}}"#);
    assert!(validate_descriptor(&d, true).await.is_err());
}

#[tokio::test]
async fn proxy_and_transcode_block_private_hosts_unless_allowed() {
    for mode in ["proxy", "transcode"] {
        let ip = desc(&format!(r#"{{"mode":"{mode}","url":"http://127.0.0.1:9/x"}}"#));
        assert!(validate_descriptor(&ip, false).await.is_err());
        assert!(validate_descriptor(&ip, true).await.is_ok());

        let host = desc(&format!(r#"{{"mode":"{mode}","url":"http://localhost:9/x"}}"#));
        assert!(validate_descriptor(&host, false).await.is_err());

        let v6 = desc(&format!(r#"{{"mode":"{mode}","url":"http://[::1]:9/x"}}"#));
        assert!(validate_descriptor(&v6, false).await.is_err());

        let v6_compat = desc(&format!(r#"{{"mode":"{mode}","url":"http://[::127.0.0.1]:9/x"}}"#));
        assert!(validate_descriptor(&v6_compat, false).await.is_err());
        assert!(validate_descriptor(&v6_compat, true).await.is_ok());

        let public = desc(&format!(r#"{{"mode":"{mode}","url":"http://8.8.8.8/x"}}"#));
        assert!(validate_descriptor(&public, false).await.is_ok());
    }
}

#[tokio::test]
async fn direct_urls_may_point_at_private_hosts() {
    let d = desc(r#"{"mode":"direct","url":"http://192.168.1.50/stream.m3u8"}"#);
    assert!(validate_descriptor(&d, false).await.is_ok());
}
