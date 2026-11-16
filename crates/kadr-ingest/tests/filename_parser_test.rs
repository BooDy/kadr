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

#[test]
fn test_all_expanded_video_extensions_scene_and_paren_movies() {
    let parser = FilenameParser::new();
    let extensions = [
        "mkv", "mp4", "m4v", "mov", "webm", "avi", "ts", "m2ts", "mts", "3gp", "3g2",
    ];

    for ext in extensions {
        let scene_name = format!(
            "Cairo.Station.1958.1080p.BluRay.x264-Restored.{}",
            ext.to_ascii_uppercase()
        );
        let parsed_scene = parser
            .parse(&scene_name)
            .unwrap_or_else(|| panic!("scene movie should parse for .{ext}"));
        assert_eq!(parsed_scene.title, "Cairo Station");
        assert_eq!(parsed_scene.year, Some(1958));
        assert_eq!(parsed_scene.resolution.as_deref(), Some("1080p"));
        assert_eq!(parsed_scene.source.as_deref(), Some("bluray"));
        assert_eq!(parsed_scene.video_codec.as_deref(), Some("x264"));
        assert_eq!(parsed_scene.release_group.as_deref(), Some("Restored"));
        assert_eq!(parsed_scene.container, ext);
        assert!(!parsed_scene.is_episode);

        let paren_name = format!("The Land (1969) [720p].{ext}");
        let parsed_paren = parser
            .parse(&paren_name)
            .unwrap_or_else(|| panic!("paren movie should parse for .{ext}"));
        assert_eq!(parsed_paren.title, "The Land");
        assert_eq!(parsed_paren.year, Some(1969));
        assert_eq!(parsed_paren.resolution.as_deref(), Some("720p"));
        assert_eq!(parsed_paren.container, ext);

        let simple_name = format!("Family_Clip.{ext}");
        let parsed_simple = parser
            .parse(&simple_name)
            .unwrap_or_else(|| panic!("simple movie should parse for .{ext}"));
        assert_eq!(parsed_simple.title, "Family Clip");
        assert_eq!(parsed_simple.year, None);
        assert_eq!(parsed_simple.container, ext);
    }
}

#[test]
fn test_all_expanded_video_extensions_tv_episodes_and_folder_cues() {
    let parser = FilenameParser::new();
    let extensions = [
        "mov", "m4v", "ts", "m2ts", "mts", "3gp", "3g2", "avi", "webm",
    ];

    for ext in extensions {
        let sxx_name = format!("Severance.S01E09.The.We.We.Are.1080p.WEB-DL.x265.{ext}");
        let parsed_sxx = parser
            .parse(&sxx_name)
            .unwrap_or_else(|| panic!("SxxExx should parse for .{ext}"));
        assert!(parsed_sxx.is_episode);
        assert_eq!(parsed_sxx.series_title.as_deref(), Some("Severance"));
        assert_eq!(parsed_sxx.season, Some(1));
        assert_eq!(parsed_sxx.episode, Some(9));
        assert_eq!(parsed_sxx.episode_title.as_deref(), Some("The We We Are"));
        assert_eq!(parsed_sxx.container, ext);

        let alt_name = format!("Twin.Peaks.2x07.Lonely.Souls.720p.{ext}");
        let parsed_alt = parser
            .parse(&alt_name)
            .unwrap_or_else(|| panic!("1x02 should parse for .{ext}"));
        assert!(parsed_alt.is_episode);
        assert_eq!(parsed_alt.series_title.as_deref(), Some("Twin Peaks"));
        assert_eq!(parsed_alt.season, Some(2));
        assert_eq!(parsed_alt.episode, Some(7));
        assert_eq!(parsed_alt.episode_title.as_deref(), Some("Lonely Souls"));
        assert_eq!(parsed_alt.container, ext);

        let folder_path = format!("/media/shows/Andor (2022)/Season 01/10 - One Way Out.{ext}");
        let parsed_folder = parser
            .parse_with_path(Path::new(&folder_path))
            .unwrap_or_else(|| panic!("season folder cue should parse for .{ext}"));
        assert!(parsed_folder.is_episode);
        assert_eq!(parsed_folder.series_title.as_deref(), Some("Andor"));
        assert_eq!(parsed_folder.year, Some(2022));
        assert_eq!(parsed_folder.season, Some(1));
        assert_eq!(parsed_folder.episode, Some(10));
        assert_eq!(parsed_folder.episode_title.as_deref(), Some("One Way Out"));
        assert_eq!(parsed_folder.container, ext);
    }

    // Ensure extension-named bare filenames without a dot are rejected
    assert!(parser.parse("mkv").is_none());
    assert!(parser.parse("mov").is_none());
    assert!(parser.parse("3gp").is_none());
}
