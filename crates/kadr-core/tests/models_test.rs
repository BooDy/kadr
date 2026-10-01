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
            ..Default::default()
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

#[test]
fn test_auth_and_user_models_serialization_roundtrip() {
    use kadr_core::models::{AuthClaims, User, UserRole};

    let user = User {
        id: "usr_abc123".to_string(),
        username: "ghareeb".to_string(),
        pin_hash: "$argon2id$...".to_string(),
        role: UserRole::Admin,
        created_at: 1700000000,
    };

    let serialized = serde_json::to_string(&user).expect("serialization failed");
    let deserialized: User = serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(user, deserialized);

    // Test alias deserialization for UserRole
    let role_admin: UserRole = serde_json::from_str("\"Admin\"").unwrap();
    assert_eq!(role_admin, UserRole::Admin);
    let role_admin_upper: UserRole = serde_json::from_str("\"ADMIN\"").unwrap();
    assert_eq!(role_admin_upper, UserRole::Admin);
    let role_admin_snake: UserRole = serde_json::from_str("\"admin\"").unwrap();
    assert_eq!(role_admin_snake, UserRole::Admin);

    let role_std: UserRole = serde_json::from_str("\"Standard\"").unwrap();
    assert_eq!(role_std, UserRole::Standard);
    let role_std_snake: UserRole = serde_json::from_str("\"standard\"").unwrap();
    assert_eq!(role_std_snake, UserRole::Standard);

    let claims = AuthClaims {
        sub: user.id.clone(),
        username: user.username.clone(),
        role: user.role,
        exp: 1700086400,
        iat: 1700000000,
    };
    let claims_json = serde_json::to_string(&claims).expect("serialization failed");
    let deserialized_claims: AuthClaims =
        serde_json::from_str(&claims_json).expect("deserialization failed");
    assert_eq!(claims, deserialized_claims);
}

#[test]
fn test_playback_models_serialization_roundtrip() {
    use kadr_core::models::{PlaybackSession, PlaybackState, WatchState};

    let state = PlaybackState {
        user_id: "usr_1".to_string(),
        media_item_id: 42,
        playback_position_seconds: 3500,
        watch_state: WatchState::InProgress,
        last_watched_at: 1700003500,
        play_count: 2,
    };

    let serialized = serde_json::to_string(&state).expect("serialization failed");
    let deserialized: PlaybackState =
        serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(state, deserialized);

    // Test WatchState enum values
    let ws_unwatched: WatchState = serde_json::from_str("\"unwatched\"").unwrap();
    assert_eq!(ws_unwatched, WatchState::Unwatched);
    let ws_in_progress: WatchState = serde_json::from_str("\"in_progress\"").unwrap();
    assert_eq!(ws_in_progress, WatchState::InProgress);
    let ws_completed: WatchState = serde_json::from_str("\"completed\"").unwrap();
    assert_eq!(ws_completed, WatchState::Completed);

    let session = PlaybackSession {
        session_id: "sess_xyz".to_string(),
        user_id: "usr_1".to_string(),
        media_item_id: 42,
        duration_seconds: 7200,
        current_position_seconds: 3500,
        started_at: 1700000000,
        last_heartbeat_at: 1700003500,
    };

    let session_json = serde_json::to_string(&session).expect("serialization failed");
    let deserialized_session: PlaybackSession =
        serde_json::from_str(&session_json).expect("deserialization failed");
    assert_eq!(session, deserialized_session);
}
