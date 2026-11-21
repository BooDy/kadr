use crate::error::Result;
use crate::repos::filter_compiler::{compile_filter_node, CompiledFilter};
use deadpool_sqlite::Pool;
use kadr_core::ast::WidgetFilterConfig;
use kadr_core::filter_ast::CatalogFacets;
use kadr_core::models::{AudioStreamInfo, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use rusqlite::types::Value;
use std::path::PathBuf;

pub const SELECT_JOINED_COLUMNS: &str =
    "m.id, m.library_id, m.item_type, m.title, m.original_title, m.release_year, \
 m.duration_seconds, m.added_at, m.file_path, m.file_name, m.file_size, \
 m.resolution, m.video_codec, m.audio_codec, m.audio_channels, m.container, m.metadata, m.locked_fields, m.audio_tracks_json";

fn current_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn resolve_effective_library_id(
    library_id: Option<&str>,
    filters: Option<&WidgetFilterConfig>,
) -> Option<String> {
    if filters.map(|f| f.all_libraries).unwrap_or(false) {
        None
    } else {
        filters
            .and_then(|f| f.library_id.clone())
            .or_else(|| library_id.map(|s| s.to_string()))
    }
}

pub fn privacy_clause(unlocked_ids: &[String]) -> (String, Vec<Value>) {
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
            .map(|id| Value::Text(id.clone()))
            .collect();
        (clause, params)
    }
}

pub fn build_filter_envelope(
    filters: Option<&WidgetFilterConfig>,
    unlocked_ids: &[String],
    current_time_epoch_secs: i64,
) -> CompiledFilter {
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    let mut needs_playback_join = false;
    let mut references_item_type = false;

    // 1. Fail-closed outer privacy filter
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
            clauses.push(format!("(m.library_id NOT IN ({placeholders}))"));
            for lib_id in &f.exclude_library_ids {
                params.push(Value::Text(lib_id.clone()));
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
                "(NOT EXISTS (
                    SELECT 1 FROM json_each(json_extract(m.metadata, '$.genres'))
                    WHERE LOWER(value) IN ({placeholders})
                ))"
            ));
            for genre in &f.exclude_genres {
                params.push(Value::Text(genre.trim().to_lowercase()));
            }
        }

        // 4. Exclude by date added (max age cutoff)
        if let Some(days) = f.max_age_days {
            if days > 0 {
                let cutoff = current_time_epoch_secs.saturating_sub(days as i64 * 86_400);
                clauses.push("(m.added_at >= ?)".to_string());
                params.push(Value::Integer(cutoff));
            }
        }

        // 5. Declarative recursive FilterNode AST (wrapped in parentheses inside outer AND)
        if let Some(ref tree) = f.rule_tree {
            let compiled = compile_filter_node(tree, current_time_epoch_secs);
            clauses.push(format!("({})", compiled.sql));
            params.extend(compiled.params);
            needs_playback_join = compiled.needs_playback_join;
            references_item_type = compiled.references_item_type;
        }
    }

    CompiledFilter {
        sql: clauses.join(" AND "),
        params,
        needs_playback_join,
        references_item_type,
    }
}

pub fn build_filter_clauses(
    filters: Option<&WidgetFilterConfig>,
    unlocked_ids: &[String],
    current_time_epoch_secs: i64,
) -> (String, Vec<Value>) {
    let env = build_filter_envelope(filters, unlocked_ids, current_time_epoch_secs);
    (env.sql, env.params)
}

