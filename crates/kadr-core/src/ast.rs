use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use crate::filter_ast::FilterNode;
use crate::models::TechnicalInfo;

/// Represents high-level screen identifiers in the application.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScreenId {
    Home,
    Movies,
    Shows,
    Custom(String),
}

impl Serialize for ScreenId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ScreenId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ScreenIdVisitor;

        impl<'de> serde::de::Visitor<'de> for ScreenIdVisitor {
            type Value = ScreenId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a screen ID string or object")
            }

            fn visit_str<E>(self, value: &str) -> Result<ScreenId, E>
            where
                E: serde::de::Error,
            {
                Ok(value.parse().unwrap())
            }

            fn visit_string<E>(self, value: String) -> Result<ScreenId, E>
            where
                E: serde::de::Error,
            {
                Ok(value.parse().unwrap())
            }

            fn visit_map<M>(self, mut access: M) -> Result<ScreenId, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                if let Some((key, val)) = access.next_entry::<String, String>()? {
                    if key == "custom" {
                        return Ok(ScreenId::Custom(val));
                    } else {
                        return Ok(key.parse().unwrap());
                    }
                }
                Err(serde::de::Error::custom("expected screen ID"))
            }
        }

        deserializer.deserialize_any(ScreenIdVisitor)
    }
}

impl fmt::Display for ScreenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScreenId::Home => write!(f, "home"),
            ScreenId::Movies => write!(f, "movies"),
            ScreenId::Shows => write!(f, "shows"),
            ScreenId::Custom(s) => write!(f, "{s}"),
        }
    }
}

impl FromStr for ScreenId {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "home" | "Home" => ScreenId::Home,
            "movies" | "Movies" => ScreenId::Movies,
            "shows" | "Shows" => ScreenId::Shows,
            other => ScreenId::Custom(other.to_string()),
        })
    }
}

impl From<&str> for ScreenId {
    fn from(s: &str) -> Self {
        s.parse().unwrap()
    }
}

impl From<String> for ScreenId {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

/// Semantic device tier used for per-device layout scoping and 4-tier fallback resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetDevice {
    #[default]
    All,
    Tv,
    Web,
    Mobile,
}

impl fmt::Display for TargetDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetDevice::All => write!(f, "all"),
            TargetDevice::Tv => write!(f, "tv"),
            TargetDevice::Web => write!(f, "web"),
            TargetDevice::Mobile => write!(f, "mobile"),
        }
    }
}

impl FromStr for TargetDevice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "all" => Ok(TargetDevice::All),
            "tv" => Ok(TargetDevice::Tv),
            "web" => Ok(TargetDevice::Web),
            "mobile" => Ok(TargetDevice::Mobile),
            _ => Err(format!(
                "Invalid target_device '{s}': expected all, tv, web, or mobile"
            )),
        }
    }
}

/// Parses a strict 5-byte ASCII 24-hour `"HH:MM"` string into minutes from midnight (`0..=1439`).
pub fn parse_hhmm_minutes(s: &str) -> Result<u16, String> {
    let bytes = s.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes[0].is_ascii_digit()
        || !bytes[1].is_ascii_digit()
        || !bytes[3].is_ascii_digit()
        || !bytes[4].is_ascii_digit()
    {
        return Err(format!(
            "Invalid time '{s}': expected 24-hour HH:MM (00:00..23:59)"
        ));
    }
    let hours = u16::from(bytes[0] - b'0') * 10 + u16::from(bytes[1] - b'0');
    let mins = u16::from(bytes[3] - b'0') * 10 + u16::from(bytes[4] - b'0');
    if hours > 23 || mins > 59 {
        return Err(format!(
            "Invalid time '{s}': expected 24-hour HH:MM (00:00..23:59)"
        ));
    }
    Ok(hours * 60 + mins)
}

