use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] deadpool_sqlite::PoolError),
    #[error("Build error: {0}")]
    Build(#[from] deadpool_sqlite::BuildError),
    #[error("Interact error: {0}")]
    Interact(#[from] deadpool_sqlite::InteractError),
    #[error("Migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("Entity not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<deadpool_sqlite::ConfigError> for StorageError {
    fn from(e: deadpool_sqlite::ConfigError) -> Self {
        match e {}
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
