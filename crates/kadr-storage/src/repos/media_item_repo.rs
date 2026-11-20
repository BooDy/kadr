use crate::error::Result;
use crate::repos::widget_queries::WidgetQueries;
use deadpool_sqlite::Pool;
use kadr_core::ast::WidgetFilterConfig;
use kadr_core::models::{
    AudioStreamInfo, MediaItem, MediaMetadata, MediaType, MetadataUpdate, TechnicalInfo,
};
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct MediaItemRepository {
    pool: Pool,
}

pub type MediaItemRepo = MediaItemRepository;

impl MediaItemRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub fn widget_queries(&self) -> WidgetQueries {
        WidgetQueries::new(self.pool.clone())
    }

    pub async fn batch_upsert(&self, items: &[MediaItem]) -> Result<usize> {
        self.upsert_batch(items).await
    }

    pub async fn upsert_batch(&self, items: &[MediaItem]) -> Result<usize> {
        if items.is_empty() {
            return Ok(0);
        }
        let items = items.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let mut count = 0;
            {
                let mut existing_stmt = tx.prepare(
                    "SELECT title, original_title, release_year, metadata, locked_fields
                     FROM media_items
                     WHERE file_path = ?1",
                )?;
                let mut stmt = tx.prepare(
                    "INSERT INTO media_items (
                        library_id, item_type, title, original_title, release_year,
                        duration_seconds, added_at, file_path, file_name, file_size,
                        resolution, video_codec, audio_codec, audio_channels, container, metadata,
                        locked_fields, audio_tracks_json
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                     ON CONFLICT(file_path) DO UPDATE SET
                        library_id = excluded.library_id,
                        item_type = excluded.item_type,
                        title = excluded.title,
                        original_title = excluded.original_title,
                        release_year = excluded.release_year,
                        duration_seconds = excluded.duration_seconds,
                        file_name = excluded.file_name,
                        file_size = excluded.file_size,
                        resolution = excluded.resolution,
                        video_codec = excluded.video_codec,
                        audio_codec = excluded.audio_codec,
                        audio_channels = excluded.audio_channels,
                        container = excluded.container,
                        metadata = excluded.metadata,
                        locked_fields = excluded.locked_fields,
                        audio_tracks_json = COALESCE(excluded.audio_tracks_json, media_items.audio_tracks_json)",
                )?;

                struct ExistingMetadataRow {
                    title: String,
                    original_title: Option<String>,
                    release_year: Option<i32>,
                    meta_json: String,
                    locks_json: String,
                }

                for item in &items {
                    let path_str = item.file_path.to_string_lossy().into_owned();
                    let existing_row: Option<ExistingMetadataRow> = existing_stmt
                        .query_row(params![&path_str], |row| {
                            Ok(ExistingMetadataRow {
                                title: row.get(0)?,
                                original_title: row.get(1)?,
                                release_year: row.get(2)?,
                                meta_json: row.get(3)?,
                                locks_json: row.get(4)?,
                            })
                        })
                        .optional()?;

                    let (final_title, final_orig_title, final_year, final_meta, final_locks) =
                        if let Some(ex) = existing_row {
                            let ex_title = ex.title;
                            let ex_orig_title = ex.original_title;
                            let ex_year = ex.release_year;
                            let ex_meta_str = ex.meta_json;
                            let ex_locks_str = ex.locks_json;
                            let ex_meta: MediaMetadata =
                                serde_json::from_str(&ex_meta_str).unwrap_or_default();
                            let mut locks: Vec<String> =
                                serde_json::from_str(&ex_locks_str).unwrap_or_default();
                            for lf in &item.locked_fields {
                                if !locks.contains(lf) {
                                    locks.push(lf.clone());
                                }
                            }
                            let lock_set: HashSet<&str> =
                                locks.iter().map(String::as_str).collect();

                            let title = if lock_set.contains("title") {
                                ex_title
                            } else {
                                item.title.clone()
                            };
                            let orig_title = if lock_set.contains("original_title") {
                                ex_orig_title
                            } else {
                                item.original_title.clone()
                            };
                            let year = if lock_set.contains("release_year") {
                                ex_year
                            } else {
                                item.release_year
                            };

                            let mut meta = item.metadata.clone();
                            if lock_set.contains("overview") {
                                meta.overview = ex_meta.overview;
                            }
                            if lock_set.contains("director") {
                                meta.director = ex_meta.director;
                            }
                            if lock_set.contains("writers") {
                                meta.writers = ex_meta.writers;
                            }
                            if lock_set.contains("actors") {
                                meta.actors = ex_meta.actors;
                            }
                            if lock_set.contains("genres") {
                                meta.genres = ex_meta.genres;
                            }
                            if lock_set.contains("tags") {
                                meta.tags = ex_meta.tags;
                            }
                            if lock_set.contains("studio") {
                                meta.studio = ex_meta.studio;
                            }
                            if lock_set.contains("country") {
                                meta.country = ex_meta.country;
                            }
                            if lock_set.contains("language") {
                                meta.language = ex_meta.language;
                            }
                            if lock_set.contains("rating") {
                                meta.rating = ex_meta.rating;
                            }
                            if lock_set.contains("poster_path") {
                                meta.poster_path = ex_meta.poster_path;
                            }
                            if lock_set.contains("backdrop_path") {
                                meta.backdrop_path = ex_meta.backdrop_path;
                            }
                            if lock_set.contains("series_title") {
                                meta.series_title = ex_meta.series_title;
                            }
                            if lock_set.contains("season") {
                                meta.season = ex_meta.season;
                            }
                            if lock_set.contains("episode") {
                                meta.episode = ex_meta.episode;
                            }
                            if !meta.extra.contains_key("tmdb_id") {
                                if let Some(tid) = ex_meta.extra.get("tmdb_id") {
                                    meta.extra.insert("tmdb_id".to_string(), tid.clone());
                                }
                            }
                            if !meta.extra.contains_key("tmdb_media_type") {
                                if let Some(tmt) = ex_meta.extra.get("tmdb_media_type") {
                                    meta.extra
                                        .insert("tmdb_media_type".to_string(), tmt.clone());
                                }
                            }
                            (title, orig_title, year, meta, locks)
                        } else {
                            let mut locks = Vec::new();
                            for lf in &item.locked_fields {
                                if !locks.contains(lf) {
                                    locks.push(lf.clone());
                                }
                            }
                            (
                                item.title.clone(),
                                item.original_title.clone(),
                                item.release_year,
                                item.metadata.clone(),
                                locks,
                            )
                        };

                    let type_str = serde_json::to_string(&item.item_type)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let meta_json =
                        serde_json::to_string(&final_meta).unwrap_or_else(|_| "{}".to_string());
                    let locks_json =
                        serde_json::to_string(&final_locks).unwrap_or_else(|_| "[]".to_string());
                    let audio_tracks_json: Option<String> =
                        if item.technical.audio_tracks.is_empty() {
                            None
                        } else {
                            serde_json::to_string(&item.technical.audio_tracks).ok()
                        };

                    stmt.execute(params![
                        item.library_id,
                        type_str,
                        final_title,
                        final_orig_title,
                        final_year,
                        item.technical.duration_seconds,
                        item.added_at,
                        path_str,
                        item.file_name,
                        item.file_size as i64,
                        item.technical.resolution,
                        item.technical.video_codec,
                        item.technical.audio_codec,
                        item.technical.audio_channels,
                        item.technical.container,
                        meta_json,
                        locks_json,
                        audio_tracks_json,
                    ])?;
                    count += 1;
                }
            }
            tx.commit()?;
            Ok(count)
        })
        .await?
    }

    pub async fn update_audio_tracks(
        &self,
        item_id: i64,
        tracks: &[AudioStreamInfo],
    ) -> Result<()> {
        let tracks_json: Option<String> = if tracks.is_empty() {
            None
        } else {
            Some(serde_json::to_string(tracks)?)
        };
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "UPDATE media_items SET audio_tracks_json = ?1 WHERE id = ?2",
                params![tracks_json, item_id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn update_metadata(
        &self,
        id: i64,
        patch: &MetadataUpdate,
    ) -> Result<Option<MediaItem>> {
        let patch = patch.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let sql = format!("SELECT {SELECT_COLUMNS} FROM media_items WHERE id = ?1");
            let maybe_item = {
                let mut stmt = tx.prepare(&sql)?;
                let mut rows = stmt.query(params![id])?;
                if let Some(row) = rows.next()? {
                    Some(map_media_item_row(row)?)
                } else {
                    None
                }
            };

            let Some(mut item) = maybe_item else {
                return Ok(None);
            };

            let mut locks: Vec<String> = if patch.reset_locks {
                Vec::new()
            } else {
                item.locked_fields.clone()
            };

            fn add_lock(locks: &mut Vec<String>, field: &str) {
                if !locks.iter().any(|f| f == field) {
                    locks.push(field.to_string());
                }
            }

            fn norm_opt_str(val: &Option<String>) -> Option<String> {
                val.as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
            }

            fn norm_vec(vals: &[String]) -> Vec<String> {
                let mut out = Vec::new();
                for v in vals {
                    let trimmed = v.trim();
                    if !trimmed.is_empty() && !out.iter().any(|existing: &String| existing == trimmed) {
                        out.push(trimmed.to_string());
                    }
                }
                out
            }

            if let Some(ref t) = patch.title {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    item.title = trimmed.to_string();
                    add_lock(&mut locks, "title");
                }
            }
            if let Some(ref v) = patch.original_title {
                item.original_title = norm_opt_str(v);
                add_lock(&mut locks, "original_title");
            }
            if let Some(v) = patch.release_year {
                item.release_year = v;
                add_lock(&mut locks, "release_year");
            }
            if let Some(ref v) = patch.overview {
                item.metadata.overview = norm_opt_str(v);
                add_lock(&mut locks, "overview");
            }
            if let Some(ref v) = patch.director {
                item.metadata.director = norm_opt_str(v);
                add_lock(&mut locks, "director");
            }
            if let Some(ref v) = patch.writers {
                item.metadata.writers = norm_vec(v);
                add_lock(&mut locks, "writers");
            }
            if let Some(ref v) = patch.actors {
                item.metadata.actors = norm_vec(v);
                add_lock(&mut locks, "actors");
            }
            if let Some(ref v) = patch.genres {
                item.metadata.genres = norm_vec(v);
                add_lock(&mut locks, "genres");
            }
            if let Some(ref v) = patch.tags {
                item.metadata.tags = norm_vec(v);
                add_lock(&mut locks, "tags");
            }
            if let Some(ref v) = patch.studio {
                item.metadata.studio = norm_opt_str(v);
                add_lock(&mut locks, "studio");
            }
            if let Some(ref v) = patch.country {
                item.metadata.country = norm_opt_str(v);
                add_lock(&mut locks, "country");
            }
            if let Some(ref v) = patch.language {
                item.metadata.language = norm_opt_str(v);
                add_lock(&mut locks, "language");
            }
            if let Some(v) = patch.rating {
                item.metadata.rating = v;
                add_lock(&mut locks, "rating");
            }
            if let Some(ref v) = patch.poster_path {
                item.metadata.poster_path = norm_opt_str(v);
                add_lock(&mut locks, "poster_path");
            }
            if let Some(ref v) = patch.backdrop_path {
                item.metadata.backdrop_path = norm_opt_str(v);
                add_lock(&mut locks, "backdrop_path");
            }
            if let Some(ref v) = patch.series_title {
                item.metadata.series_title = norm_opt_str(v);
                add_lock(&mut locks, "series_title");
            }
            if let Some(v) = patch.season {
                item.metadata.season = v;
                add_lock(&mut locks, "season");
            }
            if let Some(v) = patch.episode {
                item.metadata.episode = v;
                add_lock(&mut locks, "episode");
            }
            if let Some(v) = patch.tmdb_id {
                match v {
                    Some(tid) => {
                        item.metadata
                            .extra
                            .insert("tmdb_id".to_string(), serde_json::json!(tid));
                    }
                    None => {
                        item.metadata.extra.remove("tmdb_id");
                    }
                }
            }
            if let Some(ref v) = patch.tmdb_media_type {
                match norm_opt_str(v) {
                    Some(mt) => {
                        item.metadata
                            .extra
                            .insert("tmdb_media_type".to_string(), serde_json::json!(mt));
                    }
                    None => {
                        item.metadata.extra.remove("tmdb_media_type");
                    }
                }
            }

            item.locked_fields = locks;
            let meta_json =
                serde_json::to_string(&item.metadata).unwrap_or_else(|_| "{}".to_string());
            let locks_json =
                serde_json::to_string(&item.locked_fields).unwrap_or_else(|_| "[]".to_string());

            tx.execute(
                "UPDATE media_items
                 SET title = ?1, original_title = ?2, release_year = ?3, metadata = ?4, locked_fields = ?5
                 WHERE id = ?6",
                params![
                    &item.title,
                    &item.original_title,
                    item.release_year,
                    meta_json,
                    locks_json,
                    id,
                ],
            )?;

            tx.commit()?;
            Ok(Some(item))
        })
        .await?
    }

    pub async fn delete_by_path<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let path_str = path.as_ref().to_string_lossy().into_owned();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute(
                "DELETE FROM media_items WHERE file_path = ?1",
                params![path_str],
            )?;
            Ok(rows > 0)
        })
        .await?
    }

    pub async fn find_by_path<P: AsRef<Path>>(&self, path: P) -> Result<Option<MediaItem>> {
        let path_str = path.as_ref().to_string_lossy().into_owned();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!("SELECT {SELECT_COLUMNS} FROM media_items WHERE file_path = ?1");
            let mut stmt = c.prepare(&sql)?;
            let mut rows = stmt.query(params![path_str])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_media_item_row(row)?))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn find_by_paths(
        &self,
        paths: &[&std::path::Path],
    ) -> Result<std::collections::HashMap<std::path::PathBuf, MediaItem>> {
        if paths.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let path_strings: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let placeholders = path_strings
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT {SELECT_COLUMNS} FROM media_items WHERE file_path IN ({placeholders})"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows =
                stmt.query_map(rusqlite::params_from_iter(path_strings), map_media_item_row)?;
            let mut map = std::collections::HashMap::new();
            for row in rows {
                let item = row?;
                map.insert(item.file_path.clone(), item);
            }
            Ok(map)
        })
        .await?
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<MediaItem>> {
        self.get_by_id(id).await
    }

    pub async fn get_by_id(&self, id: i64) -> Result<Option<MediaItem>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!("SELECT {SELECT_COLUMNS} FROM media_items WHERE id = ?1");
            let mut stmt = c.prepare(&sql)?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_media_item_row(row)?))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn get_by_ids(&self, item_ids: &[i64]) -> Result<HashMap<i64, MediaItem>> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let item_ids = item_ids.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql =
                format!("SELECT {SELECT_COLUMNS} FROM media_items WHERE id IN ({placeholders})");
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(item_ids), map_media_item_row)?;
            let mut map = HashMap::new();
            for row in rows {
                let item = row?;
                if let Some(id) = item.id {
                    map.insert(id, item);
                }
            }
            Ok(map)
        })
        .await?
    }

    pub async fn list_by_library(
        &self,
        library_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MediaItem>> {
        let lib_id = library_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!(
                "SELECT {SELECT_COLUMNS}
                 FROM media_items
                 WHERE library_id = ?1
                 ORDER BY title ASC
                 LIMIT ?2 OFFSET ?3"
            );
            let mut stmt = c.prepare(&sql)?;

            let rows = stmt.query_map(
                params![lib_id, limit as i64, offset as i64],
                map_media_item_row,
            )?;

            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
        .await?
    }

    pub async fn list_all_by_library(&self, library_id: &str) -> Result<Vec<MediaItem>> {
        let lib_id = library_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!(
                "SELECT {SELECT_COLUMNS}
                 FROM media_items
                 WHERE library_id = ?1
                 ORDER BY id ASC"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(params![lib_id], map_media_item_row)?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
        .await?
    }

    pub async fn count_by_library(&self, library_id: &str) -> Result<usize> {
        let lib_id = library_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let count: i64 = c.query_row(
                "SELECT COUNT(*) FROM media_items WHERE library_id = ?1",
                params![lib_id],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
        .await?
    }

    pub async fn find_recently_added(
        &self,
        limit: u32,
        unlocked_ids: &[String],
    ) -> Result<Vec<MediaItem>> {
        self.widget_queries()
            .find_recently_added(limit, unlocked_ids)
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
        self.widget_queries()
            .find_recently_added_paginated(library_id, limit, offset, unlocked_ids, filters)
            .await
    }

    pub async fn find_top_rated(
        &self,
        limit: u32,
        unlocked_ids: &[String],
    ) -> Result<Vec<MediaItem>> {
        self.widget_queries()
            .find_top_rated(limit, unlocked_ids)
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
        self.widget_queries()
            .find_top_rated_paginated(library_id, limit, offset, unlocked_ids, filters)
            .await
    }

    pub async fn find_by_genre(
        &self,
        genre: &str,
        limit: u32,
        unlocked_ids: &[String],
    ) -> Result<Vec<MediaItem>> {
        self.widget_queries()
            .find_by_genre(genre, limit, unlocked_ids)
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
        self.widget_queries()
            .find_by_genre_paginated(genre, library_id, limit, offset, unlocked_ids, filters)
            .await
    }

    pub async fn find_by_library_paginated(
        &self,
        library_id: &str,
        limit: u32,
        offset: u32,
        sort_by: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<(Vec<MediaItem>, u64)> {
        self.widget_queries()
            .find_by_library_paginated(library_id, limit, offset, sort_by, unlocked_ids, filters)
            .await
    }

    pub async fn find_by_library(
        &self,
        library_id: &str,
        page: u32,
        page_size: u32,
        unlocked_ids: &[String],
    ) -> Result<(Vec<MediaItem>, u64)> {
        let offset = page.saturating_sub(1) * page_size;
        self.find_by_library_paginated(library_id, page_size, offset, None, unlocked_ids, None)
            .await
    }

    pub async fn find_spotlight_candidate(
        &self,
        library_id: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Option<MediaItem>> {
        self.widget_queries()
            .find_spotlight_candidate(library_id, unlocked_ids, filters)
            .await
    }

    pub async fn find_spotlight_candidate_default(&self) -> Result<Option<MediaItem>> {
        self.find_spotlight_candidate(None, &[], None).await
    }

    pub async fn find_episodes_by_series(&self, series_title: &str) -> Result<Vec<MediaItem>> {
        let title_param = series_title.to_string();
        let prefix_param = format!("{series_title}%");
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!(
                "SELECT {SELECT_COLUMNS} FROM media_items \
                 WHERE ( \
                    (json_extract(metadata, '$.series_title') IS NOT NULL AND LOWER(json_extract(metadata, '$.series_title')) = LOWER(?1)) \
                    OR (item_type = 'episode' AND LOWER(title) LIKE LOWER(?2)) \
                 ) \
                 ORDER BY \
                    CAST(COALESCE(json_extract(metadata, '$.season'), json_extract(metadata, '$.season_number'), 0) AS INTEGER) ASC, \
                    CAST(COALESCE(json_extract(metadata, '$.episode'), json_extract(metadata, '$.episode_number'), 0) AS INTEGER) ASC, \
                    title ASC, \
                    id ASC"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(params![title_param, prefix_param], map_media_item_row)?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        }).await?
    }

    pub async fn find_show_by_title(
        &self,
        library_id: &str,
        title: &str,
    ) -> Result<Option<MediaItem>> {
        let library_id = library_id.to_string();
        let title = title.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!(
                "SELECT {SELECT_COLUMNS} FROM media_items \
                 WHERE library_id = ?1 AND item_type = 'show' AND LOWER(title) = LOWER(?2) \
                 LIMIT 1"
            );
            let mut stmt = c.prepare(&sql)?;
            let mut rows = stmt.query(params![library_id, title])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_media_item_row(row)?))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    /// Deduplicates media items in storage that point to the same physical file on disk (via symlinks, hardlinks, or alias paths).
    /// Preserves user playback states by migrating them to the canonical item.
    pub async fn deduplicate_media_items(&self) -> Result<usize> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let (duplicates_to_delete, updates) = {
                let mut stmt = c.prepare("SELECT id, library_id, file_path FROM media_items ORDER BY id ASC")?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;

                let mut seen_canonical: std::collections::HashMap<std::path::PathBuf, i64> =
                    std::collections::HashMap::new();
                let mut duplicates: Vec<(i64, i64)> = Vec::new();
                let mut updates: Vec<(i64, String)> = Vec::new();

                for r in rows {
                    let (id, _lib_id, path_str) = r?;
                    let path = std::path::Path::new(&path_str);
                    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
                    let canon_str = canon.to_string_lossy().to_string();

                    if let Some(&keep_id) = seen_canonical.get(&canon) {
                        duplicates.push((id, keep_id));
                    } else {
                        seen_canonical.insert(canon, id);
                        if canon_str != path_str {
                            updates.push((id, canon_str));
                        }
                    }
                }
                (duplicates, updates)
            };

            if duplicates_to_delete.is_empty() && updates.is_empty() {
                return Ok(0);
            }

            let tx = c.transaction()?;
            let mut affected_playlists = std::collections::BTreeSet::new();
            for (del_id, keep_id) in &duplicates_to_delete {
                {
                    let mut pl_stmt = tx.prepare(
                        "SELECT playlist_id FROM playlist_items WHERE media_item_id = ?1",
                    )?;
                    let pl_rows = pl_stmt.query_map(rusqlite::params![del_id], |r| {
                        r.get::<_, String>(0)
                    })?;
                    for pl_id in pl_rows {
                        affected_playlists.insert(pl_id?);
                    }
                }
                tx.execute(
                    "UPDATE OR IGNORE playlist_items SET media_item_id = ?1 WHERE media_item_id = ?2",
                    rusqlite::params![keep_id, del_id],
                )?;
                tx.execute(
                    "DELETE FROM playlist_items WHERE media_item_id = ?1",
                    rusqlite::params![del_id],
                )?;

                let _ = tx.execute(
                    "UPDATE OR IGNORE user_playback_states SET media_item_id = ?1 WHERE media_item_id = ?2",
                    rusqlite::params![keep_id, del_id],
                );
                let _ = tx.execute(
                    "DELETE FROM user_playback_states WHERE media_item_id = ?1",
                    rusqlite::params![del_id],
                );
                let _ = tx.execute(
                    "DELETE FROM media_subtitles WHERE media_item_id = ?1",
                    rusqlite::params![del_id],
                );
                tx.execute(
                    "DELETE FROM media_items WHERE id = ?1",
                    rusqlite::params![del_id],
                )?;
            }

            for pl_id in affected_playlists {
                let ordered_ids: Vec<i64> = {
                    let mut stmt = tx.prepare(
                        "SELECT media_item_id FROM playlist_items \
                         WHERE playlist_id = ?1 ORDER BY position ASC, added_at ASC",
                    )?;
                    let rows = stmt.query_map(rusqlite::params![&pl_id], |r| r.get::<_, i64>(0))?;
                    let mut ids = Vec::new();
                    for r in rows {
                        ids.push(r?);
                    }
                    ids
                };
                for (idx, media_id) in ordered_ids.into_iter().enumerate() {
                    tx.execute(
                        "UPDATE playlist_items SET position = ?1 WHERE playlist_id = ?2 AND media_item_id = ?3",
                        rusqlite::params![idx as i64, &pl_id, media_id],
                    )?;
                }
            }

            for (id, canon_str) in updates {
                let _ = tx.execute(
                    "UPDATE OR IGNORE media_items SET file_path = ?1 WHERE id = ?2",
                    rusqlite::params![canon_str, id],
                );
            }

            tx.commit()?;
            Ok(duplicates_to_delete.len())
        })
        .await?
    }
}

pub(crate) const SELECT_COLUMNS: &str =
    "id, library_id, item_type, title, original_title, release_year, \
 duration_seconds, added_at, file_path, file_name, file_size, \
 resolution, video_codec, audio_codec, audio_channels, container, metadata, locked_fields, audio_tracks_json";

pub(crate) fn map_media_item_row(row: &rusqlite::Row) -> rusqlite::Result<MediaItem> {
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
