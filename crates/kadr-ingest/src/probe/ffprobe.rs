use crate::error::Result;
use kadr_core::models::TechnicalInfo;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub async fn run_ffprobe<P: AsRef<Path>>(path: P) -> Result<Option<TechnicalInfo>> {
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

    Ok(Some(info))
}
