use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtworkPaths {
    pub poster: Option<PathBuf>,
    pub backdrop: Option<PathBuf>,
}

pub fn find_artwork<P: AsRef<Path>>(media_file: P) -> ArtworkPaths {
    let media_path = media_file.as_ref();
    let parent = match media_path.parent() {
        Some(p) => p,
        None => return ArtworkPaths::default(),
    };

    let stem = media_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut poster_candidates = Vec::new();
    if !stem.is_empty() {
        poster_candidates.push(parent.join(format!("{}-poster.jpg", stem)));
        poster_candidates.push(parent.join(format!("{}-poster.png", stem)));
        poster_candidates.push(parent.join(format!("{}-poster.jpeg", stem)));
        poster_candidates.push(parent.join(format!("{}-poster.webp", stem)));
    }
    poster_candidates.extend([
        parent.join("poster.jpg"),
        parent.join("poster.png"),
        parent.join("poster.jpeg"),
        parent.join("poster.webp"),
        parent.join("cover.jpg"),
        parent.join("cover.png"),
        parent.join("cover.jpeg"),
        parent.join("folder.jpg"),
        parent.join("folder.png"),
    ]);

    let mut backdrop_candidates = Vec::new();
    if !stem.is_empty() {
        backdrop_candidates.push(parent.join(format!("{}-backdrop.jpg", stem)));
        backdrop_candidates.push(parent.join(format!("{}-backdrop.png", stem)));
        backdrop_candidates.push(parent.join(format!("{}-backdrop.jpeg", stem)));
        backdrop_candidates.push(parent.join(format!("{}-backdrop.webp", stem)));
        backdrop_candidates.push(parent.join(format!("{}-fanart.jpg", stem)));
        backdrop_candidates.push(parent.join(format!("{}-fanart.png", stem)));
        backdrop_candidates.push(parent.join(format!("{}-fanart.webp", stem)));
    }
    backdrop_candidates.extend([
        parent.join("backdrop.jpg"),
        parent.join("backdrop.png"),
        parent.join("backdrop.jpeg"),
        parent.join("backdrop.webp"),
        parent.join("fanart.jpg"),
        parent.join("fanart.png"),
        parent.join("fanart.webp"),
        parent.join("background.jpg"),
        parent.join("background.png"),
    ]);

    let poster = poster_candidates.into_iter().find(|p| p.is_file());
    let backdrop = backdrop_candidates.into_iter().find(|p| p.is_file());

    ArtworkPaths { poster, backdrop }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_stem_named_artwork_takes_precedence() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("Iron Man (2008).mkv");
        let general_poster = dir.path().join("poster.jpg");
        let specific_poster = dir.path().join("Iron Man (2008)-poster.jpg");
        let general_backdrop = dir.path().join("backdrop.jpg");
        let specific_backdrop = dir.path().join("Iron Man (2008)-backdrop.jpg");

        File::create(&media).unwrap();
        File::create(&general_poster).unwrap();
        File::create(&specific_poster).unwrap();
        File::create(&general_backdrop).unwrap();
        File::create(&specific_backdrop).unwrap();

        let artwork = find_artwork(&media);
        assert_eq!(artwork.poster, Some(specific_poster));
        assert_eq!(artwork.backdrop, Some(specific_backdrop));
    }

    #[test]
    fn test_fallback_to_cover_and_fanart() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("film.mp4");
        let cover = dir.path().join("cover.jpg");
        let fanart = dir.path().join("fanart.jpg");

        File::create(&media).unwrap();
        File::create(&cover).unwrap();
        File::create(&fanart).unwrap();

        let artwork = find_artwork(&media);
        assert_eq!(artwork.poster, Some(cover));
        assert_eq!(artwork.backdrop, Some(fanart));
    }

    #[test]
    fn test_artwork_missing() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("film.mp4");
        File::create(&media).unwrap();

        let artwork = find_artwork(&media);
        assert_eq!(artwork.poster, None);
        assert_eq!(artwork.backdrop, None);
    }
}
