use kadr_core::models::TechnicalInfo;
use kadr_core::transcoding::{
    build_ffmpeg_hls_args, evaluate_transcode, AudioAction, TranscodeDecision, TranscodeParameters,
    VideoAction,
};
use std::path::Path;

#[test]
fn test_evaluate_transcode_audio_only_remux() {
    let tech = TechnicalInfo {
        container: Some("mkv".to_string()),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("dts".to_string()),
        resolution: Some("1080p".to_string()),
        duration_seconds: 3600,
        ..Default::default()
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: Some(1080),
        audio_track_index: 0,
        start_time: 0.0,
    };

    let decision = evaluate_transcode(&tech, &params, None);
    assert_eq!(decision.video, VideoAction::Copy);
    match decision.audio {
        AudioAction::Transcode { codec, .. } => assert_eq!(codec, "aac"),
        _ => panic!("Expected audio transcode to aac"),
    }
}

#[test]
fn test_evaluate_transcode_full_video_reencode_for_hevc() {
    let tech = TechnicalInfo {
        container: Some("mkv".to_string()),
        video_codec: Some("hevc".to_string()),
        audio_codec: Some("aac".to_string()),
        resolution: Some("4k".to_string()),
        duration_seconds: 3600,
        ..Default::default()
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: Some(1080),
        audio_track_index: 0,
        start_time: 15.0,
    };

    let decision = evaluate_transcode(&tech, &params, None);
    match decision.video {
        VideoAction::Transcode { max_height, .. } => assert_eq!(max_height, Some(1080)),
        _ => panic!("Expected full video transcode"),
    }
    assert_eq!(decision.audio, AudioAction::Copy);
}

#[test]
fn test_evaluate_transcode_downscale_same_codec() {
    let tech = TechnicalInfo {
        container: Some("mkv".to_string()),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        resolution: Some("4k".to_string()),
        duration_seconds: 3600,
        ..Default::default()
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: Some(1080),
        audio_track_index: 0,
        start_time: 0.0,
    };

    let decision = evaluate_transcode(&tech, &params, None);
    match decision.video {
        VideoAction::Transcode { max_height, .. } => assert_eq!(max_height, Some(1080)),
        _ => panic!("Expected video downscale transcode"),
    }
    assert_eq!(decision.audio, AudioAction::Copy);
}

#[test]
fn test_build_ffmpeg_hls_args_construction() {
    let decision = kadr_core::transcoding::TranscodeDecision {
        video: VideoAction::Copy,
        audio: AudioAction::Transcode {
            codec: "aac".to_string(),
            channels: 2,
            bitrate_kbps: 192,
        },
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: None,
        audio_track_index: 1,
        start_time: 42.5,
    };

    let args = build_ffmpeg_hls_args(
        Path::new("/media/movie.mkv"),
        Path::new("/tmp/hls_test"),
        &decision,
        &params,
    );

    assert!(args.contains(&"-ss".to_string()));
    assert!(args.contains(&"42.500".to_string()));
    assert!(args.contains(&"-c:v".to_string()));
    assert!(args.contains(&"copy".to_string()));
    assert!(args.contains(&"-c:a".to_string()));
    assert!(args.contains(&"aac".to_string()));
    assert!(args.contains(&"independent_segments".to_string()));
}

#[test]
fn test_build_ffmpeg_hls_args_hwaccel_vaapi() {
    let decision = kadr_core::transcoding::TranscodeDecision {
        video: VideoAction::Transcode {
            codec: "h264".to_string(),
            max_height: Some(720),
            hwaccel: Some("vaapi".to_string()),
        },
        audio: AudioAction::Copy,
    };
    let params = TranscodeParameters {
        video_codec: "h264".to_string(),
        audio_codec: "aac".to_string(),
        max_height: Some(720),
        audio_track_index: 0,
        start_time: 0.0,
    };

    let args = build_ffmpeg_hls_args(
        Path::new("/media/movie.mkv"),
        Path::new("/tmp/hls_test"),
        &decision,
        &params,
    );

    assert!(args.contains(&"-c:v".to_string()));
    assert!(args.contains(&"h264_vaapi".to_string()));
    assert!(args.contains(&"-vf".to_string()));
    assert!(args.contains(&"scale=-2:'min(ih,720)'".to_string()));
    assert!(!args.contains(&"-ss".to_string()));
}

