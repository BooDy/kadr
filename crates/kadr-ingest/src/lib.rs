pub mod error;
pub mod parser;
pub mod probe;
pub mod sidecars;
pub mod thumbnail;
pub mod watcher;

pub use error::{IngestError, Result};
pub use sidecars::{find_subtitles_for_media, DiscoveredSubtitle};
pub use thumbnail::ThumbnailExtractor;
