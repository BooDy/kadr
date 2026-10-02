// crates/kadr-server/tests/config_test.rs
use kadr_core::models::MediaType;
use kadr_server::config::AppConfig;
use std::path::PathBuf;

#[test]
fn test_config_parsing_from_str() {
    let toml_str = r#"
        [server]
        host = "127.0.0.1"
        port = 8096
        data_dir = "./test_data"

        [storage]
        database_path = "./test_data/kadr.db"
        max_readers = 4

        [scanner]
        debounce_millis = 500
        use_ffprobe = false

        [[libraries]]
        id = "movies"
        name = "Movies"
        path = "./test_media"
        media_type = "Movie"
    "#;

    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 8096);
    assert_eq!(config.server.data_dir, PathBuf::from("./test_data"));
    assert_eq!(
        config.storage.database_path,
        PathBuf::from("./test_data/kadr.db")
    );
    assert_eq!(config.storage.max_readers, 4);
    assert_eq!(config.scanner.debounce_millis, 500);
    assert!(!config.scanner.use_ffprobe);
    assert_eq!(config.libraries.len(), 1);
    assert_eq!(config.libraries[0].id, "movies");
    assert_eq!(config.libraries[0].name, "Movies");
    assert_eq!(config.libraries[0].path, PathBuf::from("./test_media"));
    assert_eq!(config.libraries[0].media_type, MediaType::Movie);
}

#[test]
fn test_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 8096);
    assert_eq!(config.server.data_dir, PathBuf::from("./data"));
    assert_eq!(
        config.storage.database_path,
        PathBuf::from("./data/kadr.db")
    );
    assert_eq!(config.storage.max_readers, 4);
    assert_eq!(config.scanner.debounce_millis, 500);
    assert!(config.scanner.use_ffprobe);
    assert!(config.libraries.is_empty());
}

#[test]
fn test_empty_toml_uses_defaults() {
    let config: AppConfig = toml::from_str("").unwrap();
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 8096);
    assert_eq!(config.storage.max_readers, 4);
    assert!(config.scanner.use_ffprobe);
    assert!(config.libraries.is_empty());
}

#[test]
fn test_load_from_file_and_root_kadr_toml() {
    let root_toml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../kadr.toml");
    let config = AppConfig::load_from_file(&root_toml).expect("should load root kadr.toml");
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 8096);
    assert_eq!(
        config.storage.database_path,
        PathBuf::from("./data/kadr.db")
    );
    assert_eq!(config.storage.max_readers, 4);
    assert_eq!(config.scanner.debounce_millis, 500);
    assert!(config.scanner.use_ffprobe);
}

#[test]
fn test_multiple_libraries_and_media_types() {
    let toml_str = r#"
        [[libraries]]
        id = "movies"
        name = "Movies"
        path = "/media/movies"
        media_type = "movie"

        [[libraries]]
        id = "shows"
        name = "TV Shows"
        path = "/media/shows"
        media_type = "Show"
    "#;

    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.libraries.len(), 2);
    assert_eq!(config.libraries[0].media_type, MediaType::Movie);
    assert_eq!(config.libraries[1].media_type, MediaType::Show);
}
