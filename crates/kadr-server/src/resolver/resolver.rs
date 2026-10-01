use futures::future::join_all;
use kadr_core::ast::{
    CardViewModel, ItemDetailsPayload, QueryMacro, ScreenLayout, WidgetNode, WidgetQueryBinding,
};
use kadr_core::models::MediaType;
use kadr_storage::error::Result;
use kadr_storage::repos::{MediaItemRepository, PlaybackRepository};

use super::card::to_card_view_model;

/// Concurrent async resolver for Declarative Widget AST layouts and queries.
#[derive(Clone)]
pub struct WidgetResolver {
    media_repo: MediaItemRepository,
    playback_repo: PlaybackRepository,
}

impl WidgetResolver {
    /// Creates a new `WidgetResolver` with the provided storage repositories.
    pub fn new(media_repo: MediaItemRepository, playback_repo: PlaybackRepository) -> Self {
        Self {
            media_repo,
            playback_repo,
        }
    }

    /// Resolves an entire screen layout concurrently.
    pub async fn resolve_screen(&self, screen: ScreenLayout, user_id: &str) -> ScreenLayout {
        let futures = screen
            .widgets
            .into_iter()
            .map(|widget| self.resolve_widget(widget, user_id));
        let widgets = join_all(futures).await;

        ScreenLayout {
            id: screen.id,
            title: screen.title,
            widgets,
        }
    }

    async fn resolve_widget(&self, widget: WidgetNode, user_id: &str) -> WidgetNode {
        match widget {
            WidgetNode::HeroBanner {
                id,
                binding,
                data: _,
            } => {
                let card = self.resolve_hero_banner_data(&binding, user_id).await;
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
                    .resolve_widget_data(&binding, user_id, 0)
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
                    .resolve_widget_data(&binding, user_id, 0)
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
                    .resolve_item_details(item_id, user_id)
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
    ) -> Option<CardViewModel> {
        let candidate = match binding.macro_type {
            QueryMacro::SpotlightItem { item_id: Some(id) } => {
                self.media_repo.get_by_id(id).await.ok().flatten()
            }
            QueryMacro::SpotlightItem { item_id: None } => {
                self.media_repo.find_spotlight_candidate().await.ok().flatten()
            }
            QueryMacro::ItemDetails { item_id } => {
                self.media_repo.get_by_id(item_id).await.ok().flatten()
            }
            _ => {
                let (cards, _, _) = self.resolve_widget_data(binding, user_id, 0).await.ok()?;
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
        match &binding.macro_type {
            QueryMacro::ContinueWatching => {
                let states = self.playback_repo.get_continue_watching(user_id).await?;
                let page_states: Vec<_> = states
                    .into_iter()
                    .skip(offset as usize)
                    .take(binding.limit as usize)
                    .collect();

                let has_more = page_states.len() == binding.limit as usize;

                let futures = page_states.into_iter().map(|state| {
                    let media_repo = self.media_repo.clone();
                    async move {
                        if let Ok(Some(item)) = media_repo.get_by_id(state.media_item_id).await {
                            Some(to_card_view_model(&item, Some(&state)))
                        } else {
                            None
                        }
                    }
                });

                let cards: Vec<CardViewModel> =
                    join_all(futures).await.into_iter().flatten().collect();

                let next_cursor = if has_more {
                    Some((offset + cards.len() as u32).to_string())
                } else {
                    None
                };

                Ok((cards, next_cursor, None))
            }
            QueryMacro::RecentlyAdded => {
                let items = self
                    .media_repo
                    .find_recently_added(None, binding.limit, offset)
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
                    .find_top_rated(binding.limit, offset)
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
                    .find_by_genre(genre, binding.limit, offset)
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
                let (items, total) = self
                    .media_repo
                    .find_by_library_paginated(
                        library_id,
                        binding.limit,
                        offset,
                        binding.sort.as_deref(),
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
                    self.media_repo.get_by_id(*id).await?
                } else {
                    self.media_repo.find_spotlight_candidate().await?
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
                    let playback = self.playback_repo.get_state(user_id, *item_id).await?;
                    let card = to_card_view_model(&item, playback.as_ref());
                    Ok((vec![card], None, None))
                } else {
                    Ok((Vec::new(), None, None))
                }
            }
        }
    }

    /// Hydrates a slice of `MediaItem`s by fetching each item's playback state concurrently.
    async fn hydrate_items_to_cards(
        &self,
        items: Vec<kadr_core::models::MediaItem>,
        user_id: &str,
    ) -> Vec<CardViewModel> {
        let futures = items.into_iter().map(|item| {
            let playback_repo = self.playback_repo.clone();
            let user_id = user_id.to_string();
            async move {
                let playback = if let Some(id) = item.id {
                    playback_repo.get_state(&user_id, id).await.ok().flatten()
                } else {
                    None
                };
                to_card_view_model(&item, playback.as_ref())
            }
        });

        join_all(futures).await
    }

    /// Resolves comprehensive single item details including playback state and child episodes.
    pub async fn resolve_item_details(
        &self,
        item_id: i64,
        user_id: &str,
    ) -> Result<Option<ItemDetailsPayload>> {
        let item = match self.media_repo.get_by_id(item_id).await? {
            Some(i) => i,
            None => return Ok(None),
        };

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
