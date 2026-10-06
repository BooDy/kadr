use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

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

/// Normalized card view model for UI presentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

/// Predefined query macro types for dynamic data resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMacro {
    ContinueWatching,
    RecentlyAdded,
    TopRated,
    LibraryItems { library_id: String },
    GenreShelf { genre: String },
    SpotlightItem { item_id: Option<i64> },
    ItemDetails { item_id: i64 },
}

/// Configuration for filtering items returned by a widget query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Query binding configuration linking a widget to a data source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetNode {
    HeroBanner {
        id: String,
        binding: WidgetQueryBinding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<CardViewModel>,
    },
    Carousel {
        id: String,
        title: String,
        binding: WidgetQueryBinding,
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
            WidgetNode::ItemDetails { id, .. } => id,
        }
    }

    /// Returns a reference to the query binding if this widget has one.
    pub fn binding(&self) -> Option<&WidgetQueryBinding> {
        match self {
            WidgetNode::HeroBanner { binding, .. } => Some(binding),
            WidgetNode::Carousel { binding, .. } => Some(binding),
            WidgetNode::Grid { binding, .. } => Some(binding),
            WidgetNode::ItemDetails { .. } => None,
        }
    }

    /// Returns true if this widget node has been populated with data.
    pub fn is_hydrated(&self) -> bool {
        match self {
            WidgetNode::HeroBanner { data, .. } => data.is_some(),
            WidgetNode::Carousel { items, .. } => items.is_some(),
            WidgetNode::Grid { items, .. } => items.is_some(),
            WidgetNode::ItemDetails { details, .. } => details.is_some(),
        }
    }
}

/// Comprehensive details payload for single item view inspection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDetailsPayload {
    pub card: CardViewModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical: Option<TechnicalInfo>,
    pub stream_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_position_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episodes: Option<Vec<CardViewModel>>,
}

/// Top-level layout container representing a complete screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenLayout {
    pub id: ScreenId,
    pub title: String,
    pub widgets: Vec<WidgetNode>,
}

impl ScreenLayout {
    pub fn new(id: ScreenId, title: impl Into<String>, widgets: Vec<WidgetNode>) -> Self {
        Self {
            id,
            title: title.into(),
            widgets,
        }
    }
}
