use std::time::{SystemTime, UNIX_EPOCH};

use kadr_core::ast::CardViewModel;
use kadr_core::models::{MediaItem, MediaType, PlaybackState, WatchState};

/// Normalizes a `MediaItem` and optional `PlaybackState` into a client-ready `CardViewModel`.
pub fn to_card_view_model(item: &MediaItem, playback: Option<&PlaybackState>) -> CardViewModel {
    let id = item.id.unwrap_or(0);
    let title = item.title.clone();

    // Subtitle normalization:
    // If Episode: check series_title, season, episode. Format e.g. "S01E02" or "S01E02 - Title".
    // Else (Movie/Video): format "{year} • {resolution}" or "{year}".
    let subtitle = match item.item_type {
        MediaType::Episode => {
            let ep_code = match (item.metadata.season, item.metadata.episode) {
                (Some(s), Some(e)) => Some(format!("S{:02}E{:02}", s, e)),
                (Some(s), None) => Some(format!("S{:02}", s)),
                (None, Some(e)) => Some(format!("E{:02}", e)),
                (None, None) => None,
            };

            match (&item.metadata.series_title, ep_code) {
                (Some(series), Some(code)) if !series.trim().is_empty() => {
                    Some(format!("{code} - {series}"))
                }
                (None, Some(code)) => Some(code),
                (Some(series), None) if !series.trim().is_empty() => Some(series.clone()),
                _ => None,
            }
        }
        _ => {
            let year = item.release_year;
            let resolution = item
                .technical
                .resolution
                .as_deref()
                .filter(|r| !r.trim().is_empty());

            match (year, resolution) {
                (Some(y), Some(r)) => Some(format!("{y} • {r}")),
                (Some(y), None) => Some(format!("{y}")),
                (None, Some(r)) => Some(r.to_string()),
                (None, None) => None,
            }
        }
    };

    let poster_url = item
        .metadata
        .poster_path
        .as_ref()
        .filter(|p| !p.trim().is_empty())
        .map(|_| format!("/api/v1/artwork/{id}/poster"));

    let backdrop_url = item
        .metadata
        .backdrop_path
        .as_ref()
        .filter(|p| !p.trim().is_empty())
        .map(|_| format!("/api/v1/artwork/{id}/backdrop"));

    let media_type = match item.item_type {
        MediaType::Movie => "movie",
        MediaType::Show => "show",
        MediaType::Season => "season",
        MediaType::Episode => "episode",
        MediaType::Unknown => "unknown",
    }
    .to_string();

    let playback_progress = if let Some(p) = playback {
        if item.technical.duration_seconds > 0 {
            Some(
                (p.playback_position_seconds as f32 / item.technical.duration_seconds as f32)
                    .clamp(0.0, 1.0),
            )
        } else {
            None
        }
    } else {
        None
    };

    let rating = item.metadata.rating;
    let release_year = item
        .release_year
        .and_then(|y| if y > 0 { Some(y as u32) } else { None });

    // Badge calculation:
    // - If playback has WatchState::InProgress: "RESUME"
    // - Else if item added within last 14 days: "NEW"
    // - Else if resolution contains "4K" or "2160": "4K"
    // - Else: None
    let badge = if let Some(p) = playback {
        if p.watch_state == WatchState::InProgress {
            Some("RESUME".to_string())
        } else {
            calculate_item_badge(item)
        }
    } else {
        calculate_item_badge(item)
    };

    CardViewModel {
        id,
        title,
        subtitle,
        poster_url,
        backdrop_url,
        media_type,
        playback_progress,
        rating,
        release_year,
        badge,
    }
}

fn calculate_item_badge(item: &MediaItem) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // 14 days in seconds = 14 * 86400 = 1_209_600
    if item.added_at > 0 && now >= item.added_at && (now - item.added_at) <= 14 * 86400 {
        Some("NEW".to_string())
    } else if let Some(res) = &item.technical.resolution {
        let upper = res.to_ascii_uppercase();
        if upper.contains("4K") || upper.contains("2160") {
            Some("4K".to_string())
        } else {
            None
        }
    } else {
        None
    }
}
