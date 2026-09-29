pub mod error;
pub mod migrations;
pub mod pool;

pub use error::{Result, StorageError};
pub use pool::{create_in_memory_pool, create_pool, initialize_database};
pub type StoragePool = deadpool_sqlite::Pool;
