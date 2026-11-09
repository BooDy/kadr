use crate::error::Result;
use deadpool_sqlite::Pool;
use rusqlite::params;
use std::collections::HashMap;

/// Small key-value store for settings changed from the web UI.
#[derive(Clone)]
pub struct SettingsRepository {
    pool: Pool,
}

impl SettingsRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT value FROM settings WHERE key = ?1")?;
            let mut rows = stmt.query(params![key])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get::<_, String>(0)?)),
                None => Ok(None),
            }
        })
        .await?
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        self.set_many(&[(key.to_string(), value.to_string())]).await
    }

    /// Upserts all entries in a single transaction (all or nothing).
    pub async fn set_many(&self, entries: &[(String, String)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let entries = entries.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                )?;
                for (key, value) in &entries {
                    stmt.execute(params![key, value])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?
    }

    pub async fn get_all(&self) -> Result<HashMap<String, String>> {
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT key, value FROM settings")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out = HashMap::new();
            for r in rows {
                let (k, v) = r?;
                out.insert(k, v);
            }
            Ok(out)
        })
        .await?
    }
}
