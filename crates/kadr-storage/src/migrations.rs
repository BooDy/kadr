use crate::error::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/001_initial_schema.sql")),
        M::up(include_str!("migrations/002_playback_and_auth.sql")),
        M::up(include_str!("migrations/003_widget_query_indexes.sql")),
        M::up(include_str!("migrations/004_subtitles.sql")),
        M::up(include_str!("migrations/005_private_libraries.sql")),
        M::up(include_str!("migrations/006_multiple_library_paths.sql")),
    ])
}

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    Ok(())
}
