// crates/kadr-core/tests/models_test.rs
use kadr_core::models::{Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo};
use std::path::PathBuf;

#[test]
fn test_media_item_serialization_roundtrip() {
    let item = MediaItem {
        id: Some(42),
        library_id: "movies".to_string(),
        item_type: MediaType::Movie,
        title: "Cairo Station".to_string(),
        original_title: Some("Bab El Hadid".to_string()),
        release_year: Some(1958),
        added_at: 1700000000,
        file_path: PathBuf::from("/media/movies/Cairo Station (1958).mkv"),
        file_name: "Cairo Station (1958).mkv".to_string(),
        file_size: 4_500_000_000,
        technical: TechnicalInfo {
            duration_seconds: 4620,
            resolution: Some("1080p".to_string()),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
        },
        metadata: MediaMetadata {
            director: Some("Youssef Chahine".to_string()),
            writers: vec!["Abdel Hay Adib".to_string()],
            actors: vec!["Farid Shawqi".to_string(), "Hind Rostom".to_string()],
            overview: Some("A crippled newspaper vendor becomes obsessed with a lemonade seller.".to_string()),
            country: Some("Egypt".to_string()),
            language: Some("ara".to_string()),
            tags: vec!["classic".to_string(), "drama".to_string()],
            studio: Some("Studio Misr".to_string()),
            poster_path: Some("/media/movies/poster.jpg".to_string()),
            backdrop_path: None,
            release_group: Some("Ghareeb".to_string()),
        },
    };

    let serialized = serde_json::to_string(&item).expect("serialization failed");
    let deserialized: MediaItem = serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(item, deserialized);
}

#[test]
fn test_library_serialization_roundtrip() {
    let lib = Library {
        id: "movies".to_string(),
        name: "Movies".to_string(),
        path: PathBuf::from("/media/movies"),
        media_type: MediaType::Movie,
        created_at: 1700000000,
    };

    let serialized = serde_json::to_string(&lib).expect("serialization failed");
    let deserialized: Library = serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(lib, deserialized);
}
