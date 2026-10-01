use std::fs;
use std::path::{Path, PathBuf};

use kadr_core::subtitles::{SubtitleFormat, SubtitleSource, SubtitleTrack};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSubtitle {
    pub source: SubtitleSource,
    pub language: String,
    pub title: Option<String>,
    pub format: SubtitleFormat,
    pub file_path: Option<PathBuf>,
    pub stream_index: Option<u32>,
    pub is_default: bool,
    pub is_forced: bool,
}

impl DiscoveredSubtitle {
    pub fn into_subtitle_track(self, media_item_id: i64, created_at: i64) -> SubtitleTrack {
        SubtitleTrack {
            id: 0,
            media_item_id,
            source: self.source,
            language: self.language,
            title: self.title,
            format: self.format,
            file_path: self.file_path.map(|p| p.to_string_lossy().to_string()),
            stream_index: self.stream_index,
            is_default: self.is_default,
            is_forced: self.is_forced,
            created_at,
        }
    }
}

fn language_display_name(lang: &str) -> &'static str {
    match lang {
        "eng" => "English",
        "ara" => "Arabic",
        "fre" => "French",
        "spa" => "Spanish",
        "ger" => "German",
        "ita" => "Italian",
        "jpn" => "Japanese",
        "kor" => "Korean",
        "chi" => "Chinese",
        "und" => "Undetermined",
        _ => "",
    }
}

fn map_known_language(token: &str) -> Option<&'static str> {
    match token {
        "en" | "eng" | "english" => Some("eng"),
        "ar" | "ara" | "arabic" => Some("ara"),
        "fr" | "fre" | "fra" | "french" => Some("fre"),
        "es" | "spa" | "spanish" => Some("spa"),
        "de" | "ger" | "deu" | "german" => Some("ger"),
        "it" | "ita" | "italian" => Some("ita"),
        "ja" | "jpn" | "japanese" => Some("jpn"),
        "ko" | "kor" | "korean" => Some("kor"),
        "zh" | "zho" | "chi" | "chinese" => Some("chi"),
        _ => None,
    }
}

fn is_non_language_keyword(token: &str) -> bool {
    matches!(
        token,
        "forced"
            | "default"
            | "sdh"
            | "cc"
            | "hi"
            | "sub"
            | "subs"
            | "subtitle"
            | "subtitles"
            | "srt"
            | "vtt"
            | "ass"
    )
}

fn parse_subtitle_filename(
    sub_path: &Path,
    media_stem: &str,
    ext: &str,
) -> Option<DiscoveredSubtitle> {
    let format = SubtitleFormat::from_extension(ext);
    if format == SubtitleFormat::Unknown {
        return None;
    }

    let file_name = sub_path.file_name().and_then(|s| s.to_str())?;
    let stem_len = file_name.len().saturating_sub(ext.len() + 1);
    let stem_str = &file_name[..stem_len];

    // Determine relevant string to tokenize
    let token_source = stem_str.strip_prefix(media_stem).unwrap_or(stem_str);

    let raw_tokens: Vec<&str> = token_source
        .split(|c: char| c == '.' || c == '_' || c == '-' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    let mut is_forced = false;
    let mut is_default = false;
    let mut is_sdh = false;
    let mut clean_tokens = Vec::new();

    for token in raw_tokens {
        let lower = token.to_ascii_lowercase();
        if lower == "forced" {
            is_forced = true;
        } else if lower == "default" {
            is_default = true;
        } else if lower == "sdh" || lower == "cc" {
            is_sdh = true;
        } else {
            clean_tokens.push(lower);
        }
    }

    // Language extraction
    let mut detected_language: Option<String> = None;

    // 1. Check known languages
    for token in &clean_tokens {
        if let Some(canonical) = map_known_language(token) {
            detected_language = Some(canonical.to_string());
            break;
        }
    }

    // 2. Check 2 or 3 letter lowercase alpha token if not recognized
    if detected_language.is_none() {
        for token in &clean_tokens {
            if (token.len() == 2 || token.len() == 3)
                && token.chars().all(|c| c.is_ascii_alphabetic())
                && !is_non_language_keyword(token)
            {
                detected_language = Some(token.clone());
                break;
            }
        }
    }

    let language = detected_language.unwrap_or_else(|| "und".to_string());

    let display_name = match language_display_name(&language) {
        "" => language.clone(),
        name => name.to_string(),
    };

    let title = if is_sdh {
        Some(format!("{} [SDH]", display_name))
    } else {
        Some(display_name)
    };

    Some(DiscoveredSubtitle {
        source: SubtitleSource::Sidecar,
        language,
        title,
        format,
        file_path: Some(sub_path.to_path_buf()),
        stream_index: None,
        is_default,
        is_forced,
    })
}

pub fn find_subtitles_for_media<P: AsRef<Path>>(media_path: P) -> Vec<DiscoveredSubtitle> {
    let p = media_path.as_ref();
    let parent = match p.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };

    let media_stem = match p.file_stem().and_then(|s| s.to_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return Vec::new(),
    };

    // Find other video files in parent to disambiguate subtitles in Subs/ subdirectories and parent dir
    let mut other_video_stems = Vec::new();
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path != p {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ["mkv", "mp4", "webm", "avi"].contains(&ext.to_ascii_lowercase().as_str()) {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            other_video_stems.push(stem.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut discovered = Vec::new();

    // 1. Scan immediate parent directory
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let file_name = match path.file_name().and_then(|s| s.to_str()) {
                        Some(f) => f,
                        None => continue,
                    };
                    // Must match or start with media_stem
                    let starts_with_stem_dot = file_name.starts_with(&format!("{media_stem}."));
                    let equals_stem = match file_name.strip_suffix(&format!(".{ext}")) {
                        Some(stem) => stem == media_stem,
                        None => false,
                    };

                    if starts_with_stem_dot || equals_stem {
                        // If another longer video stem also matches this subtitle, let the more specific video own it
                        let more_specific_match = other_video_stems.iter().any(|other| {
                            other.len() > media_stem.len()
                                && (file_name.starts_with(&format!("{other}."))
                                    || file_name == format!("{other}.{ext}"))
                        });
                        if more_specific_match {
                            continue;
                        }

                        if let Some(sub) = parse_subtitle_filename(&path, media_stem, ext) {
                            discovered.push(sub);
                        }
                    }
                }
            }
        }
    }

    // 2. Scan immediate Subs/ or Subtitles/ directory
    for sub_dir_name in &["Subs", "subs", "Subtitles", "subtitles"] {
        let sub_dir = parent.join(sub_dir_name);
        if sub_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&sub_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                                Some(f) => f,
                                None => continue,
                            };
                            // If it starts with another video stem in the parent directory, ignore
                            let belongs_to_other = other_video_stems.iter().any(|other| {
                                file_name.starts_with(&format!("{other}."))
                                    || file_name == format!("{other}.{ext}")
                            });
                            if belongs_to_other {
                                continue;
                            }

                            if let Some(sub) = parse_subtitle_filename(&path, media_stem, ext) {
                                discovered.push(sub);
                            }
                        }
                    }
                }
            }
        }
    }

    discovered.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    discovered.dedup_by(|a, b| a.file_path == b.file_path);
    discovered
}
