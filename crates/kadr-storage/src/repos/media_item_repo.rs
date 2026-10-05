use crate::error::Result;
use crate::repos::widget_queries::WidgetQueries;
use deadpool_sqlite::Pool;
use kadr_core::ast::WidgetFilterConfig;
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use rusqlite::params;
use std::collections::HashMap;
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
                let mut stmt = tx.prepare(
                    "INSERT INTO media_items (
                        library_id, item_type, title, original_title, release_year,
                        duration_seconds, added_at, file_path, file_name, file_size,
                        resolution, video_codec, audio_codec, audio_channels, container, metadata
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
                        metadata = excluded.metadata",
                )?;

                for item in &items {
                    let type_str = serde_json::to_string(&item.item_type)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let meta_json = serde_json::to_string(&item.metadata)
                        .unwrap_or_else(|_| "{}".to_string());

                    stmt.execute(params![
                        item.library_id,
                        type_str,
                        item.title,
                        item.original_title,
                        item.release_year,
                        item.technical.duration_seconds,
                        item.added_at,
                        item.file_path.to_string_lossy().into_owned(),
                        item.file_name,
                        item.file_size as i64,
                        item.technical.resolution,
                        item.technical.video_codec,
                        item.technical.audio_codec,
                        item.technical.audio_channels,
                        item.technical.container,
                        meta_json,
                    ])?;
                    count += 1;
                }
            }
            tx.commit()?;
            Ok(count)
        }).await?
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
            let mut stmt = c.prepare(
                "SELECT id, library_id, item_type, title, original_title, release_year,
                        duration_seconds, added_at, file_path, file_name, file_size,
                        resolution, video_codec, audio_codec, audio_channels, container, metadata
                 FROM media_items
                 WHERE id = ?1",
            )?;
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
            let mut stmt = c.prepare(
                "SELECT id, library_id, item_type, title, original_title, release_year,
                        duration_seconds, added_at, file_path, file_name, file_size,
                        resolution, video_codec, audio_codec, audio_channels, container, metadata
                 FROM media_items
                 WHERE library_id = ?1
                 ORDER BY title ASC
                 LIMIT ?2 OFFSET ?3",
            )?;

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
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        self.widget_queries()
            .find_top_rated_paginated(limit, offset, unlocked_ids, filters)
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
        limit: u32,
        offset: u32,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Vec<MediaItem>> {
        self.widget_queries()
            .find_by_genre_paginated(genre, limit, offset, unlocked_ids, filters)
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
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<Option<MediaItem>> {
        self.widget_queries()
            .find_spotlight_candidate(unlocked_ids, filters)
            .await
    }

    pub async fn find_spotlight_candidate_default(&self) -> Result<Option<MediaItem>> {
        self.find_spotlight_candidate(&[], None).await
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
}

const SELECT_COLUMNS: &str = "id, library_id, item_type, title, original_title, release_year, \
 duration_seconds, added_at, file_path, file_name, file_size, \
 resolution, video_codec, audio_codec, audio_channels, container, metadata";

fn map_media_item_row(row: &rusqlite::Row) -> rusqlite::Result<MediaItem> {
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
