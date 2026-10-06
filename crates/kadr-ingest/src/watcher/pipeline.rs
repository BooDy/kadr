use crate::error::Result;
use crate::parser::FilenameParser;
use crate::probe::TechnicalProber;
use crate::sidecars::{DiscoveredSubtitle, SidecarScanner};
use crate::thumbnail::ThumbnailExtractor;
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::warn;

pub struct IngestPipeline {
    filename_parser: FilenameParser,
    sidecar_scanner: SidecarScanner,
    prober: TechnicalProber,
    thumbnail_extractor: Option<ThumbnailExtractor>,
}

impl Default for IngestPipeline {
    fn default() -> Self {
        Self::new(true, None)
    }
}

impl IngestPipeline {
    pub fn new(enable_ffprobe: bool, thumbnails_dir: Option<PathBuf>) -> Self {
        Self {
            filename_parser: FilenameParser::new(),
            sidecar_scanner: SidecarScanner::new(),
            prober: TechnicalProber::new(enable_ffprobe),
            thumbnail_extractor: thumbnails_dir.map(ThumbnailExtractor::new),
        }
    }

    pub fn thumbnail_extractor(&self) -> Option<&ThumbnailExtractor> {
        self.thumbnail_extractor.as_ref()
    }

    pub async fn process_file<P: AsRef<Path>>(
        &self,
        library: &Library,
        path: P,
    ) -> Result<Option<(MediaItem, Vec<DiscoveredSubtitle>)>> {
        let path = path.as_ref();
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => return Ok(None),
        };

        let parsed = match self.filename_parser.parse_with_path(path) {
            Some(p) => p,
            None => return Ok(None),
        };

        let metadata_fs = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };
        let file_size = metadata_fs.len();
        let added_at = metadata_fs
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut technical = self.prober.probe(path).await?;
        if technical.resolution.is_none() {
            technical.resolution = parsed.resolution;
        }
        if technical.video_codec.is_none() {
            technical.video_codec = parsed.video_codec;
        }
        if technical.container.is_none() {
            technical.container = Some(parsed.container);
        }

        let artwork = self.sidecar_scanner.find_artwork(path);
        let nfo = match self.sidecar_scanner.find_nfo_for_media(path) {
            Ok(opt) => opt,
            Err(e) => {
                warn!(path = ?path, error = ?e, "Failed to parse .nfo sidecar; falling back to filename metadata");
                None
            }
        };

        let mut item_type = library.media_type;
        let mut final_title = parsed.title;
        let mut final_year = parsed.year;
        let mut original_title = None;
        let mut meta = MediaMetadata::default();

        if parsed.is_episode || library.media_type == MediaType::Show {
            item_type = MediaType::Episode;
            meta.series_title = parsed.series_title.clone();
            meta.season = parsed.season;
            meta.episode = parsed.episode;
            final_title = parsed
                .episode_title
                .clone()
                .unwrap_or_else(|| format!("Episode {}", parsed.episode.unwrap_or(1)));

            if meta.series_title.is_none() && library.media_type == MediaType::Show {
                if let Some(parent) = path.parent() {
                    let parent_name = parent.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    let lower = parent_name.to_lowercase();
                    let is_season = (lower.starts_with("season") || lower.starts_with('s'))
                        && lower
                            .trim_start_matches("season")
                            .trim_start_matches('s')
                            .trim_matches(|c: char| c == '.' || c == '_' || c == '-' || c.is_whitespace())
                            .chars()
                            .all(|c| c.is_ascii_digit());
                    if is_season {
                        if let Some(gp) = parent.parent() {
                            if gp != library.path && !gp.as_os_str().is_empty() {
                                meta.series_title = gp
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .map(String::from);
                            }
                        }
                    } else if parent != library.path && !parent.as_os_str().is_empty() {
                        meta.series_title = Some(parent_name.to_string());
                    }
                }
            }
        }

        if let Some(nfo_data) = nfo {
            if let Some(t) = nfo_data.title {
                final_title = t;
            }
            if let Some(y) = nfo_data.year {
                final_year = Some(y);
            }
            original_title = nfo_data.original_title;
            meta.overview = nfo_data.overview;
            meta.director = nfo_data.director;
            meta.studio = nfo_data.studio;
            meta.actors = nfo_data.actors;
            meta.tags = nfo_data.tags;
        }

        let mut poster_path = artwork.poster.and_then(|p| p.to_str().map(String::from));
        if poster_path.is_none() {
            if let Some(ref extractor) = self.thumbnail_extractor {
                if let Ok(Some(thumb_path)) = extractor.extract_thumbnail(path, technical.duration_seconds).await {
                    poster_path = Some(thumb_path.to_string_lossy().to_string());
                }
            }
        }

        meta.release_group = parsed.release_group;
        meta.poster_path = poster_path;
        meta.backdrop_path = artwork.backdrop.and_then(|p| p.to_str().map(String::from));

        let subtitles = self.sidecar_scanner.find_subtitles(path);

        Ok(Some((
            MediaItem {
                id: None,
                library_id: library.id.clone(),
                item_type,
                title: final_title,
                original_title,
                release_year: final_year,
                added_at,
                file_path: path.to_path_buf(),
                file_name: filename.to_string(),
                file_size,
                technical,
                metadata: meta,
            },
            subtitles,
        )))
    }
}
