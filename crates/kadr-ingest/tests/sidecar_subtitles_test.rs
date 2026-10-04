use std::fs::{self, File};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

use kadr_core::models::{Library, MediaItem, MediaType};
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource};
use kadr_ingest::sidecars::{find_subtitles_for_media, DiscoveredSubtitle};
use kadr_ingest::watcher::{IngestMessage, IngestWorker};
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, SubtitleRepository};

#[test]
fn test_find_subtitles_alongside_media() {
    let dir = tempdir().unwrap();
    let movie = dir.path().join("Dune.2021.1080p.mkv");
    let srt = dir.path().join("Dune.2021.1080p.srt");
    let vtt = dir.path().join("Dune.2021.1080p.en.vtt");
    let ass = dir.path().join("Dune.2021.1080p.ara.ass");
    let sub = dir.path().join("Dune.2021.1080p.fr.sub");

    File::create(&movie).unwrap();
    File::create(&srt).unwrap();
    File::create(&vtt).unwrap();
    File::create(&ass).unwrap();
    File::create(&sub).unwrap();

    let mut subs = find_subtitles_for_media(&movie);
    assert_eq!(subs.len(), 4);

    // Sort by format for stable assertions
    subs.sort_by_key(|s| s.format.as_str());

    let ass_sub = subs
        .iter()
        .find(|s| s.format == SubtitleFormat::Ass)
        .unwrap();
    assert_eq!(ass_sub.source, SubtitleSource::Sidecar);
    assert_eq!(ass_sub.language, "ara");
    assert_eq!(ass_sub.title.as_deref(), Some("Arabic"));
    assert!(!ass_sub.is_forced);
    assert!(!ass_sub.is_default);

    let srt_sub = subs
        .iter()
        .find(|s| s.format == SubtitleFormat::Srt)
        .unwrap();
    assert_eq!(srt_sub.language, "und");

    let sub_sub = subs
        .iter()
        .find(|s| s.format == SubtitleFormat::Sub)
        .unwrap();
    assert_eq!(sub_sub.language, "fre");
    assert_eq!(sub_sub.title.as_deref(), Some("French"));

    let vtt_sub = subs
        .iter()
        .find(|s| s.format == SubtitleFormat::Vtt)
        .unwrap();
    assert_eq!(vtt_sub.language, "eng");
    assert_eq!(vtt_sub.title.as_deref(), Some("English"));
}

#[test]
fn test_language_tag_extraction() {
    let dir = tempdir().unwrap();
    let movie = dir.path().join("Film.2020.mkv");
    File::create(&movie).unwrap();

    let cases = vec![
        ("Film.2020.en.srt", "eng", "English"),
        ("Film.2020.eng.srt", "eng", "English"),
        ("Film.2020.ar.srt", "ara", "Arabic"),
        ("Film.2020.ara.vtt", "ara", "Arabic"),
        ("Film.2020.fr.srt", "fre", "French"),
        ("Film.2020.fre.srt", "fre", "French"),
        ("Film.2020.fra.srt", "fre", "French"),
        ("Film.2020.es.srt", "spa", "Spanish"),
        ("Film.2020.spa.srt", "spa", "Spanish"),
        ("Film.2020.de.srt", "ger", "German"),
        ("Film.2020.ger.srt", "ger", "German"),
        ("Film.2020.deu.srt", "ger", "German"),
        ("Film.2020.it.srt", "ita", "Italian"),
        ("Film.2020.ita.srt", "ita", "Italian"),
        ("Film.2020.ja.srt", "jpn", "Japanese"),
        ("Film.2020.jpn.srt", "jpn", "Japanese"),
        ("Film.2020.ko.srt", "kor", "Korean"),
        ("Film.2020.kor.srt", "kor", "Korean"),
        ("Film.2020.zh.srt", "chi", "Chinese"),
        ("Film.2020.zho.srt", "chi", "Chinese"),
        ("Film.2020.chi.srt", "chi", "Chinese"),
        ("Film.2020.pt.srt", "pt", "pt"),
        ("Film.2020.srt", "und", "Undetermined"),
    ];

    for (filename, expected_lang, expected_title) in cases {
        let sub_path = dir.path().join(filename);
        File::create(&sub_path).unwrap();

        let subs = find_subtitles_for_media(&movie);
        let found = subs
            .iter()
            .find(|s| s.file_path.as_deref() == Some(&sub_path));
        assert!(
            found.is_some(),
            "Expected to find subtitle for {}",
            filename
        );
        let s = found.unwrap();
        assert_eq!(s.language, expected_lang, "Mismatch for {}", filename);
        assert_eq!(
            s.title.as_deref(),
            Some(expected_title),
            "Mismatch title for {}",
            filename
        );

        let _ = fs::remove_file(&sub_path);
    }
}

