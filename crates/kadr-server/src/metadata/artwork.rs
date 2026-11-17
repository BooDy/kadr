use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::plugins::url_guard::ensure_public_host;

pub const MAX_ARTWORK_BYTES: usize = 15 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkSlot {
    Poster,
    Backdrop,
}

impl ArtworkSlot {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Poster => "poster",
            Self::Backdrop => "backdrop",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArtworkError {
    #[error("Invalid artwork URL: {0}")]
    InvalidUrl(String),
    #[error("Blocked artwork URL: {0}")]
    BlockedHost(String),
    #[error("Artwork exceeds 15 MiB limit")]
    TooLarge,
    #[error("Response is not a supported image format")]
    UnsupportedFormat,
    #[error("Failed to download artwork: {0}")]
    Network(String),
    #[error("Failed to store artwork on disk: {0}")]
    Io(String),
}

#[derive(Clone)]
pub struct ArtworkDownloader {
    artwork_dir: PathBuf,
    allow_private_hosts: bool,
    http: reqwest::Client,
}

impl ArtworkDownloader {
    pub fn new(artwork_dir: PathBuf, allow_private_hosts: bool) -> Self {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            artwork_dir,
            allow_private_hosts,
            http,
        }
    }

    pub fn artwork_dir(&self) -> &Path {
        &self.artwork_dir
    }

    pub fn allow_private_hosts(&self) -> bool {
        self.allow_private_hosts
    }

    pub async fn download_and_save(
        &self,
        item_id: i64,
        slot: ArtworkSlot,
        raw_url: &str,
        allow_private_override: bool,
    ) -> Result<PathBuf, ArtworkError> {
        let allow_private = self.allow_private_hosts || allow_private_override;
        let mut current_url = reqwest::Url::parse(raw_url.trim())
            .map_err(|e| ArtworkError::InvalidUrl(e.to_string()))?;

        let mut resp = None;
        for _ in 0..=MAX_REDIRECTS {
            validate_artwork_url(&current_url, allow_private).await?;
            let r = self
                .http
                .get(current_url.clone())
                .send()
                .await
                .map_err(|e| ArtworkError::Network(e.to_string()))?;

            if r.status().is_redirection() {
                let loc = r
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|h| h.to_str().ok())
                    .ok_or_else(|| {
                        ArtworkError::Network("Redirect missing Location header".to_string())
                    })?;
                current_url = current_url
                    .join(loc)
                    .map_err(|e| ArtworkError::InvalidUrl(e.to_string()))?;
                continue;
            }
            resp = Some(r);
            break;
        }

        let mut resp = resp
            .ok_or_else(|| ArtworkError::Network("Too many redirects fetching artwork".into()))?;

        if !resp.status().is_success() {
            return Err(ArtworkError::Network(format!(
                "Upstream returned HTTP {}",
                resp.status()
            )));
        }

        if let Some(len) = resp.content_length() {
            if len > MAX_ARTWORK_BYTES as u64 {
                return Err(ArtworkError::TooLarge);
            }
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);

        let mut buf = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ArtworkError::Network(e.to_string()))?
        {
            if buf.len().saturating_add(chunk.len()) > MAX_ARTWORK_BYTES {
                return Err(ArtworkError::TooLarge);
            }
            buf.extend_from_slice(&chunk);
        }

        let ext = detect_image_extension(&buf, content_type.as_deref())?;

        tokio::fs::create_dir_all(&self.artwork_dir)
            .await
            .map_err(|e| ArtworkError::Io(e.to_string()))?;

        let stem = format!("item_{item_id}_{}", slot.as_str());
        let final_path = self.artwork_dir.join(format!("{stem}.{ext}"));
        let tmp_path = self
            .artwork_dir
            .join(format!(".{stem}.{}.tmp", uuid::Uuid::new_v4()));

        tokio::fs::write(&tmp_path, &buf)
            .await
            .map_err(|e| ArtworkError::Io(e.to_string()))?;
        tokio::fs::rename(&tmp_path, &final_path)
            .await
            .map_err(|e| ArtworkError::Io(e.to_string()))?;

        // Clean up any stale sibling extensions from previous downloads.
        for other_ext in ["jpg", "png", "webp"] {
            if other_ext != ext {
                let stale = self.artwork_dir.join(format!("{stem}.{other_ext}"));
                let _ = tokio::fs::remove_file(stale).await;
            }
        }

        Ok(final_path)
    }
}

async fn validate_artwork_url(url: &reqwest::Url, allow_private: bool) -> Result<(), ArtworkError> {
    let scheme = url.scheme();
    if allow_private {
        if !matches!(scheme, "http" | "https") {
            return Err(ArtworkError::InvalidUrl(format!(
                "Unsupported URL scheme '{scheme}'"
            )));
        }
        Ok(())
    } else {
        if scheme != "https" {
            return Err(ArtworkError::InvalidUrl(
                "Artwork URL must use https://".to_string(),
            ));
        }
        ensure_public_host(url)
            .await
            .map_err(ArtworkError::BlockedHost)
    }
}

pub fn detect_image_extension(
    bytes: &[u8],
    content_type: Option<&str>,
) -> Result<&'static str, ArtworkError> {
    if bytes.is_empty() {
        return Err(ArtworkError::UnsupportedFormat);
    }

    // 1. Magic bytes inspection
    if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        return Ok("jpg");
    }
    if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Ok("png");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok("webp");
    }

    // 2. Content-Type header fallback (for valid image/* responses)
    if let Some(ct) = content_type {
        let mime = ct
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if mime == "image/png" {
            return Ok("png");
        }
        if mime == "image/webp" {
            return Ok("webp");
        }
        if mime.starts_with("image/") {
            return Ok("jpg");
        }
    }

    Err(ArtworkError::UnsupportedFormat)
}
