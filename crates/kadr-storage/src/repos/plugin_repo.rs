use crate::error::Result;
use deadpool_sqlite::Pool;
use kadr_core::plugin::{PluginManifest, PluginPermission, PluginRecord, PluginStatus};
use rusqlite::params;

#[derive(Clone)]
pub struct PluginRepository {
    pool: Pool,
}

fn conversion_err<E: std::error::Error + Send + Sync + 'static>(
    idx: usize,
    e: E,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<PluginRecord> {
    let manifest_s: String = row.get(3)?;
    let perms_s: String = row.get(4)?;
    let status_s: String = row.get(5)?;
    let manifest: PluginManifest =
        serde_json::from_str(&manifest_s).map_err(|e| conversion_err(3, e))?;
    let granted_permissions: Vec<PluginPermission> =
        serde_json::from_str(&perms_s).map_err(|e| conversion_err(4, e))?;
    let status = PluginStatus::parse(&status_s).unwrap_or(PluginStatus::Disabled);
    Ok(PluginRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        manifest,
        granted_permissions,
        status,
        created_at: row.get(6)?,
    })
}

const COLUMNS: &str = "id, name, base_url, manifest, granted_permissions, status, created_at";

impl PluginRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, rec: &PluginRecord) -> Result<()> {
        let rec = rec.clone();
        let manifest = serde_json::to_string(&rec.manifest)?;
        let perms = serde_json::to_string(&rec.granted_permissions)?;
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO plugins (id, name, base_url, manifest, granted_permissions, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    rec.id,
                    rec.name,
                    rec.base_url,
                    manifest,
                    perms,
                    rec.status.as_str(),
                    rec.created_at
                ],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn get(&self, id: &str) -> Result<Option<PluginRecord>> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!("SELECT {COLUMNS} FROM plugins WHERE id = ?1");
            let mut stmt = c.prepare(&sql)?;
            let mut rows = stmt.query(params![id])?;
            match rows.next()? {
                Some(row) => Ok(Some(map_row(row)?)),
                None => Ok(None),
            }
        })
        .await?
    }

    pub async fn list(&self) -> Result<Vec<PluginRecord>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let sql = format!("SELECT {COLUMNS} FROM plugins ORDER BY id ASC");
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map([], map_row)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .await?
    }

    pub async fn set_status(&self, id: &str, status: PluginStatus) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute(
                "UPDATE plugins SET status = ?1 WHERE id = ?2",
                params![status.as_str(), id],
            )?;
            Ok(n > 0)
        })
        .await?
    }

    pub async fn set_granted(&self, id: &str, granted: &[PluginPermission]) -> Result<bool> {
        let id = id.to_string();
        let perms = serde_json::to_string(granted)?;
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute(
                "UPDATE plugins SET granted_permissions = ?1 WHERE id = ?2",
                params![perms, id],
            )?;
            Ok(n > 0)
        })
        .await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let n = c.execute("DELETE FROM plugins WHERE id = ?1", params![id])?;
            Ok(n > 0)
        })
        .await?
    }
}
