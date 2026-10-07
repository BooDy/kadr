use regex::Regex;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFilename {
    pub title: String,
    pub year: Option<i32>,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub source: Option<String>,
    pub release_group: Option<String>,
    pub container: String,
    pub is_episode: bool,
    pub series_title: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub episode_title: Option<String>,
}

#[derive(Default)]
struct EpisodeRestMeta {
    episode_title: Option<String>,
    resolution: Option<String>,
    source: Option<String>,
    video_codec: Option<String>,
    release_group: Option<String>,
}

pub struct FilenameParser {
    scene_regex: Regex,
    paren_year_regex: Regex,
    res_regex: Regex,
    source_regex: Regex,
    codec_regex: Regex,
    sxx_exx_regex: Regex,
    alt_num_regex: Regex,
    tag_start_regex: Regex,
    year_strip_regex: Regex,
    season_folder_regex: Regex,
    ep_file_regex: Regex,
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
        let source_regex =
            Regex::new(r"(?i)\b(bluray|blu-ray|web-dl|webdl|web-rip|webrip|hdtv|bdrip|dvdrip)\b")
                .unwrap();
        let codec_regex = Regex::new(r"(?i)\b(x264|x265|hevc|av1|h\.?264|h\.?265)\b").unwrap();

        let sxx_exx_regex = Regex::new(
            r"(?i)^(?P<series>.+?)[._\s\-]+[sS](?P<season>\d{1,2})[eE](?P<episode>\d{1,3})(?:[._\s\-]+(?P<rest>.*?))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        let alt_num_regex = Regex::new(
            r"(?i)^(?P<series>.+?)[._\s\-]+(?P<season>\d{1,2})x(?P<episode>\d{1,3})(?:[._\s\-]+(?P<rest>.*?))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        let tag_start_regex = Regex::new(
            r"(?i)(?:[._\s\-]+(?:\(|\[)?|(?:\(|\[)|\b)(?:(?:19|20)\d{2}|2160p|4k|1080p|1080i|720p|576p|480p|bluray|blu-ray|web-dl|webdl|web-rip|webrip|hdtv|bdrip|dvdrip|x264|x265|hevc|av1|h\.?264|h\.?265)\b"
        ).unwrap();

        let year_strip_regex =
            Regex::new(r"(?i)(?:\((?P<y1>(?:19|20)\d{2})\)|[._\s\-]+(?P<y2>(?:19|20)\d{2}))\s*$")
                .unwrap();

        let season_folder_regex = Regex::new(r"(?i)^(?:season[._\s\-]*|s)(\d{1,2})$").unwrap();

        let ep_file_regex = Regex::new(
            r"(?i)^(?:(?:s\d{1,2}[eE]|e|ep)[._\s\-]*)?(?P<ep>\d{1,3})(?:[._\s\-]+(?P<rest>.*?))?\.(?P<ext>mkv|mp4|webm|avi)$"
        ).unwrap();

