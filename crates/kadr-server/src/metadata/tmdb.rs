use std::time::Duration;

use kadr_core::models::{MediaType, MetadataUpdate};
use serde::{Deserialize, Serialize};

use crate::metadata::settings::TmdbAuthMode;

pub const DEFAULT_TMDB_API_BASE_URL: &str = "https://api.themoviedb.org";
pub const DEFAULT_TMDB_IMAGE_BASE_URL: &str = "https://image.tmdb.org/t/p";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TmdbMediaType {
    Movie,
    Tv,
}

impl TmdbMediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Tv => "tv",
        }
    }

    pub fn from_media_type(mt: MediaType) -> Self {
        match mt {
            MediaType::Show | MediaType::Season | MediaType::Episode => Self::Tv,
            _ => Self::Movie,
        }
    }

    pub fn parse_param(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "movie" | "movies" => Some(Self::Movie),
            "tv" | "show" | "shows" | "series" => Some(Self::Tv),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TmdbSearchMatch {
    pub tmdb_id: i64,
    pub media_type: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_year: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TmdbEnrichedDetails {
    pub patch: MetadataUpdate,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TmdbError {
    #[error("TMDb API key is not configured")]
    MissingCredentials,
    #[error("TMDb authentication failed (HTTP {0})")]
    Unauthorized(u16),
    #[error("TMDb item not found")]
    NotFound,
    #[error("TMDb request failed: {0}")]
    Upstream(String),
}

#[derive(Clone)]
pub struct TmdbClient {
    base_url: String,
    image_base_url: String,
    http: reqwest::Client,
}

impl Default for TmdbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TmdbClient {
    pub fn new() -> Self {
        Self::with_urls(DEFAULT_TMDB_API_BASE_URL, DEFAULT_TMDB_IMAGE_BASE_URL)
    }

    pub fn with_urls(base_url: impl Into<String>, image_base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            image_base_url: image_base_url.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn image_base_url(&self) -> &str {
        &self.image_base_url
    }

    fn build_image_url(&self, size: &str, path: Option<&str>) -> Option<String> {
        let p = path?.trim();
        if p.is_empty() {
            return None;
        }
        let normalized = if p.starts_with('/') {
            p.to_string()
        } else {
            format!("/{p}")
        };
        Some(format!("{}/{size}{normalized}", self.image_base_url))
    }

    fn apply_auth(
        &self,
        req: reqwest::RequestBuilder,
        auth: &TmdbAuthMode,
    ) -> reqwest::RequestBuilder {
        match auth {
            TmdbAuthMode::V3ApiKey(key) => req.query(&[("api_key", key.as_str())]),
            TmdbAuthMode::V4Bearer(token) => req.bearer_auth(token),
        }
    }

    pub async fn search(
        &self,
        raw_api_key: &str,
        language: &str,
        media_type: TmdbMediaType,
        query: &str,
        year: Option<i32>,
    ) -> Result<Vec<TmdbSearchMatch>, TmdbError> {
        let auth = TmdbAuthMode::from_raw(raw_api_key).ok_or(TmdbError::MissingCredentials)?;
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }

        let endpoint = format!("{}/3/search/{}", self.base_url, media_type.as_str());
        let mut req = self.http.get(&endpoint).query(&[
            ("query", q),
            (
                "language",
                if language.trim().is_empty() {
                    "en-US"
                } else {
                    language.trim()
                },
            ),
        ]);
        if let Some(y) = year {
            let y_str = y.to_string();
            match media_type {
                TmdbMediaType::Movie => {
                    req = req.query(&[("year", y_str.as_str())]);
                }
                TmdbMediaType::Tv => {
                    req = req.query(&[("first_air_date_year", y_str.as_str())]);
                }
            }
        }
        req = self.apply_auth(req, &auth);

        let resp = req
            .send()
            .await
            .map_err(|e| TmdbError::Upstream(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(TmdbError::Unauthorized(status.as_u16()));
        }
        if !status.is_success() {
            return Err(TmdbError::Upstream(format!("HTTP {status}")));
        }

        let parsed: RawSearchResponse = resp
            .json()
            .await
            .map_err(|e| TmdbError::Upstream(e.to_string()))?;

        let mut out = Vec::with_capacity(parsed.results.len());
        for r in parsed.results {
            let title = r
                .title
                .or(r.name)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let original_title = r
                .original_title
                .or(r.original_name)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let release_year = r
                .release_date
                .as_deref()
                .or(r.first_air_date.as_deref())
                .and_then(parse_year_prefix);
            let overview = r
                .overview
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let rating = r.vote_average.filter(|v| *v > 0.0);
            let poster_url = self.build_image_url("w500", r.poster_path.as_deref());
            let backdrop_url = self.build_image_url("w1280", r.backdrop_path.as_deref());

            out.push(TmdbSearchMatch {
                tmdb_id: r.id,
                media_type: media_type.as_str().to_string(),
                title,
                original_title,
                release_year,
                overview,
                rating,
                poster_url,
                backdrop_url,
            });
        }
        Ok(out)
    }

    pub async fn fetch_details(
        &self,
        raw_api_key: &str,
        language: &str,
        media_type: TmdbMediaType,
        tmdb_id: i64,
    ) -> Result<TmdbEnrichedDetails, TmdbError> {
        let auth = TmdbAuthMode::from_raw(raw_api_key).ok_or(TmdbError::MissingCredentials)?;
        let endpoint = format!("{}/3/{}/{}", self.base_url, media_type.as_str(), tmdb_id);
        let lang = if language.trim().is_empty() {
            "en-US"
        } else {
            language.trim()
        };

        let req = self
            .http
            .get(&endpoint)
            .query(&[("append_to_response", "credits"), ("language", lang)]);
        let req = self.apply_auth(req, &auth);

        let resp = req
            .send()
            .await
            .map_err(|e| TmdbError::Upstream(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(TmdbError::Unauthorized(status.as_u16()));
        }
        if status.as_u16() == 404 {
            return Err(TmdbError::NotFound);
        }
        if !status.is_success() {
            return Err(TmdbError::Upstream(format!("HTTP {status}")));
        }

        let d: RawDetailsResponse = resp
            .json()
            .await
            .map_err(|e| TmdbError::Upstream(e.to_string()))?;

        let title = d
            .title
            .or(d.name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let original_title = d
            .original_title
            .or(d.original_name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let release_year = d
            .release_date
            .as_deref()
            .or(d.first_air_date.as_deref())
            .and_then(parse_year_prefix);
        let overview = d
            .overview
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let genres: Vec<String> = d
            .genres
            .into_iter()
            .map(|g| g.name.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let credits = d.credits.unwrap_or_default();

        // Director (movie crew Director) or creators (TV created_by, fallback crew Director)
        let mut directors: Vec<String> = Vec::new();
        if media_type == TmdbMediaType::Tv {
            for creator in &d.created_by {
                let n = creator.name.trim();
                if !n.is_empty() && !directors.iter().any(|x| x == n) {
                    directors.push(n.to_string());
                }
            }
        }
        if directors.is_empty() {
            for crew in &credits.crew {
                let job = crew.job.as_deref().unwrap_or("").trim();
                if job.eq_ignore_ascii_case("Director")
                    || job.eq_ignore_ascii_case("Series Director")
                {
                    let n = crew.name.trim();
                    if !n.is_empty() && !directors.iter().any(|x| x == n) {
                        directors.push(n.to_string());
                    }
                }
            }
        }
        let director = if directors.is_empty() {
            None
        } else {
            Some(directors.join(", "))
        };

        // Writers
        let mut writers: Vec<String> = Vec::new();
        for crew in &credits.crew {
            let job = crew.job.as_deref().unwrap_or("").trim();
            let dept = crew.department.as_deref().unwrap_or("").trim();
            let is_writer = dept.eq_ignore_ascii_case("Writing")
                || job.eq_ignore_ascii_case("Writer")
                || job.eq_ignore_ascii_case("Screenplay")
                || job.eq_ignore_ascii_case("Story")
                || job.eq_ignore_ascii_case("Teleplay")
                || job.eq_ignore_ascii_case("Author");
            if is_writer {
                let n = crew.name.trim();
                if !n.is_empty() && !writers.iter().any(|x| x == n) {
                    writers.push(n.to_string());
                }
            }
        }

        // Top 10 cast actors
        let mut cast = credits.cast;
        cast.sort_by_key(|c| c.order.unwrap_or(i32::MAX));
        let mut actors: Vec<String> = Vec::new();
        for member in cast {
            let n = member.name.trim();
            if !n.is_empty() && !actors.iter().any(|x| x == n) {
                actors.push(n.to_string());
                if actors.len() >= 10 {
                    break;
                }
            }
        }

        let studio = d
            .production_companies
            .into_iter()
            .chain(d.networks)
            .map(|c| c.name.trim().to_string())
            .find(|s| !s.is_empty());

        let country = d
            .origin_country
            .into_iter()
            .map(|c| c.trim().to_string())
            .find(|s| !s.is_empty())
            .or_else(|| {
                d.production_countries
                    .into_iter()
                    .map(|c| {
                        let iso = c.iso_3166_1.trim();
                        if !iso.is_empty() {
                            iso.to_string()
                        } else {
                            c.name.trim().to_string()
                        }
                    })
                    .find(|s| !s.is_empty())
            });

        let orig_lang = d
            .original_language
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let rating = d.vote_average.filter(|v| *v > 0.0);

        let poster_url = self.build_image_url("w500", d.poster_path.as_deref());
        let backdrop_url = self.build_image_url("w1280", d.backdrop_path.as_deref());

        let mut patch = MetadataUpdate {
            tmdb_id: Some(Some(tmdb_id)),
            tmdb_media_type: Some(Some(media_type.as_str().to_string())),
            ..Default::default()
        };

        if let Some(t) = title {
            patch.title = Some(t);
        }
        if let Some(ot) = original_title {
            patch.original_title = Some(Some(ot));
        }
        if let Some(ry) = release_year {
            patch.release_year = Some(Some(ry));
        }
        if let Some(ov) = overview {
            patch.overview = Some(Some(ov));
        }
        if !genres.is_empty() {
            patch.genres = Some(genres);
        }
        if let Some(dir) = director {
            patch.director = Some(Some(dir));
        }
        if !writers.is_empty() {
            patch.writers = Some(writers);
        }
        if !actors.is_empty() {
            patch.actors = Some(actors);
        }
        if let Some(st) = studio {
            patch.studio = Some(Some(st));
        }
        if let Some(co) = country {
            patch.country = Some(Some(co));
        }
        if let Some(lg) = orig_lang {
            patch.language = Some(Some(lg));
        }
        if let Some(rt) = rating {
            patch.rating = Some(Some(rt));
        }

        Ok(TmdbEnrichedDetails {
            patch,
            poster_url,
            backdrop_url,
        })
    }
}

fn parse_year_prefix(date_str: &str) -> Option<i32> {
    let trimmed = date_str.trim();
    if trimmed.len() >= 4 {
        trimmed[..4].parse::<i32>().ok().filter(|y| *y > 1800)
    } else {
        None
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawSearchResponse {
    #[serde(default)]
    results: Vec<RawSearchItem>,
}

#[derive(Debug, Default, Deserialize)]
struct RawSearchItem {
    id: i64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    vote_average: Option<f32>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDetailsResponse {
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    original_language: Option<String>,
    vote_average: Option<f32>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    genres: Vec<RawNamed>,
    #[serde(default)]
    production_companies: Vec<RawNamed>,
    #[serde(default)]
    networks: Vec<RawNamed>,
    #[serde(default)]
    origin_country: Vec<String>,
    #[serde(default)]
    production_countries: Vec<RawCountry>,
    #[serde(default)]
    created_by: Vec<RawNamed>,
    credits: Option<RawCredits>,
}

#[derive(Debug, Default, Deserialize)]
struct RawNamed {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawCountry {
    #[serde(default)]
    iso_3166_1: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawCredits {
    #[serde(default)]
    cast: Vec<RawCastMember>,
    #[serde(default)]
    crew: Vec<RawCrewMember>,
}

#[derive(Debug, Default, Deserialize)]
struct RawCastMember {
    #[serde(default)]
    name: String,
    order: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawCrewMember {
    #[serde(default)]
    name: String,
    job: Option<String>,
    department: Option<String>,
}
