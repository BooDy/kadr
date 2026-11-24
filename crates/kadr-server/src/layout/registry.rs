use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};

use kadr_core::ast::{
    QueryMacro, ResolvedLayoutScope, ScreenId, ScreenLayout, TargetDevice, WidgetNode,
    WidgetQueryBinding,
};
use kadr_storage::repos::StoredLayoutOverride;

pub type LayoutError = String;

/// Composite key identifying a per-user and/or per-device layout override slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutScopeKey {
    pub screen_id: ScreenId,
    pub user_id: Option<String>,
    pub target_device: TargetDevice,
}

#[derive(Debug, Clone, Default)]
struct LayoutRegistryInner {
    order: Vec<ScreenId>,
    screens: HashMap<ScreenId, ScreenLayout>,
    scoped_overrides: HashMap<LayoutScopeKey, ScreenLayout>,
    disk_overridden: HashSet<ScreenId>,
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

        // 6. Match by widget binding pointing to library_id
        for screen in inner.screens.values() {
            for widget in &screen.widgets {
                if let WidgetNode::Grid { binding, .. }
                | WidgetNode::Carousel { binding, .. }
                | WidgetNode::DenseList { binding, .. } = widget
                {
                    if let QueryMacro::LibraryItems {
                        library_id: lib_ref,
                    } = &binding.macro_type
                    {
                        if lib_ref == library_id {
                            return Some(screen.clone());
                        }
                    }
                }
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
    pub fn register_screen(&self, mut screen: ScreenLayout) {
        screen.strip_runtime_fields();
        let mut inner = self.inner.write().expect("layout registry lock poisoned");
        if !inner.screens.contains_key(&screen.id) {
            inner.order.push(screen.id.clone());
        }
        inner.screens.insert(screen.id.clone(), screen);
    }

    /// Loads persisted SQLite scoped overrides into the in-memory registry.
    pub fn load_scoped_overrides(&self, entries: Vec<StoredLayoutOverride>) {
        let mut inner = self.inner.write().expect("layout registry lock poisoned");
        for mut entry in entries {
            let screen_id: ScreenId = entry.screen_id.parse().unwrap();
            entry.layout.id = screen_id.clone();
            entry.layout.strip_runtime_fields();
            inner.scoped_overrides.insert(
                LayoutScopeKey {
                    screen_id,
                    user_id: entry.user_id,
                    target_device: entry.target_device,
                },
                entry.layout,
            );
        }
    }

    /// Sets or updates a scoped layout override in memory.
    pub fn set_scoped_override(
        &self,
        screen_id: ScreenId,
        user_id: Option<String>,
        target_device: TargetDevice,
        mut screen: ScreenLayout,
    ) {
        screen.id = screen_id.clone();
        screen.strip_runtime_fields();
        let mut inner = self.inner.write().expect("layout registry lock poisoned");
        inner.scoped_overrides.insert(
            LayoutScopeKey {
                screen_id,
                user_id,
                target_device,
            },
            screen,
        );
    }

    /// Removes a single scoped layout override from memory, returning `true` if present.
    pub fn remove_scoped_override(
        &self,
        screen_id: &ScreenId,
        user_id: Option<&str>,
        target_device: TargetDevice,
    ) -> bool {
        let mut inner = self.inner.write().expect("layout registry lock poisoned");
        inner
            .scoped_overrides
            .remove(&LayoutScopeKey {
                screen_id: screen_id.clone(),
                user_id: user_id.map(str::to_string),
                target_device,
            })
            .is_some()
    }

    /// Removes all scoped layout overrides associated with `screen_id`.
    pub fn remove_all_overrides_for_screen(&self, screen_id: &ScreenId) {
        let mut inner = self.inner.write().expect("layout registry lock poisoned");
        inner
            .scoped_overrides
            .retain(|k, _| &k.screen_id != screen_id);
    }

    /// Resolves a canonical screen layout using the 4-tier fallback chain:
    /// 1. `(Some(user_id), requested_device)` (when `requested_device != All`)
    /// 2. `(Some(user_id), TargetDevice::All)`
    /// 3. `(None, requested_device)` (when `requested_device != All`)
    /// 4. `(None, TargetDevice::All)` baseline
    pub fn resolve_scoped_screen(
        &self,
        canonical_id: &ScreenId,
        requested_user_id: Option<&str>,
        requested_device: TargetDevice,
    ) -> Option<ScreenLayout> {
        let inner = self.inner.read().expect("layout registry lock poisoned");

        if let Some(uid) = requested_user_id {
            if requested_device != TargetDevice::All {
                let key = LayoutScopeKey {
                    screen_id: canonical_id.clone(),
                    user_id: Some(uid.to_string()),
                    target_device: requested_device,
                };
                if let Some(screen) = inner.scoped_overrides.get(&key) {
                    let mut out = screen.clone();
                    out.resolved_scope = Some(ResolvedLayoutScope {
                        user_id: Some(uid.to_string()),
                        target_device: requested_device,
                        is_exact_override: true,
                    });
                    return Some(out);
                }
            }

            let key_all = LayoutScopeKey {
                screen_id: canonical_id.clone(),
                user_id: Some(uid.to_string()),
                target_device: TargetDevice::All,
            };
            if let Some(screen) = inner.scoped_overrides.get(&key_all) {
                let mut out = screen.clone();
                out.resolved_scope = Some(ResolvedLayoutScope {
                    user_id: Some(uid.to_string()),
                    target_device: TargetDevice::All,
                    is_exact_override: requested_device == TargetDevice::All,
                });
                return Some(out);
            }
        }

        if requested_device != TargetDevice::All {
            let key_global_dev = LayoutScopeKey {
                screen_id: canonical_id.clone(),
                user_id: None,
                target_device: requested_device,
            };
            if let Some(screen) = inner.scoped_overrides.get(&key_global_dev) {
                let mut out = screen.clone();
                out.resolved_scope = Some(ResolvedLayoutScope {
                    user_id: None,
                    target_device: requested_device,
                    is_exact_override: requested_user_id.is_none(),
                });
                return Some(out);
            }
        }

        let mut out = inner.screens.get(canonical_id)?.clone();
        let is_exact_override = requested_user_id.is_none()
            && requested_device == TargetDevice::All
            && inner.disk_overridden.contains(canonical_id);
        out.resolved_scope = Some(ResolvedLayoutScope {
            user_id: None,
            target_device: TargetDevice::All,
            is_exact_override,
        });
        Some(out)
    }

    /// Saves a screen layout to disk as JSON and updates in-memory registry.
    pub fn save_screen(&self, mut screen: ScreenLayout, dir: &Path) -> Result<(), std::io::Error> {
        screen.strip_runtime_fields();
        std::fs::create_dir_all(dir)?;
        let file_path = dir.join(format!("{}.json", screen.id));
        let serialized = serde_json::to_string_pretty(&screen)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&file_path, serialized)?;
        let screen_id = screen.id.clone();
        self.register_screen(screen);
        self.inner
            .write()
            .expect("layout registry lock poisoned")
            .disk_overridden
            .insert(screen_id);
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
        let default = match id {
            ScreenId::Home => default_home_layout(),
            ScreenId::Movies => default_movies_layout(),
            ScreenId::Shows => default_shows_layout(),
            ScreenId::Custom(name) => {
                return Err(format!(
                    "Custom screen '{name}' cannot be reset, use delete"
                ));
            }
        };
        self.register_screen(default.clone());
        self.inner
            .write()
            .expect("layout registry lock poisoned")
            .disk_overridden
            .remove(id);
        Ok(default)
    }

    /// Resets a library-backed custom screen to its factory default layout and removes any disk override.
    pub fn reset_library_screen(
        &self,
        library_id: &str,
        library_name: &str,
        dir: &Path,
    ) -> Result<ScreenLayout, LayoutError> {
        let default = default_library_layout(library_id, library_name);
        let id = default.id.clone();
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
        self.register_screen(default.clone());
        self.inner
            .write()
            .expect("layout registry lock poisoned")
            .disk_overridden
            .remove(&id);
        Ok(default)
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
                let toml_path = dir.join(format!("{id}.toml"));
                if toml_path.exists() {
                    let _ = std::fs::remove_file(&toml_path);
                }
                let mut inner = self.inner.write().expect("layout registry lock poisoned");
                inner.screens.remove(id);
                inner.order.retain(|s| s != id);
                inner.disk_overridden.remove(id);
                inner.scoped_overrides.retain(|k, _| &k.screen_id != id);
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
                            let sid = layout.id.clone();
                            self.register_screen(layout);
                            self.inner
                                .write()
                                .expect("layout registry lock poisoned")
                                .disk_overridden
                                .insert(sid);
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
                            let sid = layout.id.clone();
                            self.register_screen(layout);
                            self.inner
                                .write()
                                .expect("layout registry lock poisoned")
                                .disk_overridden
                                .insert(sid);
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
                time_window: None,
                data: None,
            },
            WidgetNode::Carousel {
                id: "continue_watching".to_string(),
                title: "Continue Watching".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::ContinueWatching),
                time_window: None,
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "my_favorites".to_string(),
                title: "My Favorites".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::Favorites),
                time_window: None,
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "recently_added".to_string(),
                title: "Recently Added".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::RecentlyAdded),
                time_window: None,
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "top_rated".to_string(),
                title: "Top Rated".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::TopRated),
                time_window: None,
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "genre_action".to_string(),
                title: "Action & Adventure".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::GenreShelf {
                    genre: "Action".to_string(),
                }),
                time_window: None,
                items: None,
                next_cursor: None,
            },
            WidgetNode::Carousel {
                id: "genre_scifi".to_string(),
                title: "Sci-Fi & Fantasy".to_string(),
                binding: WidgetQueryBinding::new(QueryMacro::GenreShelf {
                    genre: "Sci-Fi".to_string(),
                }),
                time_window: None,
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
            time_window: None,
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
            time_window: None,
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
            time_window: None,
            items: None,
            next_cursor: None,
            total_count: None,
        }],
    )
}
