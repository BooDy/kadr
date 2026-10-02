use crate::error::Result;
use deadpool_sqlite::Pool;
use kadr_core::models::{Library, MediaType};
use rusqlite::params;
use std::path::PathBuf;

#[derive(Clone)]
pub struct LibraryRepository {
    pool: Pool,
}

impl LibraryRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, lib: &Library) -> Result<()> {
        let lib = lib.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO libraries (id, name, path, media_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    path = excluded.path,
                    media_type = excluded.media_type",
                params![
                    lib.id,
                    lib.name,
                    lib.path.to_string_lossy().into_owned(),
                    serde_json::to_string(&lib.media_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                    lib.created_at,
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_all(&self) -> Result<Vec<Library>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut stmt = c.prepare(
                "SELECT id, name, path, media_type, created_at FROM libraries ORDER BY name ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path: String = row.get(2)?;
                let media_type_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let media_type: MediaType =
                    serde_json::from_str(&format!("\"{}\"", media_type_str))
                        .unwrap_or(MediaType::Unknown);

                Ok(Library {
                    id,
                    name,
                    path: PathBuf::from(path),
                    media_type,
                    created_at,
                })
            })?;

            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            Ok(result)
        })
        .await?
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<Library>> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, name, path, media_type, created_at FROM libraries WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path: String = row.get(2)?;
                let media_type_str: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let media_type: MediaType =
                    serde_json::from_str(&format!("\"{}\"", media_type_str))
                        .unwrap_or(MediaType::Unknown);

                Ok(Some(Library {
                    id,
                    name,
                    path: PathBuf::from(path),
                    media_type,
                    created_at,
                }))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute("DELETE FROM libraries WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        })
        .await?
    }
}
