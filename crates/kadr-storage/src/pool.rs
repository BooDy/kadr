use crate::error::Result;
use crate::migrations::run_migrations;
use deadpool_sqlite::{Config, Hook, HookError, Pool, Runtime};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::OnceCell;

fn apply_connection_pragmas(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA cache_size = -8000;
         PRAGMA temp_store = MEMORY;",
    )
}

fn ensure_wal_mode(conn: &mut Connection) -> rusqlite::Result<()> {
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    }
    Ok(())
}

pub fn create_pool<P: AsRef<Path>>(path: P, max_size: usize) -> Result<Pool> {
    let path_ref = path.as_ref();
    let is_in_memory = path_ref == Path::new(":memory:");
    let wal_initialized = Arc::new(OnceCell::new());

    let cfg = Config::new(path_ref);
    let pool = cfg
        .builder(Runtime::Tokio1)?
        .max_size(max_size)
        .post_create(Hook::async_fn(move |conn, _| {
            let wal_initialized = Arc::clone(&wal_initialized);
            Box::pin(async move {
                conn.interact(apply_connection_pragmas)
                    .await
                    .map_err(|e| HookError::message(e.to_string()))?
                    .map_err(HookError::Backend)?;

                if !is_in_memory && !wal_initialized.initialized() {
                    wal_initialized
                        .get_or_try_init(|| async {
                            conn.interact(ensure_wal_mode)
                                .await
                                .map_err(|e| HookError::message(e.to_string()))?
                                .map_err(HookError::Backend)?;
                            Ok::<(), HookError>(())
                        })
                        .await?;
                }

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
