// crates/kadr-core/src/subtitles.rs
use serde::{Deserialize, Serialize};

pub mod transcoder;
pub use transcoder::srt_to_webvtt;

/// Represents where the subtitle originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleSource {
    /// Found alongside media on disk (.srt, .vtt, .ass).
    Sidecar,
    /// Found inside media container (MKV/MP4 embedded stream).
    Embedded,
    /// Downloaded from online providers (OpenSubtitles).
    Downloaded,
}

impl SubtitleSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sidecar => "sidecar",
            Self::Embedded => "embedded",
            Self::Downloaded => "downloaded",
        }
    }
}

impl std::fmt::Display for SubtitleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SubtitleSource {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sidecar" => Ok(Self::Sidecar),
            "embedded" => Ok(Self::Embedded),
            "downloaded" => Ok(Self::Downloaded),
            _ => Err(()),
        }
    }
}

/// Supported subtitle formats in Kadr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFormat {
    Srt,
    Vtt,
    Ass,
    Sub,
    Unknown,
}

impl SubtitleFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().trim_start_matches('.') {
            "srt" => Self::Srt,
            "vtt" => Self::Vtt,
            "ass" => Self::Ass,
            "sub" => Self::Sub,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ass => "ass",
            Self::Sub => "sub",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for SubtitleFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SubtitleFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "srt" => Ok(Self::Srt),
            "vtt" => Ok(Self::Vtt),
            "ass" => Ok(Self::Ass),
            "sub" => Ok(Self::Sub),
            "unknown" => Ok(Self::Unknown),
            _ => Ok(Self::Unknown),
        }
    }
}

/// Metadata and location of a cataloged subtitle track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub id: i64,
    pub media_item_id: i64,
    pub source: SubtitleSource,
    pub language: String,              // e.g. "eng", "ara", "fre", "und"
    pub title: Option<String>,          // e.g. "English [SDH]"
    pub format: SubtitleFormat,
    pub file_path: Option<String>,      // Disk path
    pub stream_index: Option<u32>,      // Stream index if embedded
    pub is_default: bool,
    pub is_forced: bool,
    pub created_at: i64,
}

/// Match candidate returned from OpenSubtitles REST search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineSubtitleMatch {
    pub id: String,                    // OpenSubtitles file ID
    pub language: String,              // e.g. "en", "ar"
    pub release_name: Option<String>,  // Release name/hash sync
    pub hearing_impaired: bool,
    pub format: String,                // "srt"
    pub download_count: u32,
    pub rating: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_format_helpers() {
        assert_eq!(SubtitleFormat::from_extension("srt"), SubtitleFormat::Srt);
        assert_eq!(SubtitleFormat::from_extension(".vtt"), SubtitleFormat::Vtt);
        assert_eq!(SubtitleFormat::from_extension("ASS"), SubtitleFormat::Ass);
        assert_eq!(SubtitleFormat::from_extension("sub"), SubtitleFormat::Sub);
        assert_eq!(SubtitleFormat::from_extension("mkv"), SubtitleFormat::Unknown);

        assert_eq!(SubtitleFormat::Srt.as_str(), "srt");
        assert_eq!(SubtitleFormat::Srt.to_string(), "srt");
        assert_eq!(SubtitleFormat::from_str("srt").unwrap(), SubtitleFormat::Srt);
    }

    #[test]
    fn test_source_helpers() {
        assert_eq!(SubtitleSource::Sidecar.as_str(), "sidecar");
        assert_eq!(SubtitleSource::Embedded.as_str(), "embedded");
        assert_eq!(SubtitleSource::Downloaded.as_str(), "downloaded");
        assert_eq!(SubtitleSource::Sidecar.to_string(), "sidecar");

        assert_eq!(SubtitleSource::from_str("sidecar").unwrap(), SubtitleSource::Sidecar);
        assert_eq!(SubtitleSource::from_str("embedded").unwrap(), SubtitleSource::Embedded);
        assert_eq!(SubtitleSource::from_str("downloaded").unwrap(), SubtitleSource::Downloaded);
        assert!(SubtitleSource::from_str("invalid").is_err());
    }
}
