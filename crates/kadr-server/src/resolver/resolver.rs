use futures::future::join_all;
use kadr_core::ast::{
    CardViewModel, ItemDetailsPayload, QueryMacro, ScreenId, ScreenLayout, WidgetFilterConfig,
    WidgetNode, WidgetQueryBinding,
};
use kadr_core::models::{MediaItem, MediaType};
use kadr_storage::error::Result;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository};

use super::card::to_card_view_model;

/// Concurrent async resolver for Declarative Widget AST layouts and queries.
#[derive(Clone)]
pub struct WidgetResolver {
    media_repo: MediaItemRepository,
    playback_repo: PlaybackRepository,
    lib_repo: LibraryRepository,
}

impl WidgetResolver {
    /// Creates a new `WidgetResolver` with the provided storage repositories.
    pub fn new(media_repo: MediaItemRepository, playback_repo: PlaybackRepository) -> Self {
        let lib_repo = LibraryRepository::new(media_repo.pool().clone());
        Self {
            media_repo,
            playback_repo,
            lib_repo,
        }
    }

    /// Resolves a ScreenId into its associated library UUID if applicable.
    pub async fn resolve_library_id_for_screen(&self, screen_id: &ScreenId) -> Option<String> {
        match screen_id {
            ScreenId::Home => None,
            ScreenId::Movies => {
                if let Ok(libs) = self.lib_repo.get_all().await {
                    libs.into_iter()
                        .find(|l| l.media_type == MediaType::Movie || l.name.eq_ignore_ascii_case("movies"))
                        .map(|l| l.id)
                } else {
                    None
                }
            }
            ScreenId::Shows => {
                if let Ok(libs) = self.lib_repo.get_all().await {
                    libs.into_iter()
                        .find(|l| l.media_type == MediaType::Show || l.name.eq_ignore_ascii_case("shows") || l.name.eq_ignore_ascii_case("tv shows"))
                        .map(|l| l.id)
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

    /// Determines the effective library ID for a widget binding given the screen's default library.
    pub fn determine_effective_library_id<'a>(
        &self,
        binding: &'a WidgetQueryBinding,
        default_library_id: Option<&'a str>,
    ) -> Option<&'a str> {
        if binding.filters.as_ref().map(|f| f.all_libraries).unwrap_or(false) {
            None
        } else if let Some(lib_id) = binding.filters.as_ref().and_then(|f| f.library_id.as_deref()) {
            Some(lib_id)
        } else if let QueryMacro::LibraryItems { ref library_id } = binding.macro_type {
            Some(library_id.as_str())
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
            Ok(Some(lib)) => {
                if lib.is_private && !unlocked_ids.contains(&item.library_id) {
                    return false;
                }
                true
            }
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
            if f.exclude_library_ids.iter().any(|id| id == &item.library_id) {
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

            // 4. Max age cutoff
            if let Some(days) = f.max_age_days {
                if days > 0 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let cutoff = now - (days as i64 * 86_400);
                    if item.added_at < cutoff {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Resolves an entire screen layout concurrently.
    pub async fn resolve_screen(&self, screen: ScreenLayout, user_id: &str) -> ScreenLayout {
        self.resolve_screen_with_unlocked(screen, user_id, &[]).await
    }

    /// Resolves an entire screen layout concurrently with unlocked private libraries.
    pub async fn resolve_screen_with_unlocked(
        &self,
        screen: ScreenLayout,
        user_id: &str,
        unlocked_ids: &[String],
    ) -> ScreenLayout {
        let default_library_id = self.resolve_library_id_for_screen(&screen.id).await;
        let futures = screen
            .widgets
            .into_iter()
            .map(|widget| self.resolve_widget(widget, user_id, unlocked_ids, default_library_id.as_deref()));
        let widgets = join_all(futures).await;

        ScreenLayout {
            id: screen.id,
            title: screen.title,
            widgets,
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
                data: _,
            } => {
                let card = self
                    .resolve_hero_banner_data(&binding, user_id, unlocked_ids, default_library_id)
                    .await;
                WidgetNode::HeroBanner {
                    id,
                    binding,
                    data: card,
                }
            }
            WidgetNode::Carousel {
                id,
                title,
                binding,
                items: _,
                next_cursor: _,
            } => {
                let (cards, next_cursor, _) = self
                    .resolve_widget_data_with_library(&binding, user_id, 0, unlocked_ids, default_library_id)
                    .await
                    .unwrap_or_else(|err| {
                        tracing::warn!("Failed to resolve carousel {id}: {err}");
                        (Vec::new(), None, None)
                    });

                WidgetNode::Carousel {
                    id,
                    title,
                    binding,
                    items: Some(cards),
                    next_cursor,
                }
            }
            WidgetNode::Grid {
                id,
                title,
                binding,
                columns,
                items: _,
                next_cursor: _,
                total_count: _,
            } => {
                let (cards, next_cursor, total_count) = self
                    .resolve_widget_data_with_library(&binding, user_id, 0, unlocked_ids, default_library_id)
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
                    .is_item_matching_filters(&item, binding.filters.as_ref(), unlocked_ids, effective_library_id)
                    .await
                {
                    Some(item)
                } else {
                    None
                }
            }
            QueryMacro::SpotlightItem { item_id: None } => self
                .media_repo
                .find_spotlight_candidate(effective_library_id, unlocked_ids, binding.filters.as_ref())
                .await
                .ok()
                .flatten(),
            QueryMacro::ItemDetails { item_id } => {
                let item = self.media_repo.get_by_id(item_id).await.ok().flatten()?;
                if self
                    .is_item_matching_filters(&item, binding.filters.as_ref(), unlocked_ids, effective_library_id)
                    .await
                {
                    Some(item)
                } else {
                    None
                }
            }
            _ => {
                let (cards, _, _) = self
                    .resolve_widget_data_with_library(binding, user_id, 0, unlocked_ids, default_library_id)
                    .await
                    .ok()?;
                return cards.into_iter().next();
            }
        };

        if let Some(item) = candidate {
            let playback = if let Some(item_id) = item.id {
                self.playback_repo
                    .get_state(user_id, item_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
            Some(to_card_view_model(&item, playback.as_ref()))
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
        self.resolve_widget_data_with_library(binding, user_id, offset, &[], None).await
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
        self.resolve_widget_data_with_library(binding, user_id, offset, unlocked_ids, None).await
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

                let mut cards = Vec::with_capacity(page_states.len());
                for state in &page_states {
                    if let Some(item) = items_map.get(&state.media_item_id) {
                        if self
                            .is_item_matching_filters(item, binding.filters.as_ref(), unlocked_ids, effective_library_id)
                            .await
                        {
                            cards.push(to_card_view_model(item, Some(state)));
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
                    .find_recently_added_paginated(
                        effective_library_id,
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
                    .find_top_rated_paginated(
                        effective_library_id,
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
                    .find_by_genre_paginated(
                        genre,
                        effective_library_id,
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
                    .find_by_library_paginated(
                        target_lib,
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
                            .is_item_matching_filters(&i, binding.filters.as_ref(), unlocked_ids, effective_library_id)
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
                        .find_spotlight_candidate(effective_library_id, unlocked_ids, binding.filters.as_ref())
                        .await?
                };

                if let Some(item) = candidate {
                    let playback = if let Some(id) = item.id {
                        self.playback_repo.get_state(user_id, id).await?
                    } else {
                        None
                    };
                    let card = to_card_view_model(&item, playback.as_ref());
                    Ok((vec![card], None, None))
                } else {
                    Ok((Vec::new(), None, None))
                }
            }
            QueryMacro::ItemDetails { item_id } => {
                if let Some(item) = self.media_repo.get_by_id(*item_id).await? {
                    if self
                        .is_item_matching_filters(&item, binding.filters.as_ref(), unlocked_ids, effective_library_id)
                        .await
                    {
                        let playback = self.playback_repo.get_state(user_id, *item_id).await?;
                        let card = to_card_view_model(&item, playback.as_ref());
                        Ok((vec![card], None, None))
                    } else {
                        Ok((Vec::new(), None, None))
                    }
                } else {
                    Ok((Vec::new(), None, None))
                }
            }
        }
    }

    /// Hydrates a slice of `MediaItem`s by batch-fetching playback states.
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

        items
            .iter()
            .map(|item| to_card_view_model(item, item.id.and_then(|id| states.get(&id))))
            .collect()
    }

    /// Resolves comprehensive single item details including playback state and child episodes.
    pub async fn resolve_item_details(
        &self,
        item_id: i64,
        user_id: &str,
    ) -> Result<Option<ItemDetailsPayload>> {
        self.resolve_item_details_with_unlocked(item_id, user_id, &[]).await
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
        let card = to_card_view_model(&item, playback.as_ref());

        let overview = item.metadata.overview.clone();
        let genres = item.metadata.genres.clone();
        let duration_seconds = if item.technical.duration_seconds > 0 {
            Some(item.technical.duration_seconds as u64)
        } else {
            None
        };
        let technical = Some(item.technical.clone());
        let stream_url = format!("/api/v1/stream/{item_id}");
        let resume_position_seconds = playback
            .as_ref()
            .map(|p| p.playback_position_seconds.max(0) as u64);

        let episodes = if item.item_type == MediaType::Show {
            let episode_items = self.media_repo.find_episodes_by_series(&item.title).await?;
            let ep_cards = self.hydrate_items_to_cards(episode_items, user_id).await;
            Some(ep_cards)
        } else {
            None
        };

        Ok(Some(ItemDetailsPayload {
            card,
            overview,
            genres,
            duration_seconds,
            technical,
            stream_url,
            resume_position_seconds,
            episodes,
        }))
    }
}
