use crate::error::Result;
use deadpool_sqlite::Pool;
use kadr_core::models::{Library, MediaType};
use rusqlite::params;
use std::path::PathBuf;

#[derive(Clone)]
pub struct LibraryRepository {
    pool: Pool,
}

pub type LibraryRepo = LibraryRepository;

impl LibraryRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        id: &str,
        name: &str,
        path: impl AsRef<std::path::Path>,
        media_type: MediaType,
        is_private: bool,
        pin_hash: Option<&str>,
    ) -> Result<Library> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let lib = Library {
            id: id.to_string(),
            name: name.to_string(),
            path: path.as_ref().to_path_buf(),
            media_type,
            is_private,
            pin_hash: pin_hash.map(|s| s.to_string()),
            created_at: now,
        };
        self.insert(&lib).await?;
        Ok(lib)
    }

    pub async fn insert(&self, lib: &Library) -> Result<()> {
        let lib = lib.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO libraries (id, name, path, media_type, is_private, pin_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    path = excluded.path,
                    media_type = excluded.media_type,
                    is_private = excluded.is_private,
                    pin_hash = excluded.pin_hash",
                params![
                    lib.id,
                    lib.name,
                    lib.path.to_string_lossy().into_owned(),
                    serde_json::to_string(&lib.media_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                    if lib.is_private { 1 } else { 0 },
                    lib.pin_hash,
                    lib.created_at,
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn create_library(&self, lib: &Library) -> Result<()> {
        self.insert(lib).await
    }

    pub async fn update_privacy(
        &self,
        id: &str,
        is_private: bool,
        pin_hash: Option<&str>,
    ) -> Result<()> {
        let id = id.to_string();
        let pin_hash = pin_hash.map(|s| s.to_string());
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "UPDATE libraries SET is_private = ?1, pin_hash = ?2 WHERE id = ?3",
                params![if is_private { 1 } else { 0 }, pin_hash, id],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get_all(&self) -> Result<Vec<Library>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut stmt = c.prepare(
                "SELECT id, name, path, media_type, is_private, pin_hash, created_at FROM libraries ORDER BY name ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path: String = row.get(2)?;
                let media_type_str: String = row.get(3)?;
                let is_private_int: i32 = row.get(4)?;
                let pin_hash: Option<String> = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let media_type: MediaType =
                    serde_json::from_str(&format!("\"{}\"", media_type_str))
                        .unwrap_or(MediaType::Unknown);

                Ok(Library {
                    id,
                    name,
                    path: PathBuf::from(path),
                    media_type,
                    is_private: is_private_int != 0,
                    pin_hash,
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
                "SELECT id, name, path, media_type, is_private, pin_hash, created_at FROM libraries WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path: String = row.get(2)?;
                let media_type_str: String = row.get(3)?;
                let is_private_int: i32 = row.get(4)?;
                let pin_hash: Option<String> = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let media_type: MediaType =
                    serde_json::from_str(&format!("\"{}\"", media_type_str))
                        .unwrap_or(MediaType::Unknown);

                Ok(Some(Library {
                    id,
                    name,
                    path: PathBuf::from(path),
                    media_type,
                    is_private: is_private_int != 0,
                    pin_hash,
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