        Self {
            scene_regex,
            paren_year_regex,
            res_regex,
            source_regex,
            codec_regex,
            sxx_exx_regex,
            alt_num_regex,
            tag_start_regex,
            year_strip_regex,
            season_folder_regex,
            ep_file_regex,
        }
    }

    pub fn parse(&self, filename: &str) -> Option<ParsedFilename> {
        let ext = filename.split('.').next_back()?.to_lowercase();
        if !["mkv", "mp4", "webm", "avi"].contains(&ext.as_str()) {
            return None;
        }

        // Try TV episode formats first (SxxExx or #x##)
        if let Some(caps) = self
            .sxx_exx_regex
            .captures(filename)
            .or_else(|| self.alt_num_regex.captures(filename))
        {
            let raw_series = caps.name("series").map(|m| m.as_str()).unwrap_or("");
            let (series_title, year) = self.clean_series_title(raw_series);
            if !series_title.is_empty() {
                let season = caps
                    .name("season")
                    .and_then(|m| m.as_str().parse::<u32>().ok());
                let episode = caps
                    .name("episode")
                    .and_then(|m| m.as_str().parse::<u32>().ok());
                let rest = caps.name("rest").map(|m| m.as_str());
                let rest_meta = self.parse_episode_rest(rest);

                let title = rest_meta
                    .episode_title
                    .clone()
                    .unwrap_or_else(|| format!("Episode {}", episode.unwrap_or(1)));

                return Some(ParsedFilename {
                    title,
                    year,
                    resolution: rest_meta.resolution,
                    video_codec: rest_meta.video_codec,
                    source: rest_meta.source,
                    release_group: rest_meta.release_group,
                    container: ext,
                    is_episode: true,
                    series_title: Some(series_title),
                    season,
                    episode,
                    episode_title: rest_meta.episode_title,
                });
            }
        }

        if let Some(caps) = self.scene_regex.captures(filename) {
            let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
            let clean_title = raw_title.replace(['.', '_'], " ").trim().to_string();
            if clean_title.is_empty() {
                return None;
            }
            let year = caps
                .name("year")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
            let clean_rest = rest.replace('_', " ");

            let resolution = self
                .res_regex
                .find(&clean_rest)
                .map(|m| m.as_str().to_lowercase());
            let source = self.extract_source(&clean_rest);
            let video_codec = self
                .codec_regex
                .find(&clean_rest)
                .map(|m| m.as_str().to_lowercase());
            let release_group = Self::extract_release_group(rest);

            return Some(ParsedFilename {
                title: clean_title,
                year,
                resolution,
                video_codec,
                source,
                release_group,
                container: ext,
                is_episode: false,
                series_title: None,
                season: None,
                episode: None,
                episode_title: None,
            });
        }

        if let Some(caps) = self.paren_year_regex.captures(filename) {
            let raw_title = caps.name("title").map(|m| m.as_str()).unwrap_or("");
            let clean_title = raw_title.replace(['.', '_'], " ").trim().to_string();
            if clean_title.is_empty() {
                return None;
            }
            let year = caps
                .name("year")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
            let clean_rest = rest.replace('_', " ");

            let resolution = self
                .res_regex
                .find(&clean_rest)
                .map(|m| m.as_str().to_lowercase());
            let source = self.extract_source(&clean_rest);
            let video_codec = self
                .codec_regex
                .find(&clean_rest)
                .map(|m| m.as_str().to_lowercase());
            let release_group = Self::extract_release_group(rest);

            return Some(ParsedFilename {
                title: clean_title,
                year,
                resolution,
                video_codec,
                source,
                release_group,
                container: ext,
                is_episode: false,
                series_title: None,
                season: None,
                episode: None,
                episode_title: None,
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
            is_episode: false,
            series_title: None,
            season: None,
            episode: None,
            episode_title: None,
        })
    }

    pub fn parse_with_path(&self, path: &Path) -> Option<ParsedFilename> {
        let filename = path.file_name()?.to_str()?;
        let mut parsed = self.parse(filename)?;

        if !parsed.is_episode {
            self.apply_folder_cues(path, filename, &mut parsed);
        }

        Some(parsed)
    }

    fn apply_folder_cues(&self, path: &Path, filename: &str, parsed: &mut ParsedFilename) {
        let parent = match path.parent() {
            Some(p) => p,
            None => return,
        };
        let parent_name = match parent.file_name().and_then(|f| f.to_str()) {
            Some(name) => name,
            None => return,
        };
        let season_caps = match self.season_folder_regex.captures(parent_name) {
            Some(caps) => caps,
            None => return,
        };
        let season_num = match season_caps
            .get(1)
            .and_then(|m| m.as_str().parse::<u32>().ok())
        {
            Some(num) => num,
            None => return,
        };

        // Grandparent folder provides the series title
        let grandparent = match parent.parent() {
            Some(gp) => gp,
            None => return,
        };
        let grandparent_name = match grandparent.file_name().and_then(|f| f.to_str()) {
            Some(name) => name,
            None => return,
        };
        let (series_title, series_year) = self.clean_series_title(grandparent_name);
        if series_title.is_empty() {
            return;
        }

        // Filename in season folder matches episode number and optional title (e.g. "02 - Crocodile.mkv", "02.mkv", "E02.mkv", "S01E02.mkv")
        if let Some(caps) = self.ep_file_regex.captures(filename) {
            let ep_num = match caps.name("ep").and_then(|m| m.as_str().parse::<u32>().ok()) {
                Some(num) => num,
                None => return,
            };
            let rest = caps.name("rest").map(|m| m.as_str());
            let rest_meta = self.parse_episode_rest(rest);

            let title = rest_meta
                .episode_title
                .clone()
                .unwrap_or_else(|| format!("Episode {}", ep_num));

            parsed.title = title;
            parsed.is_episode = true;
            parsed.series_title = Some(series_title);
            parsed.season = Some(season_num);
            parsed.episode = Some(ep_num);
            parsed.episode_title = rest_meta.episode_title;
            if parsed.year.is_none() {
                parsed.year = series_year;
            }
            if parsed.resolution.is_none() {
                parsed.resolution = rest_meta.resolution;
            }
            if parsed.source.is_none() {
                parsed.source = rest_meta.source;
            }
            if parsed.video_codec.is_none() {
                parsed.video_codec = rest_meta.video_codec;
            }
            if parsed.release_group.is_none() {
                parsed.release_group = rest_meta.release_group;
            }
        }
    }

    fn clean_series_title(&self, raw: &str) -> (String, Option<i32>) {
        let mut year = None;
        let series_part = if let Some(caps) = self.year_strip_regex.captures(raw) {
            if let Some(y_str) = caps.name("y1").or_else(|| caps.name("y2")) {
                year = y_str.as_str().parse::<i32>().ok();
            }
            if let Some(m) = caps.get(0) {
                &raw[..m.start()]
            } else {
                raw
            }
        } else {
            raw
        };

        let cleaned = series_part
            .trim_matches(|c: char| {
                c.is_whitespace()
                    || c == '.'
                    || c == '_'
                    || c == '-'
                    || c == '('
                    || c == ')'
                    || c == '['
                    || c == ']'
            })
            .replace(['.', '_'], " ")
            .trim()
            .to_string();

        (cleaned, year)
    }

    fn parse_episode_rest(&self, rest: Option<&str>) -> EpisodeRestMeta {
        let rest = match rest {
            Some(r) if !r.is_empty() => r,
            _ => return EpisodeRestMeta::default(),
        };

        let (ep_raw, tags_raw) = if let Some(m) = self.tag_start_regex.find(rest) {
            (&rest[..m.start()], &rest[m.start()..])
        } else {
            (rest, "")
        };

        let episode_title = {
            let trimmed = ep_raw
                .trim_matches(|c: char| {
                    c.is_whitespace()
                        || c == '.'
                        || c == '_'
                        || c == '-'
                        || c == '('
                        || c == ')'
                        || c == '['
                        || c == ']'
                })
                .replace(['.', '_'], " ")
                .trim()
                .to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        };

        let clean_tags = tags_raw.replace('_', " ");
        let resolution = self
            .res_regex
            .find(&clean_tags)
            .map(|m| m.as_str().to_lowercase());
        let source = self.extract_source(&clean_tags);
        let video_codec = self
            .codec_regex
            .find(&clean_tags)
            .map(|m| m.as_str().to_lowercase());
        let release_group = Self::extract_release_group(tags_raw);

        EpisodeRestMeta {
            episode_title,
            resolution,
            source,
            video_codec,
            release_group,
        }
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
            "ray", "rip", "dl", "bdrip", "webrip", "bluray", "hdtv", "dvdrip", "x264", "x265",
            "hevc", "av1", "h264", "h265", "2160p", "4k", "1080p", "1080i", "720p", "576p", "480p",
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
