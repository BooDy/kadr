use crate::models::TechnicalInfo;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoAction {
    Copy,
    Transcode {
        codec: String,
        max_height: Option<u32>,
        hwaccel: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioAction {
    Copy,
    Transcode {
        codec: String,
        channels: u32,
        bitrate_kbps: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscodeParameters {
    pub video_codec: String,
    pub audio_codec: String,
    pub max_height: Option<u32>,
    pub audio_track_index: u32,
    pub start_time: f64,
}

impl Default for TranscodeParameters {
    fn default() -> Self {
        Self {
            video_codec: "h264".to_string(),
            audio_codec: "aac".to_string(),
            max_height: None,
            audio_track_index: 0,
            start_time: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscodeDecision {
    pub video: VideoAction,
    pub audio: AudioAction,
}

pub fn evaluate_transcode(
    tech: &TechnicalInfo,
    params: &TranscodeParameters,
    hwaccel: Option<&str>,
) -> TranscodeDecision {
    let source_video_codec = tech
        .video_codec
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let target_video_codec = params.video_codec.to_lowercase();

    let height_ok = match (tech.height(), params.max_height) {
        (Some(src_h), Some(max_h)) => src_h <= max_h,
        _ => true,
    };

    let video_action = if source_video_codec == target_video_codec && height_ok {
        VideoAction::Copy
    } else {
        VideoAction::Transcode {
            codec: target_video_codec,
            max_height: params.max_height,
            hwaccel: hwaccel.map(|s| s.to_string()),
        }
    };

    let source_audio_codec = tech
        .audio_codec
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let target_audio_codec = params.audio_codec.to_lowercase();

    let audio_action = if source_audio_codec == target_audio_codec {
        AudioAction::Copy
    } else {
        AudioAction::Transcode {
            codec: target_audio_codec,
            channels: 2,
            bitrate_kbps: 192,
        }
    };

    TranscodeDecision {
        video: video_action,
        audio: audio_action,
    }
}

pub fn build_ffmpeg_hls_args(
    input_path: &Path,
    output_dir: &Path,
    decision: &TranscodeDecision,
    params: &TranscodeParameters,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];

    if params.start_time > 0.0 {
        args.push("-ss".into());
        args.push(format!("{:.3}", params.start_time));
    }

    args.push("-i".into());
    args.push(input_path.to_string_lossy().to_string());

    // Stream mapping
    args.push("-map".into());
    args.push("0:v:0".into());
    args.push("-map".into());
    args.push(format!("0:a:{}", params.audio_track_index));

    // Video codec flags
    match &decision.video {
        VideoAction::Copy => {
            args.push("-c:v".into());
            args.push("copy".into());
        }
        VideoAction::Transcode {
            max_height,
            hwaccel,
            ..
        } => {
            if let Some(scale_h) = max_height {
                args.push("-vf".into());
                args.push(format!("scale=-2:'min(ih,{})'", scale_h));
            }
            match hwaccel.as_deref() {
                Some("vaapi") => {
                    args.push("-c:v".into());
                    args.push("h264_vaapi".into());
                }
                Some("nvenc") => {
                    args.push("-c:v".into());
                    args.push("h264_nvenc".into());
                }
                _ => {
                    args.push("-c:v".into());
                    args.push("libx264".into());
                    args.push("-preset".into());
                    args.push("veryfast".into());
                    args.push("-crf".into());
                    args.push("23".into());
                    args.push("-pix_fmt".into());
                    args.push("yuv420p".into());
                }
            }
        }
    }

    // Audio codec flags
    match &decision.audio {
        AudioAction::Copy => {
            args.push("-c:a".into());
            args.push("copy".into());
        }
        AudioAction::Transcode {
            codec,
            channels,
            bitrate_kbps,
        } => {
            args.push("-c:a".into());
            args.push(codec.clone());
            args.push("-b:a".into());
            args.push(format!("{}k", bitrate_kbps));
            args.push("-ac".into());
            args.push(channels.to_string());
        }
    }

    // Disable embedded subtitles
    args.push("-sn".into());

    // HLS packaging options
    args.push("-f".into());
    args.push("hls".into());
    args.push("-hls_time".into());
    args.push("4".into());
    args.push("-hls_playlist_type".into());
    args.push("event".into());
    args.push("-hls_flags".into());
    args.push("independent_segments".into());
    args.push("-hls_segment_filename".into());
    args.push(
        output_dir
            .join("segment_%04d.ts")
            .to_string_lossy()
            .to_string(),
    );
    args.push(
        output_dir
            .join("playlist.m3u8")
            .to_string_lossy()
            .to_string(),
    );

    args
}
