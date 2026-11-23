use serde::{Deserialize, Serialize};

use crate::models::MediaType;

pub const DEFAULT_SEARCH_LIMIT: usize = 30;
pub const MIN_SEARCH_LIMIT: usize = 1;
pub const MAX_SEARCH_LIMIT: usize = 100;

fn default_search_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

pub fn clamp_search_limit(limit: usize) -> usize {
    limit.clamp(MIN_SEARCH_LIMIT, MAX_SEARCH_LIMIT)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub item_type: Option<MediaType>,
    #[serde(default)]
    pub library_id: Option<String>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            q: String::new(),
            item_type: None,
            library_id: None,
            limit: DEFAULT_SEARCH_LIMIT,
        }
    }
}

impl SearchQuery {
    pub fn new(q: impl Into<String>) -> Self {
        Self {
            q: q.into(),
            ..Default::default()
        }
    }

    pub fn clamped_limit(&self) -> usize {
        clamp_search_limit(self.limit)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: i64,
    pub library_id: String,
    pub library_name: String,
    pub item_type: MediaType,
    pub title: String,
    pub original_title: Option<String>,
    pub release_year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: String,
    pub backdrop_url: Option<String>,
    pub badge: Option<String>,
    pub rating: Option<f32>,
    pub duration_seconds: i64,
    pub playback_progress: Option<f32>,
    pub series_title: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub genres: Vec<String>,
    pub director: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchResponse {
    pub query: String,
    pub total: usize,
    pub items: Vec<SearchResultItem>,
}

impl SearchResponse {
    pub fn empty(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            total: 0,
            items: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_defaults_and_clamps_limit() {
        let q = SearchQuery::new("inception");
        assert_eq!(q.limit, 30);
        assert_eq!(q.clamped_limit(), 30);

        let low = SearchQuery {
            limit: 0,
            ..SearchQuery::new("a")
        };
        assert_eq!(low.clamped_limit(), 1);

        let high = SearchQuery {
            limit: 500,
            ..SearchQuery::new("a")
        };
        assert_eq!(high.clamped_limit(), 100);
    }
}