#[test]
fn test_flag_extraction() {
    let dir = tempdir().unwrap();
    let movie = dir.path().join("Movie.mkv");
    File::create(&movie).unwrap();

    let forced_path = dir.path().join("Movie.en.forced.srt");
    let default_path = dir.path().join("Movie.ara.default.srt");
    let sdh_path = dir.path().join("Movie.en.sdh.srt");
    let combo_path = dir.path().join("Movie.fre.forced.default.sdh.srt");

    File::create(&forced_path).unwrap();
    File::create(&default_path).unwrap();
    File::create(&sdh_path).unwrap();
    File::create(&combo_path).unwrap();

    let subs = find_subtitles_for_media(&movie);
    assert_eq!(subs.len(), 4);

    let forced = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&forced_path))
        .unwrap();
    assert!(forced.is_forced);
    assert!(!forced.is_default);

    let def = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&default_path))
        .unwrap();
    assert!(!def.is_forced);
    assert!(def.is_default);

    let sdh = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&sdh_path))
        .unwrap();
    assert_eq!(sdh.title.as_deref(), Some("English [SDH]"));

    let combo = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&combo_path))
        .unwrap();
    assert!(combo.is_forced);
    assert!(combo.is_default);
    assert_eq!(combo.title.as_deref(), Some("French [SDH]"));
}

#[test]
fn test_scanning_subs_and_subtitles_subfolder() {
    let dir = tempdir().unwrap();
    let movie_dir = dir.path().join("Dune (2021)");
    fs::create_dir_all(&movie_dir).unwrap();

    let movie = movie_dir.join("Dune (2021).mkv");
    File::create(&movie).unwrap();

    let subs_dir = movie_dir.join("Subs");
    fs::create_dir_all(&subs_dir).unwrap();
    let sub1 = subs_dir.join("2_English.srt");
    File::create(&sub1).unwrap();

    let subtitles_dir = movie_dir.join("Subtitles");
    fs::create_dir_all(&subtitles_dir).unwrap();
    let sub2 = subtitles_dir.join("3_Arabic.forced.vtt");
    File::create(&sub2).unwrap();

    let sub3 = subs_dir.join("Dune (2021).es.ass");
    File::create(&sub3).unwrap();

    let subs = find_subtitles_for_media(&movie);
    assert_eq!(subs.len(), 3);

    let found_sub1 = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&sub1))
        .unwrap();
    assert_eq!(found_sub1.language, "eng");
    assert_eq!(found_sub1.format, SubtitleFormat::Srt);

    let found_sub2 = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&sub2))
        .unwrap();
    assert_eq!(found_sub2.language, "ara");
    assert!(found_sub2.is_forced);
    assert_eq!(found_sub2.format, SubtitleFormat::Vtt);

    let found_sub3 = subs
        .iter()
        .find(|s| s.file_path.as_deref() == Some(&sub3))
        .unwrap();
    assert_eq!(found_sub3.language, "spa");
    assert_eq!(found_sub3.format, SubtitleFormat::Ass);
}

