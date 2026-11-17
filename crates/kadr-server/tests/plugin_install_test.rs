#![cfg(unix)]

mod plugin_support;

use axum::{extract::State, http::StatusCode, routing::get, Extension, Json, Router};
use flate2::{write::GzEncoder, Compression};
use kadr_server::plugins::{forge::ForgeClient, PluginService};
use plugin_support::{build, call, call_raw, enabled_settings};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn build_plugin_tar_gz(manifest: &Value, script: &str) -> Vec<u8> {
    let manifest_bytes = serde_json::to_vec(manifest).unwrap();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut tar = tar::Builder::new(&mut gz);
        for (name, data, mode) in [
            ("manifest.json", manifest_bytes.as_slice(), 0o644),
            ("plugin", script.as_bytes(), 0o755),
            (
                "web/dist/index.html",
                b"<h1>Photos UI</h1>".as_slice(),
                0o644,
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(mode);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, name, data).unwrap();
        }
        tar.finish().unwrap();
    }
    gz.finish().unwrap()
}

#[derive(Clone)]
struct ForgeState {
    tag: Arc<Mutex<String>>,
    archive: Arc<Mutex<Vec<u8>>>,
}

#[tokio::test]
async fn managed_plugin_install_approve_publish_resolve_update_and_remove_lifecycle() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let mut h = build(enabled_settings()).await;

    // Bind real TCP listener for Kadr so the spawned child plugin can call POST /api/v1/plugin/*
    let kadr_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let kadr_port = kadr_listener.local_addr().unwrap().port();

    let plugin_script = r#"#!/usr/bin/env python3
import http.server, json, os, pathlib, signal, sys, threading, urllib.request

data_dir = pathlib.Path(os.environ["PLUGIN_DATA_DIR"])
exe_dir = pathlib.Path(__file__).resolve().parent
cwd_web = pathlib.Path("web/dist/index.html").is_file()
exe_web = (exe_dir / "web/dist/index.html").is_file()
(data_dir / "boot_marker.txt").write_text(json.dumps({"cwd_web": cwd_web, "exe_web": exe_web}))
(data_dir / "persistent.txt").write_text("user-state")

expected = "Bearer " + os.environ["KADR_PLUGIN_SECRET"]
kadr_url = os.environ["KADR_URL"]
token = os.environ["KADR_PLUGIN_TOKEN"]
host, port = os.environ["PLUGIN_BIND"].split(":")

def publish():
    for method, path, body in [
        ("POST", "/api/v1/plugin/libraries", {"id": "git-hello-lib", "name": "Git Hello", "media_type": "movie"}),
        ("PUT", "/api/v1/plugin/libraries/git-hello-lib/items", {"items": [{"remote_id": "vid1", "title": "Git Video 1", "duration_seconds": 120}]}),
    ]:
        req = urllib.request.Request(
            kadr_url + path,
            data=json.dumps(body).encode(),
            headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
            method=method,
        )
        try:
            urllib.request.urlopen(req, timeout=3)
        except Exception:
            pass

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        if self.path == "/health":
            code = 200 if self.headers.get("Authorization") == expected else 401
            self.send_response(code); self.end_headers()
    def do_POST(self):
        if self.path == "/playback/resolve":
            if self.headers.get("Authorization") != expected:
                self.send_response(401); self.end_headers(); return
            body = json.dumps({"mode": "direct", "url": "https://cdn.example.com/git-vid1.mp4"}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)

signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
srv = http.server.HTTPServer((host, int(port)), Handler)
threading.Thread(target=publish, daemon=True).start()
srv.serve_forever()
"#;

    let v1_manifest = json!({
        "id": "githello",
        "name": "Git Hello",
        "version": "0.1.0",
        "api_version": 1,
        "permissions": ["items:publish"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    });
    let v1_archive = build_plugin_tar_gz(&v1_manifest, plugin_script);

    let forge_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let forge_base = format!("http://{}", forge_listener.local_addr().unwrap());
    let target_asset = format!(
        "githello-{}-{}.tar.gz",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let forge_state = ForgeState {
        tag: Arc::new(Mutex::new("v0.1.0".to_string())),
        archive: Arc::new(Mutex::new(v1_archive)),
    };
    let fb = forge_base.clone();
    let ta = target_asset.clone();
    let ta2 = target_asset.clone();
    let forge_app = Router::new()
        .route(
            "/repos/acme/githello/releases/latest",
            get(move |State(st): State<ForgeState>| {
                let fb = fb.clone();
                let ta = ta.clone();
                async move {
                    let tag = st.tag.lock().unwrap().clone();
                    Json(json!({
                        "tag_name": tag,
                        "assets": [
                            { "name": ta, "browser_download_url": format!("{fb}/download/bundle.tar.gz") },
                            { "name": "checksums.txt", "browser_download_url": format!("{fb}/download/checksums.txt") }
                        ]
                    }))
                }
            }),
        )
        .route(
            "/download/bundle.tar.gz",
            get(|State(st): State<ForgeState>| async move { st.archive.lock().unwrap().clone() }),
        )
        .route(
            "/download/checksums.txt",
            get(move |State(st): State<ForgeState>| {
                let ta2 = ta2.clone();
                async move {
                    let bytes = st.archive.lock().unwrap().clone();
                    format!("{:x}  {ta2}\n", Sha256::digest(&bytes))
                }
            }),
        )
        .with_state(forge_state.clone());
    tokio::spawn(async move {
        axum::serve(forge_listener, forge_app).await.unwrap();
    });

    let mut cfg = kadr_server::config::AppConfig {
        plugins: enabled_settings(),
        ..Default::default()
    };
    cfg.server.host = "127.0.0.1".to_string();
    cfg.server.port = kadr_port;
    cfg.server.data_dir = h.data_dir.path().to_path_buf();
    let shared_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
    let svc = PluginService::new(
        h.media_repo.pool().clone(),
        h.jwt_secret.as_bytes(),
        shared_cfg,
        h.events.clone(),
    )
    .with_forge_client(ForgeClient::new(Duration::from_secs(5)).with_github_api_base(&forge_base));

    h.app = h.app.layer(Extension(svc.clone()));
    let app_for_server = h.app.clone();
    tokio::spawn(async move {
        axum::serve(kadr_listener, app_for_server).await.unwrap();
    });

    // 1. Install from Git -> status must be pending, and binary MUST NOT have executed yet!
    let (status, body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/install",
        Some(&h.admin_token),
        Some(json!({
            "repo_url": format!("{forge_base}/acme/githello"),
            "forge": "github"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "install failed: {body}");
    assert_eq!(body["plugin"]["status"], "pending");
    assert_eq!(body["plugin"]["install_kind"], "managed");
    assert_eq!(body["plugin"]["installed_tag"], "v0.1.0");
    let boot_marker = h
        .data_dir
        .path()
        .join("plugins/githello/data/boot_marker.txt");
    assert!(
        !boot_marker.exists(),
        "binary must never run before admin permission approval"
    );

    // 2. Approve -> spawns supervised child, which verifies static assets & publishes 'Git Video 1'!
    let (status, approve_body) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/approve",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "approve failed: {approve_body}");
    assert_eq!(approve_body["plugin"]["status"], "enabled");
    assert!(svc.supervisor().is_running("githello").await);
    assert!(boot_marker.exists());
    let boot_info: Value = serde_json::from_slice(&std::fs::read(&boot_marker).unwrap()).unwrap();
    assert_eq!(boot_info["cwd_web"], true);
    assert_eq!(boot_info["exe_web"], true);

    // Wait briefly for async publish thread in the plugin to finish creating its virtual library
    for _ in 0..30 {
        if h.media_repo
            .count_by_library("git-hello-lib")
            .await
            .unwrap_or(0)
            == 1
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        h.media_repo
            .count_by_library("git-hello-lib")
            .await
            .unwrap(),
        1
    );

    // 3. Update when tag unchanged -> updated: false
    let (status, noop) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/update",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noop["updated"], false);

    // 4. Update v0.2.0 with same permissions -> seamless restart, requires_approval: false
    let v2_manifest = json!({
        "id": "githello",
        "name": "Git Hello",
        "version": "0.2.0",
        "api_version": 1,
        "permissions": ["items:publish"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    });
    *forge_state.tag.lock().unwrap() = "v0.2.0".to_string();
    *forge_state.archive.lock().unwrap() = build_plugin_tar_gz(&v2_manifest, plugin_script);

    let (status, up_v2) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/update",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update v2 failed: {up_v2}");
    assert_eq!(up_v2["updated"], true);
    assert_eq!(up_v2["previous_version"], "0.1.0");
    assert_eq!(up_v2["requires_approval"], false);
    assert_eq!(up_v2["plugin"]["manifest"]["version"], "0.2.0");
    assert!(svc.supervisor().is_running("githello").await);

    // 5. Update v0.3.0 requesting NEW permission playback:resolve -> stops child & drops to pending!
    let v3_manifest = json!({
        "id": "githello",
        "name": "Git Hello",
        "version": "0.3.0",
        "api_version": 1,
        "permissions": ["items:publish", "playback:resolve"],
        "capabilities": { "virtual_library": true, "playback_resolver": true }
    });
    *forge_state.tag.lock().unwrap() = "v0.3.0".to_string();
    *forge_state.archive.lock().unwrap() = build_plugin_tar_gz(&v3_manifest, plugin_script);

    let (status, up_v3) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/update",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(up_v3["updated"], true);
    assert_eq!(up_v3["requires_approval"], true);
    assert_eq!(up_v3["plugin"]["status"], "pending");
    assert!(!svc.supervisor().is_running("githello").await);

    // Re-approve v0.3.0 with playback:resolve and verify direct stream redirect works!
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/approve",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let item = h
        .media_repo
        .find_by_path(std::path::Path::new("plugin://githello/vid1"))
        .await
        .unwrap()
        .unwrap();
    let (stream_status, headers, _) = call_raw(
        &h.app,
        "GET",
        &format!("/api/v1/stream/{}", item.id.unwrap()),
        Some(&h.user_token),
        &[],
        None,
    )
    .await;
    assert_eq!(stream_status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        headers.get("location").unwrap(),
        "https://cdn.example.com/git-vid1.mp4"
    );

    // 6. Disable stops process; Remove with purge=true cleans up entire plugin directory.
    let (status, _) = call(
        &h.app,
        "POST",
        "/api/v1/admin/plugins/githello/disable",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!svc.supervisor().is_running("githello").await);

    let (status, _) = call(
        &h.app,
        "DELETE",
        "/api/v1/admin/plugins/githello?purge=true",
        Some(&h.admin_token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!h.data_dir.path().join("plugins/githello").exists());

    svc.shutdown_managed_plugins().await;
}