/// Optional 24-hour local-time visibility window for a shelf/widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// 24-hour "HH:MM" start time (inclusive), 00:00..=23:59.
    pub start: String,
    /// 24-hour "HH:MM" end time (exclusive, except when start == end which is rejected), 00:00..=23:59.
    pub end: String,
}

impl TimeWindow {
    pub fn validate(&self) -> Result<(), String> {
        let start_mins = parse_hhmm_minutes(&self.start)?;
        let end_mins = parse_hhmm_minutes(&self.end)?;
        if start_mins == end_mins {
            return Err("Time window start and end must differ".to_string());
        }
        Ok(())
    }

    pub fn is_active_at_minutes(&self, current_minutes: u16) -> bool {
        let (Ok(s), Ok(e)) = (
            parse_hhmm_minutes(&self.start),
            parse_hhmm_minutes(&self.end),
        ) else {
            return false;
        };
        if s == e {
            return false;
        }
        let m = current_minutes % 1440;
        if s < e {
            m >= s && m < e
        } else {
            m >= s || m < e
        }
    }
}

/// Normalized card view model for UI presentation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardViewModel {
    pub id: i64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback_progress: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_year: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_favorite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
}

/// Predefined query macro types for dynamic data resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMacro {
    ContinueWatching,
    RecentlyAdded,
    TopRated,
    Favorites,
    LibraryItems { library_id: String },
    GenreShelf { genre: String },
    SpotlightItem { item_id: Option<i64> },
    ItemDetails { item_id: i64 },
    PlaylistItems { playlist_id: String },
    CustomQuery,
}

/// Configuration for filtering items returned by a widget query.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WidgetFilterConfig {
    /// When true, always excludes items from private libraries, even if unlocked.
    #[serde(default)]
    pub exclude_private: bool,

    /// Library IDs to exclude from results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_library_ids: Vec<String>,

    /// Genres to exclude from results (case-insensitive matching).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_genres: Vec<String>,

    /// Maximum age in days from current time; excludes items added before (now - max_age_days).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u32>,

    /// When true, includes items from all libraries even when placed on a specific library screen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all_libraries: bool,

    /// Explicit library ID to filter to. Overrides the screen's default library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,

    /// Recursive declarative filter rule tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_tree: Option<FilterNode>,
}

/// Query binding configuration linking a widget to a data source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetQueryBinding {
    pub macro_type: QueryMacro,
    #[serde(default = "default_widget_limit")]
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<WidgetFilterConfig>,
}

pub fn default_widget_limit() -> u32 {
    20
}

impl WidgetQueryBinding {
    pub fn new(macro_type: QueryMacro) -> Self {
        Self {
            macro_type,
            limit: default_widget_limit(),
            sort: None,
            filters: None,
        }
    }

    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort = Some(sort.into());
        self
    }

    pub fn with_filters(mut self, filters: WidgetFilterConfig) -> Self {
        self.filters = Some(filters);
        self
    }
}

/// Declarative widget node in a screen layout AST.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetNode {
    HeroBanner {
        id: String,
        binding: WidgetQueryBinding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_window: Option<TimeWindow>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<CardViewModel>,
    },
    Carousel {
        id: String,
        title: String,
        binding: WidgetQueryBinding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_window: Option<TimeWindow>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        items: Option<Vec<CardViewModel>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
    },
    Grid {
        id: String,
        title: String,
        binding: WidgetQueryBinding,
        columns: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_window: Option<TimeWindow>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        items: Option<Vec<CardViewModel>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_count: Option<u64>,
    },
    DenseList {
        id: String,
        title: String,
        binding: WidgetQueryBinding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_window: Option<TimeWindow>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        items: Option<Vec<CardViewModel>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_count: Option<u64>,
    },
    ItemDetails {
        id: String,
        item_id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<ItemDetailsPayload>,
    },
}

