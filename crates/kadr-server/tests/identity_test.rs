use kadr_server::identity::{load_or_create_server_id, ServerIdentity};
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn test_load_or_create_server_id_creates_new_uuid_when_empty() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data_subdir"); // test nested dir creation as well

    let server_id = load_or_create_server_id(&data_dir).expect("should create server_id");
    let parsed = Uuid::parse_str(&server_id).expect("should be valid UUID");
    assert_eq!(parsed.get_version(), Some(uuid::Version::Random));

    let file_path = data_dir.join("server_id");
    assert!(file_path.is_file());
    let saved_content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(saved_content.trim(), server_id);

    let identity = ServerIdentity {
        id: server_id.clone(),
    };
    assert_eq!(identity.id, server_id);
}

#[test]
fn test_load_or_create_server_id_reads_existing_uuid() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path();
    let file_path = data_dir.join("server_id");
    let existing_uuid = "123e4567-e89b-12d3-a456-426614174000";
    std::fs::write(&file_path, format!("  {}\n  ", existing_uuid)).unwrap();

    let server_id = load_or_create_server_id(data_dir).expect("should read existing server_id");
    assert_eq!(server_id, existing_uuid);

    // Verify it didn't overwrite the file
    let saved_content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(saved_content, format!("  {}\n  ", existing_uuid));
}