pub fn resolve_sort_clause(
    sort: Option<&str>,
    default_clause: &'static str,
) -> (&'static str, bool) {
    match sort.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("title:asc") | Some("title_asc") => ("m.title ASC, m.id ASC", false),
        Some("title:desc") | Some("title_desc") => ("m.title DESC, m.id DESC", false),
        Some("release_year:asc")
        | Some("release_year_asc")
        | Some("year:asc")
        | Some("year_asc") => ("m.release_year ASC NULLS LAST, m.id ASC", false),
        Some("release_year:desc")
        | Some("release_year_desc")
        | Some("year:desc")
        | Some("year_desc") => ("m.release_year DESC NULLS LAST, m.id DESC", false),
        Some("added_at:asc") | Some("added_at_asc") => ("m.added_at ASC, m.id ASC", false),
        Some("added_at:desc") | Some("added_at_desc") => ("m.added_at DESC, m.id DESC", false),
        Some("rating:desc") | Some("rating_desc") => (
            "CAST(json_extract(m.metadata, '$.rating') AS REAL) DESC NULLS LAST, m.id DESC",
            false,
        ),
        Some("rating:asc") | Some("rating_asc") => (
            "CAST(json_extract(m.metadata, '$.rating') AS REAL) ASC NULLS LAST, m.id ASC",
            false,
        ),
        Some("duration_seconds:asc")
        | Some("duration_seconds_asc")
        | Some("duration:asc")
        | Some("duration_asc") => ("m.duration_seconds ASC, m.id ASC", false),
        Some("duration_seconds:desc")
        | Some("duration_seconds_desc")
        | Some("duration:desc")
        | Some("duration_desc") => ("m.duration_seconds DESC, m.id DESC", false),
        Some("last_watched_at:desc") | Some("last_watched_at_desc") => {
            ("COALESCE(ups.last_watched_at, 0) DESC, m.id DESC", true)
        }
        Some("last_watched_at:asc") | Some("last_watched_at_asc") => {
            ("COALESCE(ups.last_watched_at, 0) ASC, m.id ASC", true)
        }
        _ => (default_clause, false),
    }
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
    let locked_fields: Vec<String> = row
        .get::<_, String>("locked_fields")
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default();
    let audio_tracks: Vec<AudioStreamInfo> = row
        .get::<_, Option<String>>("audio_tracks_json")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

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
            audio_tracks,
        },
        metadata,
        locked_fields,
    })
}

fn query_scalar_facet(
    c: &rusqlite::Connection,
    expr: &str,
    base_where: &str,
    base_params: &[Value],
) -> rusqlite::Result<Vec<String>> {
    let sql = format!(
        "SELECT TRIM(CAST({expr} AS TEXT)) AS val \
         FROM media_items m \
         JOIN libraries l ON m.library_id = l.id \
         WHERE {base_where} AND {expr} IS NOT NULL AND TRIM(CAST({expr} AS TEXT)) != '' \
         GROUP BY LOWER(val) \
         ORDER BY COUNT(*) DESC, val ASC \
         LIMIT 100"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(base_params.iter().cloned()),
        |r| r.get::<_, String>(0),
    )?;
    rows.collect()
}

