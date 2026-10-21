use kadr_core::models::TechnicalInfo;
use kadr_core::transcoding::{
    build_ffmpeg_hls_args, evaluate_transcode, AudioAction, TranscodeParameters, VideoAction,
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
