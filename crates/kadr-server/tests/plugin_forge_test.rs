use axum::{
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::get,
    Json, Router,
};
use kadr_core::plugin::ForgeKind;
use kadr_server::plugins::forge::{
    parse_repo_url, select_target_asset, verify_checksum, ForgeClient, ForgeError, ReleaseAsset,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Duration;

fn asset(name: &str) -> ReleaseAsset {
    ReleaseAsset {
        name: name.to_string(),
        download_url: format!("https://example.com/{name}"),
    }
}

#[test]
fn repo_url_parsing_strips_git_and_enforces_scheme() {
    let r = parse_repo_url("https://github.com/acme/plugin-iptv.git/", false).unwrap();
    assert_eq!(r.canonical_url, "https://github.com/acme/plugin-iptv");
    assert_eq!(r.segments, vec!["acme", "plugin-iptv"]);

    let gl = parse_repo_url("https://gitlab.com/group/sub/plugin-iptv", false).unwrap();
    assert_eq!(gl.segments, vec!["group", "sub", "plugin-iptv"]);

    assert!(parse_repo_url("http://github.com/acme/repo", false).is_err());
    assert!(parse_repo_url("http://127.0.0.1:8080/acme/repo", true).is_ok());
    assert!(parse_repo_url("https://github.com/only-owner", false).is_err());
    assert!(parse_repo_url("https://github.com/acme/repo?foo=1", false).is_err());
}

#[test]
fn target_asset_prefers_canonical_then_aliases_and_rejects_ambiguous() {
    let assets = vec![
        asset("hello-linux-amd64.tar.gz"),
        asset("hello-linux-x86_64.tar.gz"),
        asset("hello-darwin-arm64.tar.gz"),
    ];
    let exact = select_target_asset(&assets, "linux", "x86_64").unwrap();
    assert_eq!(exact.name, "hello-linux-x86_64.tar.gz");

    let alias = select_target_asset(&assets, "macos", "aarch64").unwrap();
    assert_eq!(alias.name, "hello-darwin-arm64.tar.gz");

    let missing = select_target_asset(&assets, "linux", "riscv64").unwrap_err();
    assert!(matches!(missing, ForgeError::InvalidRequest(msg) if msg.contains("linux-riscv64")));

    let dup = vec![
        asset("a-linux-x86_64.tar.gz"),
        asset("b-linux-x86_64.tar.gz"),
    ];
    assert!(matches!(
        select_target_asset(&dup, "linux", "x86_64").unwrap_err(),
        ForgeError::InvalidRequest(_)
    ));
}

#[test]
fn checksum_verification_handles_single_and_multi_file_formats() {
    let digest = "a".repeat(64);
    verify_checksum(
        "p-linux-x86_64.tar.gz",
        "p-linux-x86_64.tar.gz.sha256",
        &format!("{digest}\n"),
        &digest,
    )
    .unwrap();

    let sums = format!(
        "{}  other.tar.gz\n{} *p-linux-x86_64.tar.gz\n",
        "b".repeat(64),
        digest
    );
    verify_checksum("p-linux-x86_64.tar.gz", "checksums.txt", &sums, &digest).unwrap();

    assert!(matches!(
        verify_checksum(
            "p-linux-x86_64.tar.gz",
            "checksums.txt",
            &sums,
            &"c".repeat(64)
        ),
        Err(ForgeError::ChecksumMismatch(_))
    ));
    assert!(matches!(
        verify_checksum("missing.tar.gz", "checksums.txt", &sums, &digest),
        Err(ForgeError::ChecksumMismatch(_))
    ));
}

#[tokio::test]
async fn forge_client_fetches_github_gitea_and_gitlab_subgroup_with_redirects_and_checksums() {
    let payload = b"fake-tar-gz-bytes".to_vec();
    let hex_hash = format!("{:x}", Sha256::digest(&payload));
    let target_name = format!(
        "hello-{}-{}.tar.gz",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let b_gh = base.clone();
    let t_gh = target_name.clone();
    let b_gt = base.clone();
    let t_gt = target_name.clone();
    let b_gl = base.clone();
    let t_gl = target_name.clone();
    let sums_body = format!("{hex_hash}  {target_name}\n");
    let archive_body = payload.clone();

    let app = Router::new()
        .route(
            "/repos/acme/hello/releases/latest",
            get(move |headers: HeaderMap| {
                let b = b_gh.clone();
                let t = t_gh.clone();
                async move {
                    if !headers.contains_key(header::USER_AGENT) {
                        return StatusCode::FORBIDDEN.into_response();
                    }
                    Json(json!({
                        "tag_name": "v1.0.0",
                        "assets": [
                            { "name": t, "browser_download_url": format!("{b}/cdn/redirect") },
                            { "name": "checksums.txt", "browser_download_url": format!("{b}/files/checksums.txt") }
                        ]
                    }))
                    .into_response()
                }
            }),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/releases/latest",
            get(move |Path((owner, _repo)): Path<(String, String)>| {
                let b = b_gt.clone();
                let t = t_gt.clone();
                async move {
                    if owner != "gitea-owner" {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    Json(json!({
                        "tag_name": "v2.0.0",
                        "assets": [
                            { "name": t, "browser_download_url": format!("{b}/files/archive.tar.gz") }
                        ]
                    }))
                    .into_response()
                }
            }),
        )
        .route(
            "/api/v4/projects/{project}/releases/permalink/latest",
            get({
                let b = base.clone();
                move |Path(project): Path<String>| {
                    let b = b.clone();
                    async move {
                        let enc = urlencoding::encode(&project);
                        Redirect::temporary(&format!("{b}/api/v4/projects/{enc}/releases/v3.0.0"))
                    }
                }
            }),
        )
        .route(
            "/api/v4/projects/{project}/releases/{tag}",
            get(move |Path((_project, tag)): Path<(String, String)>| {
                let b = b_gl.clone();
                let t = t_gl.clone();
                async move {
                    Json(json!({
                        "tag_name": tag,
                        "assets": {
                            "links": [
                                { "name": t, "direct_asset_url": format!("{b}/files/archive.tar.gz") }
                            ]
                        }
                    }))
                }
            }),
        )
        .route(
            "/cdn/redirect",
            get({
                let b = base.clone();
                move || {
                    let b = b.clone();
                    async move { Redirect::temporary(&format!("{b}/files/archive.tar.gz")) }
                }
            }),
        )
        .route(
            "/files/archive.tar.gz",
            get(move || {
                let body = archive_body.clone();
                async move { body }
            }),
        )
        .route(
            "/files/checksums.txt",
            get(move || {
                let s = sums_body.clone();
                async move { s }
            }),
        );

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = ForgeClient::new(Duration::from_secs(5)).with_github_api_base(&base);

    // 1. GitHub (via hint + overridden API base) with CDN redirect + checksum verification
    let gh = client
        .fetch_release_bundle(
            &format!("{base}/acme/hello"),
            None,
            Some(ForgeKind::Github),
            true,
        )
        .await
        .unwrap();
    assert_eq!(gh.forge_kind, ForgeKind::Github);
    assert_eq!(gh.tag_name, "v1.0.0");
    assert_eq!(gh.archive_bytes, payload);

    // 2. Self-hosted Gitea auto-detected on 2-segment path
    let gt = client
        .fetch_release_bundle(&format!("{base}/gitea-owner/hello"), None, None, true)
        .await
        .unwrap();
    assert_eq!(gt.forge_kind, ForgeKind::Gitea);
    assert_eq!(gt.tag_name, "v2.0.0");

    // 3. Self-hosted GitLab auto-detected via 404 fallback on 2-segment path
    let gl2 = client
        .fetch_release_bundle(&format!("{base}/group/hello"), None, None, true)
        .await
        .unwrap();
    assert_eq!(gl2.forge_kind, ForgeKind::Gitlab);
    assert_eq!(gl2.tag_name, "v3.0.0");

    // 4. Self-hosted GitLab nested subgroup (>2 segments goes straight to GitLab v4)
    let gl3 = client
        .fetch_release_bundle(&format!("{base}/group/sub/hello"), None, None, true)
        .await
        .unwrap();
    assert_eq!(gl3.forge_kind, ForgeKind::Gitlab);
    assert_eq!(gl3.tag_name, "v3.0.0");

    // 5. SSRF block when allow_private = false
    let blocked = client
        .fetch_release_bundle(&format!("{base}/gitea-owner/hello"), None, None, false)
        .await
        .unwrap_err();
    assert!(matches!(blocked, ForgeError::InvalidRequest(_)));
}
