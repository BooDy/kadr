use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use kadr_core::ast::{QueryMacro, ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding};

pub type LayoutError = String;

#[derive(Debug, Clone, Default)]
struct LayoutRegistryInner {
    order: Vec<ScreenId>,
    screens: HashMap<ScreenId, ScreenLayout>,
}

/// In-memory layout registry holding built-in and user-customized screen layouts.
#[derive(Debug, Clone)]
pub struct LayoutRegistry {
    inner: Arc<RwLock<LayoutRegistryInner>>,
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
        let registry = Self {
            inner: Arc::new(RwLock::new(LayoutRegistryInner::default())),
        };

        registry.register_screen(default_home_layout());
        registry.register_screen(default_movies_layout());
        registry.register_screen(default_shows_layout());

        registry
    }

    /// Retrieves a screen layout by screen identifier.
    pub fn get_screen(&self, id: &ScreenId) -> Option<ScreenLayout> {
        self.inner
            .read()
            .expect("layout registry lock poisoned")
            .screens
            .get(id)
            .cloned()
    }

    /// Finds a screen layout matching a library either by its exact ID,
    /// or by library name (e.g. "Movies" -> ScreenId::Movies, "TV Shows" -> ScreenId::Shows, "Porn" -> ScreenId::Custom("Porn")).
    pub fn find_screen_for_library(
        &self,
        library_id: &str,
        library_name: &str,
    ) -> Option<ScreenLayout> {
        let inner = self.inner.read().expect("layout registry lock poisoned");

        // 1. Direct match by library ID
        let lib_id_parsed: ScreenId = library_id.parse().unwrap();
        if let Some(screen) = inner.screens.get(&lib_id_parsed) {
            return Some(screen.clone());
        }

        // 2. Standard built-in mapping by library name
        if library_name.eq_ignore_ascii_case("movies") {
            if let Some(screen) = inner.screens.get(&ScreenId::Movies) {
                return Some(screen.clone());
            }
        }
        if library_name.eq_ignore_ascii_case("shows")
            || library_name.eq_ignore_ascii_case("tv shows")
            || library_name.eq_ignore_ascii_case("tv_shows")
            || library_name.eq_ignore_ascii_case("tv-shows")
        {
            if let Some(screen) = inner.screens.get(&ScreenId::Shows) {
                return Some(screen.clone());
            }
        }

        // 3. Match by ScreenId parsed from library name (e.g. ScreenId::Custom("Porn"))
        let name_id: ScreenId = library_name.parse().unwrap();
        if let Some(screen) = inner.screens.get(&name_id) {
            return Some(screen.clone());
        }

        // 4. Case-insensitive match on screen ID string
        for (id, screen) in &inner.screens {
            if id.to_string().eq_ignore_ascii_case(library_name) {
                return Some(screen.clone());
            }
        }

        // 5. Case-insensitive match on screen title
        for screen in inner.screens.values() {
            if screen.title.eq_ignore_ascii_case(library_name) {
                return Some(screen.clone());
            }
        }

        None
    }

    /// Lists all registered screens in registration order as pairs of `(ScreenId, Title)`.
    pub fn list_screens(&self) -> Vec<(ScreenId, String)> {
        let inner = self.inner.read().expect("layout registry lock poisoned");
        inner
            .order
            .iter()
            .filter_map(|id| {
                inner
                    .screens
                    .get(id)
                    .map(|s| (s.id.clone(), s.title.clone()))
            })
            .collect()
    }

    /// Registers or overrides a screen layout.
    pub fn register_screen(&self, screen: ScreenLayout) {
        let mut inner = self.inner.write().expect("layout registry lock poisoned");
        if !inner.screens.contains_key(&screen.id) {
            inner.order.push(screen.id.clone());
        }
        inner.screens.insert(screen.id.clone(), screen);
    }

    /// Saves a screen layout to disk as JSON and updates in-memory registry.
    pub fn save_screen(&self, screen: ScreenLayout, dir: &Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let file_path = dir.join(format!("{}.json", screen.id));
        let serialized = serde_json::to_string_pretty(&screen)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&file_path, serialized)?;
        self.register_screen(screen);
        Ok(())
    }

    /// Resets a built-in screen layout to factory default and removes its override file.
    pub fn reset_screen(&self, id: &ScreenId, dir: &Path) -> Result<ScreenLayout, LayoutError> {
        let file_path = dir.join(format!("{id}.json"));
        if file_path.exists() {
            if let Err(e) = std::fs::remove_file(&file_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::error!("Failed to remove layout file {}: {e}", file_path.display());
                    return Err(format!(
                        "Failed to remove layout file {}: {e}",
                        file_path.display()
                    ));
                }
            }
        }
        match id {
            ScreenId::Home => {
                let default = default_home_layout();
                self.register_screen(default.clone());
                Ok(default)
            }
            ScreenId::Movies => {
                let default = default_movies_layout();
                self.register_screen(default.clone());
                Ok(default)
            }
            ScreenId::Shows => {
                let default = default_shows_layout();
                self.register_screen(default.clone());
                Ok(default)
            }
            ScreenId::Custom(name) => Err(format!(
                "Custom screen '{name}' cannot be reset, use delete"
            )),
        }
    }

    /// Deletes a custom screen from disk and removes it from the in-memory registry.
    pub fn delete_custom_screen(&self, id: &ScreenId, dir: &Path) -> Result<(), LayoutError> {
        match id {
            ScreenId::Home | ScreenId::Movies | ScreenId::Shows => {
                Err("Built-in screens cannot be deleted".to_string())
            }
            ScreenId::Custom(_) => {
                let file_path = dir.join(format!("{id}.json"));
                if file_path.exists() {
                    if let Err(e) = std::fs::remove_file(&file_path) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            tracing::error!(
                                "Failed to remove layout file {}: {e}",
                                file_path.display()
                            );
                            return Err(format!(
                                "Failed to remove layout file {}: {e}",
                                file_path.display()
                            ));
                        }
                    }
                }
                let mut inner = self.inner.write().expect("layout registry lock poisoned");
                inner.screens.remove(id);
                inner.order.retain(|s| s != id);
                Ok(())
            }
        }
    }

    /// Loads screen definitions (TOML or JSON) from the specified directory if present.
    /// Invalid files are logged with warnings and do not abort loading of other files.
    pub fn load_overrides_from_dir(&self, dir: &Path) -> Result<(), std::io::Error> {
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

/// Builds the default screen layout for a library.
pub fn default_library_layout(library_id: &str, library_name: &str) -> ScreenLayout {
    let screen_id: ScreenId = library_id.parse().unwrap();
    ScreenLayout::new(
        screen_id,
        library_name,
        vec![WidgetNode::Grid {
            id: format!("{library_id}_grid"),
            title: format!("All {library_name}"),
            binding: WidgetQueryBinding::new(QueryMacro::LibraryItems {
                library_id: library_id.to_string(),
            })
            .with_limit(30)
            .with_sort("title:asc".to_string()),
            columns: 6,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    )
}
