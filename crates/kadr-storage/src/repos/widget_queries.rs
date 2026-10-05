use crate::error::Result;
use deadpool_sqlite::Pool;
use kadr_core::ast::WidgetFilterConfig;
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use std::path::PathBuf;

pub const SELECT_JOINED_COLUMNS: &str =
    "m.id, m.library_id, m.item_type, m.title, m.original_title, m.release_year, \
 m.duration_seconds, m.added_at, m.file_path, m.file_name, m.file_size, \
 m.resolution, m.video_codec, m.audio_codec, m.audio_channels, m.container, m.metadata";

fn current_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn privacy_clause(unlocked_ids: &[String]) -> (String, Vec<rusqlite::types::Value>) {
    if unlocked_ids.is_empty() {
        ("(l.is_private = 0)".to_string(), Vec::new())
    } else {
        let placeholders = unlocked_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let clause = format!("(l.is_private = 0 OR l.id IN ({}))", placeholders);
        let params = unlocked_ids
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        (clause, params)
    }
}

pub fn build_filter_clauses(
    filters: Option<&WidgetFilterConfig>,
    unlocked_ids: &[String],
    current_time_epoch_secs: i64,
) -> (String, Vec<rusqlite::types::Value>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();

    // 1. Privacy filter
    if filters.map(|f| f.exclude_private).unwrap_or(false) {
        clauses.push("(l.is_private = 0)".to_string());
    } else {
        let (priv_clause, priv_params) = privacy_clause(unlocked_ids);
        clauses.push(priv_clause);
        params.extend(priv_params);
    }

    if let Some(f) = filters {
        // 2. Exclude specific library IDs
        if !f.exclude_library_ids.is_empty() {
            let placeholders = f
                .exclude_library_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            clauses.push(format!("m.library_id NOT IN ({placeholders})"));
            for lib_id in &f.exclude_library_ids {
                params.push(rusqlite::types::Value::Text(lib_id.clone()));
            }
        }

        // 3. Exclude specific genres
        if !f.exclude_genres.is_empty() {
            let placeholders = f
                .exclude_genres
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            clauses.push(format!(
                "NOT EXISTS (
                    SELECT 1 FROM json_each(json_extract(m.metadata, '$.genres'))
                    WHERE LOWER(value) IN ({placeholders})
                )"
            ));
            for genre in &f.exclude_genres {
                params.push(rusqlite::types::Value::Text(genre.trim().to_lowercase()));
            }
        }

        // 4. Exclude by date added (max age cutoff)
        if let Some(days) = f.max_age_days {
            if days > 0 {
                let cutoff = current_time_epoch_secs - (days as i64 * 86_400);
                clauses.push("m.added_at >= ?".to_string());
                params.push(rusqlite::types::Value::Integer(cutoff));
            }
        }
    }

    (clauses.join(" AND "), params)
}

pub fn map_media_item_row(row: &rusqlite::Row) -> rusqlite::Result<MediaItem> {
    let id: i64 = row.get(0)?;
    let library_id: String = row.get(1)?;
    let item_type_str: String = row.get(2)?;
    let title: String = row.get(3)?;
    let original_title: Option<String> = row.get(4)?;
    let release_year: Option<i32> = row.get(5)?;
    let duration_seconds: i64 = row.get(6)?;
    let added_at: i64 = row.get(7)?;
    let file_path: String = row.get(8)?;
    let file_name: String = row.get(9)?;
    let file_size: i64 = row.get(10)?;
    let resolution: Option<String> = row.get(11)?;
    let video_codec: Option<String> = row.get(12)?;
    let audio_codec: Option<String> = row.get(13)?;
    let audio_channels: Option<u8> = row.get(14)?;
    let container: Option<String> = row.get(15)?;
    let metadata_str: String = row.get(16)?;

    let item_type: MediaType =
        serde_json::from_str(&format!("\"{}\"", item_type_str)).unwrap_or(MediaType::Unknown);
    let metadata: MediaMetadata = serde_json::from_str(&metadata_str).unwrap_or_default();

    Ok(MediaItem {
        id: Some(id),
        library_id,
        item_type,
        title,
        original_title,
        release_year,
        added_at,
        file_path: PathBuf::from(file_path),
        file_name,
        file_size: file_size as u64,
        technical: TechnicalInfo {
            duration_seconds,
            resolution,
            video_codec,
            audio_codec,
            audio_channels,
            container,
        },
        metadata,
    })
}

