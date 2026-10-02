pub mod opensubtitles;
pub mod service;

pub use opensubtitles::{OpenSubtitlesClient, OpenSubtitlesError};
pub use service::{SubtitleDeliveryService, SubtitleServiceError};
