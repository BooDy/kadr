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

        // Match: Title (Year) [Tags].ext
        let paren_year_regex = Regex::new(
            r"(?i)^(?P<title>.+?)\s*\((?P<year>(?:19|20)\d{2})\)(?:\s*(?P<rest>.*?))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        let res_regex = Regex::new(r"(?i)\b(2160p|4k|1080p|1080i|720p|576p|480p)\b").unwrap();
        let source_regex = Regex::new(r"(?i)\b(bluray|blu-ray|web-dl|webrip|hdtv|dvdrip)\b").unwrap();
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
            let year = caps.name("year").and_then(|m| m.as_str().parse::<i32>().ok());
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
            let clean_rest = rest.replace('_', " ");

            let resolution = self.res_regex.find(&clean_rest).map(|m| m.as_str().to_lowercase());
            let source = self.source_regex.find(&clean_rest).map(|m| m.as_str().to_string());
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
            let year = caps.name("year").and_then(|m| m.as_str().parse::<i32>().ok());
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
            let clean_rest = rest.replace('_', " ");

            let resolution = self.res_regex.find(&clean_rest).map(|m| m.as_str().to_lowercase());
            let source = self.source_regex.find(&clean_rest).map(|m| m.as_str().to_string());
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

    fn extract_release_group(rest: &str) -> Option<String> {
        let idx = rest.rfind('-')?;
        let candidate = rest[idx + 1..].trim();
        if !candidate.is_empty()
            && !candidate.eq_ignore_ascii_case("dl")
            && candidate.chars().all(|c| c.is_alphanumeric())
        {
            Some(candidate.to_string())
        } else {
            None
        }
    }
}
