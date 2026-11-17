use crate::plugins::url_guard::ensure_public_host;
use axum::http::header;
use kadr_core::plugin::ForgeKind;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_CHECKSUM_BYTES: usize = 64 * 1024;
const MAX_REDIRECTS: usize = 5;
const USER_AGENT_VALUE: &str = concat!("kadr/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, thiserror::Error)]
pub enum ForgeError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    ChecksumMismatch(String),
    #[error("{0}")]
    ForgeError(String),
    #[error("Forge request timed out")]
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRepo {
    pub canonical_url: String,
    pub origin: String,
    pub host: String,
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone)]
pub struct DownloadedBundle {
    pub forge_kind: ForgeKind,
    pub canonical_repo_url: String,
    pub tag_name: String,
    pub asset_name: String,
    pub archive_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ResolvedRelease {
    forge_kind: ForgeKind,
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

pub fn parse_repo_url(raw: &str, allow_private: bool) -> Result<ParsedRepo, ForgeError> {
    let trimmed = raw.trim();
    let url = reqwest::Url::parse(trimmed)
        .map_err(|e| ForgeError::InvalidRequest(format!("invalid repo_url: {e}")))?;
    let scheme = url.scheme();
    if scheme != "https" && !(allow_private && scheme == "http") {
        return Err(ForgeError::InvalidRequest(
            "repo_url must use https:// (or http:// when allow_private_proxy is enabled)"
                .to_string(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ForgeError::InvalidRequest(
            "repo_url must not include query parameters or fragments".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ForgeError::InvalidRequest("repo_url has no host".to_string()))?
        .to_ascii_lowercase();
    let mut segments: Vec<String> = url
        .path_segments()
        .map(|it| {
            it.filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    if let Some(last) = segments.last_mut() {
        if let Some(stripped) = last.strip_suffix(".git") {
            *last = stripped.to_string();
        }
    }
    if segments.len() < 2
        || segments
            .iter()
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(ForgeError::InvalidRequest(
            "repo_url must include at least owner and repository path segments".to_string(),
        ));
    }
    let origin = match url.port() {
        Some(port) => format!("{}://{}:{}", scheme, host, port),
        None => format!("{}://{}", scheme, host),
    };
    let canonical_url = format!("{origin}/{}", segments.join("/"));
    Ok(ParsedRepo {
        canonical_url,
        origin,
        host,
        segments,
    })
}

fn os_aliases(os: &str) -> Vec<&str> {
    match os {
        "macos" => vec!["darwin"],
        _ => vec![],
    }
}

fn arch_aliases(arch: &str) -> Vec<&str> {
    match arch {
        "x86_64" => vec!["amd64"],
        "aarch64" => vec!["arm64"],
        _ => vec![],
    }
}

pub fn select_target_asset<'a>(
    assets: &'a [ReleaseAsset],
    os: &str,
    arch: &str,
) -> Result<&'a ReleaseAsset, ForgeError> {
    let canonical_suffix = format!("-{os}-{arch}.tar.gz");
    let exact: Vec<&ReleaseAsset> = assets
        .iter()
        .filter(|a| a.name.to_ascii_lowercase().ends_with(&canonical_suffix))
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    if exact.len() > 1 {
        let names: Vec<&str> = exact.iter().map(|a| a.name.as_str()).collect();
        return Err(ForgeError::InvalidRequest(format!(
            "multiple release assets match '{canonical_suffix}': {}",
            names.join(", ")
        )));
    }

    let mut alias_suffixes = Vec::new();
    let mut os_list = vec![os];
    os_list.extend(os_aliases(os));
    let mut arch_list = vec![arch];
    arch_list.extend(arch_aliases(arch));
    for o in &os_list {
        for a in &arch_list {
            if *o == os && *a == arch {
                continue;
            }
            alias_suffixes.push(format!("-{o}-{a}.tar.gz"));
        }
    }

    let aliased: Vec<&ReleaseAsset> = assets
        .iter()
        .filter(|asset| {
            let lower = asset.name.to_ascii_lowercase();
            alias_suffixes.iter().any(|s| lower.ends_with(s))
        })
        .collect();
    if aliased.len() == 1 {
        return Ok(aliased[0]);
    }
    if aliased.len() > 1 {
        let names: Vec<&str> = aliased.iter().map(|a| a.name.as_str()).collect();
        return Err(ForgeError::InvalidRequest(format!(
            "multiple release assets match target '{os}-{arch}': {}",
            names.join(", ")
        )));
    }

    Err(ForgeError::InvalidRequest(format!(
        "no release asset matching '*{canonical_suffix}' found for host target {os}-{arch}"
    )))
}

fn find_checksum_asset<'a>(
    assets: &'a [ReleaseAsset],
    archive_name: &str,
) -> Option<&'a ReleaseAsset> {
    let exact_sha = format!("{archive_name}.sha256");
    if let Some(a) = assets.iter().find(|a| a.name == exact_sha) {
        return Some(a);
    }
    for name in ["checksums.txt", "SHA256SUMS"] {
        if let Some(a) = assets.iter().find(|a| a.name == name) {
            return Some(a);
        }
    }
    assets.iter().find(|a| {
        let lower = a.name.to_ascii_lowercase();
        lower.contains("sha256")
            && !lower.ends_with(".tar.gz.sha256")
            && !lower.ends_with(".tar.gz")
            && !lower.ends_with(".sig")
            && !lower.ends_with(".asc")
    })
}

pub fn verify_checksum(
    archive_name: &str,
    checksum_asset_name: &str,
    checksum_text: &str,
    actual_sha256_hex: &str,
) -> Result<(), ForgeError> {
    let single_file_sha = checksum_asset_name.ends_with(".sha256");
    let mut expected_hex: Option<String> = None;

    for raw_line in checksum_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        match parts.next() {
            None if single_file_sha => {
                expected_hex = Some(hash.to_ascii_lowercase());
                break;
            }
            Some(file_token) => {
                let fname = file_token.trim_start_matches('*');
                let basename = fname.rsplit('/').next().unwrap_or(fname);
                if basename == archive_name || (single_file_sha && fname == "-") {
                    expected_hex = Some(hash.to_ascii_lowercase());
                    break;
                }
            }
            None => {}
        }
    }

    let Some(expected) = expected_hex else {
        return Err(ForgeError::ChecksumMismatch(format!(
            "checksum asset '{checksum_asset_name}' does not contain an entry for '{archive_name}'"
        )));
    };
    if expected != actual_sha256_hex.to_ascii_lowercase() {
        return Err(ForgeError::ChecksumMismatch(format!(
            "SHA-256 mismatch for '{archive_name}' (expected {expected}, got {actual_sha256_hex})"
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct GithubOrGiteaRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubOrGiteaAsset>,
}

#[derive(Deserialize)]
struct GithubOrGiteaAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GitlabRelease {
    tag_name: String,
    #[serde(default)]
    assets: GitlabAssets,
}

#[derive(Default, Deserialize)]
struct GitlabAssets {
    #[serde(default)]
    links: Vec<GitlabAssetLink>,
}

#[derive(Deserialize)]
struct GitlabAssetLink {
    name: String,
    #[serde(default)]
    direct_asset_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Clone)]
pub struct ForgeClient {
    http: reqwest::Client,
    timeout_ms: Arc<AtomicU64>,
    github_api_base: String,
}

impl ForgeClient {
    pub fn new(timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client builds with static configuration");
        Self {
            http,
            timeout_ms: Arc::new(AtomicU64::new(timeout.as_millis().max(1000) as u64)),
            github_api_base: "https://api.github.com".to_string(),
        }
    }

    pub fn with_github_api_base(mut self, base: &str) -> Self {
        self.github_api_base = base.trim_end_matches('/').to_string();
        self
    }

    pub fn set_timeout(&self, timeout: Duration) {
        self.timeout_ms
            .store(timeout.as_millis().max(1000) as u64, Ordering::Relaxed);
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.load(Ordering::Relaxed))
    }

    async fn validate_hop(
        &self,
        url: &reqwest::Url,
        allow_private: bool,
    ) -> Result<(), ForgeError> {
        let scheme = url.scheme();
        if scheme != "https" && !(allow_private && scheme == "http") {
            return Err(ForgeError::InvalidRequest(format!(
                "disallowed URL scheme '{scheme}'"
            )));
        }
        if !allow_private {
            ensure_public_host(url)
                .await
                .map_err(ForgeError::InvalidRequest)?;
        }
        Ok(())
    }

    async fn get_following_redirects(
        &self,
        raw_url: &str,
        accept: Option<&str>,
        timeout: Duration,
        allow_private: bool,
    ) -> Result<reqwest::Response, ForgeError> {
        let mut current = reqwest::Url::parse(raw_url)
            .map_err(|e| ForgeError::InvalidRequest(format!("invalid url '{raw_url}': {e}")))?;

        for _hop in 0..=MAX_REDIRECTS {
            self.validate_hop(&current, allow_private).await?;
            let mut req = self
                .http
                .get(current.clone())
                .header(header::USER_AGENT, USER_AGENT_VALUE)
                .timeout(timeout);
            if let Some(a) = accept {
                req = req.header(header::ACCEPT, a);
            }
            let resp = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    ForgeError::Timeout
                } else {
                    ForgeError::ForgeError(format!("forge request failed: {e}"))
                }
            })?;
            if resp.status().is_redirection() {
                let loc = resp
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        ForgeError::ForgeError("redirect missing Location header".to_string())
                    })?;
                current = current.join(loc).map_err(|e| {
                    ForgeError::ForgeError(format!("invalid redirect Location '{loc}': {e}"))
                })?;
                continue;
            }
            return Ok(resp);
        }
        Err(ForgeError::ForgeError(
            "too many redirects from forge".to_string(),
        ))
    }

    async fn download_bounded(
        &self,
        raw_url: &str,
        max_bytes: usize,
        allow_private: bool,
    ) -> Result<Vec<u8>, ForgeError> {
        let download_timeout = self.timeout().max(Duration::from_secs(60));
        let mut resp = self
            .get_following_redirects(raw_url, None, download_timeout, allow_private)
            .await?;
        let status = resp.status();
        if status.is_server_error() {
            return Err(ForgeError::ForgeError(format!(
                "asset download returned {status}"
            )));
        }
        if !status.is_success() {
            return Err(ForgeError::InvalidRequest(format!(
                "asset download returned {status}"
            )));
        }
        if let Some(len) = resp.content_length() {
            if len > max_bytes as u64 {
                return Err(ForgeError::InvalidRequest(format!(
                    "release asset exceeds maximum size of {max_bytes} bytes"
                )));
            }
        }
        let mut buf = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| {
            if e.is_timeout() {
                ForgeError::Timeout
            } else {
                ForgeError::ForgeError(format!("error reading asset body: {e}"))
            }
        })? {
            if buf.len() + chunk.len() > max_bytes {
                return Err(ForgeError::InvalidRequest(format!(
                    "release asset exceeds maximum size of {max_bytes} bytes"
                )));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }

    async fn fetch_github_or_gitea(
        &self,
        forge_kind: ForgeKind,
        api_url: &str,
        accept: Option<&str>,
        allow_private: bool,
    ) -> Result<Option<ResolvedRelease>, ForgeError> {
        let resp = self
            .get_following_redirects(api_url, accept, self.timeout(), allow_private)
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_server_error() {
            return Err(ForgeError::ForgeError(format!(
                "{} API returned {status}",
                forge_kind.as_str()
            )));
        }
        if !status.is_success() {
            return Err(ForgeError::InvalidRequest(format!(
                "{} API returned {status}",
                forge_kind.as_str()
            )));
        }
        let parsed: GithubOrGiteaRelease = resp
            .json()
            .await
            .map_err(|e| ForgeError::ForgeError(format!("invalid release JSON: {e}")))?;
        Ok(Some(ResolvedRelease {
            forge_kind,
            tag_name: parsed.tag_name,
            assets: parsed
                .assets
                .into_iter()
                .map(|a| ReleaseAsset {
                    name: a.name,
                    download_url: a.browser_download_url,
                })
                .collect(),
        }))
    }

    async fn fetch_gitlab(
        &self,
        repo: &ParsedRepo,
        tag: Option<&str>,
        allow_private: bool,
    ) -> Result<Option<ResolvedRelease>, ForgeError> {
        let joined = repo.segments.join("/");
        let project = urlencoding::encode(&joined);
        let api_url = match tag {
            Some(t) => format!(
                "{}/api/v4/projects/{project}/releases/{}",
                repo.origin,
                urlencoding::encode(t)
            ),
            None => format!(
                "{}/api/v4/projects/{project}/releases/permalink/latest",
                repo.origin
            ),
        };
        let resp = self
            .get_following_redirects(
                &api_url,
                Some("application/json"),
                self.timeout(),
                allow_private,
            )
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_server_error() {
            return Err(ForgeError::ForgeError(format!(
                "gitlab API returned {status}"
            )));
        }
        if !status.is_success() {
            return Err(ForgeError::InvalidRequest(format!(
                "gitlab API returned {status}"
            )));
        }
        let parsed: GitlabRelease = resp
            .json()
            .await
            .map_err(|e| ForgeError::ForgeError(format!("invalid gitlab release JSON: {e}")))?;
        Ok(Some(ResolvedRelease {
            forge_kind: ForgeKind::Gitlab,
            tag_name: parsed.tag_name,
            assets: parsed
                .assets
                .links
                .into_iter()
                .filter_map(|l| {
                    let download_url = l
                        .direct_asset_url
                        .filter(|s| !s.is_empty())
                        .or_else(|| l.url.filter(|s| !s.is_empty()))?;
                    Some(ReleaseAsset {
                        name: l.name,
                        download_url,
                    })
                })
                .collect(),
        }))
    }

    async fn resolve_release(
        &self,
        repo: &ParsedRepo,
        tag: Option<&str>,
        forge_hint: Option<ForgeKind>,
        allow_private: bool,
    ) -> Result<ResolvedRelease, ForgeError> {
        let detected = forge_hint.or(match repo.host.as_str() {
            "github.com" => Some(ForgeKind::Github),
            "gitlab.com" => Some(ForgeKind::Gitlab),
            "codeberg.org" => Some(ForgeKind::Gitea),
            _ if repo.segments.len() > 2 => Some(ForgeKind::Gitlab),
            _ => None,
        });

        let not_found = || {
            ForgeError::InvalidRequest(match tag {
                Some(t) => format!("release '{t}' not found at {}", repo.canonical_url),
                None => format!("no published release found at {}", repo.canonical_url),
            })
        };

        match detected {
            Some(ForgeKind::Github) => {
                if repo.segments.len() != 2 {
                    return Err(ForgeError::InvalidRequest(
                        "GitHub repository URL must have exact <owner>/<repo> path".to_string(),
                    ));
                }
                let (owner, name) = (&repo.segments[0], &repo.segments[1]);
                let url = match tag {
                    Some(t) => format!(
                        "{}/repos/{owner}/{name}/releases/tags/{}",
                        self.github_api_base,
                        urlencoding::encode(t)
                    ),
                    None => format!(
                        "{}/repos/{owner}/{name}/releases/latest",
                        self.github_api_base
                    ),
                };
                self.fetch_github_or_gitea(
                    ForgeKind::Github,
                    &url,
                    Some("application/vnd.github+json"),
                    allow_private,
                )
                .await?
                .ok_or_else(not_found)
            }
            Some(ForgeKind::Gitea) => {
                if repo.segments.len() != 2 {
                    return Err(ForgeError::InvalidRequest(
                        "Gitea repository URL must have exact <owner>/<repo> path".to_string(),
                    ));
                }
                let (owner, name) = (&repo.segments[0], &repo.segments[1]);
                let url = match tag {
                    Some(t) => format!(
                        "{}/api/v1/repos/{owner}/{name}/releases/tags/{}",
                        repo.origin,
                        urlencoding::encode(t)
                    ),
                    None => format!(
                        "{}/api/v1/repos/{owner}/{name}/releases/latest",
                        repo.origin
                    ),
                };
                self.fetch_github_or_gitea(
                    ForgeKind::Gitea,
                    &url,
                    Some("application/json"),
                    allow_private,
                )
                .await?
                .ok_or_else(not_found)
            }
            Some(ForgeKind::Gitlab) => self
                .fetch_gitlab(repo, tag, allow_private)
                .await?
                .ok_or_else(not_found),
            None => {
                let (owner, name) = (&repo.segments[0], &repo.segments[1]);
                let gitea_url = match tag {
                    Some(t) => format!(
                        "{}/api/v1/repos/{owner}/{name}/releases/tags/{}",
                        repo.origin,
                        urlencoding::encode(t)
                    ),
                    None => format!(
                        "{}/api/v1/repos/{owner}/{name}/releases/latest",
                        repo.origin
                    ),
                };
                if let Some(rel) = self
                    .fetch_github_or_gitea(
                        ForgeKind::Gitea,
                        &gitea_url,
                        Some("application/json"),
                        allow_private,
                    )
                    .await?
                {
                    return Ok(rel);
                }
                self.fetch_gitlab(repo, tag, allow_private)
                    .await?
                    .ok_or_else(not_found)
            }
        }
    }

    pub async fn fetch_release_bundle(
        &self,
        repo_url: &str,
        tag: Option<&str>,
        forge_hint: Option<ForgeKind>,
        allow_private: bool,
    ) -> Result<DownloadedBundle, ForgeError> {
        let parsed = parse_repo_url(repo_url, allow_private)?;
        let tag_clean = tag.map(str::trim).filter(|t| !t.is_empty());
        let release = self
            .resolve_release(&parsed, tag_clean, forge_hint, allow_private)
            .await?;

        let asset = select_target_asset(
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
        )?;
        let archive_bytes = self
            .download_bounded(&asset.download_url, MAX_ARCHIVE_BYTES, allow_private)
            .await?;

        if let Some(cs_asset) = find_checksum_asset(&release.assets, &asset.name) {
            let cs_bytes = self
                .download_bounded(&cs_asset.download_url, MAX_CHECKSUM_BYTES, allow_private)
                .await?;
            let cs_text = String::from_utf8(cs_bytes).map_err(|_| {
                ForgeError::ChecksumMismatch("checksum asset is not valid UTF-8".to_string())
            })?;
            let actual_hex = format!("{:x}", Sha256::digest(&archive_bytes));
            verify_checksum(&asset.name, &cs_asset.name, &cs_text, &actual_hex)?;
        }

        Ok(DownloadedBundle {
            forge_kind: release.forge_kind,
            canonical_repo_url: parsed.canonical_url,
            tag_name: release.tag_name,
            asset_name: asset.name.clone(),
            archive_bytes,
        })
    }
}
