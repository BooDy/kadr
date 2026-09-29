use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid media item: {0}")]
    ValidationError(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
