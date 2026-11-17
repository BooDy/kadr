use std::time::{SystemTime, UNIX_EPOCH};

use deadpool_sqlite::Pool;
use kadr_core::models::{MediaMetadata, MediaType};
use kadr_core::search::{SearchQuery, SearchResponse, SearchResultItem};

use crate::error::Result;
use crate::repos::widget_queries::privacy_clause;

const MAX_QUERY_TOKENS: usize = 32;

fn is_ignored_in_token(c: char) -> bool {
    // Strip Arabic tashkeel/harakat & tatweel (kashida) without splitting the word,
    // since SQLite's `unicode61 remove_diacritics 2` only strips Latin diacritics.
    matches!(
        c as u32,
        0x0610..=0x061A | 0x0640 | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED
    )
}

fn is_combining_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05C7
            | 0x0711
            | 0x0730..=0x074A
            | 0x07A6..=0x07B0
            | 0x07EB..=0x07F3
            | 0x0816..=0x082D
            | 0x08D3..=0x08FF
            | 0x0900..=0x0903
            | 0x093A..=0x094F
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0x3099..=0x309A
            | 0xFE20..=0xFE2F
    )
}

fn is_fts_token_char(c: char) -> bool {
    c.is_alphanumeric() || is_combining_mark(c)
}

/// Sanitizes arbitrary user input into a safe SQLite FTS5 `MATCH` expression.
///
/// Strips/splits on FTS5 syntax & punctuation characters (`"`, `*`, `^`, `:`, `(`, `)`, `-`, `+`,
/// etc.), preserves Unicode letters/numbers and combining diacritics (e.g. Arabic harakat, French
/// accents), and wraps every token in double quotes with a prefix wildcard (`"token"*`) joined by
/// `AND` so FTS5 reserved keywords (`AND`, `OR`, `NOT`, `NEAR`) and punctuation can never trigger
/// an FTS5 syntax error.
pub fn sanitize_fts_query(raw: &str) -> Option<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in raw.chars() {
        if is_ignored_in_token(ch) {
            continue;
        }
        if is_fts_token_char(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            if current.chars().any(char::is_alphanumeric) {
                tokens.push(std::mem::take(&mut current));
                if tokens.len() >= MAX_QUERY_TOKENS {
                    break;
                }
            } else {
                current.clear();
            }
        }
    }

    if tokens.len() < MAX_QUERY_TOKENS
        && !current.is_empty()
        && current.chars().any(char::is_alphanumeric)
    {
        tokens.push(current);
    }

    if tokens.is_empty() {
        return None;
    }

    let formatted: Vec<String> = tokens
        .into_iter()
        .map(|tok| {
            let escaped = tok.replace('"', "\"\"");
            format!("\"{escaped}\"*")
        })
        .collect();

    Some(formatted.join(" AND "))
}

fn is_remote_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

pub fn resolve_poster_url(id: i64, metadata: &MediaMetadata) -> String {
    if let Some(path) = metadata
        .poster_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if is_remote_url(path) {
            return path.to_string();
        }
        return format!("/api/v1/artwork/{id}/poster");
    }

    if let Some(remote) = metadata
        .extra
        .get("poster_url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return remote.to_string();
    }

    format!("/api/v1/artwork/{id}/poster")
}

pub fn resolve_backdrop_url(id: i64, metadata: &MediaMetadata) -> Option<String> {
    if let Some(path) = metadata
        .backdrop_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if is_remote_url(path) {
            return Some(path.to_string());
        }
        return Some(format!("/api/v1/artwork/{id}/backdrop"));
    }

    if let Some(remote) = metadata
        .extra
        .get("backdrop_url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(remote.to_string());
    }

    None
}

