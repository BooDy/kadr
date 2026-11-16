use crate::error::Result;
use crate::sidecars::subtitles::{language_display_name, normalize_language_tag};
use crate::sidecars::DiscoveredSubtitle;
use kadr_core::models::TechnicalInfo;
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct FfprobeOutput {
    pub technical: TechnicalInfo,
    pub embedded_subtitles: Vec<DiscoveredSubtitle>,
}

pub type ProbedMedia = FfprobeOutput;

fn disposition_flag(stream: &Value, key: &str) -> bool {
    stream["disposition"][key].as_i64() == Some(1)
        || stream["disposition"][key].as_str() == Some("1")
}

fn map_subtitle_codec(codec_name: &str) -> Option<SubtitleFormat> {
    match codec_name.trim().to_ascii_lowercase().as_str() {
        "subrip" | "srt" | "text" | "mov_text" | "tx3g" => Some(SubtitleFormat::Srt),
        "ass" | "ssa" => Some(SubtitleFormat::Ass),
        "webvtt" | "vtt" => Some(SubtitleFormat::Vtt),
        _ => None,
    }
}

fn is_generic_handler_title(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    lower.is_empty()
        || lower == "subtitlehandler"
        || lower == "gpac streaming text handler"
        || lower == "sbtl"
        || lower == "core media subtitle"
}

pub fn parse_embedded_subtitles_from_json(json: &Value) -> Vec<DiscoveredSubtitle> {
    let mut out = Vec::new();
    let Some(streams) = json["streams"].as_array() else {
        return out;
    };

    for stream in streams {
        if stream["codec_type"].as_str() != Some("subtitle") {
            continue;
        }
        let Some(codec_name) = stream["codec_name"].as_str() else {
            continue;
        };
        let Some(format) = map_subtitle_codec(codec_name) else {
            continue;
        };
        let Some(stream_index) = stream["index"].as_u64().and_then(|i| u32::try_from(i).ok())
        else {
            continue;
        };

        let raw_lang = stream["tags"]["language"]
            .as_str()
            .or_else(|| stream["tags"]["LANGUAGE"].as_str())
            .unwrap_or("und");
        let language = normalize_language_tag(raw_lang);

        let is_default = disposition_flag(stream, "default");
        let is_forced = disposition_flag(stream, "forced");
        let is_sdh = disposition_flag(stream, "hearing_impaired");

        let raw_title = stream["tags"]["title"]
            .as_str()
            .or_else(|| stream["tags"]["TITLE"].as_str())
            .map(str::trim)
            .filter(|t| !is_generic_handler_title(t));

        let title = if let Some(explicit) = raw_title {
            explicit.to_string()
        } else {
            let base = if language == "und" {
                format!("Track {}", stream_index)
            } else {
                match language_display_name(&language) {
                    "" | "Undetermined" => language.to_ascii_uppercase(),
                    name => name.to_string(),
                }
            };
            let mut label = base;
            if is_sdh {
                label.push_str(" [SDH]");
            }
            if is_forced {
                label.push_str(" [Forced]");
            }
            label
        };

        out.push(DiscoveredSubtitle {
            source: SubtitleSource::Embedded,
            language,
            title: Some(title),
            format,
            file_path: None,
            stream_index: Some(stream_index),
            is_default,
            is_forced,
        });
    }

    out
}

pub async fn run_ffprobe_full<P: AsRef<Path>>(path: P) -> Result<Option<FfprobeOutput>> {
    let output = match Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path.as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        Ok(out) if out.status.success() => out,
        _ => return Ok(None),
    };

    let json: Value = match serde_json::from_slice(&output.stdout) {
        Ok(j) => j,
        Err(_) => return Ok(None),
    };

    let mut info = TechnicalInfo::default();

    if let Some(duration_str) = json["format"]["duration"].as_str() {
        if let Ok(secs) = duration_str.parse::<f64>() {
            info.duration_seconds = secs.round() as i64;
        }
    }

    if let Some(streams) = json["streams"].as_array() {
        for stream in streams {
            let codec_type = stream["codec_type"].as_str().unwrap_or("");
            let codec_name = stream["codec_name"].as_str().map(|s| s.to_string());

            if codec_type == "video" && info.video_codec.is_none() {
                let is_attached_pic = stream["disposition"]["attached_pic"].as_i64() == Some(1)
                    || stream["disposition"]["attached_pic"].as_str() == Some("1");
                if is_attached_pic {
                    continue;
                }

                info.video_codec = codec_name;
                let width = stream["width"].as_i64().unwrap_or(0);
                let height = stream["height"].as_i64().unwrap_or(0);
                if width >= 3800 || height >= 2000 {
                    info.resolution = Some("4k".to_string());
                } else if width >= 1900 || height >= 1000 {
                    info.resolution = Some("1080p".to_string());
                } else if width >= 1200 || height >= 700 {
                    info.resolution = Some("720p".to_string());
                } else if height > 0 {
                    info.resolution = Some("480p".to_string());
                }
            } else if codec_type == "audio" && info.audio_codec.is_none() {
                info.audio_codec = codec_name;
                info.audio_channels = stream["channels"]
                    .as_u64()
                    .and_then(|c| u8::try_from(c).ok());
            }
        }
    }

    let embedded_subtitles = parse_embedded_subtitles_from_json(&json);

    Ok(Some(FfprobeOutput {
        technical: info,
        embedded_subtitles,
    }))
}

pub async fn run_ffprobe<P: AsRef<Path>>(path: P) -> Result<Option<TechnicalInfo>> {
    Ok(run_ffprobe_full(path).await?.map(|out| out.technical))
}

pub async fn probe_embedded_subtitles<P: AsRef<Path>>(path: P) -> Result<Vec<DiscoveredSubtitle>> {
    Ok(run_ffprobe_full(path)
        .await?
        .map(|out| out.embedded_subtitles)
        .unwrap_or_default())
}
