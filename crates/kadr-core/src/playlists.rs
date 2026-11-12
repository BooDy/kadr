use serde::{Deserialize, Deserializer, Serialize};

use crate::ast::CardViewModel;

fn bool_from_opt<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<bool>::deserialize(deserializer).map(|opt| opt.unwrap_or(false))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_public: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub owner_user_id: String,
    pub owner_username: String,
    pub is_public: bool,
    pub is_editable: bool,
    pub visible_item_count: usize,
    pub locked_item_count: usize,
    pub total_duration_seconds: i64,
    pub cover_poster_urls: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains_queried_item: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistItemView {
    pub position: usize,
    pub added_at: i64,
    pub duration_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_position_seconds: Option<i64>,
    pub card: CardViewModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistDetail {
    pub summary: PlaylistSummary,
    pub items: Vec<PlaylistItemView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistsListResponse {
    pub playlists: Vec<PlaylistSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddShowItemsSpec {
    pub show_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season_number: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFolderItemsSpec {
    pub library_id: String,
    pub path: String,
    #[serde(default, deserialize_with = "bool_from_opt")]
    pub recursive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddPlaylistItemsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_ids: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<AddShowItemsSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<AddFolderItemsSpec>,
}

impl AddPlaylistItemsRequest {
    pub fn is_empty(&self) -> bool {
        let ids_empty = self.item_ids.as_ref().is_none_or(Vec::is_empty);
        ids_empty && self.show.is_none() && self.folder.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePlaylistRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "bool_from_opt")]
    pub is_public: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<AddPlaylistItemsRequest>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePlaylistRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddPlaylistItemsResponse {
    pub added_count: usize,
    pub skipped_duplicate_count: usize,
    pub summary: PlaylistSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReorderPlaylistItemsRequest {
    pub item_ids: Vec<i64>,
}