fn calculate_badge(
    watch_state: Option<&str>,
    added_at: i64,
    resolution: Option<&str>,
    now_epoch_secs: i64,
) -> Option<String> {
    if matches!(watch_state, Some(ws) if ws.eq_ignore_ascii_case("in_progress")) {
        return Some("RESUME".to_string());
    }

    if added_at > 0 && now_epoch_secs >= added_at && (now_epoch_secs - added_at) <= 14 * 86_400 {
        return Some("NEW".to_string());
    }

    if let Some(res) = resolution {
        let upper = res.to_ascii_uppercase();
        if upper.contains("4K") || upper.contains("2160") {
            return Some("4K".to_string());
        }
    }

    None
}

#[derive(Clone)]
pub struct SearchRepository {
    pool: Pool,
}

pub type SearchRepo = SearchRepository;

impl SearchRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn search(
        &self,
        query: &SearchQuery,
        user_id: &str,
        unlocked_library_ids: &[String],
    ) -> Result<SearchResponse> {
        let trimmed_query = query.q.trim().to_string();
        if trimmed_query.is_empty() {
            return Ok(SearchResponse::empty(""));
        }

        let Some(fts_match) = sanitize_fts_query(&trimmed_query) else {
            return Ok(SearchResponse::empty(trimmed_query));
        };

        let limit = query.clamped_limit();
        let item_type_filter = query.item_type.map(|mt| {
            serde_json::to_string(&mt)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string()
        });
        let library_filter = query
            .library_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let unlocked = unlocked_library_ids.to_vec();
        let uid = user_id.to_string();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (priv_clause, priv_params) = privacy_clause(&unlocked);

            let mut where_parts = vec!["media_items_fts MATCH ?".to_string(), priv_clause];
            let mut filter_params = Vec::with_capacity(priv_params.len() + 4);
            filter_params.push(rusqlite::types::Value::Text(fts_match));
            filter_params.extend(priv_params);

            if let Some(it) = item_type_filter {
                where_parts.push("m.item_type = ?".to_string());
                filter_params.push(rusqlite::types::Value::Text(it));
            }

            if let Some(lid) = library_filter {
                where_parts.push("m.library_id = ?".to_string());
                filter_params.push(rusqlite::types::Value::Text(lid));
            }

            let where_sql = where_parts.join(" AND ");

            let count_sql = format!(
                "SELECT COUNT(*) \
                 FROM media_items_fts \
                 JOIN media_items m ON m.id = media_items_fts.rowid \
                 JOIN libraries l ON l.id = m.library_id \
                 WHERE {where_sql}"
            );
            let total: i64 = c.query_row(
                &count_sql,
                rusqlite::params_from_iter(filter_params.clone()),
                |row| row.get(0),
            )?;

            if total <= 0 {
                return Ok(SearchResponse {
                    query: trimmed_query,
                    total: 0,
                    items: Vec::new(),
                });
            }

            let mut select_params = Vec::with_capacity(filter_params.len() + 2);
            select_params.push(rusqlite::types::Value::Text(uid));
            select_params.extend(filter_params);
            select_params.push(rusqlite::types::Value::Integer(limit as i64));

            // Column weights match `media_items_fts` order:
            // title (10.0), original_title (8.0), series_title (7.0), overview (2.0),
            // people (5.0), genres_tags (4.0), file_name (1.5), release_year (3.0)
            let select_sql = format!(
                "SELECT \
                    m.id, \
                    m.library_id, \
                    l.name, \
                    m.item_type, \
                    m.title, \
                    m.original_title, \
                    m.release_year, \
                    m.duration_seconds, \
                    m.added_at, \
                    m.resolution, \
                    m.metadata, \
                    ups.playback_position_seconds, \
                    ups.watch_state \
                 FROM media_items_fts \
                 JOIN media_items m ON m.id = media_items_fts.rowid \
                 JOIN libraries l ON l.id = m.library_id \
                 LEFT JOIN user_playback_states ups \
                    ON ups.media_item_id = m.id AND ups.user_id = ? \
                 WHERE {where_sql} \
                 ORDER BY \
                    bm25(media_items_fts, 10.0, 8.0, 7.0, 2.0, 5.0, 4.0, 1.5, 3.0) ASC, \
                    m.added_at DESC, \
                    m.id DESC \
                 LIMIT ?"
            );

            let mut stmt = c.prepare(&select_sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(select_params), |row| {
                let id: i64 = row.get(0)?;
                let library_id: String = row.get(1)?;
                let library_name: String = row.get(2)?;
                let item_type_str: String = row.get(3)?;
                let title: String = row.get(4)?;
                let original_title: Option<String> = row.get(5)?;
                let release_year: Option<i32> = row.get(6)?;
                let duration_seconds: i64 = row.get(7)?;
                let added_at: i64 = row.get(8)?;
                let resolution: Option<String> = row.get(9)?;
                let metadata_str: String = row.get(10)?;
                let playback_position_seconds: Option<i64> = row.get(11)?;
                let watch_state: Option<String> = row.get(12)?;

                let item_type: MediaType = serde_json::from_str(&format!("\"{item_type_str}\""))
                    .unwrap_or(MediaType::Unknown);
                let metadata: MediaMetadata =
                    serde_json::from_str(&metadata_str).unwrap_or_default();

                let playback_progress = match (playback_position_seconds, watch_state.as_deref()) {
                    (Some(pos), Some(_)) if duration_seconds > 0 => {
                        Some((pos as f32 / duration_seconds as f32).clamp(0.0, 1.0))
                    }
                    _ => None,
                };

                let badge =
                    calculate_badge(watch_state.as_deref(), added_at, resolution.as_deref(), now);

                let poster_url = resolve_poster_url(id, &metadata);
                let backdrop_url = resolve_backdrop_url(id, &metadata);

                Ok(SearchResultItem {
                    id,
                    library_id,
                    library_name,
                    item_type,
                    title,
                    original_title: original_title.filter(|s| !s.trim().is_empty()),
                    release_year,
                    overview: metadata.overview.filter(|s| !s.trim().is_empty()),
                    poster_url,
                    backdrop_url,
                    badge,
                    rating: metadata.rating,
                    duration_seconds,
                    playback_progress,
                    series_title: metadata.series_title.filter(|s| !s.trim().is_empty()),
                    season: metadata.season,
                    episode: metadata.episode,
                    genres: metadata.genres,
                    director: metadata.director.filter(|s| !s.trim().is_empty()),
                })
            })?;

            let mut items = Vec::new();
            for row in rows {
                items.push(row?);
            }

            Ok(SearchResponse {
                query: trimmed_query,
                total: total as usize,
                items,
            })
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_handles_punctuation_operators_and_unicode() {
        assert_eq!(
            sanitize_fts_query("Spider-Man: Into"),
            Some("\"Spider\"* AND \"Man\"* AND \"Into\"*".to_string())
        );
        assert_eq!(sanitize_fts_query("C++"), Some("\"C\"*".to_string()));
        assert_eq!(
            sanitize_fts_query("foo\"bar"),
            Some("\"foo\"* AND \"bar\"*".to_string())
        );
        assert_eq!(
            sanitize_fts_query("باب الحديد"),
            Some("\"باب\"* AND \"الحديد\"*".to_string())
        );
        assert_eq!(sanitize_fts_query("2024"), Some("\"2024\"*".to_string()));
        assert_eq!(
            sanitize_fts_query("NEAR(a, b) OR NOT (x:y)^2*"),
            Some(
                "\"NEAR\"* AND \"a\"* AND \"b\"* AND \"OR\"* AND \"NOT\"* AND \"x\"* AND \"y\"* AND \"2\"*"
                    .to_string()
            )
        );
        assert_eq!(sanitize_fts_query("   "), None);
        assert_eq!(sanitize_fts_query("\"*^:()-+"), None);
    }
}
