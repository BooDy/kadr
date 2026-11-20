pub mod ffprobe;
pub mod pure_rust;

pub use ffprobe::{
    parse_audio_streams_from_json, parse_embedded_subtitles_from_json, probe_audio_streams,
    probe_embedded_subtitles, run_ffprobe, run_ffprobe_full, FfprobeOutput, ProbedMedia,
};

use crate::error::Result;
use crate::sidecars::DiscoveredSubtitle;
use kadr_core::models::TechnicalInfo;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TechnicalProber {
    enable_ffprobe: bool,
}

impl TechnicalProber {
    pub fn new(enable_ffprobe: bool) -> Self {
        Self { enable_ffprobe }
    }

    pub async fn probe<P: AsRef<Path>>(&self, path: P) -> Result<TechnicalInfo> {
        let (info, _) = self.probe_with_subtitles(path).await?;
        Ok(info)
    }

    pub async fn probe_with_subtitles<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<(TechnicalInfo, Vec<DiscoveredSubtitle>)> {
        let mut base_info = pure_rust::inspect_container(&path)?;
        let mut embedded_subtitles = Vec::new();

        if self.enable_ffprobe {
            if let Ok(Some(probed)) = ffprobe::run_ffprobe_full(&path).await {
                if probed.technical.duration_seconds > 0 {
                    base_info.duration_seconds = probed.technical.duration_seconds;
                }
                if probed.technical.resolution.is_some() {
                    base_info.resolution = probed.technical.resolution;
                }
                if probed.technical.video_codec.is_some() {
                    base_info.video_codec = probed.technical.video_codec;
                }
                if probed.technical.audio_codec.is_some() {
                    base_info.audio_codec = probed.technical.audio_codec;
                }
                if probed.technical.audio_channels.is_some() {
                    base_info.audio_channels = probed.technical.audio_channels;
                }
                if !probed.technical.audio_tracks.is_empty() {
                    base_info.audio_tracks = probed.technical.audio_tracks;
                }
                embedded_subtitles = probed.embedded_subtitles;
            }
        }

        Ok((base_info, embedded_subtitles))
    }
}
