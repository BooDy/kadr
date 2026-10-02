use kadr_ingest::sidecars::SidecarScanner;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_parse_nfo_file() {
    let dir = tempdir().unwrap();
    let nfo_path = dir.path().join("movie.nfo");
    let xml_content = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<movie>
    <title>The Land</title>
    <originaltitle>Al-Ard</originaltitle>
    <year>1969</year>
    <plot>Peasants in an Egyptian village struggle against feudal oppression.</plot>
    <director>Youssef Chahine</director>
    <studio>General Company for Arab Film Production</studio>
    <actor>
        <name>Mahmoud El-Meliguy</name>
    </actor>
    <actor>
        <name>Ezzat El Alaili</name>
    </actor>
    <genre>Drama</genre>
    <genre>Historical</genre>
</movie>"#;

    let mut file = File::create(&nfo_path).unwrap();
    file.write_all(xml_content.as_bytes()).unwrap();

    let scanner = SidecarScanner::new();
    let nfo = scanner
        .read_nfo(&nfo_path)
        .unwrap()
        .expect("nfo should parse");

    assert_eq!(nfo.title.as_deref(), Some("The Land"));
    assert_eq!(nfo.original_title.as_deref(), Some("Al-Ard"));
    assert_eq!(nfo.year, Some(1969));
    assert_eq!(nfo.director.as_deref(), Some("Youssef Chahine"));
    assert_eq!(nfo.actors, vec!["Mahmoud El-Meliguy", "Ezzat El Alaili"]);
    assert_eq!(nfo.tags, vec!["Drama", "Historical"]);
}

#[test]
fn test_discover_artwork() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("film.mkv");
    let poster_path = dir.path().join("poster.jpg");
    let backdrop_path = dir.path().join("backdrop.jpg");

    File::create(&video_path).unwrap();
    File::create(&poster_path).unwrap();
    File::create(&backdrop_path).unwrap();

    let scanner = SidecarScanner::new();
    let artwork = scanner.find_artwork(&video_path);

    assert_eq!(artwork.poster, Some(poster_path));
    assert_eq!(artwork.backdrop, Some(backdrop_path));
}

#[test]
fn test_find_nfo_for_media() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("film.mkv");
    let nfo_path = dir.path().join("film.nfo");

    let xml_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Specific Film</title>
</movie>"#;

    File::create(&video_path).unwrap();
    let mut file = File::create(&nfo_path).unwrap();
    file.write_all(xml_content.as_bytes()).unwrap();

    let scanner = SidecarScanner::new();
    let nfo = scanner
        .find_nfo_for_media(&video_path)
        .unwrap()
        .expect("film.nfo should be found");
    assert_eq!(nfo.title.as_deref(), Some("Specific Film"));
}

#[test]
fn test_find_nfo_for_media_movie_fallback() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("film.mkv");
    let nfo_path = dir.path().join("movie.nfo");

    let xml_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
    <title>Fallback Film</title>
</movie>"#;

    File::create(&video_path).unwrap();
    let mut file = File::create(&nfo_path).unwrap();
    file.write_all(xml_content.as_bytes()).unwrap();

    let scanner = SidecarScanner::new();
    let nfo = scanner
        .find_nfo_for_media(&video_path)
        .unwrap()
        .expect("movie.nfo should be found");
    assert_eq!(nfo.title.as_deref(), Some("Fallback Film"));
}

#[test]
fn test_parse_nfo_unicode_arabic() {
    let dir = tempdir().unwrap();
    let nfo_path = dir.path().join("movie.nfo");
    let xml_content = r#"<?xml version="1.0" encoding="UTF-8" ?>
<movie>
    <title>الأرض</title>
    <director>يوسف شاهين</director>
    <actor><name>محمود المليجي</name></actor>
</movie>"#;

    let mut file = File::create(&nfo_path).unwrap();
    file.write_all(xml_content.as_bytes()).unwrap();

    let scanner = SidecarScanner::new();
    let nfo = scanner
        .read_nfo(&nfo_path)
        .unwrap()
        .expect("unicode nfo should parse");

    assert_eq!(nfo.title.as_deref(), Some("الأرض"));
    assert_eq!(nfo.director.as_deref(), Some("يوسف شاهين"));
    assert_eq!(nfo.actors, vec!["محمود المليجي"]);
}

#[test]
fn test_missing_nfo_returns_none() {
    let dir = tempdir().unwrap();
    let nfo_path = dir.path().join("missing.nfo");
    let scanner = SidecarScanner::new();
    assert!(scanner.read_nfo(&nfo_path).unwrap().is_none());
}

#[test]
fn test_parse_nfo_multibyte_year_no_panic() {
    let dir = tempdir().unwrap();
    let nfo_path = dir.path().join("arabic_dates.nfo");
    let xml_content = r#"<?xml version="1.0" encoding="UTF-8" ?>
<movie>
    <title>الكيف</title>
    <premiered>سنة 1985</premiered>
</movie>"#;

    let mut file = File::create(&nfo_path).unwrap();
    file.write_all(xml_content.as_bytes()).unwrap();

    let scanner = SidecarScanner::new();
    let nfo = scanner
        .read_nfo(&nfo_path)
        .unwrap()
        .expect("nfo should parse");

    assert_eq!(nfo.title.as_deref(), Some("الكيف"));
    assert_eq!(nfo.year, Some(1985));
}

#[test]
fn test_discover_webp_artwork() {
    let dir = tempdir().unwrap();
    let video_path = dir.path().join("movie.mkv");
    let backdrop_path = dir.path().join("movie-backdrop.webp");

    File::create(&video_path).unwrap();
    File::create(&backdrop_path).unwrap();

    let scanner = SidecarScanner::new();
    let artwork = scanner.find_artwork(&video_path);

    assert_eq!(artwork.backdrop, Some(backdrop_path));
}
