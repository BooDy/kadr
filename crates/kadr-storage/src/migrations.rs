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
        M::up(include_str!("migrations/007_plugins.sql")),
        M::up(include_str!("migrations/008_settings.sql")),
        M::up(include_str!("migrations/009_playlists.sql")),
        M::up(include_str!("migrations/010_managed_plugins.sql")),
        M::up(include_str!("migrations/011_search_fts.sql")),
        M::up(include_str!("migrations/012_metadata_overrides.sql")),
        M::up(include_str!("migrations/013_audio_tracks.sql")),
        M::up(include_str!("migrations/014_filter_ast_indexes.sql")),
        M::up(include_str!("migrations/015_user_favorites.sql")),
    ])
}

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    Ok(())
}
