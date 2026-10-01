// crates/kadr-core/tests/subtitles_test.rs
use kadr_core::subtitles::{
    srt_to_webvtt, OnlineSubtitleMatch, SubtitleFormat, SubtitleSource, SubtitleTrack,
};

#[test]
fn test_srt_to_webvtt_header_and_empty_input() {
    assert_eq!(srt_to_webvtt(""), "WEBVTT\n");
    assert_eq!(srt_to_webvtt("   \n\r\n\t  "), "WEBVTT\n");
    assert_eq!(srt_to_webvtt("\u{FEFF}"), "WEBVTT\n");
    assert_eq!(srt_to_webvtt("\u{FEFF}   \n  "), "WEBVTT\n");
}

#[test]
fn test_srt_to_webvtt_timestamp_normalization() {
    let srt = "1\n00:01:23,456 --> 00:01:25,789\nHello world\n";
    let vtt = srt_to_webvtt(srt);

    assert!(vtt.starts_with("WEBVTT\n\n"));
    assert!(vtt.contains("00:01:23.456 --> 00:01:25.789"));
    assert_eq!(
        vtt,
        "WEBVTT\n\n1\n00:01:23.456 --> 00:01:25.789\nHello world\n"
    );
}

#[test]
fn test_srt_to_webvtt_utf8_bom_removal() {
    let srt = "\u{FEFF}1\n00:00:01,000 --> 00:00:02,000\nBOM test\n";
    let vtt = srt_to_webvtt(srt);

    assert!(!vtt.contains('\u{FEFF}'));
    assert_eq!(
        vtt,
        "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nBOM test\n"
    );
}

#[test]
fn test_srt_to_webvtt_formatting_tags() {
    let srt = "1\n00:00:01,000 --> 00:00:03,000\n<i>Italic</i> and <b>Bold</b> and <u>Underline</u>\n<font color=\"#ff0000\">Colored</font> {\\an8}Top text\n";
    let vtt = srt_to_webvtt(srt);

    assert!(vtt.contains("<i>Italic</i> and <b>Bold</b> and <u>Underline</u>"));
    assert!(vtt.contains("Colored Top text"));
    assert!(!vtt.contains("<font"));
    assert!(!vtt.contains("</font>"));
    assert!(!vtt.contains("{\\an8}"));
}

#[test]
fn test_srt_to_webvtt_multiline_cues_and_normalization() {
    let srt = "\r\n1\r\n00:00:01,234 --> 00:00:03,456\r\nLine 1\r\nLine 2\r\nLine 3\r\n\r\n\r\n2\r\n00:00:05,000 --> 00:00:07,000\r\nSecond cue line\r\n\r\n\r\n";
    let vtt = srt_to_webvtt(srt);

    let expected = "WEBVTT\n\n1\n00:00:01.234 --> 00:00:03.456\nLine 1\nLine 2\nLine 3\n\n2\n00:00:05.000 --> 00:00:07.000\nSecond cue line\n";
    assert_eq!(vtt, expected);
}

#[test]
fn test_srt_to_webvtt_without_sequence_numbers() {
    let srt = "00:00:01,000 --> 00:00:02,000\nNo sequence number cue\n";
    let vtt = srt_to_webvtt(srt);

    assert_eq!(
        vtt,
        "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nNo sequence number cue\n"
    );
}

#[test]
fn test_subtitle_source_serialization_roundtrip() {
    for (source, expected_json) in [
        (SubtitleSource::Sidecar, "\"sidecar\""),
        (SubtitleSource::Embedded, "\"embedded\""),
        (SubtitleSource::Downloaded, "\"downloaded\""),
    ] {
        let serialized = serde_json::to_string(&source).expect("serialize source");
        assert_eq!(serialized, expected_json);
        let deserialized: SubtitleSource =
            serde_json::from_str(&serialized).expect("deserialize source");
        assert_eq!(deserialized, source);
    }
}

#[test]
fn test_subtitle_format_serialization_roundtrip() {
    for (format, expected_json) in [
        (SubtitleFormat::Srt, "\"srt\""),
        (SubtitleFormat::Vtt, "\"vtt\""),
        (SubtitleFormat::Ass, "\"ass\""),
        (SubtitleFormat::Sub, "\"sub\""),
        (SubtitleFormat::Unknown, "\"unknown\""),
    ] {
        let serialized = serde_json::to_string(&format).expect("serialize format");
        assert_eq!(serialized, expected_json);
        let deserialized: SubtitleFormat =
            serde_json::from_str(&serialized).expect("deserialize format");
        assert_eq!(deserialized, format);
    }
}

#[test]
fn test_subtitle_track_serialization_roundtrip() {
    let track = SubtitleTrack {
        id: 101,
        media_item_id: 42,
        source: SubtitleSource::Sidecar,
        language: "ara".to_string(),
        title: Some("Arabic Full".to_string()),
        format: SubtitleFormat::Srt,
        file_path: Some("/media/movies/Cairo Station (1958).ara.srt".to_string()),
        stream_index: None,
        is_default: true,
        is_forced: false,
        created_at: 1700000000,
    };

    let serialized = serde_json::to_string(&track).expect("serialize track");
    let deserialized: SubtitleTrack = serde_json::from_str(&serialized).expect("deserialize track");
    assert_eq!(track, deserialized);

    // Verify embedded track with stream index
    let embedded_track = SubtitleTrack {
        id: 102,
        media_item_id: 42,
        source: SubtitleSource::Embedded,
        language: "eng".to_string(),
        title: Some("English [SDH]".to_string()),
        format: SubtitleFormat::Vtt,
        file_path: None,
        stream_index: Some(2),
        is_default: false,
        is_forced: false,
        created_at: 1700000001,
    };

    let serialized = serde_json::to_string(&embedded_track).expect("serialize track");
    let deserialized: SubtitleTrack = serde_json::from_str(&serialized).expect("deserialize track");
    assert_eq!(embedded_track, deserialized);
}

#[test]
fn test_online_subtitle_match_serialization_roundtrip() {
    let match_item = OnlineSubtitleMatch {
        id: "opensubtitles_12345".to_string(),
        language: "ar".to_string(),
        release_name: Some("Cairo.Station.1958.1080p.BluRay.x264".to_string()),
        hearing_impaired: false,
        format: "srt".to_string(),
        download_count: 5432,
        rating: Some(4.85),
    };

    let serialized = serde_json::to_string(&match_item).expect("serialize match");
    let deserialized: OnlineSubtitleMatch =
        serde_json::from_str(&serialized).expect("deserialize match");
    assert_eq!(match_item, deserialized);
}
