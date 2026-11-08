use kadr_core::models::{MediaItem, MediaMetadata};
use kadr_core::plugin::*;
use std::path::{Path, PathBuf};

#[test]
fn plugin_path_roundtrip_and_remote_id_may_contain_slashes() {
    let p = plugin_item_path("iptv", "group/ch 1");
    assert_eq!(p, PathBuf::from("plugin://iptv/group/ch 1"));
    let r = parse_plugin_path(&p).unwrap();
    assert_eq!(r.plugin_id, "iptv");
    assert_eq!(r.remote_id, "group/ch 1");
}

#[test]
fn parse_rejects_non_plugin_and_malformed_paths() {
    assert!(parse_plugin_path(Path::new("/media/movie.mkv")).is_none());
    assert!(parse_plugin_path(Path::new("plugin://iptv")).is_none());
    assert!(parse_plugin_path(Path::new("plugin://iptv/")).is_none());
    assert!(parse_plugin_path(Path::new("plugin:///abc")).is_none());
    assert!(is_plugin_path(Path::new("plugin://iptv/library/x")));
    assert!(!is_plugin_path(Path::new("/plugin://x")));
}

#[test]
fn remote_id_validation() {
    assert!(validate_remote_id("abc-123").is_ok());
    assert!(validate_remote_id("").is_err());
    assert!(validate_remote_id("bad\nid").is_err());
    assert!(validate_remote_id(&"x".repeat(257)).is_err());
}

#[test]
fn media_item_accessors() {
    let mut item = MediaItem::default();
    assert!(item.plugin_ref().is_none());
    assert!(!item.is_live());
    assert!(item.is_seekable());

    item.file_path = plugin_item_path("iptv", "ch1");
    let mut meta = MediaMetadata::default();
    meta.extra.insert("live".into(), serde_json::json!(true));
    item.metadata = meta;
    assert_eq!(item.plugin_ref().unwrap().remote_id, "ch1");
    assert!(item.is_live());
    assert!(!item.is_seekable(), "live items are never seekable");

    let mut vod = MediaItem::default();
    vod.metadata
        .extra
        .insert("seekable".into(), serde_json::json!(false));
    assert!(!vod.is_seekable());
}

#[test]
fn manifest_validation() {
    let ok = PluginManifest {
        id: "iptv".into(),
        name: "IPTV".into(),
        version: "0.1.0".into(),
        api_version: 1,
        permissions: vec![],
        capabilities: Default::default(),
    };
    assert!(ok.validate().is_ok());
    let mut bad = ok.clone();
    bad.id = "Has Space".into();
    assert!(bad.validate().is_err());
    let mut bad = ok.clone();
    bad.api_version = 2;
    assert!(bad.validate().is_err());
    let mut bad = ok;
    bad.name = "  ".into();
    assert!(bad.validate().is_err());
}

#[test]
fn descriptor_and_manifest_json_shapes() {
    let m: PluginManifest = serde_json::from_str(
        r#"{"id":"iptv","name":"IPTV","version":"0.1.0","api_version":1,
            "permissions":["items:publish","playback:resolve"],
            "capabilities":{"virtual_library":true,"playback_resolver":true}}"#,
    )
    .unwrap();
    assert_eq!(
        m.permissions,
        vec![
            PluginPermission::ItemsPublish,
            PluginPermission::PlaybackResolve
        ]
    );

    let d: PlaybackDescriptor =
        serde_json::from_str(r#"{"mode":"proxy","url":"https://x/y.mp4"}"#).unwrap();
    assert_eq!(d.mode, PlaybackMode::Proxy);
    assert!(d.headers.is_empty());
    assert_eq!(d.expires_in_secs, None);
}
