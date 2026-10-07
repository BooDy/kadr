use crate::error::{Result, StorageError};
use deadpool_sqlite::Pool;
use kadr_core::models::{Library, MediaType};
use rusqlite::params;
use std::collections::HashMap;
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

    pub async fn create_with_paths(
        &self,
        id: &str,
        name: &str,
        paths: &[PathBuf],
        media_type: MediaType,
        is_private: bool,
        pin_hash: Option<&str>,
    ) -> Result<Library> {
        if paths.is_empty() {
            return Err(StorageError::InvalidInput(
                "A library must have at least one path".to_string(),
            ));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let lib = Library {
            id: id.to_string(),
            name: name.to_string(),
            path: paths[0].clone(),
            paths: paths.to_vec(),
            media_type,
            is_private,
            pin_hash: pin_hash.map(|s| s.to_string()),
            created_at: now,
        };
        self.insert(&lib).await?;
        Ok(lib)
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
        self.create_with_paths(
            id,
            name,
            &[path.as_ref().to_path_buf()],
            media_type,
            is_private,
            pin_hash,
        )
        .await
    }

    pub async fn insert(&self, lib: &Library) -> Result<()> {
        let lib = lib.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let primary_path = if !lib.path.as_os_str().is_empty() {
                lib.path.to_string_lossy().into_owned()
            } else if let Some(first) = lib.paths.first() {
                first.to_string_lossy().into_owned()
            } else {
                String::new()
            };

            tx.execute(
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
                    primary_path,
                    serde_json::to_string(&lib.media_type)
                        .unwrap_or_default()
                        .trim_matches('"'),
                    if lib.is_private { 1 } else { 0 },
                    lib.pin_hash,
                    lib.created_at,
                ],
            )?;

            let paths_to_insert = if !lib.paths.is_empty() {
                lib.paths.clone()
            } else if !lib.path.as_os_str().is_empty() {
                vec![lib.path.clone()]
            } else {
                Vec::new()
            };

            for p in paths_to_insert {
                let p_str = p.to_string_lossy().into_owned();
                tx.execute(
                    "INSERT OR IGNORE INTO library_paths (library_id, path, created_at)
                     VALUES (?1, ?2, ?3)",
                    params![lib.id, p_str, lib.created_at],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn create_library(&self, lib: &Library) -> Result<()> {
        self.insert(lib).await
    }

    pub async fn add_path(
        &self,
        library_id: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<()> {
        let library_id = library_id.to_string();
        let path_str = path.as_ref().to_string_lossy().into_owned();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let lib_exists: bool = c.query_row(
                "SELECT EXISTS(SELECT 1 FROM libraries WHERE id = ?1)",
                params![library_id],
                |row| row.get(0),
            )?;
            if !lib_exists {
                return Err(StorageError::NotFound(format!("Library {library_id} not found")));
            }
            c.execute(
                "INSERT OR IGNORE INTO library_paths (library_id, path, created_at) VALUES (?1, ?2, ?3)",
                params![library_id, path_str, now],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn remove_path(
        &self,
        library_id: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<()> {
        let library_id = library_id.to_string();
        let path_str = path.as_ref().to_string_lossy().into_owned();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;

            let lib_exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM libraries WHERE id = ?1)",
                params![library_id],
                |row| row.get(0),
            )?;
            if !lib_exists {
                return Err(StorageError::NotFound(format!("Library {library_id} not found")));
            }

            let path_exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM library_paths WHERE library_id = ?1 AND path = ?2)",
                params![library_id, path_str],
                |row| row.get(0),
            )?;

            if !path_exists {
                return Ok(());
            }

            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM library_paths WHERE library_id = ?1",
                params![library_id],
                |row| row.get(0),
            )?;

            if count <= 1 {
                return Err(StorageError::InvalidInput(
                    "Cannot remove the last remaining path from a library".to_string(),
                ));
            }

            tx.execute(
                "DELETE FROM library_paths WHERE library_id = ?1 AND path = ?2",
                params![library_id, path_str],
            )?;

            let current_primary: String = tx.query_row(
                "SELECT path FROM libraries WHERE id = ?1",
                params![library_id],
                |row| row.get(0),
            )?;

            if current_primary == path_str {
                let new_primary: String = tx.query_row(
                    "SELECT path FROM library_paths WHERE library_id = ?1 ORDER BY id ASC LIMIT 1",
                    params![library_id],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "UPDATE libraries SET path = ?1 WHERE id = ?2",
                    params![new_primary, library_id],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await?
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

    pub async fn update_name(&self, id: &str, name: &str) -> Result<Library> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(StorageError::InvalidInput(
                "Library name cannot be empty".to_string(),
            ));
        }

        let lib_id = id.to_string();
        let trimmed_name = trimmed.to_string();
        let rows_affected = {
            let conn = self.pool.get().await?;
            conn.interact(move |c| -> Result<usize> {
                let affected = c.execute(
                    "UPDATE libraries SET name = ?1 WHERE id = ?2",
                    params![trimmed_name, lib_id],
                )?;
                Ok(affected)
            })
            .await??
        };

        if rows_affected == 0 {
            return Err(StorageError::NotFound(format!("Library {id} not found")));
        }

        self.get_by_id(id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("Library {id} not found")))
    }

    pub async fn get_all(&self) -> Result<Vec<Library>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut path_stmt = c.prepare(
                "SELECT library_id, path FROM library_paths ORDER BY id ASC",
            )?;
            let mut path_map: HashMap<String, Vec<PathBuf>> = HashMap::new();
            let path_rows = path_stmt.query_map([], |row| {
                let lib_id: String = row.get(0)?;
                let p: String = row.get(1)?;
                Ok((lib_id, PathBuf::from(p)))
            })?;
            for item in path_rows {
                let (lib_id, path) = item?;
                path_map.entry(lib_id).or_default().push(path);
            }

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

                let primary_path = PathBuf::from(path);
                let paths = match path_map.remove(&id) {
                    Some(ps) if !ps.is_empty() => ps,
                    _ => {
                        if !primary_path.as_os_str().is_empty() {
                            vec![primary_path.clone()]
                        } else {
                            Vec::new()
                        }
                    }
                };

                Ok(Library {
                    id,
                    name,
                    path: primary_path,
                    paths,
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

                let primary_path = PathBuf::from(path);

                let mut path_stmt = c.prepare(
                    "SELECT path FROM library_paths WHERE library_id = ?1 ORDER BY id ASC",
                )?;
                let path_rows = path_stmt.query_map(params![id], |r| {
                    let p: String = r.get(0)?;
                    Ok(PathBuf::from(p))
                })?;
                let mut paths = Vec::new();
                for p in path_rows {
                    paths.push(p?);
                }
                if paths.is_empty() && !primary_path.as_os_str().is_empty() {
                    paths.push(primary_path.clone());
                }

                Ok(Some(Library {
                    id,
                    name,
                    path: primary_path,
                    paths,
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
            let tx = c.transaction()?;
            tx.execute("DELETE FROM library_paths WHERE library_id = ?1", params![id])?;
            let rows = tx.execute("DELETE FROM libraries WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(rows > 0)
        })
        .await?
    }
}
