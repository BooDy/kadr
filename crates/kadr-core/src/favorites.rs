use serde::{Deserialize, Serialize};

use crate::ast::CardViewModel;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FavoriteItemView {
    pub favorited_at: i64,
    pub duration_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_position_seconds: Option<i64>,
    pub card: CardViewModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FavoritesListResponse {
    pub items: Vec<FavoriteItemView>,
    pub total_count: u64,
    pub locked_item_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteStatusResponse {
    pub item_id: i64,
    pub is_favorite: bool,
}