#[derive(Clone)]
pub struct WidgetQueries {
    pool: Pool,
}

impl WidgetQueries {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn find_recently_added(
        &self,
        limit: u32,
        unlocked_ids: &[String],
    ) -> Result<Vec<MediaItem>> {
        self.find_recently_added_paginated(None, limit, 0, unlocked_ids, None)
            .await
    }

    pub async fn find_recently_added_paginated(
        &self,
        library_id: Option<&str>,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        let lib_id = library_id.map(|s| s.to_string());
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (filter_clause, filter_params) =
                build_filter_clauses(filters_owned.as_ref(), &unlocked, now);
            let mut sql_params = Vec::new();
            let where_clause = if let Some(lid) = lib_id {
                sql_params.push(rusqlite::types::Value::Text(lid));
                format!("WHERE m.library_id = ? AND {filter_clause}")
            } else {
                format!("WHERE {filter_clause}")
            };
            sql_params.extend(filter_params);
            sql_params.push(rusqlite::types::Value::Integer(limit as i64));
            sql_params.push(rusqlite::types::Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {where_clause} \
                 ORDER BY m.added_at DESC, m.id DESC \
                 LIMIT ? OFFSET ?"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows =
                stmt.query_map(rusqlite::params_from_iter(sql_params), map_media_item_row)?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
        .await?
    }

    pub async fn find_top_rated(
        &self,
        limit: u32,
        unlocked_ids: &[String],
    ) -> Result<Vec<MediaItem>> {
        self.find_top_rated_paginated(limit, 0, unlocked_ids, None).await
    }

    pub async fn find_top_rated_paginated(
        &self,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (filter_clause, mut sql_params) =
                build_filter_clauses(filters_owned.as_ref(), &unlocked, now);
            sql_params.push(rusqlite::types::Value::Integer(limit as i64));
            sql_params.push(rusqlite::types::Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE json_extract(m.metadata, '$.rating') IS NOT NULL \
                   AND {filter_clause} \
                 ORDER BY CAST(json_extract(m.metadata, '$.rating') AS REAL) DESC, m.id DESC \
                 LIMIT ? OFFSET ?"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows =
                stmt.query_map(rusqlite::params_from_iter(sql_params), map_media_item_row)?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
        .await?
    }

    pub async fn find_by_genre(
        &self,
        genre: &str,
        limit: u32,
        unlocked_ids: &[String],
    ) -> Result<Vec<MediaItem>> {
        self.find_by_genre_paginated(genre, limit, 0, unlocked_ids, None)
            .await
    }

    pub async fn find_by_genre_paginated(
        &self,
        genre: &str,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        let genre_str = genre.to_string();
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (filter_clause, filter_params) =
                build_filter_clauses(filters_owned.as_ref(), &unlocked, now);
            let mut sql_params = vec![
                rusqlite::types::Value::Text(genre_str.clone()),
                rusqlite::types::Value::Text(genre_str),
            ];
            sql_params.extend(filter_params);
            sql_params.push(rusqlite::types::Value::Integer(limit as i64));
            sql_params.push(rusqlite::types::Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE ( \
                    (json_extract(m.metadata, '$.genres') IS NOT NULL AND EXISTS ( \
                        SELECT 1 FROM json_each(json_extract(m.metadata, '$.genres')) WHERE LOWER(value) = LOWER(?) \
                    )) \
                    OR (json_extract(m.metadata, '$.tags') IS NOT NULL AND EXISTS ( \
                        SELECT 1 FROM json_each(json_extract(m.metadata, '$.tags')) WHERE LOWER(value) = LOWER(?) \
                    )) \
                 ) AND {filter_clause} \
                 ORDER BY m.title ASC, m.id ASC \
                 LIMIT ? OFFSET ?"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(sql_params), map_media_item_row)?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
        .await?
    }

    pub async fn find_by_library_paginated(
        &self,
        library_id: &str,
        limit: u32,
        offset: u32,
        sort: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<(Vec<MediaItem>, u64)> {
        let lib_id = library_id.to_string();
        let order_clause = match sort.map(|s| s.trim().to_lowercase()).as_deref() {
            Some("title:desc") | Some("title_desc") => "m.title DESC, m.id DESC",
            Some("release_year:asc")
            | Some("release_year_asc")
            | Some("year:asc")
            | Some("year_asc") => "m.release_year ASC NULLS LAST, m.id ASC",
            Some("release_year:desc")
            | Some("release_year_desc")
            | Some("year:desc")
            | Some("year_desc") => "m.release_year DESC NULLS LAST, m.id DESC",
            Some("added_at:asc") | Some("added_at_asc") => "m.added_at ASC, m.id ASC",
            Some("added_at:desc") | Some("added_at_desc") => "m.added_at DESC, m.id DESC",
            Some("rating:desc") | Some("rating_desc") => {
                "CAST(json_extract(m.metadata, '$.rating') AS REAL) DESC NULLS LAST, m.id DESC"
            }
            Some("rating:asc") | Some("rating_asc") => {
                "CAST(json_extract(m.metadata, '$.rating') AS REAL) ASC NULLS LAST, m.id ASC"
            }
            _ => "m.title ASC, m.id ASC",
        };

        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (filter_clause, filter_params) =
                build_filter_clauses(filters_owned.as_ref(), &unlocked, now);

            let mut count_params = vec![rusqlite::types::Value::Text(lib_id.clone())];
            count_params.extend(filter_params.clone());

            let count_sql = format!(
                "SELECT COUNT(*) FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE m.library_id = ? AND {filter_clause}"
            );
            let mut count_stmt = c.prepare(&count_sql)?;
            let count: i64 =
                count_stmt.query_row(rusqlite::params_from_iter(count_params), |row| row.get(0))?;

            let mut select_params = vec![rusqlite::types::Value::Text(lib_id)];
            select_params.extend(filter_params);
            select_params.push(rusqlite::types::Value::Integer(limit as i64));
            select_params.push(rusqlite::types::Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE m.library_id = ? AND {filter_clause} \
                 ORDER BY {order_clause} \
                 LIMIT ? OFFSET ?"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(select_params),
                map_media_item_row,
            )?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok((result, count as u64))
        })
        .await?
    }

    pub async fn find_spotlight_candidate(
        &self,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Option<MediaItem>> {
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (filter_clause, sql_params) =
                build_filter_clauses(filters_owned.as_ref(), &unlocked, now);
            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE {filter_clause} \
                 ORDER BY \
                    CASE \
                        WHEN json_extract(m.metadata, '$.backdrop_path') IS NOT NULL \
                             AND json_extract(m.metadata, '$.backdrop_path') != '' \
                             AND json_extract(m.metadata, '$.rating') IS NOT NULL \
                        THEN 3 \
                        WHEN json_extract(m.metadata, '$.backdrop_path') IS NOT NULL \
                             AND json_extract(m.metadata, '$.backdrop_path') != '' \
                        THEN 2 \
                        WHEN json_extract(m.metadata, '$.rating') IS NOT NULL \
                        THEN 1 \
                        ELSE 0 \
                    END DESC, \
                    CAST(json_extract(m.metadata, '$.rating') AS REAL) DESC NULLS LAST, \
                    m.added_at DESC, \
                    m.id DESC \
                 LIMIT 1"
            );
            let mut stmt = c.prepare(&sql)?;
            let mut rows = stmt.query(rusqlite::params_from_iter(sql_params))?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_media_item_row(row)?))
            } else {
                Ok(None)
            }
        })
        .await?
    }
}
