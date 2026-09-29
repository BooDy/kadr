pub mod ffprobe;
pub mod pure_rust;

use crate::error::Result;
use kadr_core::models::TechnicalInfo;
use std::path::Path;

pub struct TechnicalProber {
    enable_ffprobe: bool,
}

impl TechnicalProber {
    pub fn new(enable_ffprobe: bool) -> Self {
        Self { enable_ffprobe }
    }

    pub async fn probe<P: AsRef<Path>>(&self, path: P) -> Result<TechnicalInfo> {
        let mut base_info = pure_rust::inspect_container(&path)?;

        if self.enable_ffprobe {
            if let Ok(Some(probed)) = ffprobe::run_ffprobe(&path).await {
                if probed.duration_seconds > 0 {
                    base_info.duration_seconds = probed.duration_seconds;
                }
                if probed.resolution.is_some() {
                    base_info.resolution = probed.resolution;
                }
                if probed.video_codec.is_some() {
                    base_info.video_codec = probed.video_codec;
                }
                if probed.audio_codec.is_some() {
                    base_info.audio_codec = probed.audio_codec;
                }
                if probed.audio_channels.is_some() {
                    base_info.audio_channels = probed.audio_channels;
                }
            }
        }

        Ok(base_info)
    }
}
