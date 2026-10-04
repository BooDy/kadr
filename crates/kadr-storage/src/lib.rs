pub mod error;
pub mod migrations;
pub mod pool;
pub mod repos;

pub use error::{Result, StorageError};
pub use migrations::run_migrations;
pub use pool::{create_in_memory_pool, create_pool, initialize_database};
pub use repos::{
    LibraryRepo, LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
pub type StoragePool = deadpool_sqlite::Pool;
