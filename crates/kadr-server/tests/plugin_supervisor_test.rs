#![cfg(unix)]

use kadr_server::plugins::supervisor::{kadr_local_url, PluginSupervisor, SpawnSpec, StartOutcome};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn kadr_local_url_formats_wildcard_ipv4_and_ipv6_hosts() {
    assert_eq!(kadr_local_url("0.0.0.0", 8492), "http://127.0.0.1:8492");
    assert_eq!(kadr_local_url("::", 8492), "http://127.0.0.1:8492");
    assert_eq!(kadr_local_url("[::]", 8492), "http://127.0.0.1:8492");
    assert_eq!(kadr_local_url("::1", 8492), "http://[::1]:8492");
    assert_eq!(kadr_local_url("127.0.0.1", 9000), "http://127.0.0.1:9000");
}

#[tokio::test]
async fn supervisor_spawns_isolated_env_waits_for_health_restarts_on_crash_and_stops_cleanly() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("plugin");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    // Parent secret env that MUST NOT leak to the child process:
    std::env::set_var("LEAK_CANARY_TEST", "should-be-stripped");

    let script = r#"#!/usr/bin/env python3
import http.server, json, os, pathlib, signal, sys

pathlib.Path(os.environ["PLUGIN_DATA_DIR"], "env.json").write_text(json.dumps({
    "bind": os.environ.get("PLUGIN_BIND"),
    "kadr_url": os.environ.get("KADR_URL"),
    "secret": os.environ.get("KADR_PLUGIN_SECRET"),
    "token": os.environ.get("KADR_PLUGIN_TOKEN"),
    "canary": os.environ.get("LEAK_CANARY_TEST"),
    "cwd": os.getcwd(),
}))

expected = "Bearer " + os.environ["KADR_PLUGIN_SECRET"]
host, port = os.environ["PLUGIN_BIND"].split(":")

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        if self.path == "/health":
            if self.headers.get("Authorization") == expected:
                self.send_response(200); self.end_headers(); self.wfile.write(b"ok")
            else:
                self.send_response(401); self.end_headers()
        elif self.path == "/crash":
            self.send_response(200); self.end_headers()
            os._exit(1)

signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
http.server.HTTPServer((host, int(port)), Handler).serve_forever()
"#;
    std::fs::write(&bin_path, script).unwrap();
    std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let sup = PluginSupervisor::new();
    let ports: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let ports_cb = ports.clone();

    let outcome = sup
        .start(
            SpawnSpec {
                plugin_id: "hello".to_string(),
                bin_path: bin_path.clone(),
                data_dir: data_dir.clone(),
                kadr_url: "http://127.0.0.1:8492".to_string(),
                secret: "sec-123".to_string(),
                token: "tok-456".to_string(),
            },
            Duration::from_secs(5),
            Some(Arc::new(move |_id, url| {
                ports_cb.lock().unwrap().push(url);
            })),
        )
        .await;

    let base_url = match outcome {
        StartOutcome::Healthy { base_url } => base_url,
        StartOutcome::Unhealthy { reason, .. } => panic!("expected healthy, got: {reason}"),
    };
    assert!(sup.is_running("hello").await);

    let env_dump: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data_dir.join("env.json")).unwrap()).unwrap();
    assert_eq!(env_dump["kadr_url"], "http://127.0.0.1:8492");
    assert_eq!(env_dump["secret"], "sec-123");
    assert_eq!(env_dump["token"], "tok-456");
    assert!(env_dump["canary"].is_null(), "parent env must be cleared");

    // Trigger an unexpected crash and verify supervisor restarts it (~1s backoff).
    let _ = reqwest::get(format!("{base_url}/crash")).await;
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let recorded = ports.lock().unwrap().clone();
    assert!(
        recorded.len() >= 2,
        "expected respawn port callback, got {recorded:?}"
    );
    let new_url = recorded.last().unwrap().clone();
    let resp = reqwest::Client::new()
        .get(format!("{new_url}/health"))
        .bearer_auth("sec-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Stop cleanly and confirm the HTTP port is no longer serving.
    sup.stop("hello").await;
    assert!(!sup.is_running("hello").await);
    assert!(reqwest::get(format!("{new_url}/health")).await.is_err());
}
