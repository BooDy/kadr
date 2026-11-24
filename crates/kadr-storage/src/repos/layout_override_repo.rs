use std::str::FromStr;

use deadpool_sqlite::Pool;
use kadr_core::ast::{ScreenLayout, TargetDevice};
use rusqlite::params;

use crate::error::{Result, StorageError};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredLayoutOverride {
    pub screen_id: String,
    pub user_id: Option<String>,
    pub target_device: TargetDevice,
    pub layout: ScreenLayout,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct LayoutOverrideRepository {
    pool: Pool,
}

pub type LayoutOverrideRepo = LayoutOverrideRepository;

fn validate_scope_inputs(
    screen_id: &str,
    user_id: Option<&str>,
) -> Result<(String, Option<String>)> {
    let sid = screen_id.trim();
    if sid.is_empty() {
        return Err(StorageError::InvalidInput(
            "screen_id cannot be empty".to_string(),
        ));
    }
    let uid = match user_id {
        None => None,
        Some(u) if u.trim().is_empty() => {
            return Err(StorageError::InvalidInput(
                "user_id cannot be empty; use None for global scope".to_string(),
            ));
        }
        Some(u) => Some(u.to_string()),
    };
    Ok((sid.to_string(), uid))
}

impl LayoutOverrideRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn list_all(&self) -> Result<Vec<StoredLayoutOverride>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut stmt = c.prepare(
                "SELECT screen_id, user_id, target_device, layout_json, updated_at
                 FROM layout_overrides
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?;

            let mut out = Vec::new();
            for row in rows {
                let (screen_id, user_id, device_raw, layout_json, updated_at) = row?;
                let target_device = match TargetDevice::from_str(&device_raw) {
                    Ok(dev) => dev,
                    Err(err) => {
                        eprintln!(
                            "WARN: skipping layout_overrides row with invalid target_device \
                             (screen_id={screen_id}, user_id={user_id:?}): {err}"
                        );
                        continue;
                    }
                };
                let mut layout: ScreenLayout = match serde_json::from_str(&layout_json) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        eprintln!(
                            "WARN: skipping corrupt layout_overrides JSON \
                             (screen_id={screen_id}, user_id={user_id:?}, target_device={target_device}): {err}"
                        );
                        continue;
                    }
                };
                layout.strip_runtime_fields();
                out.push(StoredLayoutOverride {
                    screen_id,
                    user_id,
                    target_device,
                    layout,
                    updated_at,
                });
            }
            Ok(out)
        })
        .await?
    }

    pub async fn get(
        &self,
        screen_id: &str,
        user_id: Option<&str>,
        target_device: TargetDevice,
    ) -> Result<Option<StoredLayoutOverride>> {
        let (sid, uid) = validate_scope_inputs(screen_id, user_id)?;
        let device_str = target_device.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT screen_id, user_id, target_device, layout_json, updated_at
                 FROM layout_overrides
                 WHERE screen_id = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND target_device = ?3",
            )?;
            let mut rows = stmt.query(params![sid, uid, device_str])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let screen_id: String = row.get(0)?;
            let user_id: Option<String> = row.get(1)?;
            let device_raw: String = row.get(2)?;
            let layout_json: String = row.get(3)?;
            let updated_at: i64 = row.get(4)?;

            let parsed_device =
                TargetDevice::from_str(&device_raw).map_err(StorageError::InvalidInput)?;
            let mut layout: ScreenLayout = serde_json::from_str(&layout_json)?;
            layout.strip_runtime_fields();
            Ok(Some(StoredLayoutOverride {
                screen_id,
                user_id,
                target_device: parsed_device,
                layout,
                updated_at,
            }))
        })
        .await?
    }

    pub async fn upsert(
        &self,
        screen_id: &str,
        user_id: Option<&str>,
        target_device: TargetDevice,
        layout: &ScreenLayout,
        now_epoch: i64,
    ) -> Result<()> {
        let (sid, uid) = validate_scope_inputs(screen_id, user_id)?;
        let mut clean = layout.clone();
        clean.strip_runtime_fields();
        let layout_json = serde_json::to_string(&clean)?;
        let device_str = target_device.to_string();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO layout_overrides (screen_id, user_id, target_device, layout_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(screen_id, IFNULL(user_id, ''), target_device)
                 DO UPDATE SET layout_json = excluded.layout_json, updated_at = excluded.updated_at",
                params![sid, uid, device_str, layout_json, now_epoch],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn delete(
        &self,
        screen_id: &str,
        user_id: Option<&str>,
        target_device: TargetDevice,
    ) -> Result<bool> {
        let (sid, uid) = validate_scope_inputs(screen_id, user_id)?;
        let device_str = target_device.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let deleted = c.execute(
                "DELETE FROM layout_overrides
                 WHERE screen_id = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND target_device = ?3",
                params![sid, uid, device_str],
            )?;
            Ok(deleted > 0)
        })
        .await?
    }

    pub async fn delete_all_for_screen(&self, screen_id: &str) -> Result<u64> {
        let sid = screen_id.trim();
        if sid.is_empty() {
            return Err(StorageError::InvalidInput(
                "screen_id cannot be empty".to_string(),
            ));
        }
        let sid = sid.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let deleted = c.execute(
                "DELETE FROM layout_overrides WHERE screen_id = ?1",
                params![sid],
            )?;
            Ok(deleted as u64)
        })
        .await?
    }
}
