use std::str::FromStr;

use deadpool_sqlite::Pool;
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource, SubtitleTrack};
use rusqlite::params;

use crate::error::Result;

#[derive(Clone)]
pub struct SubtitleRepository {
    pool: Pool,
}

fn map_subtitle_row(row: &rusqlite::Row) -> rusqlite::Result<SubtitleTrack> {
    let id: i64 = row.get(0)?;
    let media_item_id: i64 = row.get(1)?;
    let source_str: String = row.get(2)?;
    let language: String = row.get(3)?;
    let title: Option<String> = row.get(4)?;
    let format_str: String = row.get(5)?;
    let file_path: Option<String> = row.get(6)?;
    let stream_index: Option<i64> = row.get(7)?;
    let is_default: i64 = row.get(8)?;
    let is_forced: i64 = row.get(9)?;
    let created_at: i64 = row.get(10)?;

    let source = SubtitleSource::from_str(&source_str).unwrap_or(SubtitleSource::Sidecar);
    let format = SubtitleFormat::from_str(&format_str).unwrap_or(SubtitleFormat::Unknown);

    Ok(SubtitleTrack {
        id,
        media_item_id,
        source,
        language,
        title,
        format,
        file_path,
        stream_index: stream_index.and_then(|i| u32::try_from(i).ok()),
        is_default: is_default != 0,
        is_forced: is_forced != 0,
        created_at,
    })
}

impl SubtitleRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, track: &SubtitleTrack) -> Result<i64> {
        let track = track.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO media_subtitles (
                    media_item_id, source, language, title, format, file_path,
                    stream_index, is_default, is_forced, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    track.media_item_id,
                    track.source.as_str(),
                    track.language,
                    track.title,
                    track.format.as_str(),
                    track.file_path,
                    track.stream_index.map(|idx| idx as i64),
                    if track.is_default { 1 } else { 0 },
                    if track.is_forced { 1 } else { 0 },
                    track.created_at,
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?
    }

    pub async fn batch_insert(&self, tracks: &[SubtitleTrack]) -> Result<()> {
        if tracks.is_empty() {
            return Ok(());
        }
        let tracks = tracks.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO media_subtitles (
                        media_item_id, source, language, title, format, file_path,
                        stream_index, is_default, is_forced, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                for track in &tracks {
                    stmt.execute(params![
                        track.media_item_id,
                        track.source.as_str(),
                        track.language,
                        track.title,
                        track.format.as_str(),
                        track.file_path,
                        track.stream_index.map(|idx| idx as i64),
                        if track.is_default { 1 } else { 0 },
                        if track.is_forced { 1 } else { 0 },
                        track.created_at,
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<SubtitleTrack>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, media_item_id, source, language, title, format, file_path,
                        stream_index, is_default, is_forced, created_at
                 FROM media_subtitles
                 WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_subtitle_row(row)?))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn find_by_media_item(
        &self,
        media_item_id: i64,
    ) -> Result<Vec<SubtitleTrack>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, media_item_id, source, language, title, format, file_path,
                        stream_index, is_default, is_forced, created_at
                 FROM media_subtitles
                 WHERE media_item_id = ?1
                 ORDER BY is_default DESC, is_forced DESC, language ASC, id ASC",
            )?;
            let rows = stmt.query_map(params![media_item_id], map_subtitle_row)?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        })
        .await?
    }

    pub async fn delete(&self, id: i64) -> Result<bool> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute("DELETE FROM media_subtitles WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        })
        .await?
    }

    pub async fn set_default(&self, id: i64, media_item_id: i64) -> Result<()> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "UPDATE media_subtitles SET is_default = 0 WHERE media_item_id = ?1",
                params![media_item_id],
            )?;
            tx.execute(
                "UPDATE media_subtitles SET is_default = 1 WHERE id = ?1 AND media_item_id = ?2",
                params![id, media_item_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?
    }
}
