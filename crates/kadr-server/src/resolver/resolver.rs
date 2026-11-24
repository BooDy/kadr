use futures::future::join_all;
use kadr_core::ast::{
    CardViewModel, ItemDetailsPayload, QueryMacro, ScreenId, ScreenLayout, WidgetFilterConfig,
    WidgetNode, WidgetQueryBinding,
};
use kadr_core::models::{MediaItem, MediaType, PlaybackState, WatchState};
use kadr_storage::error::Result;
use kadr_storage::repos::{
    FavoritesRepository, LibraryRepository, MediaItemRepository, PlaybackRepository,
    PlaylistRepository,
};

use super::card::to_card_view_model_with_favorite;
use crate::layout::LayoutRegistry;

/// Returns the server's current local wall-clock time as minutes since midnight (`0..1440`).
pub fn server_local_minutes() -> u16 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    #[cfg(unix)]
    {
        #[allow(deprecated)]
        let time_val = secs as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::MaybeUninit::zeroed().assume_init() };
        let ptr = unsafe { libc::localtime_r(&time_val, &mut tm) };
        if !ptr.is_null() {
            let mins = (tm.tm_hour.rem_euclid(24) as u16) * 60 + (tm.tm_min.rem_euclid(60) as u16);
            return mins % 1440;
        }
    }

    ((secs.rem_euclid(86_400) / 60) as u16) % 1440
}

/// Concurrent async resolver for Declarative Widget AST layouts and queries.
#[derive(Clone)]
pub struct WidgetResolver {
    media_repo: MediaItemRepository,
    playback_repo: PlaybackRepository,
    lib_repo: LibraryRepository,
    playlist_repo: PlaylistRepository,
    favorites_repo: FavoritesRepository,
}

impl WidgetResolver {
    /// Creates a new `WidgetResolver` with the provided storage repositories.
    pub fn new(media_repo: MediaItemRepository, playback_repo: PlaybackRepository) -> Self {
        let lib_repo = LibraryRepository::new(media_repo.pool().clone());
        let playlist_repo = PlaylistRepository::new(media_repo.pool().clone());
        let favorites_repo = FavoritesRepository::new(media_repo.pool().clone());
        Self {
            media_repo,
            playback_repo,
            lib_repo,
            playlist_repo,
            favorites_repo,
        }
    }

    /// Resolves a ScreenId into its associated library UUID if applicable.
    pub async fn resolve_library_id_for_screen(&self, screen_id: &ScreenId) -> Option<String> {
        match screen_id {
            ScreenId::Home => None,
            ScreenId::Movies => {
                if let Ok(libs) = self.lib_repo.get_all().await {
                    libs.iter()
                        .find(|l| l.name.eq_ignore_ascii_case("movies"))
                        .or_else(|| libs.iter().find(|l| l.media_type == MediaType::Movie))
                        .map(|l| l.id.clone())
                } else {
                    None
                }
            }
            ScreenId::Shows => {
                if let Ok(libs) = self.lib_repo.get_all().await {
                    libs.iter()
                        .find(|l| {
                            l.name.eq_ignore_ascii_case("shows")
                                || l.name.eq_ignore_ascii_case("tv shows")
                                || l.name.eq_ignore_ascii_case("tv_shows")
                                || l.name.eq_ignore_ascii_case("tv-shows")
                        })
                        .or_else(|| libs.iter().find(|l| l.media_type == MediaType::Show))
                        .map(|l| l.id.clone())
                } else {
                    None
                }
            }
            ScreenId::Custom(id) => {
                if let Ok(Some(lib)) = self.lib_repo.get_by_id(id).await {
                    Some(lib.id)
                } else if let Ok(libs) = self.lib_repo.get_all().await {
                    libs.into_iter()
                        .find(|l| l.id == *id || l.name.eq_ignore_ascii_case(id))
                        .map(|l| l.id)
                } else {
                    None
                }
            }
        }
    }