fn query_array_facet(
    c: &rusqlite::Connection,
    json_path: &str,
    base_where: &str,
    base_params: &[Value],
) -> rusqlite::Result<Vec<String>> {
    let safe_arr = format!(
        "CASE WHEN json_type(m.metadata, '{json_path}') = 'array' \
         THEN json_extract(m.metadata, '{json_path}') ELSE '[]' END"
    );
    let sql = format!(
        "SELECT TRIM(CAST(je.value AS TEXT)) AS val \
         FROM media_items m \
         JOIN libraries l ON m.library_id = l.id, \
         json_each({safe_arr}) je \
         WHERE {base_where} AND je.value IS NOT NULL AND TRIM(CAST(je.value AS TEXT)) != '' \
         GROUP BY LOWER(val) \
         ORDER BY COUNT(*) DESC, val ASC \
         LIMIT 100"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(base_params.iter().cloned()),
        |r| r.get::<_, String>(0),
    )?;
    rows.collect()
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
        self.find_recently_added_paginated_for_user(
            library_id,
            None,
            limit,
            offset,
            unlocked_ids,
            filters,
        )
        .await
    }

    pub async fn find_recently_added_paginated_for_user(
        &self,
        library_id: Option<&str>,
        user_id: Option<&str>,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        let filters_owned = filters.cloned();
        let lib_id = resolve_effective_library_id(library_id, filters_owned.as_ref());
        let uid = user_id.unwrap_or("").to_string();
        let unlocked = unlocked_ids.to_vec();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let envelope = build_filter_envelope(filters_owned.as_ref(), &unlocked, now);
            let mut sql_params = Vec::new();

            let playback_join = if envelope.needs_playback_join {
                sql_params.push(Value::Text(uid));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            let mut where_parts = Vec::new();
            if let Some(lid) = lib_id {
                sql_params.push(Value::Text(lid));
                where_parts.push("m.library_id = ?".to_string());
            }
            if !envelope.references_item_type {
                where_parts.push("m.item_type != 'episode'".to_string());
            }
            where_parts.push(envelope.sql);

            sql_params.extend(envelope.params);
            sql_params.push(Value::Integer(limit as i64));
            sql_params.push(Value::Integer(offset as i64));

            let where_body = where_parts.join(" AND ");
            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body} \
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
        self.find_top_rated_paginated(None, limit, 0, unlocked_ids, None)
            .await
    }

    pub async fn find_top_rated_paginated(
        &self,
        library_id: Option<&str>,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        self.find_top_rated_paginated_for_user(
            library_id,
            None,
            limit,
            offset,
            unlocked_ids,
            filters,
        )
        .await
    }

    pub async fn find_top_rated_paginated_for_user(
        &self,
        library_id: Option<&str>,
        user_id: Option<&str>,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let lib_id = resolve_effective_library_id(library_id, filters_owned.as_ref());
        let uid = user_id.unwrap_or("").to_string();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let envelope = build_filter_envelope(filters_owned.as_ref(), &unlocked, now);
            let mut sql_params = Vec::new();

            let playback_join = if envelope.needs_playback_join {
                sql_params.push(Value::Text(uid));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            let mut where_parts = Vec::new();
            if let Some(lid) = lib_id {
                sql_params.push(Value::Text(lid));
                where_parts.push("m.library_id = ?".to_string());
            }
            where_parts.push("json_extract(m.metadata, '$.rating') IS NOT NULL".to_string());
            if !envelope.references_item_type {
                where_parts.push("m.item_type != 'episode'".to_string());
            }
            where_parts.push(envelope.sql);

            sql_params.extend(envelope.params);
            sql_params.push(Value::Integer(limit as i64));
            sql_params.push(Value::Integer(offset as i64));

            let where_body = where_parts.join(" AND ");
            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body} \
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
        self.find_by_genre_paginated(genre, None, limit, 0, unlocked_ids, None)
            .await
    }

    pub async fn find_by_genre_paginated(
        &self,
        genre: &str,
        library_id: Option<&str>,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        self.find_by_genre_paginated_for_user(
            genre,
            library_id,
            None,
            limit,
            offset,
            unlocked_ids,
            filters,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn find_by_genre_paginated_for_user(
        &self,
        genre: &str,
        library_id: Option<&str>,
        user_id: Option<&str>,
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        let genre_str = genre.to_string();
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let lib_id = resolve_effective_library_id(library_id, filters_owned.as_ref());
        let uid = user_id.unwrap_or("").to_string();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let envelope = build_filter_envelope(filters_owned.as_ref(), &unlocked, now);
            let mut sql_params = Vec::new();

            let playback_join = if envelope.needs_playback_join {
                sql_params.push(Value::Text(uid));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            sql_params.push(Value::Text(genre_str.clone()));
            sql_params.push(Value::Text(genre_str));

            let item_type_condition = if envelope.references_item_type {
                ""
            } else {
                "AND m.item_type != 'episode'"
            };
            let lib_condition = if let Some(lid) = lib_id {
                sql_params.push(Value::Text(lid));
                "AND m.library_id = ?"
            } else {
                ""
            };
            sql_params.extend(envelope.params);
            sql_params.push(Value::Integer(limit as i64));
            sql_params.push(Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE ( \
                    (json_extract(m.metadata, '$.genres') IS NOT NULL AND EXISTS ( \
                        SELECT 1 FROM json_each(json_extract(m.metadata, '$.genres')) WHERE LOWER(value) = LOWER(?) \
                    )) \
                    OR (json_extract(m.metadata, '$.tags') IS NOT NULL AND EXISTS ( \
                        SELECT 1 FROM json_each(json_extract(m.metadata, '$.tags')) WHERE LOWER(value) = LOWER(?) \
                    )) \
                 ) {item_type_condition} {lib_condition} AND {} \
                 ORDER BY m.title ASC, m.id ASC \
                 LIMIT ? OFFSET ?",
                envelope.sql
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

    pub async fn find_by_library_paginated(
        &self,
        library_id: &str,
        limit: u32,
        offset: u32,
        sort: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<(Vec<MediaItem>, u64)> {
        self.find_by_library_paginated_for_user(
            library_id,
            None,
            limit,
            offset,
            sort,
            unlocked_ids,
            filters,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn find_by_library_paginated_for_user(
        &self,
        library_id: &str,
        user_id: Option<&str>,
        limit: u32,
        offset: u32,
        sort: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<(Vec<MediaItem>, u64)> {
        let lib_id = library_id.to_string();
        let uid = user_id.unwrap_or("").to_string();
        let (order_clause, sort_needs_playback_join) =
            resolve_sort_clause(sort, "m.title ASC, m.id ASC");

        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let envelope = build_filter_envelope(filters_owned.as_ref(), &unlocked, now);
            let needs_join = envelope.needs_playback_join || sort_needs_playback_join;

            let mut base_params = Vec::new();
            let playback_join = if needs_join {
                base_params.push(Value::Text(uid));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            base_params.push(Value::Text(lib_id));
            let item_type_condition = if envelope.references_item_type {
                ""
            } else {
                "AND m.item_type != 'episode'"
            };
            base_params.extend(envelope.params);

            let where_body = format!(
                "m.library_id = ? {item_type_condition} AND {}",
                envelope.sql
            );

            let count_sql = format!(
                "SELECT COUNT(*) FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body}"
            );
            let mut count_stmt = c.prepare(&count_sql)?;
            let count: i64 = count_stmt.query_row(
                rusqlite::params_from_iter(base_params.iter().cloned()),
                |row| row.get(0),
            )?;

            let mut select_params = base_params;
            select_params.push(Value::Integer(limit as i64));
            select_params.push(Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body} \
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
        library_id: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Option<MediaItem>> {
        self.find_spotlight_candidate_for_user(library_id, None, unlocked_ids, filters)
            .await
    }

    pub async fn find_spotlight_candidate_for_user(
        &self,
        library_id: Option<&str>,
        user_id: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Option<MediaItem>> {
        let unlocked = unlocked_ids.to_vec();
        let filters_owned = filters.cloned();
        let lib_id = resolve_effective_library_id(library_id, filters_owned.as_ref());
        let uid = user_id.unwrap_or("").to_string();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let envelope = build_filter_envelope(filters_owned.as_ref(), &unlocked, now);
            let mut sql_params = Vec::new();

            let playback_join = if envelope.needs_playback_join {
                sql_params.push(Value::Text(uid));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            let mut where_parts = Vec::new();
            if !envelope.references_item_type {
                where_parts.push("m.item_type != 'episode'".to_string());
            }
            if let Some(lid) = lib_id {
                sql_params.push(Value::Text(lid));
                where_parts.push("m.library_id = ?".to_string());
            }
            where_parts.push(envelope.sql);
            sql_params.extend(envelope.params);

            let where_body = where_parts.join(" AND ");
            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body} \
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

    #[allow(clippy::too_many_arguments)]
    pub async fn find_custom_paginated(
        &self,
        library_id: Option<&str>,
        user_id: &str,
        limit: u32,
        offset: u32,
        sort: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<(Vec<MediaItem>, u64)> {
        let filters_owned = filters.cloned();
        let lib_id = resolve_effective_library_id(library_id, filters_owned.as_ref());
        let uid = user_id.to_string();
        let (order_clause, sort_needs_playback_join) =
            resolve_sort_clause(sort, "m.added_at DESC, m.id DESC");
        let unlocked = unlocked_ids.to_vec();
        let now = current_epoch_secs();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let envelope = build_filter_envelope(filters_owned.as_ref(), &unlocked, now);
            let needs_join = envelope.needs_playback_join || sort_needs_playback_join;

            let mut base_params = Vec::new();
            let playback_join = if needs_join {
                base_params.push(Value::Text(uid));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            let mut where_parts = Vec::new();
            if let Some(lid) = lib_id {
                base_params.push(Value::Text(lid));
                where_parts.push("m.library_id = ?".to_string());
            }
            if !envelope.references_item_type {
                where_parts.push("m.item_type != 'episode'".to_string());
            }
            where_parts.push(envelope.sql);
            base_params.extend(envelope.params);

            let where_body = where_parts.join(" AND ");

            let count_sql = format!(
                "SELECT COUNT(*) FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body}"
            );
            let mut count_stmt = c.prepare(&count_sql)?;
            let count: i64 = count_stmt.query_row(
                rusqlite::params_from_iter(base_params.iter().cloned()),
                |row| row.get(0),
            )?;

            let mut select_params = base_params;
            select_params.push(Value::Integer(limit as i64));
            select_params.push(Value::Integer(offset as i64));

            let sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS} FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 {playback_join} \
                 WHERE {where_body} \
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

    pub async fn get_catalog_facets(
        &self,
        library_id: Option<&str>,
        unlocked_ids: &[String],
    ) -> Result<CatalogFacets> {
        let lib_id = library_id.map(|s| s.to_string());
        let unlocked = unlocked_ids.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let (priv_clause, mut base_params) = privacy_clause(&unlocked);
            let base_where = if let Some(lid) = lib_id {
                base_params.push(Value::Text(lid));
                format!("{priv_clause} AND m.library_id = ?")
            } else {
                priv_clause
            };

            let genres = query_array_facet(c, "$.genres", &base_where, &base_params)?;
            let tags = query_array_facet(c, "$.tags", &base_where, &base_params)?;
            let countries = query_scalar_facet(
                c,
                "json_extract(m.metadata, '$.country')",
                &base_where,
                &base_params,
            )?;
            let languages = query_scalar_facet(
                c,
                "json_extract(m.metadata, '$.language')",
                &base_where,
                &base_params,
            )?;
            let studios = query_scalar_facet(
                c,
                "json_extract(m.metadata, '$.studio')",
                &base_where,
                &base_params,
            )?;
            let directors = query_scalar_facet(
                c,
                "json_extract(m.metadata, '$.director')",
                &base_where,
                &base_params,
            )?;
            let resolutions = query_scalar_facet(c, "m.resolution", &base_where, &base_params)?;
            let video_codecs = query_scalar_facet(c, "m.video_codec", &base_where, &base_params)?;
            let audio_codecs = query_scalar_facet(c, "m.audio_codec", &base_where, &base_params)?;
            let containers = query_scalar_facet(c, "m.container", &base_where, &base_params)?;

            let year_sql = format!(
                "SELECT MIN(m.release_year), MAX(m.release_year) \
                 FROM media_items m \
                 JOIN libraries l ON m.library_id = l.id \
                 WHERE {base_where} AND m.release_year IS NOT NULL"
            );
            let (year_min, year_max): (Option<i32>, Option<i32>) = c.query_row(
                &year_sql,
                rusqlite::params_from_iter(base_params.iter().cloned()),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;

            Ok(CatalogFacets {
                genres,
                countries,
                languages,
                studios,
                directors,
                tags,
                resolutions,
                video_codecs,
                audio_codecs,
                containers,
                year_min,
                year_max,
            })
        })
        .await?
    }
}
