use std::collections::HashMap;
use std::path::Path;

use kadr_core::ast::{QueryMacro, ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding};

/// In-memory layout registry holding built-in and user-customized screen layouts.
#[derive(Debug, Clone)]
pub struct LayoutRegistry {
    order: Vec<ScreenId>,
    screens: HashMap<ScreenId, ScreenLayout>,
}

impl Default for LayoutRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutRegistry {
    /// Creates a new `LayoutRegistry` initialized with built-in default layouts:
    /// `Home`, `Movies`, and `Shows`.
    pub fn new() -> Self {
        let mut registry = Self {
            order: Vec::new(),
            screens: HashMap::new(),
        };

        registry.register_screen(default_home_layout());
        registry.register_screen(default_movies_layout());
        registry.register_screen(default_shows_layout());

        registry
    }

    /// Retrieves a screen layout by screen identifier.
    pub fn get_screen(&self, id: &ScreenId) -> Option<ScreenLayout> {
        self.screens.get(id).cloned()
    }

    /// Lists all registered screens in registration order as pairs of `(ScreenId, Title)`.
    pub fn list_screens(&self) -> Vec<(ScreenId, String)> {
        self.order
            .iter()
            .filter_map(|id| self.screens.get(id).map(|s| (s.id.clone(), s.title.clone())))
            .collect()
    }

    /// Registers or overrides a screen layout.
    pub fn register_screen(&mut self, screen: ScreenLayout) {
        if !self.screens.contains_key(&screen.id) {
            self.order.push(screen.id.clone());
        }
        self.screens.insert(screen.id.clone(), screen);
    }

    /// Loads screen definitions (TOML or JSON) from the specified directory if present.
    /// Invalid files are logged with warnings and do not abort loading of other files.
    pub fn load_overrides_from_dir(&mut self, dir: &Path) -> Result<(), std::io::Error> {
        if !dir.exists() {
            return Ok(());
        }

        let mut entries = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            match entry {
                Ok(e) => {
                    let path = e.path();
                    if path.is_file() {
                        entries.push(path);
                    }
                }
                Err(err) => {
                    tracing::warn!("Failed to read directory entry in {}: {err}", dir.display());
                }
            }
        }

        entries.sort();

        for path in entries {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            match extension {
                "toml" => match std::fs::read_to_string(&path) {
                    Ok(content) => match toml::from_str::<ScreenLayout>(&content) {
                        Ok(layout) => {
                            tracing::info!(
                                "Loaded screen layout override for '{}' from {}",
                                layout.id,
                                path.display()
                            );
                            self.register_screen(layout);
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Failed to parse TOML layout override from {}: {err}",
                                path.display()
                            );
                        }
                    },
                    Err(err) => {
                        tracing::warn!(
                            "Failed to read layout override file {}: {err}",
                            path.display()
                        );
                    }
                },
                "json" => match std::fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<ScreenLayout>(&content) {
                        Ok(layout) => {
                            tracing::info!(
                                "Loaded screen layout override for '{}' from {}",
                                layout.id,
                                path.display()
                            );
                            self.register_screen(layout);
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Failed to parse JSON layout override from {}: {err}",
                                path.display()
                            );
                        }
                    },
                    Err(err) => {
                        tracing::warn!(
                            "Failed to read layout override file {}: {err}",
                            path.display()
                        );
                    }
                },
                _ => {}
            }
        }

        Ok(())
    }
}

/// Builds the default `Home` screen layout.
pub fn default_home_layout() -> ScreenLayout {
    ScreenLayout::new(
        ScreenId::Home,
        "Home",
        vec![
            WidgetNode::HeroBanner {
                id: "home_spotlight".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::SpotlightItem { item_id: None }),
                data: None,
            },
            WidgetNode::Carousel {
                id: "continue_watching".to_string(),
                title: "Continue Watching".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::ContinueWatching),
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "recently_added".to_string(),
                title: "Recently Added".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded),
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "top_rated".to_string(),
                title: "Top Rated".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::TopRated),
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "genre_action".to_string(),
                title: "Action & Adventure".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::GenreShelf {
                    genre: "Action".to_string(),
                }),
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "genre_scifi".to_string(),
                title: "Sci-Fi & Fantasy".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::GenreShelf {
                    genre: "Sci-Fi".to_string(),
                }),
                items: None,
                next_cursor: None,
            },
        ],
    )
}

/// Builds the default `Movies` screen layout.
pub fn default_movies_layout() -> ScreenLayout {
    ScreenLayout::new(
        ScreenId::Movies,
        "Movies",
        vec![WidgetNode::Grid {
            id: "all_movies".to_string(),
            title: "All Movies".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded)
                .with_limit(30)
                .with_sort("title:asc".to_string()),
            columns: 6,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    )
}

/// Builds the default `Shows` screen layout.
pub fn default_shows_layout() -> ScreenLayout {
    ScreenLayout::new(
        ScreenId::Shows,
        "TV Shows",
        vec![WidgetNode::Grid {
            id: "all_shows".to_string(),
            title: "All TV Shows".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded)
                .with_limit(30)
                .with_sort("title:asc".to_string()),
            columns: 6,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    )
}