    /// Resolves the associated library UUID for a concrete `ScreenLayout`, including canonical
    /// custom screens matched by screen title or `LibraryItems` widget binding.
    pub async fn resolve_library_id_for_layout(&self, screen: &ScreenLayout) -> Option<String> {
        if screen.id == ScreenId::Home {
            return None;
        }
        if let Some(lib_id) = self.resolve_library_id_for_screen(&screen.id).await {
            return Some(lib_id);
        }

        let Ok(libs) = self.lib_repo.get_all().await else {
            return None;
        };

        if let Some(lib) = libs
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(&screen.title))
        {
            return Some(lib.id.clone());
        }

        for widget in &screen.widgets {
            if let WidgetNode::Grid { binding, .. }
            | WidgetNode::Carousel { binding, .. }
            | WidgetNode::DenseList { binding, .. } = widget
            {
                if let QueryMacro::LibraryItems { ref library_id } = binding.macro_type {
                    if libs.iter().any(|l| &l.id == library_id) {
                        return Some(library_id.clone());
                    }
                }
            }
        }

        None
    }

    /// Resolves the associated library UUID for a `ScreenId` using both library metadata
    /// and canonical screen mappings in `LayoutRegistry`.
    pub async fn resolve_library_id_for_screen_with_registry(
        &self,
        screen_id: &ScreenId,
        registry: &LayoutRegistry,
    ) -> Option<String> {
        if *screen_id == ScreenId::Home {
            return None;
        }

        if let ScreenId::Custom(ref id_str) = screen_id {
            if let Ok(Some(lib)) = self.lib_repo.get_by_id(id_str).await {
                return Some(lib.id);
            }
        }

        if let Some(lib_id) = self.resolve_library_id_for_screen(screen_id).await {
            return Some(lib_id);
        }

        if let Some(screen) = registry.get_screen(screen_id) {
            if let Some(lib_id) = self.resolve_library_id_for_layout(&screen).await {
                return Some(lib_id);
            }
        }

        if let Ok(libs) = self.lib_repo.get_all().await {
            for lib in libs {
                if registry
                    .find_screen_for_library(&lib.id, &lib.name)
                    .is_some_and(|canonical| &canonical.id == screen_id)
                {
                    return Some(lib.id);
                }
            }
        }

        None
    }

    /// Determines the effective library ID for a widget binding given the screen's default library.
    pub fn determine_effective_library_id<'a>(
        &self,
        binding: &'a WidgetQueryBinding,
        default_library_id: Option<&'a str>,
    ) -> Option<&'a str> {
        if binding
            .filters
            .as_ref()
            .map(|f| f.all_libraries)
            .unwrap_or(false)
        {
            None
        } else if let Some(lib_id) = binding
            .filters
            .as_ref()
            .and_then(|f| f.library_id.as_deref())
        {
            Some(lib_id)
        } else if let QueryMacro::LibraryItems { ref library_id } = binding.macro_type {
            Some(library_id.as_str())
        } else if matches!(
            binding.macro_type,
            QueryMacro::PlaylistItems { .. } | QueryMacro::Favorites
        ) {
            None
        } else {
            default_library_id
        }
    }

    /// Helper to verify whether an item is visible according to library privacy.
    async fn is_item_visible(
        &self,
        item: &kadr_core::models::MediaItem,
        unlocked_ids: &[String],
    ) -> bool {
        match self.lib_repo.get_by_id(&item.library_id).await {
            Ok(Some(lib)) => !lib.is_private || unlocked_ids.contains(&item.library_id),
            Ok(None) => true,
            Err(err) => {
                tracing::error!(
                    "Failed to fetch library {} for item visibility check: {err}",
                    item.library_id
                );
                false
            }
        }
    }

    /// Evaluates if a media item matches given widget filters, library privacy, and effective library scope.
    async fn is_item_matching_filters(
        &self,
        item: &MediaItem,
        filters: Option<&WidgetFilterConfig>,
        unlocked_ids: &[String],
        effective_library_id: Option<&str>,
        user_id: &str,
        playback_override: Option<&PlaybackState>,
    ) -> bool {
        // Scoping check: if an effective library ID is present, item must belong to it
        if let Some(target_lib) = effective_library_id {
            if item.library_id != target_lib {
                return false;
            }
        }

        // 1. Private check: if filters.exclude_private is true, item must not be in a private library
        let lib = match self.lib_repo.get_by_id(&item.library_id).await {
            Ok(Some(lib)) => lib,
            Ok(None) => return false,
            Err(err) => {
                tracing::error!(library_id = %item.library_id, error = %err, "Failed to load library for filter check");
                return false; // Fail-closed
            }
        };
        if lib.is_private {
            if filters.map(|f| f.exclude_private).unwrap_or(false) {
                return false;
            }
            if !unlocked_ids.contains(&lib.id) {
                return false;
            }
        }

        if let Some(f) = filters {
            // 2. Exclude library IDs
            if f.exclude_library_ids
                .iter()
                .any(|id| id == &item.library_id)
            {
                return false;
            }

            // 3. Exclude genres (case-insensitive)
            if !f.exclude_genres.is_empty() {
                let has_excluded_genre = f.exclude_genres.iter().any(|excluded| {
                    let excluded_trimmed = excluded.trim();
                    item.metadata
                        .genres
                        .iter()
                        .any(|g| g.trim().eq_ignore_ascii_case(excluded_trimmed))
                });
                if has_excluded_genre {
                    return false;
                }
            }

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            // 4. Max age cutoff
            if let Some(days) = f.max_age_days {
                if days > 0 {
                    let cutoff = now - (days as i64 * 86_400);
                    if item.added_at < cutoff {
                        return false;
                    }
                }
            }

            // 5. Declarative filter rule tree
            if let Some(ref tree) = f.rule_tree {
                let fetched_playback = if playback_override.is_none() && tree.needs_playback_state()
                {
                    if let Some(id) = item.id {
                        self.playback_repo
                            .get_state(user_id, id)
                            .await
                            .ok()
                            .flatten()
                    } else {
                        None
                    }
                } else {
                    None
                };
                let playback_ref = playback_override.or(fetched_playback.as_ref());
                let is_fav = if tree.needs_favorite_state() {
                    if let Some(id) = item.id {
                        self.favorites_repo
                            .is_favorite(user_id, id)
                            .await
                            .unwrap_or(false)
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !tree.matches_with_context(item, playback_ref, is_fav, now) {
                    return false;
                }
            }
        }

        true
    }

    /// Resolves an entire screen layout concurrently.
    pub async fn resolve_screen(&self, screen: ScreenLayout, user_id: &str) -> ScreenLayout {
        self.resolve_screen_with_unlocked(screen, user_id, &[])
            .await
    }

    /// Resolves an entire screen layout concurrently with unlocked private libraries.
    pub async fn resolve_screen_with_unlocked(
        &self,
        screen: ScreenLayout,
        user_id: &str,
        unlocked_ids: &[String],
    ) -> ScreenLayout {
        let default_library_id = self.resolve_library_id_for_layout(&screen).await;
        self.resolve_screen_with_library(
            screen,
            user_id,
            unlocked_ids,
            default_library_id.as_deref(),
        )
        .await
    }

    /// Resolves an entire screen layout concurrently with unlocked private libraries
    /// and an explicit default library scope at the server's current local time.
    pub async fn resolve_screen_with_library(
        &self,
        screen: ScreenLayout,
        user_id: &str,
        unlocked_ids: &[String],
        default_library_id: Option<&str>,
    ) -> ScreenLayout {
        self.resolve_screen_with_library_at(
            screen,
            user_id,
            unlocked_ids,
            default_library_id,
            server_local_minutes(),
        )
        .await
    }

    /// Resolves an entire screen layout concurrently with unlocked private libraries,
    /// an explicit default library scope, and active local clock minutes (`0..1440`).
    pub async fn resolve_screen_with_library_at(
        &self,
        screen: ScreenLayout,
        user_id: &str,
        unlocked_ids: &[String],
        default_library_id: Option<&str>,
        active_minutes: u16,
    ) -> ScreenLayout {
        let futures = screen
            .widgets
            .into_iter()
            .filter(|widget| {
                widget
                    .time_window()
                    .is_none_or(|tw| tw.is_active_at_minutes(active_minutes))
            })
            .map(|widget| self.resolve_widget(widget, user_id, unlocked_ids, default_library_id));
        let widgets = join_all(futures).await;

        ScreenLayout {
            id: screen.id,
            title: screen.title,
            widgets,
            resolved_scope: screen.resolved_scope,
        }
    }

    async fn resolve_widget(
        &self,
        widget: WidgetNode,
        user_id: &str,
        unlocked_ids: &[String],
        default_library_id: Option<&str>,
    ) -> WidgetNode {
        match widget {
            WidgetNode::HeroBanner {
                id,
                binding,
                time_window,
                data: _,
            } => {
                let card = self
                    .resolve_hero_banner_data(&binding, user_id, unlocked_ids, default_library_id)
                    .await;
                WidgetNode::HeroBanner {
                    id,
                    binding,
                    time_window,
                    data: card,
                }
            }
            WidgetNode::Carousel {
                id,
                title,
                binding,
                time_window,
                items: _,
                next_cursor: _,
            } => {
                let (cards, next_cursor, _) = self
                    .resolve_widget_data_with_library(
                        &binding,
                        user_id,
                        0,
                        unlocked_ids,
                        default_library_id,
                    )
                    .await
                    .unwrap_or_else(|err| {
                        tracing::warn!("Failed to resolve carousel {id}: {err}");
                        (Vec::new(), None, None)
                    });

                WidgetNode::Carousel {
                    id,
                    title,
                    binding,
                    time_window,
                    items: Some(cards),
                    next_cursor,
                }
            }
            WidgetNode::Grid {
                id,
                title,
                binding,
                columns,
                time_window,
                items: _,
                next_cursor: _,
                total_count: _,
            } => {
                let (cards, next_cursor, total_count) = self
                    .resolve_widget_data_with_library(
                        &binding,
                        user_id,
                        0,
                        unlocked_ids,
                        default_library_id,
                    )
                    .await
                    .unwrap_or_else(|err| {
                        tracing::warn!("Failed to resolve grid {id}: {err}");
                        (Vec::new(), None, None)
                    });

                WidgetNode::Grid {
                    id,
                    title,
                    binding,
                    columns,
                    time_window,
                    items: Some(cards),
                    next_cursor,
                    total_count,
                }
            }
            WidgetNode::DenseList {
                id,
                title,
                binding,
                time_window,
                items: _,
                next_cursor: _,
                total_count: _,
            } => {
                let (cards, next_cursor, total_count) = self
                    .resolve_widget_data_with_library(
                        &binding,
                        user_id,
                        0,
                        unlocked_ids,
                        default_library_id,
                    )
                    .await
                    .unwrap_or_else(|err| {
                        tracing::warn!("Failed to resolve dense_list {id}: {err}");
                        (Vec::new(), None, None)
                    });

                WidgetNode::DenseList {
                    id,
                    title,
                    binding,
                    time_window,
                    items: Some(cards),
                    next_cursor,
                    total_count,
                }
            }
            WidgetNode::ItemDetails {
                id,
                item_id,
                details: _,
            } => {
                let details = self
                    .resolve_item_details_with_unlocked(item_id, user_id, unlocked_ids)
                    .await
                    .unwrap_or_else(|err| {
                        tracing::warn!("Failed to resolve item details {id}: {err}");
                        None
                    });

                WidgetNode::ItemDetails {
                    id,
                    item_id,
                    details,
                }
            }
        }
    }

    async fn resolve_hero_banner_data(
        &self,
        binding: &WidgetQueryBinding,
        user_id: &str,
        unlocked_ids: &[String],
        default_library_id: Option<&str>,
    ) -> Option<CardViewModel> {
        let effective_library_id = self.determine_effective_library_id(binding, default_library_id);
        let candidate = match binding.macro_type {
            QueryMacro::SpotlightItem { item_id: Some(id) } => {
                let item = self.media_repo.get_by_id(id).await.ok().flatten()?;
                if self
                    .is_item_matching_filters(
                        &item,
                        binding.filters.as_ref(),
                        unlocked_ids,
                        effective_library_id,
                        user_id,
                        None,
                    )
                    .await
                {
                    Some(item)
                } else {
                    None
                }
            }
            QueryMacro::SpotlightItem { item_id: None } => self
                .media_repo
                .find_spotlight_candidate_for_user(
                    effective_library_id,
                    Some(user_id),
                    unlocked_ids,
                    binding.filters.as_ref(),
                )
                .await
                .ok()
                .flatten(),
            QueryMacro::ItemDetails { item_id } => {
                let item = self.media_repo.get_by_id(item_id).await.ok().flatten()?;
                if self
                    .is_item_matching_filters(
                        &item,
                        binding.filters.as_ref(),
                        unlocked_ids,
                        effective_library_id,
                        user_id,
                        None,
                    )
                    .await
                {
                    Some(item)
                } else {
                    None
                }
            }
            _ => {
                let (cards, _, _) = self
                    .resolve_widget_data_with_library(
                        binding,
                        user_id,
                        0,
                        unlocked_ids,
                        default_library_id,
                    )
                    .await
                    .ok()?;
                return cards.into_iter().next();
            }
        };

        if let Some(item) = candidate {
            let (playback, is_fav) = if let Some(item_id) = item.id {
                let pb = self
                    .playback_repo
                    .get_state(user_id, item_id)
                    .await
                    .ok()
                    .flatten();
                let fav = self
                    .favorites_repo
                    .is_favorite(user_id, item_id)
                    .await
                    .unwrap_or(false);
                (pb, fav)
            } else {
                (None, false)
            };
            Some(to_card_view_model_with_favorite(
                &item,
                playback.as_ref(),
                is_fav,
            ))
        } else {
            None
        }
    }

    /// Resolves paginated widget data for a query binding.
    /// Returns `(cards, next_cursor, total_count)`.
    pub async fn resolve_widget_data(
        &self,
        binding: &WidgetQueryBinding,
        user_id: &str,
        offset: u32,
    ) -> Result<(Vec<CardViewModel>, Option<String>, Option<u64>)> {
        self.resolve_widget_data_with_library(binding, user_id, offset, &[], None)
            .await
    }

    /// Resolves paginated widget data for a query binding with unlocked private libraries.
    /// Returns `(cards, next_cursor, total_count)`.
    pub async fn resolve_widget_data_with_unlocked(
        &self,
        binding: &WidgetQueryBinding,
        user_id: &str,
        offset: u32,
        unlocked_ids: &[String],
    ) -> Result<(Vec<CardViewModel>, Option<String>, Option<u64>)> {
        self.resolve_widget_data_with_library(binding, user_id, offset, unlocked_ids, None)
            .await
    }

    /// Resolves paginated widget data for a query binding with unlocked private libraries and optional default library scope.
    /// Returns `(cards, next_cursor, total_count)`.
    pub async fn resolve_widget_data_with_library(
        &self,
        binding: &WidgetQueryBinding,
        user_id: &str,
        offset: u32,
        unlocked_ids: &[String],
        default_library_id: Option<&str>,
    ) -> Result<(Vec<CardViewModel>, Option<String>, Option<u64>)> {
        let effective_library_id = self.determine_effective_library_id(binding, default_library_id);
        match &binding.macro_type {
            QueryMacro::ContinueWatching => {
                let states = self.playback_repo.get_continue_watching(user_id).await?;
                let page_states: Vec<_> = states
                    .into_iter()
                    .skip(offset as usize)
                    .take(binding.limit as usize)
                    .collect();

                let has_more = page_states.len() == binding.limit as usize;

                let item_ids: Vec<i64> = page_states.iter().map(|s| s.media_item_id).collect();
                let items_map = self
                    .media_repo
                    .get_by_ids(&item_ids)
                    .await
                    .unwrap_or_default();
                let favorited_ids = self
                    .favorites_repo
                    .get_favorited_ids(user_id, &item_ids)
                    .await
                    .unwrap_or_default();

                let mut cards = Vec::with_capacity(page_states.len());
                for state in &page_states {
                    if let Some(item) = items_map.get(&state.media_item_id) {
                        if self
                            .is_item_matching_filters(
                                item,
                                binding.filters.as_ref(),
                                unlocked_ids,
                                effective_library_id,
                                user_id,
                                Some(state),
                            )
                            .await
                        {
                            let is_fav = favorited_ids.contains(&state.media_item_id);
                            cards.push(to_card_view_model_with_favorite(item, Some(state), is_fav));
                        }
                    }
                }

                let next_cursor = if has_more {
                    Some((offset + page_states.len() as u32).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, None))
            }
            QueryMacro::RecentlyAdded => {
                let items = self
                    .media_repo
                    .find_recently_added_paginated_for_user(
                        effective_library_id,
                        Some(user_id),
                        binding.limit,
                        offset,
                        unlocked_ids,
                        binding.filters.as_ref(),
                    )
                    .await?;

                let has_more = items.len() == binding.limit as usize;
                let cards = self.hydrate_items_to_cards(items, user_id).await;

                let next_cursor = if has_more {
                    Some((offset + cards.len() as u32).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, None))
            }
            QueryMacro::TopRated => {
                let items = self
                    .media_repo
                    .find_top_rated_paginated_for_user(
                        effective_library_id,
                        Some(user_id),
                        binding.limit,
                        offset,
                        unlocked_ids,
                        binding.filters.as_ref(),
                    )
                    .await?;
                let has_more = items.len() == binding.limit as usize;
                let cards = self.hydrate_items_to_cards(items, user_id).await;

                let next_cursor = if has_more {
                    Some((offset + cards.len() as u32).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, None))
            }
            QueryMacro::GenreShelf { genre } => {
                let items = self
                    .media_repo
                    .find_by_genre_paginated_for_user(
                        genre,
                        effective_library_id,
                        Some(user_id),
                        binding.limit,
                        offset,
                        unlocked_ids,
                        binding.filters.as_ref(),
                    )
                    .await?;

                let has_more = items.len() == binding.limit as usize;
                let cards = self.hydrate_items_to_cards(items, user_id).await;

                let next_cursor = if has_more {
                    Some((offset + cards.len() as u32).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, None))
            }
            QueryMacro::LibraryItems { library_id } => {
                let target_lib = effective_library_id.unwrap_or(library_id.as_str());
                let (items, total) = self
                    .media_repo
                    .find_by_library_paginated_for_user(
                        target_lib,
                        Some(user_id),
                        binding.limit,
                        offset,
                        binding.sort.as_deref(),
                        unlocked_ids,
                        binding.filters.as_ref(),
                    )
                    .await?;

                let count = items.len() as u32;
                let has_more = (offset as u64 + count as u64) < total;
                let cards = self.hydrate_items_to_cards(items, user_id).await;

                let next_cursor = if has_more {
                    Some((offset + count).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, Some(total)))
            }
            QueryMacro::SpotlightItem { item_id } => {
                let candidate = if let Some(id) = item_id {
                    let item = self.media_repo.get_by_id(*id).await?;
                    if let Some(i) = item {
                        if self
                            .is_item_matching_filters(
                                &i,
                                binding.filters.as_ref(),
                                unlocked_ids,
                                effective_library_id,
                                user_id,
                                None,
                            )
                            .await
                        {
                            Some(i)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    self.media_repo
                        .find_spotlight_candidate_for_user(
                            effective_library_id,
                            Some(user_id),
                            unlocked_ids,
                            binding.filters.as_ref(),
                        )
                        .await?
                };

                if let Some(item) = candidate {
                    let (playback, is_fav) = if let Some(id) = item.id {
                        let pb = self.playback_repo.get_state(user_id, id).await?;
                        let fav = self
                            .favorites_repo
                            .is_favorite(user_id, id)
                            .await
                            .unwrap_or(false);
                        (pb, fav)
                    } else {
                        (None, false)
                    };
                    let card = to_card_view_model_with_favorite(&item, playback.as_ref(), is_fav);
                    Ok((vec![card], None, None))
                } else {
                    Ok((Vec::new(), None, None))
                }
            }
            QueryMacro::ItemDetails { item_id } => {
                if let Some(item) = self.media_repo.get_by_id(*item_id).await? {
                    if self
                        .is_item_matching_filters(
                            &item,
                            binding.filters.as_ref(),
                            unlocked_ids,
                            effective_library_id,
                            user_id,
                            None,
                        )
                        .await
                    {
                        let playback = self.playback_repo.get_state(user_id, *item_id).await?;
                        let is_fav = self
                            .favorites_repo
                            .is_favorite(user_id, *item_id)
                            .await
                            .unwrap_or(false);
                        let card =
                            to_card_view_model_with_favorite(&item, playback.as_ref(), is_fav);
                        Ok((vec![card], None, None))
                    } else {
                        Ok((Vec::new(), None, None))
                    }
                } else {
                    Ok((Vec::new(), None, None))
                }
            }
            QueryMacro::PlaylistItems { playlist_id } => {
                let Some((playlist, _owner_username)) =
                    self.playlist_repo.get_playlist_raw(playlist_id).await?
                else {
                    return Ok((Vec::new(), None, Some(0)));
                };

                if playlist.owner_user_id != user_id && !playlist.is_public {
                    return Ok((Vec::new(), None, Some(0)));
                }

                let (visible_pairs, _locked_count) = self
                    .playlist_repo
                    .get_playlist_items_with_privacy(playlist_id, unlocked_ids)
                    .await?;

                let mut filtered_items = Vec::with_capacity(visible_pairs.len());
                for (_added_at, item) in visible_pairs {
                    if self
                        .is_item_matching_filters(
                            &item,
                            binding.filters.as_ref(),
                            unlocked_ids,
                            effective_library_id,
                            user_id,
                            None,
                        )
                        .await
                    {
                        filtered_items.push(item);
                    }
                }

                let total = filtered_items.len() as u64;
                let page_items: Vec<MediaItem> = filtered_items
                    .into_iter()
                    .skip(offset as usize)
                    .take(binding.limit as usize)
                    .collect();

                let count = page_items.len() as u32;
                let cards = self.hydrate_items_to_cards(page_items, user_id).await;
                let next_cursor = if (offset as u64 + count as u64) < total {
                    Some((offset + count).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, Some(total)))
            }
            QueryMacro::CustomQuery => {
                let (items, total) = self
                    .media_repo
                    .find_custom_paginated(
                        effective_library_id,
                        user_id,
                        binding.limit,
                        offset,
                        binding.sort.as_deref(),
                        unlocked_ids,
                        binding.filters.as_ref(),
                    )
                    .await?;

                let count = items.len() as u32;
                let has_more = (offset as u64 + count as u64) < total;
                let cards = self.hydrate_items_to_cards(items, user_id).await;
                let next_cursor = if has_more {
                    Some((offset + count).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, Some(total)))
            }
            QueryMacro::Favorites => {
                let (fav_pairs, total, _locked) = self
                    .favorites_repo
                    .list_favorites(
                        user_id,
                        effective_library_id,
                        None,
                        binding.limit,
                        offset,
                        binding.sort.as_deref(),
                        unlocked_ids,
                        binding.filters.as_ref(),
                    )
                    .await?;
                let items: Vec<MediaItem> = fav_pairs.into_iter().map(|(_, item)| item).collect();
                let count = items.len() as u32;
                let has_more = (offset as u64 + count as u64) < total;
                let cards = self.hydrate_items_to_cards(items, user_id).await;
                let next_cursor = if has_more {
                    Some((offset + count).to_string())
                } else {
                    None
                };
                Ok((cards, next_cursor, Some(total)))
            }
        }
    }

    /// Hydrates a slice of `MediaItem`s by batch-fetching playback and favorite states.
    async fn hydrate_items_to_cards(
        &self,
        items: Vec<kadr_core::models::MediaItem>,
        user_id: &str,
    ) -> Vec<CardViewModel> {
        let item_ids: Vec<i64> = items.iter().filter_map(|i| i.id).collect();
        let states = self
            .playback_repo
            .get_states_for_items(user_id, &item_ids)
            .await
            .unwrap_or_default();
        let favorited_ids = self
            .favorites_repo
            .get_favorited_ids(user_id, &item_ids)
            .await
            .unwrap_or_default();

        items
            .iter()
            .map(|item| {
                let playback = item.id.and_then(|id| states.get(&id));
                let is_fav = item.id.is_some_and(|id| favorited_ids.contains(&id));
                to_card_view_model_with_favorite(item, playback, is_fav)
            })
            .collect()
    }

    /// Resolves comprehensive single item details including playback state and child episodes.
    pub async fn resolve_item_details(
        &self,
        item_id: i64,
        user_id: &str,
    ) -> Result<Option<ItemDetailsPayload>> {
        self.resolve_item_details_with_unlocked(item_id, user_id, &[])
            .await
    }

    /// Resolves comprehensive single item details including playback state and child episodes with unlocked private libraries.
    pub async fn resolve_item_details_with_unlocked(
        &self,
        item_id: i64,
        user_id: &str,
        unlocked_ids: &[String],
    ) -> Result<Option<ItemDetailsPayload>> {
        let item = match self.media_repo.get_by_id(item_id).await? {
            Some(i) => i,
            None => return Ok(None),
        };

        if !self.is_item_visible(&item, unlocked_ids).await {
            return Ok(None);
        }

        let playback = self.playback_repo.get_state(user_id, item_id).await?;
        let is_fav = self
            .favorites_repo
            .is_favorite(user_id, item_id)
            .await
            .unwrap_or(false);
        let card = to_card_view_model_with_favorite(&item, playback.as_ref(), is_fav);

        let overview = item.metadata.overview.clone();
        let genres = item.metadata.genres.clone();
        let duration_seconds = if item.technical.duration_seconds > 0 {
            Some(item.technical.duration_seconds as u64)
        } else {
            None
        };
        let technical = Some(item.technical.clone());
        let stream_url = format!("/api/v1/stream/{item_id}");
        let resume_position_seconds = playback.as_ref().and_then(|p| {
            if p.watch_state == WatchState::InProgress && p.playback_position_seconds > 0 {
                Some(p.playback_position_seconds as u64)
            } else {
                None
            }
        });

        let episodes = if item.item_type == MediaType::Show {
            let episode_items = self.media_repo.find_episodes_by_series(&item.title).await?;
            let ep_cards = self.hydrate_items_to_cards(episode_items, user_id).await;
            Some(ep_cards)
        } else {
            None
        };

        let (library_id_opt, folder_path_opt) =
            match self.lib_repo.get_by_id(&item.library_id).await {
                Ok(Some(lib)) => {
                    let roots = if lib.paths.is_empty() {
                        vec![lib.path.clone()]
                    } else {
                        lib.paths.clone()
                    };

                    let mut rel_folder = None;
                    if let Some(parent) = item.file_path.parent() {
                        let parent_canon =
                            std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
                        for root in &roots {
                            let root_canon =
                                std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
                            if let Ok(rel) = parent_canon.strip_prefix(&root_canon) {
                                let rel_str = rel.to_string_lossy().replace('\\', "/");
                                rel_folder = Some(rel_str);
                                break;
                            } else if let Ok(rel) = parent.strip_prefix(root) {
                                let rel_str = rel.to_string_lossy().replace('\\', "/");
                                rel_folder = Some(rel_str);
                                break;
                            }
                        }
                    }
                    (Some(item.library_id.clone()), rel_folder)
                }
                _ => (Some(item.library_id.clone()), None),
            };

        let tmdb_id = item.metadata.extra.get("tmdb_id").and_then(|v| v.as_i64());

        Ok(Some(ItemDetailsPayload {
            card,
            original_title: item.original_title.clone(),
            overview,
            genres,
            director: item.metadata.director.clone(),
            writers: item.metadata.writers.clone(),
            actors: item.metadata.actors.clone(),
            tags: item.metadata.tags.clone(),
            studio: item.metadata.studio.clone(),
            country: item.metadata.country.clone(),
            language: item.metadata.language.clone(),
            series_title: item.metadata.series_title.clone(),
            tmdb_id,
            locked_fields: item.locked_fields.clone(),
            duration_seconds,
            technical,
            stream_url,
            resume_position_seconds,
            episodes,
            library_id: library_id_opt,
            folder_path: folder_path_opt,
        }))
    }
}
