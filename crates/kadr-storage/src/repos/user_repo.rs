use deadpool_sqlite::Pool;
use rusqlite::params;
use kadr_core::models::{User, UserRole};
use crate::error::Result;

#[derive(Clone)]
pub struct UserRepository {
    pool: Pool,
}

fn map_user_row(row: &rusqlite::Row) -> rusqlite::Result<User> {
    let id: String = row.get(0)?;
    let username: String = row.get(1)?;
    let pin_hash: String = row.get(2)?;
    let role_str: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let role = match role_str.as_str() {
        "admin" => UserRole::Admin,
        _ => UserRole::Standard,
    };
    Ok(User {
        id,
        username,
        pin_hash,
        role,
        created_at,
    })
}

impl UserRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: &User) -> Result<()> {
        let u = user.clone();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let role_str = match u.role {
                UserRole::Admin => "admin",
                UserRole::Standard => "standard",
            };
            c.execute(
                "INSERT INTO users (id, username, pin_hash, role, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    username = excluded.username,
                    pin_hash = excluded.pin_hash,
                    role = excluded.role",
                params![u.id, u.username, u.pin_hash, role_str, u.created_at],
            )?;
            Ok(())
        }).await?
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<User>> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT id, username, pin_hash, role, created_at FROM users WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_user_row(row)?))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        let username = username.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare("SELECT id, username, pin_hash, role, created_at FROM users WHERE username = ?1")?;
            let mut rows = stmt.query(params![username])?;
            if let Some(row) = rows.next()? {
                Ok(Some(map_user_row(row)?))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn list_all(&self) -> Result<Vec<User>> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let mut stmt = c.prepare("SELECT id, username, pin_hash, role, created_at FROM users ORDER BY username ASC")?;
            let rows = stmt.query_map([], map_user_row)?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        }).await?
    }

    pub async fn count(&self) -> Result<usize> {
        let conn = self.pool.get().await?;
        conn.interact(|c| {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
            Ok(count as usize)
        }).await?
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let rows = c.execute("DELETE FROM users WHERE id = ?1", params![id])?;
            Ok(rows > 0)
        }).await?
    }
}
