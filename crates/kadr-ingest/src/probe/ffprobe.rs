use crate::error::{IngestError, Result};
use crate::sidecars::subtitles::{language_display_name, normalize_language_tag};
use crate::sidecars::DiscoveredSubtitle;
use kadr_core::models::{AudioStreamInfo, TechnicalInfo};
use kadr_core::subtitles::{SubtitleFormat, SubtitleSource};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
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

fn is_generic_audio_handler_title(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    lower.is_empty()
        || lower == "soundhandler"
        || lower == "gpac iso audio handler"
        || lower == "core media audio"
        || lower == "iso media file produced by google inc."
}

pub fn parse_audio_streams_from_json(json: &Value) -> Vec<AudioStreamInfo> {
    let mut out = Vec::new();
    let Some(streams) = json["streams"].as_array() else {
        return out;
    };

    let mut audio_ordinal = 0u32;
    for stream in streams {
        if stream["codec_type"].as_str() != Some("audio") {
            continue;
        }
        let audio_index = audio_ordinal;
        audio_ordinal += 1;

        let Some(codec_name) = stream["codec_name"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
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

        let channels = stream["channels"]
            .as_u64()
            .and_then(|c| u8::try_from(c).ok());

        let raw_title = stream["tags"]["title"]
            .as_str()
            .or_else(|| stream["tags"]["TITLE"].as_str())
            .map(str::trim)
            .filter(|t| !is_generic_audio_handler_title(t));

        let is_default = disposition_flag(stream, "default");
        let is_commentary = disposition_flag(stream, "comment")
            || disposition_flag(stream, "visual_impaired")
            || raw_title.is_some_and(|t| t.to_ascii_lowercase().contains("commentary"));

        out.push(AudioStreamInfo {
            audio_index,
            stream_index,
            codec: codec_name.to_ascii_lowercase(),
            channels,
            language,
            title: raw_title.map(str::to_string),
            is_default,
            is_commentary,
        });
    }

    out
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

async fn exec_ffprobe_json(path: &Path, include_format: bool) -> Result<Value> {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "quiet", "-print_format", "json"]);
    if include_format {
        cmd.arg("-show_format");
    }
    cmd.arg("-show_streams")
        .arg(path)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let output = tokio::time::timeout(Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| {
            tracing::warn!("ffprobe timed out after 15s for {}", path.display());
            IngestError::ParseError(format!("ffprobe timed out for {}", path.display()))
        })??;

    if !output.status.success() {
        return Err(IngestError::ParseError(format!(
            "ffprobe exited with status {} for {}",
            output.status,
            path.display()
        )));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| IngestError::ParseError(format!("Failed to parse ffprobe JSON: {e}")))
}

pub async fn run_ffprobe_full<P: AsRef<Path>>(path: P) -> Result<Option<FfprobeOutput>> {
    let json = match exec_ffprobe_json(path.as_ref(), true).await {
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
            }
        }
    }

    info.audio_tracks = parse_audio_streams_from_json(&json);
    let primary_audio = info
        .audio_tracks
        .iter()
        .find(|t| t.is_default)
        .or_else(|| info.audio_tracks.first());
    info.audio_codec = primary_audio.map(|t| t.codec.clone());
    info.audio_channels = primary_audio.and_then(|t| t.channels);

    let embedded_subtitles = parse_embedded_subtitles_from_json(&json);

    Ok(Some(FfprobeOutput {
        technical: info,
        embedded_subtitles,
    }))
}

pub async fn run_ffprobe<P: AsRef<Path>>(path: P) -> Result<Option<TechnicalInfo>> {
    Ok(run_ffprobe_full(path).await?.map(|out| out.technical))
}

pub async fn probe_audio_streams<P: AsRef<Path>>(path: P) -> Result<Vec<AudioStreamInfo>> {
    let json = exec_ffprobe_json(path.as_ref(), false).await?;
    Ok(parse_audio_streams_from_json(&json))
}

pub async fn probe_embedded_subtitles<P: AsRef<Path>>(path: P) -> Result<Vec<DiscoveredSubtitle>> {
    Ok(run_ffprobe_full(path)
        .await?
        .map(|out| out.embedded_subtitles)
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::parse_audio_streams_from_json;
    use serde_json::json;

    #[test]
    fn test_parse_audio_streams_from_json_multi_audio_and_dispositions() {
        let sample = json!({
            "streams": [
                {
                    "index": 0,
                    "codec_type": "video",
                    "codec_name": "h264",
                    "width": 1920,
                    "height": 1080
                },
                {
                    "index": 1,
                    "codec_type": "audio",
                    "codec_name": "aac",
                    "channels": 2,
                    "disposition": { "default": 1, "comment": 0 },
                    "tags": { "language": "ja", "title": "SoundHandler" }
                },
                {
                    "index": 2,
                    "codec_type": "audio",
                    "codec_name": "dts",
                    "channels": 6,
                    "disposition": { "default": 0, "comment": 0 },
                    "tags": { "language": "en", "title": "English 5.1" }
                },
                {
                    "index": 3,
                    "codec_type": "audio",
                    "codec_name": "aac",
                    "channels": 2,
                    "disposition": { "default": 0, "comment": 1 },
                    "tags": { "language": "eng", "title": "Cast Chat" }
                },
                {
                    "index": 4,
                    "codec_type": "audio",
                    "codec_name": "ac3",
                    "channels": 2,
                    "disposition": { "default": 0, "comment": 0 },
                    "tags": { "language": "eng", "title": "Director's Commentary" }
                },
                {
                    "index": 5,
                    "codec_type": "subtitle",
                    "codec_name": "subrip",
                    "tags": { "language": "eng" }
                }
            ]
        });

        let tracks = parse_audio_streams_from_json(&sample);
        assert_eq!(tracks.len(), 4);

        assert_eq!(tracks[0].audio_index, 0);
        assert_eq!(tracks[0].stream_index, 1);
        assert_eq!(tracks[0].codec, "aac");
        assert_eq!(tracks[0].channels, Some(2));
        assert_eq!(tracks[0].language, "jpn");
        assert_eq!(tracks[0].title, None);
        assert!(tracks[0].is_default);
        assert!(!tracks[0].is_commentary);

        assert_eq!(tracks[1].audio_index, 1);
        assert_eq!(tracks[1].stream_index, 2);
        assert_eq!(tracks[1].codec, "dts");
        assert_eq!(tracks[1].channels, Some(6));
        assert_eq!(tracks[1].language, "eng");
        assert_eq!(tracks[1].title.as_deref(), Some("English 5.1"));
        assert!(!tracks[1].is_default);
        assert!(!tracks[1].is_commentary);

        assert_eq!(tracks[2].audio_index, 2);
        assert_eq!(tracks[2].stream_index, 3);
        assert_eq!(tracks[2].codec, "aac");
        assert_eq!(tracks[2].channels, Some(2));
        assert_eq!(tracks[2].language, "eng");
        assert_eq!(tracks[2].title.as_deref(), Some("Cast Chat"));
        assert!(!tracks[2].is_default);
        assert!(tracks[2].is_commentary);

        assert_eq!(tracks[3].audio_index, 3);
        assert_eq!(tracks[3].stream_index, 4);
        assert_eq!(tracks[3].codec, "ac3");
        assert_eq!(tracks[3].language, "eng");
        assert_eq!(tracks[3].title.as_deref(), Some("Director's Commentary"));
        assert!(!tracks[3].is_default);
        assert!(tracks[3].is_commentary);
    }
}
