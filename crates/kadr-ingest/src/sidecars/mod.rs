pub mod artwork;
pub mod nfo;

use std::path::Path;
pub use artwork::ArtworkPaths;
pub use nfo::NfoData;

#[derive(Default, Debug, Clone)]
pub struct SidecarScanner;

impl SidecarScanner {
    pub fn new() -> Self {
        Self
    }

    pub fn read_nfo<P: AsRef<Path>>(&self, path: P) -> crate::error::Result<Option<NfoData>> {
        nfo::parse_nfo(path)
    }

    pub fn find_nfo_for_media<P: AsRef<Path>>(&self, media_path: P) -> crate::error::Result<Option<NfoData>> {
        let p = media_path.as_ref();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent = match p.parent() {
            Some(dir) => dir,
            None => return Ok(None),
        };

        let candidate_named = parent.join(format!("{}.nfo", stem));
        if candidate_named.is_file() {
            return self.read_nfo(&candidate_named);
        }

        let candidate_movie = parent.join("movie.nfo");
        if candidate_movie.is_file() {
            return self.read_nfo(&candidate_movie);
        }

        Ok(None)
    }

    pub fn find_artwork<P: AsRef<Path>>(&self, media_path: P) -> ArtworkPaths {
        artwork::find_artwork(media_path)
    }
}