#[test]
fn test_ignoring_unrelated_files() {
    let dir = tempdir().unwrap();
    let movie_a = dir.path().join("MovieA.2024.mkv");
    let movie_b = dir.path().join("MovieB.2024.mkv");

    File::create(&movie_a).unwrap();
    File::create(&movie_b).unwrap();

    // Related to MovieA
    let sub_a = dir.path().join("MovieA.2024.en.srt");
    File::create(&sub_a).unwrap();

    // Related to MovieB
    let sub_b = dir.path().join("MovieB.2024.en.srt");
    File::create(&sub_b).unwrap();

    // Unrelated files
    let poster = dir.path().join("MovieA.2024-poster.jpg");
    let nfo = dir.path().join("MovieA.2024.nfo");
    let txt = dir.path().join("MovieA.2024.txt");
    File::create(&poster).unwrap();
    File::create(&nfo).unwrap();
    File::create(&txt).unwrap();

    let subs = find_subtitles_for_media(&movie_a);
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].file_path.as_ref(), Some(&sub_a));
}

#[tokio::test]
async fn test_ingest_worker_persists_subtitles() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let lib_repo = LibraryRepository::new(pool.clone());
    let library = Library {
        id: "cinema".to_string(),
        name: "Cinema".to_string(),
        path: PathBuf::from("/media"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
        ..Default::default()
    };
    lib_repo.insert(&library).await.unwrap();

    let media_repo = MediaItemRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let worker = IngestWorker::new(rx, media_repo.clone()).with_subtitles(subtitle_repo.clone());
    let worker_handle = tokio::spawn(worker.run());

    let item = MediaItem {
        id: None,
        library_id: "cinema".to_string(),
        item_type: MediaType::Movie,
        title: "Dune".to_string(),
        original_title: None,
        release_year: Some(2021),
        added_at: 1700000000,
        file_path: PathBuf::from("/media/Dune.2021.1080p.mkv"),
        file_name: "Dune.2021.1080p.mkv".to_string(),
        file_size: 1024,
        technical: Default::default(),
        metadata: Default::default(),
    };

    let discovered_subs = vec![
        DiscoveredSubtitle {
            source: SubtitleSource::Sidecar,
            language: "eng".to_string(),
            title: Some("English".to_string()),
            format: SubtitleFormat::Srt,
            file_path: Some(PathBuf::from("/media/Dune.2021.1080p.en.srt")),
            stream_index: None,
            is_default: false,
            is_forced: false,
        },
        DiscoveredSubtitle {
            source: SubtitleSource::Sidecar,
            language: "ara".to_string(),
            title: Some("Arabic [SDH]".to_string()),
            format: SubtitleFormat::Vtt,
            file_path: Some(PathBuf::from("/media/Dune.2021.1080p.ara.sdh.vtt")),
            stream_index: None,
            is_default: true,
            is_forced: true,
        },
    ];

    tx.send(IngestMessage::Upsert(item.clone(), discovered_subs))
        .await
        .unwrap();

    // Allow worker to flush via 100ms timeout
    sleep(Duration::from_millis(200)).await;

    // Verify media item is in DB
    let items = media_repo.list_by_library("cinema", 10, 0).await.unwrap();
    assert_eq!(items.len(), 1);
    let media_id = items[0].id.expect("item must have id");

    // Verify subtitles were persisted
    let tracks = subtitle_repo.find_by_media_item(media_id).await.unwrap();
    assert_eq!(tracks.len(), 2);

    let eng = tracks.iter().find(|t| t.language == "eng").unwrap();
    assert_eq!(eng.source, SubtitleSource::Sidecar);
    assert_eq!(eng.format, SubtitleFormat::Srt);
    assert_eq!(eng.title.as_deref(), Some("English"));
    assert!(!eng.is_default);
    assert!(!eng.is_forced);

    let ara = tracks.iter().find(|t| t.language == "ara").unwrap();
    assert_eq!(ara.source, SubtitleSource::Sidecar);
    assert_eq!(ara.format, SubtitleFormat::Vtt);
    assert_eq!(ara.title.as_deref(), Some("Arabic [SDH]"));
    assert!(ara.is_default);
    assert!(ara.is_forced);

    drop(tx);
    worker_handle.await.unwrap();
}
