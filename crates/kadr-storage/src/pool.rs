use crate::error::Result;
use crate::migrations::run_migrations;
use deadpool_sqlite::{Config, Hook, HookError, Pool, Runtime};
use rusqlite::Connection;
use std::path::Path;

fn apply_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;
         PRAGMA cache_size = -8000;
         PRAGMA temp_store = MEMORY;",
    )
}

pub fn create_pool<P: AsRef<Path>>(path: P, max_size: usize) -> Result<Pool> {
    let cfg = Config::new(path.as_ref());
    let pool = cfg
        .builder(Runtime::Tokio1)?
        .max_size(max_size)
        .post_create(Hook::async_fn(|conn, _| {
            Box::pin(async move {
                conn.interact(|c| apply_pragmas(c))
                    .await
                    .map_err(|e| HookError::message(e.to_string()))?
                    .map_err(HookError::Backend)?;
                Ok(())
            })
        }))
        .build()?;
    Ok(pool)
}

pub fn create_in_memory_pool() -> Result<Pool> {
    create_pool(":memory:", 1)
}

pub async fn initialize_database(pool: &Pool) -> Result<()> {
    let conn = pool.get().await?;
    conn.interact(run_migrations).await??;
    Ok(())
}