#[test]
fn test_build_ffmpeg_hls_args_hwaccel_nvenc() {
    let decision = kadr_core::transcoding::TranscodeDecision {
        video: VideoAction::Transcode {
            codec: "h264".to_string(),
            max_height: None,
            hwaccel: Some("nvenc".to_string()),
        },
        audio: AudioAction::Copy,
    };
    let params = TranscodeParameters::default();

    let args = build_ffmpeg_hls_args(
        Path::new("/media/movie.mkv"),
        Path::new("/tmp/hls_test"),
        &decision,
        &params,
    );

    assert!(args.contains(&"-c:v".to_string()));
    assert!(args.contains(&"h264_nvenc".to_string()));
}

#[test]
fn test_technical_info_height_split_once() {
    let tech = TechnicalInfo {
        resolution: Some("720x480".to_string()),
        ..Default::default()
    };
    assert_eq!(tech.height(), Some(480));
}

#[test]
fn test_build_args_for_url_source_adds_whitelist_and_headers_before_input() {
    use kadr_core::plugin::PlaybackSource;
    use kadr_core::transcoding::build_ffmpeg_hls_args_for_source;
    use std::collections::BTreeMap;

    let decision = TranscodeDecision {
        video: VideoAction::Copy,
        audio: AudioAction::Copy,
    };
    let params = TranscodeParameters {
        video_codec: "h264".into(),
        audio_codec: "aac".into(),
        max_height: None,
        audio_track_index: 0,
        start_time: 0.0,
    };
    let mut headers = BTreeMap::new();
    headers.insert("X-Token".to_string(), "abc".to_string());
    let source = PlaybackSource::Url {
        url: "https://example.com/live.ts".into(),
        headers,
    };

    let args = build_ffmpeg_hls_args_for_source(
        &source,
        std::path::Path::new("/tmp/out"),
        &decision,
        &params,
    );

    let i = args.iter().position(|a| a == "-i").unwrap();
    assert_eq!(args[i + 1], "https://example.com/live.ts");
    let wl = args
        .iter()
        .position(|a| a == "-protocol_whitelist")
        .unwrap();
    assert!(
        wl < i,
        "protocol whitelist is an input option and must precede -i"
    );
    assert_eq!(args[wl + 1], "http,https,tcp,tls,crypto");
    let h = args.iter().position(|a| a == "-headers").unwrap();
    assert!(h < i);
    assert_eq!(args[h + 1], "X-Token: abc\r\n");
}

#[test]
fn test_file_source_wrapper_matches_path_function() {
    use kadr_core::plugin::PlaybackSource;
    use kadr_core::transcoding::build_ffmpeg_hls_args_for_source;
    let decision = TranscodeDecision {
        video: VideoAction::Copy,
        audio: AudioAction::Copy,
    };
    let params = TranscodeParameters {
        video_codec: "h264".into(),
        audio_codec: "aac".into(),
        max_height: None,
        audio_track_index: 0,
        start_time: 0.0,
    };
    let path = std::path::Path::new("/media/a.mkv");
    let out = std::path::Path::new("/tmp/out");
    assert_eq!(
        build_ffmpeg_hls_args(path, out, &decision, &params),
        build_ffmpeg_hls_args_for_source(
            &PlaybackSource::File(path.to_path_buf()),
            out,
            &decision,
            &params
        )
    );
    assert!(!build_ffmpeg_hls_args(path, out, &decision, &params)
        .contains(&"-protocol_whitelist".to_string()));
}