impl WidgetNode {
    /// Returns the unique identifier of this widget node.
    pub fn id(&self) -> &str {
        match self {
            WidgetNode::HeroBanner { id, .. } => id,
            WidgetNode::Carousel { id, .. } => id,
            WidgetNode::Grid { id, .. } => id,
            WidgetNode::DenseList { id, .. } => id,
            WidgetNode::ItemDetails { id, .. } => id,
        }
    }

    /// Returns a reference to the query binding if this widget has one.
    pub fn binding(&self) -> Option<&WidgetQueryBinding> {
        match self {
            WidgetNode::HeroBanner { binding, .. } => Some(binding),
            WidgetNode::Carousel { binding, .. } => Some(binding),
            WidgetNode::Grid { binding, .. } => Some(binding),
            WidgetNode::DenseList { binding, .. } => Some(binding),
            WidgetNode::ItemDetails { .. } => None,
        }
    }

    /// Returns a reference to the optional local-time visibility window if configured.
    pub fn time_window(&self) -> Option<&TimeWindow> {
        match self {
            WidgetNode::HeroBanner { time_window, .. }
            | WidgetNode::Carousel { time_window, .. }
            | WidgetNode::Grid { time_window, .. }
            | WidgetNode::DenseList { time_window, .. } => time_window.as_ref(),
            WidgetNode::ItemDetails { .. } => None,
        }
    }

    /// Returns true if this widget node has been populated with data.
    pub fn is_hydrated(&self) -> bool {
        match self {
            WidgetNode::HeroBanner { data, .. } => data.is_some(),
            WidgetNode::Carousel { items, .. } => items.is_some(),
            WidgetNode::Grid { items, .. } => items.is_some(),
            WidgetNode::DenseList { items, .. } => items.is_some(),
            WidgetNode::ItemDetails { details, .. } => details.is_some(),
        }
    }

    /// Clears hydrated data and pagination cursors prior to persisting the widget AST.
    pub fn strip_hydrated_data(&mut self) {
        match self {
            WidgetNode::HeroBanner { data, .. } => {
                *data = None;
            }
            WidgetNode::Carousel {
                items, next_cursor, ..
            } => {
                *items = None;
                *next_cursor = None;
            }
            WidgetNode::Grid {
                items,
                next_cursor,
                total_count,
                ..
            }
            | WidgetNode::DenseList {
                items,
                next_cursor,
                total_count,
                ..
            } => {
                *items = None;
                *next_cursor = None;
                *total_count = None;
            }
            WidgetNode::ItemDetails { details, .. } => {
                *details = None;
            }
        }
    }
}

/// Comprehensive details payload for single item view inspection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemDetailsPayload {
    pub card: CardViewModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub director: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub studio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmdb_id: Option<i64>,
    #[serde(default)]
    pub locked_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical: Option<TechnicalInfo>,
    pub stream_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_position_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes: Option<Vec<CardViewModel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
}

/// Runtime metadata describing which `(user_id, target_device)` scope tier matched during resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLayoutScope {
    /// Effective owner of the resolved layout (`None` = global, `Some(uid)` = per-user).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Effective device tier that matched during resolution.
    pub target_device: TargetDevice,
    /// True when an explicit override exists at the requested `(scope_user_id, target_device)` slot
    /// (or when a non-factory override matched during normal browsing).
    pub is_exact_override: bool,
}

/// Top-level layout container representing a complete screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenLayout {
    pub id: ScreenId,
    pub title: String,
    pub widgets: Vec<WidgetNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_scope: Option<ResolvedLayoutScope>,
}

impl ScreenLayout {
    pub fn new(id: ScreenId, title: impl Into<String>, widgets: Vec<WidgetNode>) -> Self {
        Self {
            id,
            title: title.into(),
            widgets,
            resolved_scope: None,
        }
    }

    /// Clears runtime-only scope metadata and hydrated widget data prior to persisting.
    pub fn strip_runtime_fields(&mut self) {
        self.resolved_scope = None;
        for widget in &mut self.widgets {
            widget.strip_hydrated_data();
        }
    }
}
