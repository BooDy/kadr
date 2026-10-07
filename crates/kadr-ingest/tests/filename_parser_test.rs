// crates/kadr-ingest/tests/filename_parser_test.rs
use kadr_ingest::parser::FilenameParser;
use std::path::Path;

#[test]
fn test_standard_scene_release() {
    let parser = FilenameParser::new();
    let parsed = parser
        .parse("Bab.El-Hadid.1958.Restored.1080p.BluRay.x264-Ghareeb.mkv")
        .unwrap();

    assert_eq!(parsed.title, "Bab El-Hadid");
    assert_eq!(parsed.year, Some(1958));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.source.as_deref(), Some("bluray"));
    assert_eq!(parsed.video_codec.as_deref(), Some("x264"));
    assert_eq!(parsed.container, "mkv");
    assert_eq!(parsed.release_group.as_deref(), Some("Ghareeb"));
}

#[test]
fn test_release_with_parenthesized_year() {
    let parser = FilenameParser::new();
    let parsed = parser
        .parse("The Nightingale's Prayer (1959) [1080p].mp4")
        .unwrap();

    assert_eq!(parsed.title, "The Nightingale's Prayer");
    assert_eq!(parsed.year, Some(1959));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.container, "mp4");
}

#[test]
fn test_simple_movie_file() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Cairo 30.mkv").unwrap();

    assert_eq!(parsed.title, "Cairo 30");
    assert_eq!(parsed.year, None);
    assert_eq!(parsed.container, "mkv");
}

#[test]
fn test_underscore_and_webdl_x265() {
    let parser = FilenameParser::new();
    let parsed = parser
        .parse("Al_Karnak_1975_2160p_WEB-DL_x265.mp4")
        .unwrap();

    assert_eq!(parsed.title, "Al Karnak");
    assert_eq!(parsed.year, Some(1975));
    assert_eq!(parsed.resolution.as_deref(), Some("2160p"));
    assert_eq!(parsed.source.as_deref(), Some("web-dl"));
    assert_eq!(parsed.video_codec.as_deref(), Some("x265"));
    assert_eq!(parsed.container, "mp4");
    assert_eq!(parsed.release_group, None);
}

#[test]
fn test_unsupported_extensions() {
    let parser = FilenameParser::new();
    assert!(parser.parse("movie.nfo").is_none());
    assert!(parser.parse("movie.srt").is_none());
    assert!(parser.parse("no_extension").is_none());
}

#[test]
fn test_case_insensitive_extension() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Movie.Title.2020.1080p.MKV").unwrap();

    assert_eq!(parsed.title, "Movie Title");
    assert_eq!(parsed.year, Some(2020));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.container, "mkv");
}

#[test]
fn test_hyphenated_source_bluray() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Movie.2020.1080p.Blu-Ray.mkv").unwrap();

    assert_eq!(parsed.title, "Movie");
    assert_eq!(parsed.year, Some(2020));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.source.as_deref(), Some("bluray"));
    assert_eq!(parsed.release_group, None);
    assert_eq!(parsed.container, "mkv");
}

#[test]
fn test_hyphenated_source_webrip() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Movie.2020.1080p.WEB-Rip.mkv").unwrap();

    assert_eq!(parsed.title, "Movie");
    assert_eq!(parsed.year, Some(2020));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.source.as_deref(), Some("webrip"));
    assert_eq!(parsed.release_group, None);
    assert_eq!(parsed.container, "mkv");
}

#[test]
fn test_release_group_with_underscore() {
    let parser = FilenameParser::new();
    let parsed = parser
        .parse("Movie.2020.1080p.BluRay.x264-Ghareeb_Team.mkv")
        .unwrap();

    assert_eq!(parsed.title, "Movie");
    assert_eq!(parsed.year, Some(2020));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.source.as_deref(), Some("bluray"));
    assert_eq!(parsed.video_codec.as_deref(), Some("x264"));
    assert_eq!(parsed.release_group.as_deref(), Some("Ghareeb_Team"));
    assert_eq!(parsed.container, "mkv");
}

#[test]
fn test_stemless_empty_title() {
    let parser = FilenameParser::new();
    assert!(parser.parse(".mkv").is_none());
    assert!(parser.parse("   .mp4").is_none());
}

#[test]
fn test_parenthesized_year_dot_separator() {
    let parser = FilenameParser::new();
    let parsed = parser.parse("Movie.Title.(2020).1080p.mkv").unwrap();

    assert_eq!(parsed.title, "Movie Title");
    assert_eq!(parsed.year, Some(2020));
    assert_eq!(parsed.resolution.as_deref(), Some("1080p"));
    assert_eq!(parsed.container, "mkv");
}

#[test]
fn test_parse_standard_sxx_exx_episode() {
    let parser = FilenameParser::new();
    let res = parser
        .parse(
            "What.We.Do.in.the.Shadows.(2019).-S04E01.-.Reunited.(1080p.HULU.WEB-DL.x265.Ghost).mkv",
        )
        .unwrap();
    assert!(res.is_episode);
    assert_eq!(
        res.series_title.as_deref(),
        Some("What We Do in the Shadows")
    );
    assert_eq!(res.season, Some(4));
    assert_eq!(res.episode, Some(1));
    assert_eq!(res.episode_title.as_deref(), Some("Reunited"));
}

#[test]
fn test_parse_alt_1x02_episode() {
    let parser = FilenameParser::new();
    let res = parser.parse("Hacks.1x02.Prank.Call.720p.mkv").unwrap();
    assert!(res.is_episode);
    assert_eq!(res.series_title.as_deref(), Some("Hacks"));
    assert_eq!(res.season, Some(1));
    assert_eq!(res.episode, Some(2));
    assert_eq!(res.episode_title.as_deref(), Some("Prank Call"));
}

#[test]
fn test_parse_folder_cue_fallback() {
    let parser = FilenameParser::new();
    let path = Path::new("/media/shows/Dexter/Season 01/02 - Crocodile.mkv");
    let res = parser.parse_with_path(path).unwrap();
    assert!(res.is_episode);
    assert_eq!(res.series_title.as_deref(), Some("Dexter"));
    assert_eq!(res.season, Some(1));
    assert_eq!(res.episode, Some(2));
}

#[test]
fn test_parse_with_path_movie_not_dropped() {
    let parser = FilenameParser::new();

    // Standard movie in /media/movies
    let path = Path::new("/media/movies/Bab.El-Hadid.1958.mkv");
    let res = parser.parse_with_path(path).unwrap();
    assert!(!res.is_episode);
    assert_eq!(res.title, "Bab El-Hadid");
    assert_eq!(res.year, Some(1958));
    assert_eq!(res.container, "mkv");

    // Movie in nested titled folder
    let nested_path = Path::new("/media/movies/Interstellar (2014)/Interstellar.mkv");
    let res2 = parser.parse_with_path(nested_path).unwrap();
    assert!(!res2.is_episode);
    assert_eq!(res2.title, "Interstellar");
    assert_eq!(res2.container, "mkv");
}

#[test]
fn test_parse_folder_cue_with_standalone_sxx_exx() {
    let parser = FilenameParser::new();
    let path = Path::new("/media/shows/Dexter/Season 01/S01E02.mkv");
    let res = parser.parse_with_path(path).unwrap();
    assert!(res.is_episode);
    assert_eq!(res.series_title.as_deref(), Some("Dexter"));
    assert_eq!(res.season, Some(1));
    assert_eq!(res.episode, Some(2));
}
