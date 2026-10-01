use std::path::Path;
use std::time::Duration;

use kadr_core::subtitles::OnlineSubtitleMatch;
use serde::{Deserialize, Serialize};

pub const DEFAULT_BASE_URL: &str = "https://api.opensubtitles.com/api/v1";
pub const USER_AGENT: &str = "Kadr Media Server v0.1.0";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, thiserror::Error)]
pub enum OpenSubtitlesError {
    #[error("OpenSubtitles API key is not configured")]
    NotConfigured,
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("OpenSubtitles API returned error (status {status}): {message}")]
    Api { status: u16, message: String },
    #[error("Failed to parse response: {0}")]
    Parse(String),
}

#[derive(Clone)]
pub struct OpenSubtitlesClient {
    api_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for OpenSubtitlesClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenSubtitlesClient")
            .field("is_configured", &self.is_configured())
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    id: Option<String>,
    #[serde(default)]
    attributes: SearchAttributes,
}

#[derive(Debug, Default, Deserialize)]
struct SearchAttributes {
    language: Option<String>,
    release: Option<String>,
    hearing_impaired: Option<bool>,
    download_count: Option<u32>,
    ratings: Option<f32>,
    format: Option<String>,
    #[serde(default)]
    files: Vec<SearchFile>,
}

#[derive(Debug, Deserialize)]
struct SearchFile {
    file_id: Option<u64>,
    #[serde(default)]
    file_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct DownloadRequest {
    file_id: u64,
}

#[derive(Debug, Deserialize)]
struct DownloadResponse {
    link: String,
    #[serde(default)]
    file_name: Option<String>,
}

impl OpenSubtitlesClient {
    pub fn new(api_key: Option<String>, base_url: Option<String>) -> Self {
        let api_key = match api_key {
            Some(key) => {
                let trimmed = key.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }
            None => None,
        };

        let base_url = base_url
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();

        Self {
            api_key,
            base_url,
            client,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    pub async fn search(
        &self,
        query: &str,
        year: Option<u32>,
        languages: &[String],
    ) -> Result<Vec<OnlineSubtitleMatch>, OpenSubtitlesError> {
        let api_key = match &self.api_key {
            Some(key) => key,
            None => {
                tracing::info!("OpenSubtitles API key is not configured; skipping online search");
                return Ok(Vec::new());
            }
        };

        let url = format!("{}/subtitles", self.base_url);
        let mut query_params: Vec<(&str, String)> = Vec::new();
        query_params.push(("query", query.to_string()));

        if let Some(y) = year {
            query_params.push(("year", y.to_string()));
        }

        if !languages.is_empty() {
            query_params.push(("languages", languages.join(",")));
        }

        tracing::info!(
            query = %query,
            year = ?year,
            languages = ?languages,
            url = %url,
            "Searching OpenSubtitles.com subtitles (Api-Key header omitted)"
        );

        let resp = self
            .client
            .get(&url)
            .header("Api-Key", api_key)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .query(&query_params)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(OpenSubtitlesError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let search_resp: SearchResponse = resp.json().await.map_err(|e| {
            OpenSubtitlesError::Parse(format!("Failed to parse OpenSubtitles search response: {e}"))
        })?;

        let matches = search_resp
            .data
            .into_iter()
            .map(|item| {
                let first_file = item.attributes.files.first();
                let id = first_file
                    .and_then(|f| f.file_id)
                    .map(|id| id.to_string())
                    .or(item.id)
                    .unwrap_or_default();

                let format = item
                    .attributes
                    .format
                    .or_else(|| {
                        first_file
                            .and_then(|f| f.file_name.as_deref())
                            .and_then(|name| {
                                Path::new(name)
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|s| s.to_ascii_lowercase())
                            })
                    })
                    .unwrap_or_else(|| "srt".to_string());

                OnlineSubtitleMatch {
                    id,
                    language: item.attributes.language.unwrap_or_default(),
                    release_name: item.attributes.release,
                    hearing_impaired: item.attributes.hearing_impaired.unwrap_or(false),
                    format,
                    download_count: item.attributes.download_count.unwrap_or(0),
                    rating: item.attributes.ratings,
                }
            })
            .collect();

        Ok(matches)
    }

    pub async fn download(&self, file_id: &str) -> Result<(Vec<u8>, String), OpenSubtitlesError> {
        let api_key = match &self.api_key {
            Some(key) => key,
            None => return Err(OpenSubtitlesError::NotConfigured),
        };

        let numeric_id = file_id.parse::<u64>().map_err(|e| {
            OpenSubtitlesError::Parse(format!("Invalid file_id '{file_id}': {e}"))
        })?;

        let url = format!("{}/download", self.base_url);
        let request_body = DownloadRequest {
            file_id: numeric_id,
        };

        tracing::info!(
            file_id = %file_id,
            url = %url,
            "Requesting OpenSubtitles.com download link (Api-Key header omitted)"
        );

        let resp = self
            .client
            .post(&url)
            .header("Api-Key", api_key)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(OpenSubtitlesError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let download_info: DownloadResponse = resp.json().await.map_err(|e| {
            OpenSubtitlesError::Parse(format!("Failed to parse download response: {e}"))
        })?;

        let download_url = if download_info.link.starts_with("http://")
            || download_info.link.starts_with("https://")
        {
            download_info.link
        } else {
            format!("{}{}", self.base_url, download_info.link)
        };

        let file_resp = self
            .client
            .get(&download_url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?;

        let file_status = file_resp.status();
        if !file_status.is_success() {
            let message = file_resp.text().await.unwrap_or_default();
            return Err(OpenSubtitlesError::Api {
                status: file_status.as_u16(),
                message,
            });
        }

        let file_name = download_info
            .file_name
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                Path::new(&download_url)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("subtitle.srt")
                    .to_string()
            });

        let bytes = file_resp.bytes().await?.to_vec();
        Ok((bytes, file_name))
    }
}
