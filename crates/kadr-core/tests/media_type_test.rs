use kadr_core::models::MediaType;

#[test]
fn test_expanded_media_types_serde() {
    assert_eq!(serde_json::to_string(&MediaType::Anime).unwrap(), "\"anime\"");
    assert_eq!(serde_json::to_string(&MediaType::Music).unwrap(), "\"music\"");
    assert_eq!(serde_json::to_string(&MediaType::HomeVideos).unwrap(), "\"home_videos\"");
    assert_eq!(serde_json::to_string(&MediaType::Audiobook).unwrap(), "\"audiobook\"");

    assert_eq!(serde_json::from_str::<MediaType>("\"anime\"").unwrap(), MediaType::Anime);
    assert_eq!(serde_json::from_str::<MediaType>("\"music\"").unwrap(), MediaType::Music);
    assert_eq!(serde_json::from_str::<MediaType>("\"home_videos\"").unwrap(), MediaType::HomeVideos);
    assert_eq!(serde_json::from_str::<MediaType>("\"audiobook\"").unwrap(), MediaType::Audiobook);
}
