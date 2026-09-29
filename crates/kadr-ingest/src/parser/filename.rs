use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFilename {
    pub title: String,
    pub year: Option<i32>,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub source: Option<String>,
    pub release_group: Option<String>,
    pub container: String,
}

pub struct FilenameParser {
    scene_regex: Regex,
    paren_year_regex: Regex,
    res_regex: Regex,
    source_regex: Regex,
    codec_regex: Regex,
}

impl Default for FilenameParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FilenameParser {
    pub fn new() -> Self {
        // Match: Title.Year.Quality.Source.Codec-Group.ext
        let scene_regex = Regex::new(
            r"(?i)^(?P<title>.+?)[._ ](?P<year>(?:19|20)\d{2})(?:[._ ](?P<rest>.*))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        // Match: Title (Year) [Tags].ext or Title.(Year).Tags.ext
        let paren_year_regex = Regex::new(
            r"(?i)^(?P<title>.+?)[._\s]*\((?P<year>(?:19|20)\d{2})\)(?:[._\s]*(?P<rest>.*?))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        let res_regex = Regex::new(r"(?i)\b(2160p|4k|1080p|1080i|720p|576p|480p)\b").unwrap();
        let source_regex = Regex::new(r"(?i)\b(bluray|blu-ray|web-dl|webdl|web-rip|webrip|hdtv|bdrip|dvdrip)\b").unwrap();
        let codec_regex = Regex::new(r"(?i)\b(x264|x265|hevc|av1|h\.?264|h\.?265)\b").unwrap();

        Self {
            scene_regex,
            paren_year_regex,
            res_regex,
            source_regex,
            codec_regex,
        }
    }

    pub fn parse(&self, filename: &str) -> Option<ParsedFilename> {
        let ext = filename.split('.').next_back()?.to_lowercase();
        if !["mkv", "mp4", "webm", "avi"].contains(&ext.as_str()) {
            return None;
        }

        if let Some(caps) = self.scene_regex.captures(filename) {
            let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
            let clean_title = raw_title.replace(['.', '_'], " ").trim().to_string();
            if clean_title.is_empty() {
                return None;
            }
            let year = caps.name("year").and_then(|m| m.as_str().parse::<i32>().ok());
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
            let clean_rest = rest.replace('_', " ");

            let resolution = self.res_regex.find(&clean_rest).map(|m| m.as_str().to_lowercase());
            let source = self.extract_source(&clean_rest);
            let video_codec = self.codec_regex.find(&clean_rest).map(|m| m.as_str().to_lowercase());
            let release_group = Self::extract_release_group(rest);

            return Some(ParsedFilename {
                title: clean_title,
                year,
                resolution,
                video_codec,
                source,
                release_group,
                container: ext,
            });
        }

        if let Some(caps) = self.paren_year_regex.captures(filename) {
            let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
            let clean_title = raw_title.replace(['.', '_'], " ").trim().to_string();
            if clean_title.is_empty() {
                return None;
            }
            let year = caps.name("year").and_then(|m| m.as_str().parse::<i32>().ok());
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
            let clean_rest = rest.replace('_', " ");

            let resolution = self.res_regex.find(&clean_rest).map(|m| m.as_str().to_lowercase());
            let source = self.extract_source(&clean_rest);
            let video_codec = self.codec_regex.find(&clean_rest).map(|m| m.as_str().to_lowercase());
            let release_group = Self::extract_release_group(rest);

            return Some(ParsedFilename {
                title: clean_title,
                year,
                resolution,
                video_codec,
                source,
                release_group,
                container: ext,
            });
        }

        // Fallback: Strip extension and replace dots/underscores
        let stem = match filename.rfind('.') {
            Some(idx) => &filename[..idx],
            None => filename,
        };
        let clean_title = stem.replace(['.', '_'], " ").trim().to_string();
        if clean_title.is_empty() {
            return None;
        }

        Some(ParsedFilename {
            title: clean_title,
            year: None,
            resolution: None,
            video_codec: None,
            source: None,
            release_group: None,
            container: ext,
        })
    }

    fn extract_source(&self, rest: &str) -> Option<String> {
        self.source_regex.find(rest).map(|m| {
            let s = m.as_str().to_lowercase();
            match s.as_str() {
                "blu-ray" => "bluray".to_string(),
                "web-rip" => "webrip".to_string(),
                _ => s,
            }
        })
    }

    fn extract_release_group(rest: &str) -> Option<String> {
        let idx = rest.rfind('-')?;

        // If the hyphen is part of a compound source tag like WEB- or Blu-
        let prefix = rest[..idx].to_ascii_lowercase();
        if prefix.ends_with("web") || prefix.ends_with("blu") {
            return None;
        }

        let candidate = rest[idx + 1..].trim();
        if candidate.is_empty() {
            return None;
        }

        // Must only contain alphanumeric chars or underscores
        if !candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }

        // Reject if candidate matches known source suffix, codec, or resolution
        let lower = candidate.to_ascii_lowercase();
        const REJECTED_TOKENS: &[&str] = &[
            "ray", "rip", "dl", "bdrip", "webrip", "bluray", "hdtv", "dvdrip",
            "x264", "x265", "hevc", "av1", "h264", "h265",
            "2160p", "4k", "1080p", "1080i", "720p", "576p", "480p",
        ];
        if REJECTED_TOKENS.contains(&lower.as_str()) {
            return None;
        }

        // Reject compound tokens where all parts are known tags (e.g. DL_x265)
        if candidate.contains('_')
            && candidate
                .split('_')
                .all(|part| REJECTED_TOKENS.contains(&part.to_ascii_lowercase().as_str()))
        {
            return None;
        }

        Some(candidate.to_string())
    }
}
