use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::models::{MediaItem, MediaType, PlaybackState, WatchState};

pub const MAX_FILTER_AST_DEPTH: usize = 5;
pub const MAX_FILTER_AST_RULES: usize = 32;
pub const MAX_FILTER_LIST_LEN: usize = 32;
pub const MAX_FILTER_TEXT_LEN: usize = 200;
pub const MAX_FILTER_DAYS: u32 = 36_500;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FilterValidationError {
    #[error("Filter tree exceeds maximum nesting depth of {MAX_FILTER_AST_DEPTH}")]
    MaxDepthExceeded,
    #[error("Filter tree must contain between 1 and {MAX_FILTER_AST_RULES} rules (found {0})")]
    RuleCountOutOfBounds(usize),
    #[error("Filter group cannot have empty children")]
    EmptyGroup,
    #[error("Filter text value cannot be empty or exceed {MAX_FILTER_TEXT_LEN} characters")]
    InvalidTextValue,
    #[error("Filter value list must contain between 1 and {MAX_FILTER_LIST_LEN} items")]
    InvalidListLength,
    #[error("Invalid numeric or time range: min cannot exceed max")]
    InvalidRange,
    #[error("Invalid float value: must be finite and within bounds")]
    InvalidFloatBounds,
    #[error("Invalid days window: must be between 1 and {MAX_FILTER_DAYS}")]
    InvalidDaysBounds,
    #[error("Duration seconds must be non-negative")]
    NegativeDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterNode {
    Group {
        op: LogicalOp,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
        children: Vec<FilterNode>,
    },
    Rule {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
        condition: FilterCondition,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOp {
    In,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TextMatch {
    Eq { value: String },
    In { values: Vec<String> },
    Contains { value: String },
    StartsWith { value: String },
    IsEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ArrayMatch {
    ContainsAny { values: Vec<String> },
    ContainsAll { values: Vec<String> },
    IsEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum IntComparison {
    Eq { value: i64 },
    Gte { value: i64 },
    Lte { value: i64 },
    Between { min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FloatComparison {
    Gte { value: f32 },
    Lte { value: f32 },
    Between { min: f32, max: f32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TimeComparison {
    WithinLastDays { days: u32 },
    OlderThanDays { days: u32 },
    AfterEpoch { timestamp: i64 },
    BeforeEpoch { timestamp: i64 },
    BetweenEpochs { min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WatchStateMatch {
    StateIn { values: Vec<WatchState> },
    ProgressBetween { min: f32, max: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FilterCondition {
    LibraryId {
        op: SetOp,
        values: Vec<String>,
    },
    ItemType {
        op: SetOp,
        values: Vec<MediaType>,
    },

    Title {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    OriginalTitle {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    SeriesTitle {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    Director {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    Studio {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    Country {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    Language {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    Resolution {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    VideoCodec {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    AudioCodec {
        #[serde(flatten)]
        matcher: TextMatch,
    },
    Container {
        #[serde(flatten)]
        matcher: TextMatch,
    },

    Genres {
        #[serde(flatten)]
        matcher: ArrayMatch,
    },
    Tags {
        #[serde(flatten)]
        matcher: ArrayMatch,
    },
    Actors {
        #[serde(flatten)]
        matcher: ArrayMatch,
    },
    Writers {
        #[serde(flatten)]
        matcher: ArrayMatch,
    },

    ReleaseYear {
        #[serde(flatten)]
        cmp: IntComparison,
    },
    DurationSeconds {
        #[serde(flatten)]
        cmp: IntComparison,
    },
    AudioChannels {
        #[serde(flatten)]
        cmp: IntComparison,
    },
    PlayCount {
        #[serde(flatten)]
        cmp: IntComparison,
    },
    Rating {
        #[serde(flatten)]
        cmp: FloatComparison,
    },

    AddedAt {
        #[serde(flatten)]
        cmp: TimeComparison,
    },
    LastWatchedAt {
        #[serde(flatten)]
        cmp: TimeComparison,
    },

    WatchState {
        #[serde(flatten)]
        matcher: WatchStateMatch,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogFacets {
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub directors: Vec<String>,
    pub tags: Vec<String>,
    pub resolutions: Vec<String>,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub containers: Vec<String>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
}

fn validate_text_str(s: &str) -> Result<(), FilterValidationError> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > MAX_FILTER_TEXT_LEN {
        return Err(FilterValidationError::InvalidTextValue);
    }
    Ok(())
}

fn validate_string_list(values: &[String]) -> Result<(), FilterValidationError> {
    if values.is_empty() || values.len() > MAX_FILTER_LIST_LEN {
        return Err(FilterValidationError::InvalidListLength);
    }
    for v in values {
        validate_text_str(v)?;
    }
    Ok(())
}

fn validate_text_match(m: &TextMatch) -> Result<(), FilterValidationError> {
    match m {
        TextMatch::Eq { value }
        | TextMatch::Contains { value }
        | TextMatch::StartsWith { value } => validate_text_str(value),
        TextMatch::In { values } => validate_string_list(values),
        TextMatch::IsEmpty => Ok(()),
    }
}

fn validate_array_match(m: &ArrayMatch) -> Result<(), FilterValidationError> {
    match m {
        ArrayMatch::ContainsAny { values } | ArrayMatch::ContainsAll { values } => {
            validate_string_list(values)
        }
        ArrayMatch::IsEmpty => Ok(()),
    }
}

fn validate_int_cmp(c: &IntComparison, non_negative: bool) -> Result<(), FilterValidationError> {
    match c {
        IntComparison::Eq { value }
        | IntComparison::Gte { value }
        | IntComparison::Lte { value } => {
            if non_negative && *value < 0 {
                return Err(FilterValidationError::NegativeDuration);
            }
            Ok(())
        }
        IntComparison::Between { min, max } => {
            if non_negative && (*min < 0 || *max < 0) {
                return Err(FilterValidationError::NegativeDuration);
            }
            if min > max {
                return Err(FilterValidationError::InvalidRange);
            }
            Ok(())
        }
    }
}

fn validate_time_cmp(c: &TimeComparison) -> Result<(), FilterValidationError> {
    match c {
        TimeComparison::WithinLastDays { days } | TimeComparison::OlderThanDays { days } => {
            if *days == 0 || *days > MAX_FILTER_DAYS {
                return Err(FilterValidationError::InvalidDaysBounds);
            }
            Ok(())
        }
        TimeComparison::AfterEpoch { .. } | TimeComparison::BeforeEpoch { .. } => Ok(()),
        TimeComparison::BetweenEpochs { min, max } => {
            if min > max {
                return Err(FilterValidationError::InvalidRange);
            }
            Ok(())
        }
    }
}

impl FilterCondition {
    pub fn validate(&self) -> Result<(), FilterValidationError> {
        match self {
            FilterCondition::LibraryId { values, .. } => validate_string_list(values),
            FilterCondition::ItemType { values, .. } => {
                if values.is_empty() || values.len() > MAX_FILTER_LIST_LEN {
                    Err(FilterValidationError::InvalidListLength)
                } else {
                    Ok(())
                }
            }
            FilterCondition::Title { matcher }
            | FilterCondition::OriginalTitle { matcher }
            | FilterCondition::SeriesTitle { matcher }
            | FilterCondition::Director { matcher }
            | FilterCondition::Studio { matcher }
            | FilterCondition::Country { matcher }
            | FilterCondition::Language { matcher }
            | FilterCondition::Resolution { matcher }
            | FilterCondition::VideoCodec { matcher }
            | FilterCondition::AudioCodec { matcher }
            | FilterCondition::Container { matcher } => validate_text_match(matcher),

            FilterCondition::Genres { matcher }
            | FilterCondition::Tags { matcher }
            | FilterCondition::Actors { matcher }
            | FilterCondition::Writers { matcher } => validate_array_match(matcher),

            FilterCondition::ReleaseYear { cmp } => validate_int_cmp(cmp, false),
            FilterCondition::DurationSeconds { cmp }
            | FilterCondition::AudioChannels { cmp }
            | FilterCondition::PlayCount { cmp } => validate_int_cmp(cmp, true),

            FilterCondition::Rating { cmp } => match cmp {
                FloatComparison::Gte { value } | FloatComparison::Lte { value } => {
                    if !value.is_finite() || !(0.0..=10.0).contains(value) {
                        Err(FilterValidationError::InvalidFloatBounds)
                    } else {
                        Ok(())
                    }
                }
                FloatComparison::Between { min, max } => {
                    if !min.is_finite()
                        || !max.is_finite()
                        || !(0.0..=10.0).contains(min)
                        || !(0.0..=10.0).contains(max)
                    {
                        Err(FilterValidationError::InvalidFloatBounds)
                    } else if min > max {
                        Err(FilterValidationError::InvalidRange)
                    } else {
                        Ok(())
                    }
                }
            },

            FilterCondition::AddedAt { cmp } | FilterCondition::LastWatchedAt { cmp } => {
                validate_time_cmp(cmp)
            }

            FilterCondition::WatchState { matcher } => match matcher {
                WatchStateMatch::StateIn { values } => {
                    if values.is_empty() || values.len() > MAX_FILTER_LIST_LEN {
                        Err(FilterValidationError::InvalidListLength)
                    } else {
                        Ok(())
                    }
                }
                WatchStateMatch::ProgressBetween { min, max } => {
                    if !min.is_finite()
                        || !max.is_finite()
                        || !(0.0..=1.0).contains(min)
                        || !(0.0..=1.0).contains(max)
                    {
                        Err(FilterValidationError::InvalidFloatBounds)
                    } else if min > max {
                        Err(FilterValidationError::InvalidRange)
                    } else {
                        Ok(())
                    }
                }
            },
        }
    }
}

fn eval_text_match(actual: Option<&str>, matcher: &TextMatch) -> bool {
    match matcher {
        TextMatch::IsEmpty => actual.map(|s| s.trim().is_empty()).unwrap_or(true),
        TextMatch::Eq { value } => actual
            .map(|s| {
                s.eq_ignore_ascii_case(value.trim())
                    || s.to_lowercase() == value.trim().to_lowercase()
            })
            .unwrap_or(false),
        TextMatch::In { values } => actual
            .map(|s| {
                let s_low = s.to_lowercase();
                values.iter().any(|v| v.trim().to_lowercase() == s_low)
            })
            .unwrap_or(false),
        TextMatch::Contains { value } => actual
            .map(|s| s.to_lowercase().contains(&value.trim().to_lowercase()))
            .unwrap_or(false),
        TextMatch::StartsWith { value } => actual
            .map(|s| s.to_lowercase().starts_with(&value.trim().to_lowercase()))
            .unwrap_or(false),
    }
}

fn eval_array_match(actual: &[String], matcher: &ArrayMatch) -> bool {
    match matcher {
        ArrayMatch::IsEmpty => actual.is_empty(),
        ArrayMatch::ContainsAny { values } => values.iter().any(|needle| {
            let n = needle.trim().to_lowercase();
            actual.iter().any(|elem| elem.trim().to_lowercase() == n)
        }),
        ArrayMatch::ContainsAll { values } => values.iter().all(|needle| {
            let n = needle.trim().to_lowercase();
            actual.iter().any(|elem| elem.trim().to_lowercase() == n)
        }),
    }
}

fn eval_int_cmp(actual: Option<i64>, cmp: &IntComparison) -> bool {
    let Some(v) = actual else {
        return false;
    };
    match cmp {
        IntComparison::Eq { value } => v == *value,
        IntComparison::Gte { value } => v >= *value,
        IntComparison::Lte { value } => v <= *value,
        IntComparison::Between { min, max } => v >= *min && v <= *max,
    }
}

fn eval_time_cmp(actual: Option<i64>, cmp: &TimeComparison, now_epoch: i64) -> bool {
    let Some(ts) = actual else {
        return false;
    };
    match cmp {
        TimeComparison::WithinLastDays { days } => ts >= now_epoch - (*days as i64 * 86_400),
        TimeComparison::OlderThanDays { days } => ts < now_epoch - (*days as i64 * 86_400),
        TimeComparison::AfterEpoch { timestamp } => ts >= *timestamp,
        TimeComparison::BeforeEpoch { timestamp } => ts <= *timestamp,
        TimeComparison::BetweenEpochs { min, max } => ts >= *min && ts <= *max,
    }
}

impl FilterNode {
    pub fn validate(&self) -> Result<(), FilterValidationError> {
        let mut rule_count = 0usize;
        self.validate_inner(0, &mut rule_count)?;
        if rule_count == 0 || rule_count > MAX_FILTER_AST_RULES {
            return Err(FilterValidationError::RuleCountOutOfBounds(rule_count));
        }
        Ok(())
    }

    fn validate_inner(
        &self,
        group_depth: usize,
        rule_count: &mut usize,
    ) -> Result<(), FilterValidationError> {
        match self {
            FilterNode::Group { children, .. } => {
                let next_depth = group_depth + 1;
                if next_depth > MAX_FILTER_AST_DEPTH {
                    return Err(FilterValidationError::MaxDepthExceeded);
                }
                if children.is_empty() {
                    return Err(FilterValidationError::EmptyGroup);
                }
                for child in children {
                    child.validate_inner(next_depth, rule_count)?;
                }
                Ok(())
            }
            FilterNode::Rule { condition, .. } => {
                *rule_count += 1;
                if *rule_count > MAX_FILTER_AST_RULES {
                    return Err(FilterValidationError::RuleCountOutOfBounds(*rule_count));
                }
                condition.validate()
            }
        }
    }

    pub fn references_item_type(&self) -> bool {
        match self {
            FilterNode::Group { children, .. } => children.iter().any(|c| c.references_item_type()),
            FilterNode::Rule { condition, .. } => {
                matches!(condition, FilterCondition::ItemType { .. })
            }
        }
    }

    pub fn needs_playback_state(&self) -> bool {
        match self {
            FilterNode::Group { children, .. } => children.iter().any(|c| c.needs_playback_state()),
            FilterNode::Rule { condition, .. } => matches!(
                condition,
                FilterCondition::PlayCount { .. }
                    | FilterCondition::LastWatchedAt { .. }
                    | FilterCondition::WatchState { .. }
            ),
        }
    }

    pub fn matches(
        &self,
        item: &MediaItem,
        playback: Option<&PlaybackState>,
        now_epoch: i64,
    ) -> bool {
        match self {
            FilterNode::Group {
                op,
                negated,
                children,
            } => {
                let base = match op {
                    LogicalOp::And => children
                        .iter()
                        .all(|c| c.matches(item, playback, now_epoch)),
                    LogicalOp::Or => children
                        .iter()
                        .any(|c| c.matches(item, playback, now_epoch)),
                };
                if *negated {
                    !base
                } else {
                    base
                }
            }
            FilterNode::Rule { negated, condition } => {
                let base = eval_condition(condition, item, playback, now_epoch);
                if *negated {
                    !base
                } else {
                    base
                }
            }
        }
    }
}

fn eval_condition(
    condition: &FilterCondition,
    item: &MediaItem,
    playback: Option<&PlaybackState>,
    now_epoch: i64,
) -> bool {
    match condition {
        FilterCondition::LibraryId { values, .. } => values.iter().any(|v| v == &item.library_id),
        FilterCondition::ItemType { values, .. } => values.contains(&item.item_type),

        FilterCondition::Title { matcher } => eval_text_match(Some(&item.title), matcher),
        FilterCondition::OriginalTitle { matcher } => {
            eval_text_match(item.original_title.as_deref(), matcher)
        }
        FilterCondition::SeriesTitle { matcher } => {
            eval_text_match(item.metadata.series_title.as_deref(), matcher)
        }
        FilterCondition::Director { matcher } => {
            eval_text_match(item.metadata.director.as_deref(), matcher)
        }
        FilterCondition::Studio { matcher } => {
            eval_text_match(item.metadata.studio.as_deref(), matcher)
        }
        FilterCondition::Country { matcher } => {
            eval_text_match(item.metadata.country.as_deref(), matcher)
        }
        FilterCondition::Language { matcher } => {
            eval_text_match(item.metadata.language.as_deref(), matcher)
        }
        FilterCondition::Resolution { matcher } => {
            eval_text_match(item.technical.resolution.as_deref(), matcher)
        }
        FilterCondition::VideoCodec { matcher } => {
            eval_text_match(item.technical.video_codec.as_deref(), matcher)
        }
        FilterCondition::AudioCodec { matcher } => {
            eval_text_match(item.technical.audio_codec.as_deref(), matcher)
        }
        FilterCondition::Container { matcher } => {
            eval_text_match(item.technical.container.as_deref(), matcher)
        }

        FilterCondition::Genres { matcher } => eval_array_match(&item.metadata.genres, matcher),
        FilterCondition::Tags { matcher } => eval_array_match(&item.metadata.tags, matcher),
        FilterCondition::Actors { matcher } => eval_array_match(&item.metadata.actors, matcher),
        FilterCondition::Writers { matcher } => eval_array_match(&item.metadata.writers, matcher),

        FilterCondition::ReleaseYear { cmp } => {
            eval_int_cmp(item.release_year.map(|y| y as i64), cmp)
        }
        FilterCondition::DurationSeconds { cmp } => {
            eval_int_cmp(Some(item.technical.duration_seconds), cmp)
        }
        FilterCondition::AudioChannels { cmp } => {
            eval_int_cmp(item.technical.audio_channels.map(|c| c as i64), cmp)
        }
        FilterCondition::PlayCount { cmp } => eval_int_cmp(
            Some(playback.map(|p| p.play_count as i64).unwrap_or(0)),
            cmp,
        ),
        FilterCondition::Rating { cmp } => {
            let Some(r) = item.metadata.rating else {
                return false;
            };
            match cmp {
                FloatComparison::Gte { value } => r >= *value,
                FloatComparison::Lte { value } => r <= *value,
                FloatComparison::Between { min, max } => r >= *min && r <= *max,
            }
        }

        FilterCondition::AddedAt { cmp } => eval_time_cmp(Some(item.added_at), cmp, now_epoch),
        FilterCondition::LastWatchedAt { cmp } => {
            eval_time_cmp(playback.map(|p| p.last_watched_at), cmp, now_epoch)
        }

        FilterCondition::WatchState { matcher } => match matcher {
            WatchStateMatch::StateIn { values } => {
                let effective = playback
                    .map(|p| p.watch_state)
                    .unwrap_or(WatchState::Unwatched);
                values.contains(&effective)
            }
            WatchStateMatch::ProgressBetween { min, max } => {
                if item.technical.duration_seconds <= 0 {
                    return false;
                }
                let pos = playback.map(|p| p.playback_position_seconds).unwrap_or(0);
                let ratio = (pos as f32) / (item.technical.duration_seconds as f32);
                ratio >= *min && ratio <= *max
            }
        },
    }
}
