# Manual Ordered Playlists & Client Playback Queue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable users to create, curate, share, pin to Declarative Widget AST screens, and play manual ordered playlists across Kadr's Rust server (`crates/`), React Web UI (`web/`), and external client documentation (`docs/CLIENT_GUIDE.md`, `README.md`).

**Architecture:** `kadr-core` defines playlist domain/wire models (`Playlist`, `PlaylistSummary`, `PlaylistItemView`, `PlaylistDetail`, and request/response DTOs), extends `QueryMacro` with `PlaylistItems { playlist_id }`, and extends `SystemEvent` with metadata-free `PlaylistUpdated`. `kadr-storage` adds SQLite migration `009_playlists.sql`, `PlaylistRepository` enforcing fail-closed private PIN-locked library isolation (`locked_item_count` without metadata leakage), and startup deduplication safety in `MediaItemRepository::deduplicate_media_items`. `kadr-server` exposes `/api/v1/playlists/*` (including 1-step Create & Add, Show/Season/Folder batch expansion, and visible-permutation reordering) and resolves `PlaylistItems` rails in `WidgetResolver`, while `web/` adds `PlaylistsScreen`, `PlaylistDetailView`, `AddToPlaylistModal`, Layout Studio support, and `CinemaPlayer` multi-track queue playback.

**Tech Stack:** Rust (Edition 2021), Axum 0.8, Tokio, `deadpool-sqlite` + `rusqlite` (WAL mode, `rusqlite_migration`), Serde; React 19, TypeScript 5.7, Vite 6, Vitest + Testing Library, Tailwind CSS v4, `lucide-react`.

## Global Constraints

- Memory & Portability Budget: Strictly preserve Kadr's `<= 30 MB` idle RSS target and pure-Rust static `musl` portability (`deadpool-sqlite` + `rusqlite` WAL mode, zero new native C/C++ dependencies).
- Existing Streaming & Scrobbling Pipeline: Preserve existing direct-play (`GET /api/v1/stream/{item_id}` HTTP 206 byte-range) and universal HLS fallback (`GET /api/v1/media/{item_id}/hls/master.m3u8`) routes alongside per-item playback sessions (`POST /api/v1/playback/sessions`, 10s `POST /api/v1/playback/{session_id}/progress` heartbeats, `DELETE /api/v1/playback/sessions/{session_id}`).
- Fail-Closed Private Library Isolation: Items belonging to PIN-locked private libraries (`libraries.is_private = 1`) are never leaked through playlist summaries, `cover_poster_urls`, `total_duration_seconds`, detail `items` lists, `contains_queried_item`, AST widgets, or SSE events unless the caller supplies a valid signed `UnlockedLibraries` token (`X-Kadr-Unlocked`, `X-Library-Unlock-Token`, `?unlocked=`, or `?unlock_token=`). Locked items are counted strictly as an aggregate integer in `PlaylistSummary.locked_item_count`.
- Access Control Matrix:
  - Read / Play / Hydrate (`GET`, Widget AST): allowed when `owner_user_id == auth_user.id || is_public`; returns `404 {"error": "Playlist not found"}` when private and not owned by caller; returns `200 OK` with `is_editable: auth_user.role == UserRole::Admin` on public playlists owned by another user.
  - Mutate Metadata / Items / Delete (`PATCH`, `POST .../items`, `DELETE .../items/{media_item_id}`, `PUT .../items/order`, `DELETE`): allowed when `owner_user_id == auth_user.id || auth_user.role == UserRole::Admin`; returns `404 {"error": "Playlist not found"}` when private to another user (do not leak existence) and `403 {"error": "Forbidden"}` when public and caller is neither owner nor `Admin`.
- Input Validation & Set-Membership: `playlists.name` trimmed length must be `1..=120` UTF-8 characters (`chars().count()`); `playlists.description` optional trimmed length `<= 500` UTF-8 characters (`chars().count()`, empty strings normalize to `None` / `NULL`); `PRIMARY KEY (playlist_id, media_item_id)` enforces set-membership per playlist with dense `0..N-1` `position` ordering.
- Non-Colliding Parallel Track Ownership:
  - **Track A (`backend`)** touches `crates/**` ONLY (Tasks 1–3).
  - **Track B (`frontend`)** touches `web/**` ONLY (Tasks 4–6).
  - **Track C (`client-docs`)** touches `docs/CLIENT_GUIDE.md` and `README.md` ONLY (Task 7).
- Quality Gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cd web && npm test -- --run && npm run build` must pass with zero errors or warnings.

---

## Parallel Track & File Structure Map

### Track A: `backend` (Owns `crates/**` ONLY)
- **Create:** `crates/kadr-core/src/playlists.rs` — `Playlist`, `PlaylistSummary`, `PlaylistItemView`, `PlaylistDetail`, `PlaylistsListResponse`, `CreatePlaylistRequest`, `UpdatePlaylistRequest`, `AddShowItemsSpec`, `AddFolderItemsSpec`, `AddPlaylistItemsRequest`, `AddPlaylistItemsResponse`, `ReorderPlaylistItemsRequest`.
- **Modify:** `crates/kadr-core/src/lib.rs` — export `pub mod playlists; pub use playlists::*;`.
- **Modify:** `crates/kadr-core/src/ast.rs` — add `PlaylistItems { playlist_id: String }` to `QueryMacro`.
- **Modify:** `crates/kadr-core/src/events.rs` — add `PlaylistUpdated { playlist_id, user_id, action, timestamp }` (`#[serde(rename = "playlist:updated")]`) to `SystemEvent` and `event_type()`.
- **Create:** `crates/kadr-core/tests/playlists_test.rs` — unit tests for playlist model serde contracts, `QueryMacro::PlaylistItems`, and `SystemEvent::PlaylistUpdated`.
- **Create:** `crates/kadr-storage/src/migrations/009_playlists.sql` — `playlists` and `playlist_items` tables and indexes.
- **Modify:** `crates/kadr-storage/src/migrations.rs` — register `009_playlists.sql`.
- **Create:** `crates/kadr-storage/src/repos/playlist_repo.rs` — `PlaylistRepository` CRUD, privacy-isolated summary/detail queries, batch `INSERT OR IGNORE` append, dense `0..N-1` compaction on delete, and locked-preserving visible permutation reorder.
- **Modify:** `crates/kadr-storage/src/repos/media_item_repo.rs` — expose `pub(crate)` row helpers and migrate `playlist_items` + re-compact `position` inside `deduplicate_media_items`.
- **Modify:** `crates/kadr-storage/src/repos/mod.rs` & `crates/kadr-storage/src/lib.rs` — export `PlaylistRepository`.
- **Modify:** `crates/kadr-server/src/resolver/resolver.rs` — wire `PlaylistRepository` into `WidgetResolver` and resolve `QueryMacro::PlaylistItems` with pagination, filters, and graceful fallback on missing/private playlists.
- **Create:** `crates/kadr-server/src/api/playlist_routes.rs` — Axum handlers for all 8 `/api/v1/playlists/*` endpoints including 1-step Create & Add, Show/Season/Folder expansion, `LIBRARY_LOCKED` checks, and `SystemEvent::PlaylistUpdated` emission.
- **Modify:** `crates/kadr-server/src/api/mod.rs` — register `pub mod playlist_routes;`, merge `playlist_routes::playlist_routes()`, and attach `Extension(PlaylistRepository)` inside `create_router_with_ingest`.
- **Create:** `crates/kadr-server/tests/e2e_playlists_test.rs` — end-to-end integration tests covering multi-user permissions, Create & Add, Show/Season/Folder expansion, PIN-locked library isolation, Widget AST hydration/pagination, and SSE events.

### Track B: `frontend` (Owns `web/**` ONLY)
- **Modify:** `web/src/types/index.ts` — add TypeScript playlist interfaces (`PlaylistSummary`, `PlaylistItemView`, `PlaylistDetail`, `CreatePlaylistPayload`, `UpdatePlaylistPayload`, `AddShowItemsSpec`, `AddFolderItemsSpec`, `AddPlaylistItemsPayload`, `AddPlaylistItemsResponse`, `QueueItem`, `PlaylistAddTarget`) and extend `QueryMacro` and `SystemEvent`.
- **Modify:** `web/src/api/client.ts` — add 8 playlist methods to `ApiClient`.
- **Modify:** `web/src/api/client.test.ts` — unit tests for all 8 `ApiClient` playlist methods, `contains_item_id` query encoding, and auth/unlock header forwarding.
- **Create:** `web/src/components/playlists/PlaylistFormModal.tsx` — modal for creating and editing playlist `name`, `description`, and `is_public`.
- **Create:** `web/src/components/playlists/PlaylistDetailView.tsx` — detail header, 2x2 mosaic, locked-items notice banner, `Play All`, Fisher-Yates `Shuffle`, `Edit`/`Delete`, ordered track rows, keyboard `ChevronUp`/`ChevronDown` + HTML5 drag-and-drop reorder, and item removal.
- **Create:** `web/src/components/playlists/PlaylistsScreen.tsx` — `"My Playlists"` and `"Shared on Server"` sections, 2x2 mosaic cards, `+N locked` badges, and switching into `PlaylistDetailView`.
- **Create:** `web/src/components/playlists/AddToPlaylistModal.tsx` — modal supporting `{ kind: 'item' | 'season' | 'folder' }`, membership checkmarks (`contains_queried_item`), recursive folder toggle, and 1-step `"Create new playlist & add"`.
- **Modify:** `web/src/components/browse/ItemDetailsModal.tsx` — `+ Playlist` header button, `Add Season` button on season bar, and per-episode `Add <title> to playlist` icon button.
- **Modify:** `web/src/components/browse/SpotlightWidget.tsx` — `Add to Playlist` hero CTA button.
- **Modify:** `web/src/components/browse/FolderBrowser.tsx` — `Add Folder to Playlist` action button.
- **Modify:** `web/src/components/studio/WidgetConfigModal.tsx` — `playlist_items` macro option and playlist selector with private-playlist shared-screen warning.
- **Create:** `web/src/components/playlists/PlaylistsScreen.test.tsx` & `web/src/components/playlists/AddToPlaylistModal.test.tsx` — Vitest suites for playlist views and add modal workflows.
- **Modify:** `web/src/components/player/CinemaPlayer.tsx` & `web/src/components/player/PlayerControls.tsx` — queue props, per-item state/ref/session reset on `itemId` change, `<video onEnded>` auto-advance after final scrobble heartbeat, `SkipBack`/`SkipForward` + `Shift+P`/`Shift+N`, track counter chip, and Queue popover drawer pausing the 3s inactivity timer.
- **Modify:** `web/src/App.tsx` — top-nav `Playlists` button (`ListVideo`), `previousView`, `activePlaylistId`, `playerQueue` / `playerQueueIndex`, `handlePlayQueue`, and return-to-previous-view on player close.
- **Modify:** `web/src/components/player/CinemaPlayer.test.tsx`, `web/src/components/player/PlayerControls.test.tsx`, `web/src/App.test.tsx` — Vitest queue and return-navigation tests.

### Track C: `client-docs` (Owns `docs/CLIENT_GUIDE.md` & `README.md` ONLY)
- **Modify:** `docs/CLIENT_GUIDE.md` — add Table of Contents entry, Section 9 `Playlists & Client Queue Playback` (renumbering existing Sections 9–10 to 10–11), `playlist_items` AST macro docs, `playlist:updated` SSE payload docs, ExoPlayer/AVPlayer native queue playback recipe, and updated HTTP Status & Error Matrix.
- **Modify:** `README.md` — add `Playlists & Queue Playback` subsection under `REST & Streaming API Reference` and note `playlist:updated` in SSE docs.

---

## Track A: Backend (`crates/**` ONLY)

### Task 1: `kadr-core` Playlist Models, `QueryMacro::PlaylistItems` & `SystemEvent::PlaylistUpdated`

**Files:**
- Create: `crates/kadr-core/src/playlists.rs`
- Modify: `crates/kadr-core/src/lib.rs:1-15`
- Modify: `crates/kadr-core/src/ast.rs:136-147`
- Modify: `crates/kadr-core/src/events.rs:12-59`
- Modify: `crates/kadr-server/src/resolver/resolver.rs:645-650` (temporary exhaustive-match stub replaced in Task 3)
- Test: `crates/kadr-core/tests/playlists_test.rs`

**Interfaces:**
- Consumes: `kadr_core::ast::CardViewModel` (`crates/kadr-core/src/ast.rs`)
- Produces:
  - `kadr_core::playlists::{Playlist, PlaylistSummary, PlaylistItemView, PlaylistDetail, PlaylistsListResponse, CreatePlaylistRequest, UpdatePlaylistRequest, AddShowItemsSpec, AddFolderItemsSpec, AddPlaylistItemsRequest, AddPlaylistItemsResponse, ReorderPlaylistItemsRequest}` (also re-exported at `kadr_core::*`)
  - `kadr_core::ast::QueryMacro::PlaylistItems { playlist_id: String }` (JSON: `{"playlist_items": {"playlist_id": "<uuid>"}}`)
  - `kadr_core::events::SystemEvent::PlaylistUpdated { playlist_id: String, user_id: String, action: String, timestamp: i64 }` (JSON tag `"playlist:updated"`, `event_type() -> "playlist:updated"`)

- [ ] **Step 1: Write the failing `kadr-core` integration tests**

Create `crates/kadr-core/tests/playlists_test.rs`:

```rust
use kadr_core::ast::{CardViewModel, QueryMacro, WidgetQueryBinding};
use kadr_core::events::SystemEvent;
use kadr_core::playlists::{
    AddFolderItemsSpec, AddPlaylistItemsRequest, AddPlaylistItemsResponse, AddShowItemsSpec,
    CreatePlaylistRequest, PlaylistDetail, PlaylistItemView, PlaylistSummary,
    PlaylistsListResponse, ReorderPlaylistItemsRequest, UpdatePlaylistRequest,
};

#[test]
fn playlist_summary_and_detail_serialize_exact_wire_fields() {
    let summary = PlaylistSummary {
        id: "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d".to_string(),
        name: "Sci-Fi Marathon".to_string(),
        description: None,
        owner_user_id: "u-123".to_string(),
        owner_username: "alex".to_string(),
        is_public: true,
        is_editable: true,
        visible_item_count: 1,
        locked_item_count: 2,
        total_duration_seconds: 8880,
        cover_poster_urls: vec!["/api/v1/artwork/10/poster".to_string()],
        created_at: 1762940000,
        updated_at: 1762941800,
        contains_queried_item: None,
    };

    let summary_json = serde_json::to_value(&summary).expect("serialize summary");
    assert!(summary_json.get("description").is_none());
    assert!(summary_json.get("contains_queried_item").is_none());
    assert_eq!(summary_json["locked_item_count"], 2);

    let detail = PlaylistDetail {
        summary: PlaylistSummary {
            contains_queried_item: Some(true),
            ..summary
        },
        items: vec![PlaylistItemView {
            position: 0,
            added_at: 1762940010,
            duration_seconds: 8880,
            resume_position_seconds: Some(1240),
            card: CardViewModel {
                id: 10,
                title: "Arrival".to_string(),
                subtitle: Some("2016 • 4K".to_string()),
                poster_url: Some("/api/v1/artwork/10/poster".to_string()),
                backdrop_url: Some("/api/v1/artwork/10/backdrop".to_string()),
                media_type: "movie".to_string(),
                playback_progress: Some(0.14),
                rating: Some(7.9),
                release_year: Some(2016),
                badge: Some("RESUME".to_string()),
                season: None,
                episode: None,
            },
        }],
    };

    let encoded = serde_json::to_string(&detail).expect("serialize detail");
    let decoded: PlaylistDetail = serde_json::from_str(&encoded).expect("deserialize detail");
    assert_eq!(decoded, detail);
    assert_eq!(decoded.summary.contains_queried_item, Some(true));
    assert_eq!(decoded.items[0].card.id, 10);

    let list = PlaylistsListResponse {
        playlists: vec![decoded.summary.clone()],
    };
    let list_val = serde_json::to_value(&list).expect("serialize list");
    assert_eq!(list_val["playlists"][0]["id"], "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d");
}

#[test]
fn playlist_request_and_response_payloads_round_trip() {
    let create_req: CreatePlaylistRequest = serde_json::from_str(
        r#"{
            "name": "Weekend Queue",
            "description": "4K cuts",
            "is_public": true,
            "items": {
                "item_ids": [10, 14],
                "show": { "show_id": 50, "season_number": 1 },
                "folder": { "library_id": "lib-1", "path": "Season 01", "recursive": true }
            }
        }"#,
    )
    .expect("deserialize CreatePlaylistRequest");

    assert_eq!(create_req.name, "Weekend Queue");
    assert!(create_req.is_public);
    let items = create_req.items.expect("items present");
    assert_eq!(items.item_ids, Some(vec![10, 14]));
    assert_eq!(
        items.show,
        Some(AddShowItemsSpec {
            show_id: 50,
            season_number: Some(1),
        })
    );
    assert_eq!(
        items.folder,
        Some(AddFolderItemsSpec {
            library_id: "lib-1".to_string(),
            path: "Season 01".to_string(),
            recursive: true,
        })
    );

    let patch_req: UpdatePlaylistRequest =
        serde_json::from_str(r#"{"description": ""}"#).expect("deserialize UpdatePlaylistRequest");
    assert_eq!(patch_req.name, None);
    assert_eq!(patch_req.description.as_deref(), Some(""));
    assert_eq!(patch_req.is_public, None);

    let reorder_req: ReorderPlaylistItemsRequest =
        serde_json::from_str(r#"{"item_ids": [14, 10]}"#).expect("deserialize reorder");
    assert_eq!(reorder_req.item_ids, vec![14, 10]);

    let add_req = AddPlaylistItemsRequest {
        item_ids: Some(vec![1]),
        show: None,
        folder: None,
    };
    assert!(!add_req.is_empty());

    let empty_req = AddPlaylistItemsRequest::default();
    assert!(empty_req.is_empty());

    let add_res = AddPlaylistItemsResponse {
        added_count: 2,
        skipped_duplicate_count: 1,
        summary: PlaylistSummary {
            id: "pl-1".to_string(),
            name: "Queue".to_string(),
            description: None,
            owner_user_id: "u-1".to_string(),
            owner_username: "admin".to_string(),
            is_public: false,
            is_editable: true,
            visible_item_count: 3,
            locked_item_count: 0,
            total_duration_seconds: 3600,
            cover_poster_urls: vec![],
            created_at: 100,
            updated_at: 200,
            contains_queried_item: None,
        },
    };
    let val = serde_json::to_value(&add_res).unwrap();
    assert_eq!(val["added_count"], 2);
    assert_eq!(val["skipped_duplicate_count"], 1);
}

#[test]
fn query_macro_playlist_items_and_playlist_updated_event_serialize() {
    let binding = WidgetQueryBinding::new(QueryMacro::PlaylistItems {
        playlist_id: "pl-uuid-1".to_string(),
    });
    let json = serde_json::to_value(&binding).expect("serialize binding");
    assert_eq!(
        json["macro_type"],
        serde_json::json!({ "playlist_items": { "playlist_id": "pl-uuid-1" } })
    );
    let parsed: WidgetQueryBinding = serde_json::from_value(json).expect("deserialize binding");
    assert_eq!(parsed, binding);

    let event = SystemEvent::PlaylistUpdated {
        playlist_id: "pl-uuid-1".to_string(),
        user_id: "u-1".to_string(),
        action: "items_changed".to_string(),
        timestamp: 1762942100,
    };
    assert_eq!(event.event_type(), "playlist:updated");
    let event_json = serde_json::to_value(&event).expect("serialize event");
    assert_eq!(event_json["type"], "playlist:updated");
    assert_eq!(event_json["payload"]["playlist_id"], "pl-uuid-1");
    assert_eq!(event_json["payload"]["user_id"], "u-1");
    assert_eq!(event_json["payload"]["action"], "items_changed");
    assert_eq!(event_json["payload"]["timestamp"], 1762942100);
}
```

- [ ] **Step 2: Run `cargo test -p kadr-core` to verify failure**

Run: `cargo test -p kadr-core --test playlists_test`
Expected: FAIL with `unresolved import kadr_core::playlists` and missing enum variants `QueryMacro::PlaylistItems` and `SystemEvent::PlaylistUpdated`.

- [ ] **Step 3: Implement `crates/kadr-core/src/playlists.rs`, update `ast.rs`, `events.rs`, `lib.rs`, and add stub arm in `resolver.rs`**

Create `crates/kadr-core/src/playlists.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::ast::CardViewModel;

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
    #[serde(default)]
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
        let ids_empty = self.item_ids.as_ref().map_or(true, Vec::is_empty);
        ids_empty && self.show.is_none() && self.folder.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePlaylistRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
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
```

Modify `crates/kadr-core/src/lib.rs`:

```rust
pub mod ast;
pub mod error;
pub mod events;
pub mod models;
pub mod playlists;
pub mod plugin;
pub mod subtitles;
pub mod transcoding;

pub use ast::*;
pub use error::CoreError;
pub use events::*;
pub use models::*;
pub use playlists::*;
pub use plugin::*;
pub use subtitles::*;
pub use transcoding::*;
```

Modify `QueryMacro` in `crates/kadr-core/src/ast.rs:136-147`:

```rust
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
    PlaylistItems { playlist_id: String },
}
```

Modify `SystemEvent` in `crates/kadr-core/src/events.rs`:

```rust
    #[serde(rename = "plugin:error")]
    PluginError {
        plugin_id: String,
        message: String,
        timestamp: i64,
    },
    #[serde(rename = "playlist:updated")]
    PlaylistUpdated {
        playlist_id: String,
        user_id: String,
        action: String,
        timestamp: i64,
    },
```

And in `SystemEvent::event_type(&self)`:

```rust
            Self::PluginError { .. } => "plugin:error",
            Self::PlaylistUpdated { .. } => "playlist:updated",
```

Add an exhaustive match arm in `crates/kadr-server/src/resolver/resolver.rs` right after `QueryMacro::ItemDetails { item_id } => { ... }` so the entire workspace compiles cleanly before Task 3:

```rust
            QueryMacro::PlaylistItems { .. } => Ok((Vec::new(), None, Some(0))),
```

- [ ] **Step 4: Run `kadr-core` tests and workspace check**

Run: `cargo test -p kadr-core && cargo check --workspace`
Expected: PASS across all `kadr-core` test suites (`playlists_test`, `ast_test`, `events_test`) and zero compilation errors across workspace crates.

- [ ] **Step 5: Commit Task 1**

```bash
git add crates/kadr-core/src/playlists.rs crates/kadr-core/src/lib.rs crates/kadr-core/src/ast.rs crates/kadr-core/src/events.rs crates/kadr-core/tests/playlists_test.rs crates/kadr-server/src/resolver/resolver.rs
git commit -m "feat(core): add playlist domain models, AST macro, and SSE event"
```

---

### Task 2: `kadr-storage` Migration `009_playlists.sql`, `PlaylistRepository` & Startup Deduplication Safety

**Files:**
- Create: `crates/kadr-storage/src/migrations/009_playlists.sql`
- Modify: `crates/kadr-storage/src/migrations.rs:5-17`
- Create: `crates/kadr-storage/src/repos/playlist_repo.rs`
- Modify: `crates/kadr-storage/src/repos/media_item_repo.rs:426-508`
- Modify: `crates/kadr-storage/src/repos/mod.rs:1-17`
- Modify: `crates/kadr-storage/src/lib.rs:9-13`
- Test: `crates/kadr-storage/src/repos/playlist_repo.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes:
  - `kadr_core::playlists::{Playlist, PlaylistSummary}`
  - `kadr_core::models::MediaItem`
  - `kadr_storage::error::{Result, StorageError}` (`StorageError::InvalidInput(String)`, `StorageError::NotFound(String)`)
- Produces:
  - Migration `009_playlists.sql` (`playlists` and `playlist_items` tables + indexes)
  - `kadr_storage::repos::PlaylistRepository` (re-exported as `kadr_storage::PlaylistRepository`):
    - `pub fn new(pool: Pool) -> Self`
    - `pub async fn create_playlist(&self, id: &str, owner_user_id: &str, name: &str, description: Option<&str>, is_public: bool) -> Result<Playlist>`
    - `pub async fn get_playlist_raw(&self, playlist_id: &str) -> Result<Option<(Playlist, String)>>`
    - `pub async fn update_playlist(&self, playlist_id: &str, name: Option<&str>, description: Option<Option<&str>>, is_public: Option<bool>) -> Result<Option<Playlist>>`
    - `pub async fn delete_playlist(&self, playlist_id: &str) -> Result<bool>`
    - `pub async fn get_playlist_summary_for_user(&self, playlist_id: &str, user_id: &str, is_admin: bool, unlocked_ids: &[String], contains_item_id: Option<i64>) -> Result<Option<PlaylistSummary>>`
    - `pub async fn list_playlists_for_user(&self, user_id: &str, is_admin: bool, unlocked_ids: &[String], contains_item_id: Option<i64>) -> Result<Vec<PlaylistSummary>>`
    - `pub async fn get_playlist_items_with_privacy(&self, playlist_id: &str, unlocked_ids: &[String]) -> Result<(Vec<(i64, MediaItem)>, usize)>`
    - `pub async fn add_items(&self, playlist_id: &str, media_item_ids: &[i64]) -> Result<(usize, usize)>`
    - `pub async fn remove_item(&self, playlist_id: &str, media_item_id: i64) -> Result<bool>`
    - `pub async fn reorder_visible_items(&self, playlist_id: &str, ordered_visible_ids: &[i64], unlocked_ids: &[String]) -> Result<()>`
  - Updated `MediaItemRepository::deduplicate_media_items(&self) -> Result<usize>` migrating `playlist_items.media_item_id` (`UPDATE OR IGNORE` + `DELETE`) and re-compacting `position` (`0..N-1`) for affected playlists before deleting duplicate `media_items`.

- [ ] **Step 1: Create `009_playlists.sql`, register it in `migrations.rs`, and write failing repository & deduplication tests**

Create `crates/kadr-storage/src/migrations/009_playlists.sql`:

```sql
CREATE TABLE IF NOT EXISTS playlists (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    is_public INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_playlists_owner_updated
    ON playlists(owner_user_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_playlists_public_updated
    ON playlists(is_public, updated_at DESC);

CREATE TABLE IF NOT EXISTS playlist_items (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, media_item_id)
);

CREATE INDEX IF NOT EXISTS idx_playlist_items_order
    ON playlist_items(playlist_id, position ASC);
```

Modify `crates/kadr-storage/src/migrations.rs`:

```rust
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/001_initial_schema.sql")),
        M::up(include_str!("migrations/002_playback_and_auth.sql")),
        M::up(include_str!("migrations/003_widget_query_indexes.sql")),
        M::up(include_str!("migrations/004_subtitles.sql")),
        M::up(include_str!("migrations/005_private_libraries.sql")),
        M::up(include_str!("migrations/006_multiple_library_paths.sql")),
        M::up(include_str!("migrations/007_plugins.sql")),
        M::up(include_str!("migrations/008_settings.sql")),
        M::up(include_str!("migrations/009_playlists.sql")),
    ])
}
```

Create `crates/kadr-storage/src/repos/playlist_repo.rs` with the `#[cfg(test)] mod tests` suite verifying CRUD/compaction, private-library isolation/reorder, and startup deduplication safety:

```rust
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::error::StorageError;
    use crate::pool::{create_in_memory_pool, initialize_database};
    use crate::repos::{
        LibraryRepository, MediaItemRepository, PlaylistRepository, UserRepository,
    };
    use kadr_core::models::{
        Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
    };

    fn sample_item(lib: &str, title: &str, path: PathBuf, dur: i64, poster: Option<&str>) -> MediaItem {
        MediaItem {
            id: None,
            library_id: lib.to_string(),
            item_type: MediaType::Movie,
            title: title.to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1000,
            file_path: path.clone(),
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            file_size: 1024,
            technical: TechnicalInfo {
                duration_seconds: dur,
                ..Default::default()
            },
            metadata: MediaMetadata {
                poster_path: poster.map(ToOwned::to_owned),
                ..Default::default()
            },
        }
    }

    #[tokio::test]
    async fn playlist_crud_privacy_reorder_and_dedup_safety() {
        let pool = create_in_memory_pool().unwrap();
        initialize_database(&pool).await.unwrap();

        let user_repo = UserRepository::new(pool.clone());
        let lib_repo = LibraryRepository::new(pool.clone());
        let media_repo = MediaItemRepository::new(pool.clone());
        let playlist_repo = PlaylistRepository::new(pool.clone());

        user_repo
            .create(&User {
                id: "u-1".to_string(),
                username: "alex".to_string(),
                pin_hash: "h".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();

        lib_repo
            .insert(&Library {
                id: "lib-pub".to_string(),
                name: "Public".to_string(),
                path: PathBuf::from("/pub"),
                media_type: MediaType::Movie,
                is_private: false,
                created_at: 100,
                ..Default::default()
            })
            .await
            .unwrap();

        lib_repo
            .insert(&Library {
                id: "lib-priv".to_string(),
                name: "Private".to_string(),
                path: PathBuf::from("/priv"),
                media_type: MediaType::Movie,
                is_private: true,
                pin_hash: Some("pin".to_string()),
                created_at: 100,
                ..Default::default()
            })
            .await
            .unwrap();

        let tmp = TempDir::new().unwrap();
        let real_file = tmp.path().join("canon_movie.mkv");
        let symlink_file = tmp.path().join("alias_movie.mkv");
        std::fs::write(&real_file, b"video").unwrap();
        std::os::unix::fs::symlink(&real_file, &symlink_file).unwrap();

        let id1 = media_repo
            .upsert(&sample_item("lib-pub", "One", real_file.clone(), 3600, Some("/p1.jpg")))
            .await
            .unwrap();
        let id_priv = media_repo
            .upsert(&sample_item("lib-priv", "Secret", PathBuf::from("/priv/s.mkv"), 5000, Some("/ps.jpg")))
            .await
            .unwrap();
        let id2 = media_repo
            .upsert(&sample_item("lib-pub", "Two", PathBuf::from("/pub/two.mkv"), 1800, Some("/p2.jpg")))
            .await
            .unwrap();
        let id_dup = media_repo
            .upsert(&sample_item("lib-pub", "One Alias", symlink_file, 3600, None))
            .await
            .unwrap();

        playlist_repo
            .create_playlist("pl-1", "u-1", "Weekend", Some("Queue"), false)
            .await
            .unwrap();

        let (added, skipped) = playlist_repo
            .add_items("pl-1", &[id1, id_priv, id2, id1, id_dup])
            .await
            .unwrap();
        assert_eq!((added, skipped), (4, 1));

        // Locked library isolation
        let sum_locked = playlist_repo
            .get_playlist_summary_for_user("pl-1", "u-1", false, &[], Some(id_priv))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sum_locked.visible_item_count, 3);
        assert_eq!(sum_locked.locked_item_count, 1);
        assert_eq!(sum_locked.total_duration_seconds, 9000);
        assert_eq!(sum_locked.contains_queried_item, Some(false));
        assert!(!sum_locked.cover_poster_urls.iter().any(|u| u.contains(&id_priv.to_string())));

        // Invalid reorder rejected
        let bad = playlist_repo.reorder_visible_items("pl-1", &[id2], &[]).await;
        assert!(matches!(bad, Err(StorageError::InvalidInput(_))));

        // Valid reorder while locked preserves id_priv at tail
        playlist_repo
            .reorder_visible_items("pl-1", &[id2, id_dup, id1], &[])
            .await
            .unwrap();

        // Deduplicate merges id_dup -> id1 without dropping id1 and re-compacts dense positions
        let merged = media_repo.deduplicate_media_items().await.unwrap();
        assert_eq!(merged, 1);

        let (unlocked_items, locked_cnt) = playlist_repo
            .get_playlist_items_with_privacy("pl-1", &["lib-priv".to_string()])
            .await
            .unwrap();
        assert_eq!(locked_cnt, 0);
        let ids: Vec<i64> = unlocked_items.iter().map(|(_, m)| m.id.unwrap()).collect();
        assert_eq!(ids, vec![id2, id1, id_priv]);

        // Remove middle item -> remaining positions remain dense
        assert!(playlist_repo.remove_item("pl-1", id1).await.unwrap());
        assert!(!playlist_repo.remove_item("pl-1", id1).await.unwrap());
    }
}
```

- [ ] **Step 2: Run `cargo test -p kadr-storage` to verify the new tests fail**

Run: `cargo test -p kadr-storage playlist_repo`
Expected: FAIL on unimplemented `PlaylistRepository` methods and `deduplicate_media_items` dropping `del_id` via `ON DELETE CASCADE`.

- [ ] **Step 3: Implement `PlaylistRepository` and update `MediaItemRepository::deduplicate_media_items`**

In `crates/kadr-storage/src/repos/media_item_repo.rs`:
1. Change `const SELECT_COLUMNS` and `fn map_media_item_row` (lines 502–506) to `pub(crate) const SELECT_COLUMNS` and `pub(crate) fn map_media_item_row`.
2. Update `deduplicate_media_items` transaction (lines 468–487) to collect affected `playlist_id`s, migrate `playlist_items`, and re-compact `position` to `0..N-1`:

```rust
            let tx = c.transaction()?;
            let mut affected_playlists = std::collections::BTreeSet::new();
            for (del_id, keep_id) in &duplicates_to_delete {
                {
                    let mut pl_stmt = tx.prepare(
                        "SELECT playlist_id FROM playlist_items WHERE media_item_id = ?1",
                    )?;
                    let pl_rows = pl_stmt.query_map(rusqlite::params![del_id], |r| {
                        r.get::<_, String>(0)
                    })?;
                    for pl_id in pl_rows {
                        affected_playlists.insert(pl_id?);
                    }
                }
                tx.execute(
                    "UPDATE OR IGNORE playlist_items SET media_item_id = ?1 WHERE media_item_id = ?2",
                    rusqlite::params![keep_id, del_id],
                )?;
                tx.execute(
                    "DELETE FROM playlist_items WHERE media_item_id = ?1",
                    rusqlite::params![del_id],
                )?;

                let _ = tx.execute(
                    "UPDATE OR IGNORE user_playback_states SET media_item_id = ?1 WHERE media_item_id = ?2",
                    rusqlite::params![keep_id, del_id],
                );
                let _ = tx.execute(
                    "DELETE FROM user_playback_states WHERE media_item_id = ?1",
                    rusqlite::params![del_id],
                );
                let _ = tx.execute(
                    "DELETE FROM media_subtitles WHERE media_item_id = ?1",
                    rusqlite::params![del_id],
                );
                tx.execute(
                    "DELETE FROM media_items WHERE id = ?1",
                    rusqlite::params![del_id],
                )?;
            }

            for pl_id in affected_playlists {
                let ordered_ids: Vec<i64> = {
                    let mut stmt = tx.prepare(
                        "SELECT media_item_id FROM playlist_items \
                         WHERE playlist_id = ?1 ORDER BY position ASC, added_at ASC",
                    )?;
                    let rows = stmt.query_map(rusqlite::params![&pl_id], |r| r.get::<_, i64>(0))?;
                    let mut ids = Vec::new();
                    for r in rows {
                        ids.push(r?);
                    }
                    ids
                };
                for (idx, media_id) in ordered_ids.into_iter().enumerate() {
                    tx.execute(
                        "UPDATE playlist_items SET position = ?1 WHERE playlist_id = ?2 AND media_item_id = ?3",
                        rusqlite::params![idx as i64, &pl_id, media_id],
                    )?;
                }
            }
```

Implement `crates/kadr-storage/src/repos/playlist_repo.rs`:

```rust
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use deadpool_sqlite::Pool;
use kadr_core::models::MediaItem;
use kadr_core::playlists::{Playlist, PlaylistSummary};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StorageError};
use crate::repos::media_item_repo::map_media_item_row;

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct PlaylistRepository {
    pool: Pool,
}

impl PlaylistRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn create_playlist(
        &self,
        id: &str,
        owner_user_id: &str,
        name: &str,
        description: Option<&str>,
        is_public: bool,
    ) -> Result<Playlist> {
        let id = id.to_string();
        let owner_user_id = owner_user_id.to_string();
        let name = name.to_string();
        let description = description.map(ToOwned::to_owned);
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO playlists (id, owner_user_id, name, description, is_public, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    owner_user_id,
                    name,
                    description,
                    if is_public { 1 } else { 0 },
                    now,
                    now,
                ],
            )?;

            Ok(Playlist {
                id,
                owner_user_id,
                name,
                description,
                is_public,
                created_at: now,
                updated_at: now,
            })
        })
        .await?
    }

    pub async fn get_playlist_raw(&self, playlist_id: &str) -> Result<Option<(Playlist, String)>> {
        let playlist_id = playlist_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let row = c
                .query_row(
                    "SELECT p.id, p.owner_user_id, p.name, p.description, p.is_public, p.created_at, p.updated_at, u.username
                     FROM playlists p
                     JOIN users u ON p.owner_user_id = u.id
                     WHERE p.id = ?1",
                    params![playlist_id],
                    |r| {
                        let pl = Playlist {
                            id: r.get(0)?,
                            owner_user_id: r.get(1)?,
                            name: r.get(2)?,
                            description: r.get(3)?,
                            is_public: r.get::<_, i64>(4)? != 0,
                            created_at: r.get(5)?,
                            updated_at: r.get(6)?,
                        };
                        let username: String = r.get(7)?;
                        Ok((pl, username))
                    },
                )
                .optional()?;
            Ok(row)
        })
        .await?
    }

    pub async fn update_playlist(
        &self,
        playlist_id: &str,
        name: Option<&str>,
        description: Option<Option<&str>>,
        is_public: Option<bool>,
    ) -> Result<Option<Playlist>> {
        let playlist_id = playlist_id.to_string();
        let name = name.map(ToOwned::to_owned);
        let description = description.map(|opt| opt.map(ToOwned::to_owned));
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let existing = tx
                .query_row(
                    "SELECT id, owner_user_id, name, description, is_public, created_at, updated_at
                     FROM playlists WHERE id = ?1",
                    params![&playlist_id],
                    |r| {
                        Ok(Playlist {
                            id: r.get(0)?,
                            owner_user_id: r.get(1)?,
                            name: r.get(2)?,
                            description: r.get(3)?,
                            is_public: r.get::<_, i64>(4)? != 0,
                            created_at: r.get(5)?,
                            updated_at: r.get(6)?,
                        })
                    },
                )
                .optional()?;

            let Some(mut pl) = existing else {
                return Ok(None);
            };

            let mut changed = false;
            if let Some(n) = name {
                pl.name = n;
                changed = true;
            }
            if let Some(d) = description {
                pl.description = d;
                changed = true;
            }
            if let Some(pub_flag) = is_public {
                pl.is_public = pub_flag;
                changed = true;
            }

            if changed {
                pl.updated_at = now;
                tx.execute(
                    "UPDATE playlists SET name = ?1, description = ?2, is_public = ?3, updated_at = ?4 WHERE id = ?5",
                    params![
                        &pl.name,
                        &pl.description,
                        if pl.is_public { 1 } else { 0 },
                        pl.updated_at,
                        &pl.id,
                    ],
                )?;
            }

            tx.commit()?;
            Ok(Some(pl))
        })
        .await?
    }

    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<bool> {
        let playlist_id = playlist_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let deleted = c.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
            Ok(deleted > 0)
        })
        .await?
    }

    pub async fn get_playlist_summary_for_user(
        &self,
        playlist_id: &str,
        user_id: &str,
        is_admin: bool,
        unlocked_ids: &[String],
        contains_item_id: Option<i64>,
    ) -> Result<Option<PlaylistSummary>> {
        let playlist_id = playlist_id.to_string();
        let user_id = user_id.to_string();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let raw = c
                .query_row(
                    "SELECT p.id, p.owner_user_id, p.name, p.description, p.is_public, p.created_at, p.updated_at, u.username
                     FROM playlists p
                     JOIN users u ON p.owner_user_id = u.id
                     WHERE p.id = ?1",
                    params![&playlist_id],
                    |r| {
                        Ok((
                            Playlist {
                                id: r.get(0)?,
                                owner_user_id: r.get(1)?,
                                name: r.get(2)?,
                                description: r.get(3)?,
                                is_public: r.get::<_, i64>(4)? != 0,
                                created_at: r.get(5)?,
                                updated_at: r.get(6)?,
                            },
                            r.get::<_, String>(7)?,
                        ))
                    },
                )
                .optional()?;

            let Some((pl, owner_username)) = raw else {
                return Ok(None);
            };

            if pl.owner_user_id != user_id && !pl.is_public {
                return Ok(None);
            }

            let summary = build_summary_in_conn(
                c,
                pl,
                owner_username,
                &user_id,
                is_admin,
                &unlocked_set,
                contains_item_id,
            )?;
            Ok(Some(summary))
        })
        .await?
    }

    pub async fn list_playlists_for_user(
        &self,
        user_id: &str,
        is_admin: bool,
        unlocked_ids: &[String],
        contains_item_id: Option<i64>,
    ) -> Result<Vec<PlaylistSummary>> {
        let user_id = user_id.to_string();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT p.id, p.owner_user_id, p.name, p.description, p.is_public, p.created_at, p.updated_at, u.username
                 FROM playlists p
                 JOIN users u ON p.owner_user_id = u.id
                 WHERE p.owner_user_id = ?1 OR p.is_public = 1
                 ORDER BY (p.owner_user_id = ?1) DESC, p.updated_at DESC, p.id ASC",
            )?;
            let rows = stmt.query_map(params![&user_id], |r| {
                Ok((
                    Playlist {
                        id: r.get(0)?,
                        owner_user_id: r.get(1)?,
                        name: r.get(2)?,
                        description: r.get(3)?,
                        is_public: r.get::<_, i64>(4)? != 0,
                        created_at: r.get(5)?,
                        updated_at: r.get(6)?,
                    },
                    r.get::<_, String>(7)?,
                ))
            })?;

            let mut playlists = Vec::new();
            for row in rows {
                let (pl, owner_username) = row?;
                playlists.push(build_summary_in_conn(
                    c,
                    pl,
                    owner_username,
                    &user_id,
                    is_admin,
                    &unlocked_set,
                    contains_item_id,
                )?);
            }
            Ok(playlists)
        })
        .await?
    }

    pub async fn get_playlist_items_with_privacy(
        &self,
        playlist_id: &str,
        unlocked_ids: &[String],
    ) -> Result<(Vec<(i64, MediaItem)>, usize)> {
        let playlist_id = playlist_id.to_string();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT m.id, m.library_id, m.item_type, m.title, m.original_title, m.release_year,
                        m.duration_seconds, m.added_at, m.file_path, m.file_name, m.file_size,
                        m.resolution, m.video_codec, m.audio_codec, m.audio_channels, m.container, m.metadata,
                        pi.added_at, l.is_private
                 FROM playlist_items pi
                 JOIN media_items m ON pi.media_item_id = m.id
                 JOIN libraries l ON m.library_id = l.id
                 WHERE pi.playlist_id = ?1
                 ORDER BY pi.position ASC, pi.added_at ASC",
            )?;

            let rows = stmt.query_map(params![playlist_id], |row| {
                let item = map_media_item_row(row)?;
                let item_added_at: i64 = row.get(17)?;
                let lib_is_private = row.get::<_, i64>(18)? != 0;
                Ok((item, item_added_at, lib_is_private))
            })?;

            let mut visible = Vec::new();
            let mut locked_count = 0usize;
            for row in rows {
                let (item, item_added_at, lib_is_private) = row?;
                if !lib_is_private || unlocked_set.contains(&item.library_id) {
                    visible.push((item_added_at, item));
                } else {
                    locked_count += 1;
                }
            }

            Ok((visible, locked_count))
        })
        .await?
    }

    pub async fn add_items(
        &self,
        playlist_id: &str,
        media_item_ids: &[i64],
    ) -> Result<(usize, usize)> {
        let playlist_id = playlist_id.to_string();
        let ids = media_item_ids.to_vec();
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let exists: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM playlists WHERE id = ?1",
                    params![&playlist_id],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(StorageError::NotFound(format!(
                    "Playlist {playlist_id} not found"
                )));
            }

            let mut next_pos: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM playlist_items WHERE playlist_id = ?1",
                params![&playlist_id],
                |r| r.get(0),
            )?;

            let mut added = 0usize;
            let mut skipped = 0usize;
            for media_id in ids {
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO playlist_items (playlist_id, media_item_id, position, added_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![&playlist_id, media_id, next_pos, now],
                )?;
                if inserted > 0 {
                    added += 1;
                    next_pos += 1;
                } else {
                    skipped += 1;
                }
            }

            if added > 0 {
                tx.execute(
                    "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
                    params![now, &playlist_id],
                )?;
            }

            tx.commit()?;
            Ok((added, skipped))
        })
        .await?
    }

    pub async fn remove_item(&self, playlist_id: &str, media_item_id: i64) -> Result<bool> {
        let playlist_id = playlist_id.to_string();
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let deleted = tx.execute(
                "DELETE FROM playlist_items WHERE playlist_id = ?1 AND media_item_id = ?2",
                params![&playlist_id, media_item_id],
            )?;
            if deleted == 0 {
                return Ok(false);
            }

            let remaining: Vec<i64> = {
                let mut stmt = tx.prepare(
                    "SELECT media_item_id FROM playlist_items
                     WHERE playlist_id = ?1 ORDER BY position ASC, added_at ASC",
                )?;
                let rows = stmt.query_map(params![&playlist_id], |r| r.get::<_, i64>(0))?;
                let mut ids = Vec::new();
                for r in rows {
                    ids.push(r?);
                }
                ids
            };

            for (idx, item_id) in remaining.into_iter().enumerate() {
                tx.execute(
                    "UPDATE playlist_items SET position = ?1 WHERE playlist_id = ?2 AND media_item_id = ?3",
                    params![idx as i64, &playlist_id, item_id],
                )?;
            }

            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
                params![now, &playlist_id],
            )?;

            tx.commit()?;
            Ok(true)
        })
        .await?
    }

    pub async fn reorder_visible_items(
        &self,
        playlist_id: &str,
        ordered_visible_ids: &[i64],
        unlocked_ids: &[String],
    ) -> Result<()> {
        let playlist_id = playlist_id.to_string();
        let ordered_visible_ids = ordered_visible_ids.to_vec();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let mut visible_ids = Vec::new();
            let mut locked_ids = Vec::new();

            {
                let mut stmt = tx.prepare(
                    "SELECT pi.media_item_id, m.library_id, l.is_private
                     FROM playlist_items pi
                     JOIN media_items m ON pi.media_item_id = m.id
                     JOIN libraries l ON m.library_id = l.id
                     WHERE pi.playlist_id = ?1
                     ORDER BY pi.position ASC, pi.added_at ASC",
                )?;
                let rows = stmt.query_map(params![&playlist_id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)? != 0,
                    ))
                })?;
                for r in rows {
                    let (media_id, lib_id, is_private) = r?;
                    if !is_private || unlocked_set.contains(&lib_id) {
                        visible_ids.push(media_id);
                    } else {
                        locked_ids.push(media_id);
                    }
                }
            }

            let expected_set: HashSet<i64> = visible_ids.iter().copied().collect();
            let provided_set: HashSet<i64> = ordered_visible_ids.iter().copied().collect();
            if ordered_visible_ids.len() != visible_ids.len()
                || provided_set.len() != ordered_visible_ids.len()
                || provided_set != expected_set
            {
                return Err(StorageError::InvalidInput(
                    "item_ids must be an exact permutation of visible playlist items".to_string(),
                ));
            }

            let final_order: Vec<i64> = ordered_visible_ids
                .into_iter()
                .chain(locked_ids.into_iter())
                .collect();

            for (idx, media_id) in final_order.into_iter().enumerate() {
                tx.execute(
                    "UPDATE playlist_items SET position = ?1 WHERE playlist_id = ?2 AND media_item_id = ?3",
                    params![idx as i64, &playlist_id, media_id],
                )?;
            }

            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
                params![now, &playlist_id],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }
}

fn build_summary_in_conn(
    c: &Connection,
    pl: Playlist,
    owner_username: String,
    user_id: &str,
    is_admin: bool,
    unlocked_set: &HashSet<String>,
    contains_item_id: Option<i64>,
) -> rusqlite::Result<PlaylistSummary> {
    let mut stmt = c.prepare(
        "SELECT m.id, m.library_id, m.duration_seconds, json_extract(m.metadata, '$.poster_path'), l.is_private
         FROM playlist_items pi
         JOIN media_items m ON pi.media_item_id = m.id
         JOIN libraries l ON m.library_id = l.id
         WHERE pi.playlist_id = ?1
         ORDER BY pi.position ASC, pi.added_at ASC",
    )?;
    let rows = stmt.query_map(params![&pl.id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, i64>(4)? != 0,
        ))
    })?;

    let mut visible_item_count = 0usize;
    let mut locked_item_count = 0usize;
    let mut total_duration_seconds = 0i64;
    let mut cover_poster_urls = Vec::new();
    let mut found_queried_visible = false;

    for r in rows {
        let (media_id, lib_id, dur, poster_path, is_private) = r?;
        if !is_private || unlocked_set.contains(&lib_id) {
            visible_item_count += 1;
            total_duration_seconds += dur.max(0);
            if cover_poster_urls.len() < 4
                && poster_path
                    .as_deref()
                    .map(|p| !p.trim().is_empty())
                    .unwrap_or(false)
            {
                cover_poster_urls.push(format!("/api/v1/artwork/{media_id}/poster"));
            }
            if contains_item_id == Some(media_id) {
                found_queried_visible = true;
            }
        } else {
            locked_item_count += 1;
        }
    }

    let contains_queried_item = contains_item_id.map(|_| found_queried_visible);
    let is_editable = pl.owner_user_id == user_id || is_admin;

    Ok(PlaylistSummary {
        id: pl.id,
        name: pl.name,
        description: pl.description,
        owner_user_id: pl.owner_user_id,
        owner_username,
        is_public: pl.is_public,
        is_editable,
        visible_item_count,
        locked_item_count,
        total_duration_seconds,
        cover_poster_urls,
        created_at: pl.created_at,
        updated_at: pl.updated_at,
        contains_queried_item,
    })
}
```

Re-export `PlaylistRepository` in `crates/kadr-storage/src/repos/mod.rs` and `crates/kadr-storage/src/lib.rs`:

```rust
// crates/kadr-storage/src/repos/mod.rs
pub mod playlist_repo;
pub use playlist_repo::PlaylistRepository;

// crates/kadr-storage/src/lib.rs
pub use repos::{
    LibraryRepo, LibraryRepository, MediaItemRepo, MediaItemRepository, PlaybackRepository,
    PlaylistRepository, PluginRepository, SettingsRepository, SubtitleRepository, UserRepository,
    WidgetQueries,
};
```

- [ ] **Step 4: Run `cargo test -p kadr-storage` to verify all storage tests pass**

Run: `cargo test -p kadr-storage`
Expected: PASS across all migration, `playlist_repo`, and `media_item_repo` deduplication tests.

- [ ] **Step 5: Commit Task 2**

```bash
git add crates/kadr-storage/src/migrations/009_playlists.sql crates/kadr-storage/src/migrations.rs crates/kadr-storage/src/repos/playlist_repo.rs crates/kadr-storage/src/repos/media_item_repo.rs crates/kadr-storage/src/repos/mod.rs crates/kadr-storage/src/lib.rs
git commit -m "feat(storage): add playlists schema, repository, and deduplication safety"
```

---

### Task 3: `kadr-server` `WidgetResolver` Support, `/api/v1/playlists/*` Routes & E2E Integration Suite

**Files:**
- Modify: `crates/kadr-server/src/resolver/resolver.rs:8-98, 448-655`
- Create: `crates/kadr-server/src/api/playlist_routes.rs`
- Modify: `crates/kadr-server/src/api/mod.rs:1-38, 100-108, 239-265`
- Test: `crates/kadr-server/tests/e2e_playlists_test.rs`

**Interfaces:**
- Consumes:
  - `kadr_core::playlists::*`, `kadr_core::ast::QueryMacro::PlaylistItems`, `kadr_core::events::SystemEvent::PlaylistUpdated`
  - `kadr_storage::repos::PlaylistRepository`
  - `kadr_server::auth::jwt::AuthUser`, `kadr_server::api::UnlockedLibraries`, `kadr_server::events::EventBus`, `kadr_server::resolver::to_card_view_model`
- Produces:
  - `pub fn playlist_routes() -> axum::Router` in `crates/kadr-server/src/api/playlist_routes.rs` serving:
    - `GET /api/v1/playlists` (`?contains_item_id=<optional i64>`) -> `200 OK` `PlaylistsListResponse`
    - `POST /api/v1/playlists` -> `201 Created` `PlaylistDetail`
    - `GET /api/v1/playlists/{id}` -> `200 OK` `PlaylistDetail`
    - `PATCH /api/v1/playlists/{id}` -> `200 OK` `PlaylistSummary`
    - `DELETE /api/v1/playlists/{id}` -> `204 No Content`
    - `POST /api/v1/playlists/{id}/items` -> `200 OK` `AddPlaylistItemsResponse`
    - `DELETE /api/v1/playlists/{id}/items/{media_item_id}` -> `200 OK` `PlaylistSummary`
    - `PUT /api/v1/playlists/{id}/items/order` -> `200 OK` `PlaylistSummary`
  - `WidgetResolver` hydration and pagination for `QueryMacro::PlaylistItems { playlist_id }` (leaving `WidgetResolver::new(media_repo, playback_repo)` and all `create_router*` function signatures unchanged).

- [ ] **Step 1: Write the failing end-to-end integration test suite `crates/kadr-server/tests/e2e_playlists_test.rs`**

Create `crates/kadr-server/tests/e2e_playlists_test.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kadr_core::ast::{QueryMacro, ScreenId, ScreenLayout, WidgetNode, WidgetQueryBinding};
use kadr_core::events::SystemEvent;
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole, WatchState,
};
use kadr_server::api::{create_router_with_events, UnlockTokenService};
use kadr_server::auth::jwt::JwtService;
use kadr_server::auth::rate_limiter::RateLimiter;
use kadr_server::events::EventBus;
use kadr_server::layout::LayoutRegistry;
use kadr_server::playback::SessionRegistry;
use kadr_server::resolver::WidgetResolver;
use kadr_server::subtitles::{OpenSubtitlesClient, SubtitleDeliveryService};
use kadr_server::telemetry::TelemetryCollector;
use kadr_storage::pool::{create_in_memory_pool, initialize_database};
use kadr_storage::repos::{
    LibraryRepository, MediaItemRepository, PlaybackRepository, SubtitleRepository, UserRepository,
};
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

struct TestEnv {
    app: axum::Router,
    event_bus: Arc<EventBus>,
    layout_registry: LayoutRegistry,
    unlock_svc: UnlockTokenService,
    alice_token: String,
    bob_token: String,
    admin_token: String,
    movie_a_id: i64,
    movie_b_id: i64,
    show_id: i64,
    ep_s1e1_id: i64,
    ep_s1e2_id: i64,
    ep_s2e1_id: i64,
    private_movie_id: i64,
    folder_ep1_id: i64,
    folder_ep2_id: i64,
    private_lib_id: String,
    folder_lib_id: String,
    _temp_dir: TempDir,
}

async fn send_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    unlock_token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(jwt) = token {
        builder = builder.header("Authorization", format!("Bearer {jwt}"));
    }
    if let Some(ut) = unlock_token {
        builder = builder.header("X-Kadr-Unlocked", ut);
    }
    let req = if let Some(b) = body {
        builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, val)
}

async fn setup_env() -> TestEnv {
    let temp_dir = TempDir::new().unwrap();
    let folder_root = temp_dir.path().join("library_folders");
    let sub_season = folder_root.join("Season 01");
    let nested_bonus = sub_season.join("Bonus");
    std::fs::create_dir_all(&nested_bonus).unwrap();

    let ep1_file = sub_season.join("01_pilot.mkv");
    let ep2_file = nested_bonus.join("02_making_of.mp4");
    std::fs::write(&ep1_file, b"fake-video-1").unwrap();
    std::fs::write(&ep2_file, b"fake-video-2").unwrap();

    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());
    let subtitle_repo = SubtitleRepository::new(pool.clone());

    let alice = User {
        id: "u-alice".to_string(),
        username: "alice".to_string(),
        pin_hash: "h".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let bob = User {
        id: "u-bob".to_string(),
        username: "bob".to_string(),
        pin_hash: "h".to_string(),
        role: UserRole::Standard,
        created_at: 1000,
    };
    let admin = User {
        id: "u-admin".to_string(),
        username: "admin".to_string(),
        pin_hash: "h".to_string(),
        role: UserRole::Admin,
        created_at: 1000,
    };
    user_repo.create(&alice).await.unwrap();
    user_repo.create(&bob).await.unwrap();
    user_repo.create(&admin).await.unwrap();

    let jwt_secret = "super-secret-playlist-test-key-32b!";
    let jwt_svc = JwtService::new(jwt_secret, 3600);
    let unlock_svc = UnlockTokenService::new(jwt_secret.as_bytes(), 3600);

    let alice_token = jwt_svc.generate_token(&alice).unwrap();
    let bob_token = jwt_svc.generate_token(&bob).unwrap();
    let admin_token = jwt_svc.generate_token(&admin).unwrap();

    lib_repo
        .insert(&Library {
            id: "lib-pub".to_string(),
            name: "Public Movies".to_string(),
            path: PathBuf::from("/media/pub"),
            media_type: MediaType::Movie,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let private_lib_id = "lib-priv".to_string();
    lib_repo
        .insert(&Library {
            id: private_lib_id.clone(),
            name: "Private Vault".to_string(),
            path: PathBuf::from("/media/priv"),
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("hash".to_string()),
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let folder_lib_id = "lib-folder".to_string();
    let canon_folder_root = std::fs::canonicalize(&folder_root).unwrap();
    lib_repo
        .insert(&Library {
            id: folder_lib_id.clone(),
            name: "Folder Shows".to_string(),
            path: canon_folder_root.clone(),
            paths: vec![canon_folder_root],
            media_type: MediaType::Episode,
            is_private: false,
            created_at: 1000,
            ..Default::default()
        })
        .await
        .unwrap();

    let mk_item = |lib: &str, kind: MediaType, title: &str, path: PathBuf, dur: i64, meta: MediaMetadata| MediaItem {
        id: None,
        library_id: lib.to_string(),
        item_type: kind,
        title: title.to_string(),
        original_title: None,
        release_year: Some(2024),
        added_at: 2000,
        file_path: path.clone(),
        file_name: path.file_name().unwrap().to_string_lossy().to_string(),
        file_size: 1024,
        technical: TechnicalInfo {
            duration_seconds: dur,
            resolution: Some("4K".to_string()),
            ..Default::default()
        },
        metadata: meta,
    };

    let movie_a_id = media_repo
        .upsert(&mk_item(
            "lib-pub",
            MediaType::Movie,
            "Arrival",
            PathBuf::from("/media/pub/arrival.mkv"),
            7000,
            MediaMetadata {
                poster_path: Some("/art/arrival.jpg".to_string()),
                ..Default::default()
            },
        ))
        .await
        .unwrap();

    let movie_b_id = media_repo
        .upsert(&mk_item(
            "lib-pub",
            MediaType::Movie,
            "Dune",
            PathBuf::from("/media/pub/dune.mkv"),
            9000,
            MediaMetadata {
                poster_path: Some("/art/dune.jpg".to_string()),
                ..Default::default()
            },
        ))
        .await
        .unwrap();

    let show_id = media_repo
        .upsert(&mk_item(
            "lib-pub",
            MediaType::Show,
            "Severance",
            PathBuf::from("/media/pub/Severance"),
            0,
            MediaMetadata::default(),
        ))
        .await
        .unwrap();

    let ep_s1e2_id = media_repo
        .upsert(&mk_item(
            "lib-pub",
            MediaType::Episode,
            "Half Loop",
            PathBuf::from("/media/pub/Severance/S01E02.mkv"),
            3000,
            MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(2),
                ..Default::default()
            },
        ))
        .await
        .unwrap();

    let ep_s1e1_id = media_repo
        .upsert(&mk_item(
            "lib-pub",
            MediaType::Episode,
            "Good News About Hell",
            PathBuf::from("/media/pub/Severance/S01E01.mkv"),
            3200,
            MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(1),
                episode: Some(1),
                ..Default::default()
            },
        ))
        .await
        .unwrap();

    let ep_s2e1_id = media_repo
        .upsert(&mk_item(
            "lib-pub",
            MediaType::Episode,
            "Hello Ms. Cobel",
            PathBuf::from("/media/pub/Severance/S02E01.mkv"),
            3100,
            MediaMetadata {
                series_title: Some("Severance".to_string()),
                season: Some(2),
                episode: Some(1),
                ..Default::default()
            },
        ))
        .await
        .unwrap();

    let private_movie_id = media_repo
        .upsert(&mk_item(
            &private_lib_id,
            MediaType::Movie,
            "Classified Cut",
            PathBuf::from("/media/priv/classified.mkv"),
            5400,
            MediaMetadata {
                poster_path: Some("/art/classified.jpg".to_string()),
                ..Default::default()
            },
        ))
        .await
        .unwrap();

    let folder_ep1_id = media_repo
        .upsert(&mk_item(
            &folder_lib_id,
            MediaType::Episode,
            "Folder Pilot",
            std::fs::canonicalize(&ep1_file).unwrap(),
            1800,
            MediaMetadata::default(),
        ))
        .await
        .unwrap();

    let folder_ep2_id = media_repo
        .upsert(&mk_item(
            &folder_lib_id,
            MediaType::Episode,
            "Folder Bonus",
            std::fs::canonicalize(&ep2_file).unwrap(),
            900,
            MediaMetadata::default(),
        ))
        .await
        .unwrap();

    // Seed Bob's independent playback progress on movie_a
    playback_repo
        .upsert_progress(&bob.id, movie_a_id, 1400, WatchState::InProgress, 5000)
        .await
        .unwrap();

    let limiter = RateLimiter::new(5, Duration::from_secs(60));
    let session_registry = Arc::new(SessionRegistry::new());
    let layout_registry = LayoutRegistry::new();
    let widget_resolver = Arc::new(WidgetResolver::new(media_repo.clone(), playback_repo.clone()));
    let subtitle_service = Arc::new(SubtitleDeliveryService::new(
        temp_dir.path().join("subs"),
        subtitle_repo,
        media_repo.clone(),
    ));
    let opensubtitles_client = Arc::new(OpenSubtitlesClient::new(None, None));
    let event_bus = Arc::new(EventBus::default_bus());
    let telemetry = Arc::new(TelemetryCollector::new(
        PathBuf::from(":memory:"),
        session_registry.clone(),
        event_bus.clone(),
    ));

    let app = create_router_with_events(
        user_repo,
        playback_repo,
        media_repo,
        lib_repo,
        jwt_svc,
        limiter,
        session_registry,
        layout_registry.clone(),
        widget_resolver,
        subtitle_service,
        opensubtitles_client,
        event_bus.clone(),
        telemetry,
    );

    TestEnv {
        app,
        event_bus,
        layout_registry,
        unlock_svc,
        alice_token,
        bob_token,
        admin_token,
        movie_a_id,
        movie_b_id,
        show_id,
        ep_s1e1_id,
        ep_s1e2_id,
        ep_s2e1_id,
        private_movie_id,
        folder_ep1_id,
        folder_ep2_id,
        private_lib_id,
        folder_lib_id,
        _temp_dir: temp_dir,
    }
}

#[tokio::test]
async fn multi_user_permissions_sharing_and_sse_events() {
    let env = setup_env().await;
    let mut rx = env.event_bus.subscribe();

    // Unauthenticated rejected
    let (status, _) = send_json(&env.app, "GET", "/api/v1/playlists", None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Alice 1-step Create & Add private playlist
    let (status, created) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({
            "name": "Alice Sci-Fi",
            "description": "Director cuts",
            "is_public": false,
            "items": { "item_ids": [env.movie_a_id, env.movie_b_id] }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let pl_id = created["summary"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["summary"]["visible_item_count"], 2);
    assert_eq!(created["items"][0]["card"]["id"], env.movie_a_id);
    assert!(created["items"][0].get("resume_position_seconds").is_none());

    // Verify SSE created event has zero metadata leakage
    let ev = rx.try_recv().expect("expected PlaylistUpdated created event");
    assert!(matches!(
        ev,
        SystemEvent::PlaylistUpdated { ref action, ref playlist_id, .. }
        if action == "created" && playlist_id == &pl_id
    ));

    // Bob gets 404 on Alice's private playlist (both GET and PATCH)
    let (s_get, _) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(s_get, StatusCode::NOT_FOUND);

    let (s_patch, _) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.bob_token),
        None,
        Some(json!({ "name": "Hijack" })),
    )
    .await;
    assert_eq!(s_patch, StatusCode::NOT_FOUND);

    // Alice makes playlist public
    let (s_pub, pub_sum) = send_json(
        &env.app,
        "PATCH",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        Some(json!({ "is_public": true })),
    )
    .await;
    assert_eq!(s_pub, StatusCode::OK);
    assert_eq!(pub_sum["is_public"], true);

    // Bob can now view & gets HIS OWN playback progress on movie_a, with is_editable: false
    let (s_bob, bob_detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(s_bob, StatusCode::OK);
    assert_eq!(bob_detail["summary"]["is_editable"], false);
    assert_eq!(bob_detail["items"][0]["resume_position_seconds"], 1400);

    // Bob cannot mutate public playlist -> 403 Forbidden
    let (s_bob_del, _) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.bob_token),
        None,
        None,
    )
    .await;
    assert_eq!(s_bob_del, StatusCode::FORBIDDEN);

    // Admin can mutate and delete Alice's playlist
    let (s_adm_del, _) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.admin_token),
        None,
        None,
    )
    .await;
    assert_eq!(s_adm_del, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn batch_show_season_and_sandboxed_folder_expansion() {
    let env = setup_env().await;

    let (_, created) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({ "name": "Binge Queue" })),
    )
    .await;
    let pl_id = created["summary"]["id"].as_str().unwrap().to_string();

    // Expand Season 1 only -> S01E01 then S01E02 (sorted even though S01E02 was inserted first)
    let (st, res) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({ "show": { "show_id": env.show_id, "season_number": 1 } })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["added_count"], 2);

    // Expand entire Show via item_ids -> skips S01E01 & S01E02, appends S02E01
    let (st2, res2) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [env.show_id] })),
    )
    .await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(res2["added_count"], 1);
    assert_eq!(res2["skipped_duplicate_count"], 2);

    // Non-recursive folder add -> adds Season 01/01_pilot.mkv only
    let (st3, res3) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({
            "folder": {
                "library_id": env.folder_lib_id,
                "path": "Season 01",
                "recursive": false
            }
        })),
    )
    .await;
    assert_eq!(st3, StatusCode::OK);
    assert_eq!(res3["added_count"], 1);

    // Recursive folder add -> skips pilot, adds Bonus/02_making_of.mp4
    let (st4, res4) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({
            "folder": {
                "library_id": env.folder_lib_id,
                "path": "Season 01",
                "recursive": true
            }
        })),
    )
    .await;
    assert_eq!(st4, StatusCode::OK);
    assert_eq!(res4["added_count"], 1);
    assert_eq!(res4["skipped_duplicate_count"], 1);

    // Path traversal rejected with 400
    let (st_trav, _) = send_json(
        &env.app,
        "POST",
        &format!("/api/v1/playlists/{pl_id}/items"),
        Some(&env.alice_token),
        None,
        Some(json!({
            "folder": {
                "library_id": env.folder_lib_id,
                "path": "../outside",
                "recursive": true
            }
        })),
    )
    .await;
    assert_eq!(st_trav, StatusCode::BAD_REQUEST);

    let (_, detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    let ids: Vec<i64> = detail["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["card"]["id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            env.ep_s1e1_id,
            env.ep_s1e2_id,
            env.ep_s2e1_id,
            env.folder_ep1_id,
            env.folder_ep2_id
        ]
    );
}

#[tokio::test]
async fn private_library_isolation_reorder_and_widget_ast_resolution() {
    let env = setup_env().await;
    let (unlock_tok, _) = env.unlock_svc.generate_token(&env.private_lib_id).unwrap();

    // Adding locked item without unlock token fails with 403 LIBRARY_LOCKED and does NOT create orphan playlist
    let (st_fail, err_body) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        None,
        Some(json!({
            "name": "Should Not Exist",
            "items": { "item_ids": [env.private_movie_id] }
        })),
    )
    .await;
    assert_eq!(st_fail, StatusCode::FORBIDDEN);
    assert_eq!(err_body["error"], "LIBRARY_LOCKED");

    let (_, list_check) = send_json(&env.app, "GET", "/api/v1/playlists", Some(&env.alice_token), None, None).await;
    assert_eq!(list_check["playlists"].as_array().unwrap().len(), 0);

    // Create with unlock token containing [movie_a, private_movie, movie_b]
    let (st_ok, created) = send_json(
        &env.app,
        "POST",
        "/api/v1/playlists",
        Some(&env.alice_token),
        Some(&unlock_tok),
        Some(json!({
            "name": "Mixed Vault",
            "is_public": true,
            "items": { "item_ids": [env.movie_a_id, env.private_movie_id, env.movie_b_id] }
        })),
    )
    .await;
    assert_eq!(st_ok, StatusCode::CREATED);
    let pl_id = created["summary"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["summary"]["visible_item_count"], 3);
    assert_eq!(created["summary"]["locked_item_count"], 0);

    // Without unlock token: locked item hidden from items, posters, duration, and contains_item_id
    let (_, locked_detail) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(locked_detail["summary"]["visible_item_count"], 2);
    assert_eq!(locked_detail["summary"]["locked_item_count"], 1);
    assert_eq!(locked_detail["summary"]["total_duration_seconds"], 16000);
    assert_eq!(locked_detail["items"].as_array().unwrap().len(), 2);
    assert_eq!(locked_detail["items"][0]["position"], 0);
    assert_eq!(locked_detail["items"][1]["position"], 1);
    let raw_str = locked_detail.to_string();
    assert!(!raw_str.contains("Classified Cut"));
    assert!(!raw_str.contains(&format!("/api/v1/artwork/{}/poster", env.private_movie_id)));

    // contains_item_id for locked item without token returns false
    let (_, contains_locked) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists?contains_item_id={}", env.private_movie_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(contains_locked["playlists"][0]["contains_queried_item"], false);

    // Deleting locked item without token returns 403 LIBRARY_LOCKED
    let (st_del_locked, del_err) = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}/items/{}", env.private_movie_id),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_del_locked, StatusCode::FORBIDDEN);
    assert_eq!(del_err["error"], "LIBRARY_LOCKED");

    // Reorder visible items [movie_b, movie_a] while locked -> preserves private_movie_id
    let (st_reord, _) = send_json(
        &env.app,
        "PUT",
        &format!("/api/v1/playlists/{pl_id}/items/order"),
        Some(&env.alice_token),
        None,
        Some(json!({ "item_ids": [env.movie_b_id, env.movie_a_id] })),
    )
    .await;
    assert_eq!(st_reord, StatusCode::OK);

    // Unlock again -> all 3 items intact: [movie_b, movie_a, private_movie]
    let (_, unlocked_after) = send_json(
        &env.app,
        "GET",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        Some(&unlock_tok),
        None,
    )
    .await;
    let after_ids: Vec<i64> = unlocked_after["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["card"]["id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        after_ids,
        vec![env.movie_b_id, env.movie_a_id, env.private_movie_id]
    );

    // Pin playlist as Carousel on Home screen and verify AST hydration + pagination + graceful delete fallback
    let home = ScreenLayout::new(
        ScreenId::Home,
        "Home",
        vec![WidgetNode::Carousel {
            id: "pl_rail".to_string(),
            title: "Pinned Playlist".to_string(),
            binding: WidgetQueryBinding::new(QueryMacro::PlaylistItems {
                playlist_id: pl_id.clone(),
            })
            .with_limit(1),
            items: None,
            next_cursor: None,
        }],
    );
    env.layout_registry
        .save_screen(home, &env._temp_dir.path().join("screens"))
        .unwrap();

    let (st_screen, screen_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_screen, StatusCode::OK);
    assert_eq!(screen_val["widgets"][0]["items"][0]["id"], env.movie_b_id);
    assert_eq!(screen_val["widgets"][0]["next_cursor"], "1");

    let (st_page2, page2_val) = send_json(
        &env.app,
        "GET",
        "/api/v1/widgets/pl_rail/data?screen_id=home&offset=1&limit=1",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_page2, StatusCode::OK);
    assert_eq!(page2_val["items"][0]["id"], env.movie_a_id);

    // Delete playlist -> Home screen gracefully resolves empty items instead of failing
    let _ = send_json(
        &env.app,
        "DELETE",
        &format!("/api/v1/playlists/{pl_id}"),
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    let (st_after_del, screen_after_del) = send_json(
        &env.app,
        "GET",
        "/api/v1/screens/home",
        Some(&env.alice_token),
        None,
        None,
    )
    .await;
    assert_eq!(st_after_del, StatusCode::OK);
    assert_eq!(screen_after_del["widgets"][0]["items"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run the E2E suite to verify failure (`404 Not Found` on `/api/v1/playlists`)**

Run: `cargo test -p kadr-server --test e2e_playlists_test`
Expected: FAIL because `/api/v1/playlists` routes are not registered and `WidgetResolver` returns the empty stub.

- [ ] **Step 3: Update `WidgetResolver` in `crates/kadr-server/src/resolver/resolver.rs`**

1. Import `PlaylistRepository` from `kadr_storage::repos` and add `playlist_repo: PlaylistRepository` to `WidgetResolver`, constructing `let playlist_repo = PlaylistRepository::new(media_repo.pool().clone());` inside `WidgetResolver::new`.
2. In `determine_effective_library_id` (`crates/kadr-server/src/resolver/resolver.rs:75-98`), return `None` when `matches!(binding.macro_type, QueryMacro::PlaylistItems { .. })` (unless `binding.filters.library_id` is explicitly set):

```rust
        } else if let QueryMacro::LibraryItems { ref library_id } = binding.macro_type {
            Some(library_id.as_str())
        } else if matches!(binding.macro_type, QueryMacro::PlaylistItems { .. }) {
            None
        } else {
            default_library_id
        }
```

3. Replace the `QueryMacro::PlaylistItems` stub in `resolve_widget_data_with_library` with full privacy-aware resolution and pagination (also respecting explicit `limit` passed via `binding.limit`):

```rust
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
```

- [ ] **Step 4: Implement `crates/kadr-server/src/api/playlist_routes.rs` and wire into `crates/kadr-server/src/api/mod.rs`**

Create `crates/kadr-server/src/api/playlist_routes.rs` implementing:
- `pub fn playlist_routes() -> Router`:
  ```rust
  Router::new()
      .route("/api/v1/playlists", get(list_playlists).post(create_playlist))
      .route(
          "/api/v1/playlists/{id}",
          get(get_playlist).patch(update_playlist).delete(delete_playlist),
      )
      .route("/api/v1/playlists/{id}/items", post(add_playlist_items))
      .route("/api/v1/playlists/{id}/items/order", put(reorder_playlist_items))
      .route(
          "/api/v1/playlists/{id}/items/{media_item_id}",
          delete(remove_playlist_item),
      )
  ```
- Input validation helpers:
  - `validate_name(raw: &str) -> Result<String, ApiErr>`: trims `raw`; checks `(1..=120).contains(&trimmed.chars().count())`, returning `400 {"error": "Playlist name must be between 1 and 120 characters"}` otherwise.
  - `validate_description(raw: Option<&str>) -> Result<Option<String>, ApiErr>`: trims; checks `chars().count() <= 500` (`400 {"error": "Playlist description must not exceed 500 characters"}`), normalizing empty strings to `None`.
- Permission check helpers matching Section 2.3 of the spec:
  - `load_readable_playlist`: returns `404 {"error": "Playlist not found"}` when missing or `!pl.is_public && pl.owner_user_id != auth_user.id`.
  - `load_editable_playlist`: returns `404 {"error": "Playlist not found"}` when missing or `!pl.is_public && pl.owner_user_id != auth_user.id`; returns `403 {"error": "Forbidden"}` when `pl.is_public && pl.owner_user_id != auth_user.id && auth_user.role != UserRole::Admin`.
- Batch item expansion helper `expand_items_request(&AddPlaylistItemsRequest, &UnlockedLibraries, &MediaItemRepository, &LibraryRepository) -> Result<Vec<i64>, ApiErr>`:
  - Rejects empty request (`req.is_empty()`) with `400 {"error": "No playable media items to add"}`.
  - Expands in deterministic order: `item_ids`, then `show`, then `folder`:
    1. `item_ids`: fetches each item (`404 {"error": "Media item not found"}`), checks parent library lock (`403 {"error": "LIBRARY_LOCKED"}`), rejects `MediaType::Season` (`400 {"error": "Non-playable media item type"}`), expands `MediaType::Show` via `media_repo.find_episodes_by_series(&item.title)` filtered to unlocked/public libraries (and matching `item.library_id` when episodes reside in the same library) sorted by `(metadata.season.unwrap_or(u32::MAX), metadata.episode.unwrap_or(u32::MAX), title.as_str(), id)`, or appends playable `item.id` directly.
    2. `show`: fetches `show_id` (`404` if missing, `400` if `item_type != MediaType::Show`), verifies library unlock (`403 LIBRARY_LOCKED`), fetches & filters episodes (including optional `season_number`), sorts deterministically, and returns `400 {"error": "No playable episodes found"}` if empty.
    3. `folder`: fetches `Library` (`404`), verifies unlock (`403 LIBRARY_LOCKED`), validates relative path identically to `browse_library_folders` (`400 {"error": "Invalid folder path"}` on `\0`, `..`, absolute/prefix components, or canonical path escaping library root; `404` if folder missing), walks single-level or recursive (skipping hidden `.` entries) collecting video/audio media files (`mkv`, `mp4`, `webm`, `avi`, `mov`, `m4v`, `mp3`, `flac`, `m4a`, `aac`, `ogg`, `opus`, `wav`), sorts paths lexicographically, resolves ingested items via `media_repo.find_by_paths`, filters to `item.library_id == spec.library_id` and playable types, and returns `400 {"error": "No playable media items in folder"}` if empty.
  - Deduplicates candidate IDs preserving first-seen order and returns `400 {"error": "No playable media items to add"}` if the combined result is empty.
  - In `create_playlist`: calls `expand_items_request` *before* `playlist_repo.create_playlist(...)` whenever `payload.items` is `Some(...)` so failed validation or `403 LIBRARY_LOCKED` never leaves an orphan playlist.
- Hydration helper `hydrate_playlist_detail`: fetches `PlaylistSummary` + `get_playlist_items_with_privacy`, batch-fetches caller `PlaybackState` via `playback_repo.get_states_for_items(&auth_user.id, &visible_ids)`, and constructs `Vec<PlaylistItemView>` with normalized `position: 0..items.len()`, `duration_seconds: item.technical.duration_seconds.max(0)`, `resume_position_seconds` (`Some(p.playback_position_seconds)` when `p.watch_state == WatchState::InProgress && p.playback_position_seconds > 0`, else `None`), and `card: to_card_view_model(&item, state)`.
- Emits `SystemEvent::PlaylistUpdated` via `event_bus.publish(...)` with `action` set to `"created"`, `"updated"`, `"deleted"`, or `"items_changed"`.

In `crates/kadr-server/src/api/mod.rs`:
1. Declare `pub mod playlist_routes;`.
2. Import `PlaylistRepository` from `kadr_storage::repos`.
3. Inside `create_router_with_ingest`:
   ```rust
   let playlist_repo = PlaylistRepository::new(media_repo.pool().clone());
   ```
   Merge `.merge(playlist_routes::playlist_routes())` on `router` and attach `.layer(Extension(playlist_repo))`.

- [ ] **Step 5: Run E2E playlist tests, full workspace tests, and Clippy**

Run:
```bash
cargo test -p kadr-server --test e2e_playlists_test
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: 100% PASS across all unit and E2E test suites and zero Clippy warnings.

- [ ] **Step 6: Commit Task 3**

```bash
git add crates/kadr-server/src/resolver/resolver.rs crates/kadr-server/src/api/playlist_routes.rs crates/kadr-server/src/api/mod.rs crates/kadr-server/tests/e2e_playlists_test.rs
git commit -m "feat(server): add playlist REST endpoints, batch expansion, and AST resolver"
```

---

## Track B: Frontend (`web/**` ONLY)

### Task 4: TypeScript Playlist Contracts & `ApiClient` Methods (`web/src/types/index.ts`, `web/src/api/client.ts`)

**Files:**
- Modify: `web/src/types/index.ts:57-65, 222-261, 430-432`
- Modify: `web/src/api/client.ts:1-34, 625-630`
- Modify: `web/src/api/client.test.ts`

**Interfaces:**
- Consumes: Existing `CardViewModel`, `ApiClient.request<T>` (`Authorization: Bearer <token>` and `X-Kadr-Unlocked` automatic injection)
- Produces:
  - Types in `web/src/types/index.ts`:
    - `PlaylistSummary`, `PlaylistItemView`, `PlaylistDetail`, `CreatePlaylistPayload`, `UpdatePlaylistPayload`, `AddShowItemsSpec`, `AddFolderItemsSpec`, `AddPlaylistItemsPayload`, `AddPlaylistItemsResponse`, `QueueItem` (`type QueueItem = PlaylistItemView`), `PlaylistAddTarget`
    - Extended `QueryMacro` union with `| { playlist_items: { playlist_id: string } }`
    - Extended `SystemEvent` union with `| { type: 'playlist:updated'; payload: { playlist_id: string; user_id: string; action: string; timestamp: number } }`
  - Methods on `ApiClient` (`web/src/api/client.ts`):
    - `getPlaylists(containsItemId?: number): Promise<{ playlists: PlaylistSummary[] }>`
    - `createPlaylist(payload: CreatePlaylistPayload): Promise<PlaylistDetail>`
    - `getPlaylist(id: string): Promise<PlaylistDetail>`
    - `updatePlaylist(id: string, payload: UpdatePlaylistPayload): Promise<PlaylistSummary>`
    - `deletePlaylist(id: string): Promise<void>`
    - `addPlaylistItems(id: string, payload: AddPlaylistItemsPayload): Promise<AddPlaylistItemsResponse>`
    - `removePlaylistItem(id: string, mediaItemId: number): Promise<PlaylistSummary>`
    - `reorderPlaylistItems(id: string, itemIds: number[]): Promise<PlaylistSummary>`

- [ ] **Step 1: Add failing unit tests in `web/src/api/client.test.ts` for all 8 playlist methods**

Append a `describe('Playlists API', ...)` block in `web/src/api/client.test.ts`:

```ts
describe('Playlists API', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    vi.restoreAllMocks();
  });

  it('fetches playlists with optional contains_item_id and forwards auth + unlock headers', async () => {
    const client = new ApiClient('http://localhost:8492');
    client.setToken('jwt-abc');
    client.storeUnlockToken('lib-priv', 'unlock-sig-1');

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ playlists: [] }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      })
    );

    await client.getPlaylists();
    expect(fetchSpy).toHaveBeenLastCalledWith(
      'http://localhost:8492/api/v1/playlists',
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer jwt-abc',
          'X-Kadr-Unlocked': 'unlock-sig-1',
        }),
      })
    );

    await client.getPlaylists(42);
    expect(fetchSpy).toHaveBeenLastCalledWith(
      'http://localhost:8492/api/v1/playlists?contains_item_id=42',
      expect.any(Object)
    );
  });

  it('calls create, get, update, delete, addItems, removeItem, and reorderItems endpoints', async () => {
    const client = new ApiClient('');
    client.setToken('jwt-abc');

    const mockSummary = {
      id: 'pl-1',
      name: 'Sci-Fi',
      owner_user_id: 'u-1',
      owner_username: 'alex',
      is_public: false,
      is_editable: true,
      visible_item_count: 2,
      locked_item_count: 0,
      total_duration_seconds: 7200,
      cover_poster_urls: [],
      created_at: 100,
      updated_at: 200,
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (_url, init) => {
      if (init?.method === 'DELETE' && String(_url).endsWith('/playlists/pl-1')) {
        return new Response(null, { status: 204 });
      }
      if (String(_url).endsWith('/playlists') && init?.method === 'POST') {
        return new Response(JSON.stringify({ summary: mockSummary, items: [] }), {
          status: 201,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (String(_url).endsWith('/playlists/pl-1') && (!init?.method || init.method === 'GET')) {
        return new Response(JSON.stringify({ summary: mockSummary, items: [] }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }
      if (String(_url).endsWith('/items') && init?.method === 'POST') {
        return new Response(
          JSON.stringify({ added_count: 2, skipped_duplicate_count: 0, summary: mockSummary }),
          { status: 200, headers: { 'Content-Type': 'application/json' } }
        );
      }
      return new Response(JSON.stringify(mockSummary), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    await client.createPlaylist({ name: 'Sci-Fi', is_public: false, items: { item_ids: [10, 14] } });
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'Sci-Fi', is_public: false, items: { item_ids: [10, 14] } }),
      })
    );

    await client.getPlaylist('pl-1');
    expect(fetchSpy).toHaveBeenCalledWith('/api/v1/playlists/pl-1', expect.any(Object));

    await client.updatePlaylist('pl-1', { name: 'Updated', is_public: true });
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ name: 'Updated', is_public: true }),
      })
    );

    await client.addPlaylistItems('pl-1', { show: { show_id: 50, season_number: 1 } });
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1/items',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ show: { show_id: 50, season_number: 1 } }),
      })
    );

    await client.reorderPlaylistItems('pl-1', [14, 10]);
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1/items/order',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ item_ids: [14, 10] }),
      })
    );

    await client.removePlaylistItem('pl-1', 10);
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1/items/10',
      expect.objectContaining({ method: 'DELETE' })
    );

    await client.deletePlaylist('pl-1');
    expect(fetchSpy).toHaveBeenCalledWith(
      '/api/v1/playlists/pl-1',
      expect.objectContaining({ method: 'DELETE' })
    );
  });
});
```

- [ ] **Step 2: Run Vitest on `client.test.ts` to verify failure**

Run: `cd web && npm test -- --run src/api/client.test.ts`
Expected: FAIL with `client.getPlaylists is not a function`.

- [ ] **Step 3: Add playlist interfaces to `web/src/types/index.ts` and implement `ApiClient` methods in `web/src/api/client.ts`**

In `web/src/types/index.ts`:
1. Extend `QueryMacro` (`lines 57-65`):
```ts
export type QueryMacro =
  | 'continue_watching'
  | 'recently_added'
  | 'top_rated'
  | { library_items: { library_id: string } }
  | { genre_shelf: { genre: string } }
  | { spotlight_item: { item_id?: number } }
  | { item_details: { item_id: number } }
  | { playlist_items: { playlist_id: string } };
```

2. Extend `SystemEvent` (`lines 257-261`):
```ts
  | {
      type: 'system:telemetry';
      payload: TelemetrySnapshot;
    }
  | {
      type: 'playlist:updated';
      payload: {
        playlist_id: string;
        user_id: string;
        action: string;
        timestamp: number;
      };
    };
```

3. Append playlist interfaces at the end of `web/src/types/index.ts`:
```ts
// Playlists & Playback Queue Types
export interface PlaylistSummary {
  id: string;
  name: string;
  description?: string;
  owner_user_id: string;
  owner_username: string;
  is_public: boolean;
  is_editable: boolean;
  visible_item_count: number;
  locked_item_count: number;
  total_duration_seconds: number;
  cover_poster_urls: string[];
  created_at: number;
  updated_at: number;
  contains_queried_item?: boolean;
}

export interface PlaylistItemView {
  position: number;
  added_at: number;
  duration_seconds: number;
  resume_position_seconds?: number;
  card: CardViewModel;
}

export type QueueItem = PlaylistItemView;

export interface PlaylistDetail {
  summary: PlaylistSummary;
  items: PlaylistItemView[];
}

export interface AddShowItemsSpec {
  show_id: number;
  season_number?: number;
}

export interface AddFolderItemsSpec {
  library_id: string;
  path: string;
  recursive?: boolean;
}

export interface AddPlaylistItemsPayload {
  item_ids?: number[];
  show?: AddShowItemsSpec;
  folder?: AddFolderItemsSpec;
}

export interface CreatePlaylistPayload {
  name: string;
  description?: string;
  is_public?: boolean;
  items?: AddPlaylistItemsPayload;
}

export interface UpdatePlaylistPayload {
  name?: string;
  description?: string;
  is_public?: boolean;
}

export interface AddPlaylistItemsResponse {
  added_count: number;
  skipped_duplicate_count: number;
  summary: PlaylistSummary;
}

export type PlaylistAddTarget =
  | { kind: 'item'; itemId: number; title: string }
  | { kind: 'season'; showId: number; seasonNumber: number; title: string }
  | { kind: 'folder'; libraryId: string; path: string; recursive: boolean; title: string };
```

In `web/src/api/client.ts`, import the new playlist types and add the 8 methods on `ApiClient`:

```ts
  // Playlists
  public async getPlaylists(
    containsItemId?: number
  ): Promise<{ playlists: PlaylistSummary[] }> {
    const query =
      containsItemId !== undefined
        ? `?contains_item_id=${encodeURIComponent(String(containsItemId))}`
        : '';
    return this.request<{ playlists: PlaylistSummary[] }>(`/api/v1/playlists${query}`);
  }

  public async createPlaylist(
    payload: CreatePlaylistPayload
  ): Promise<PlaylistDetail> {
    return this.request<PlaylistDetail>('/api/v1/playlists', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  public async getPlaylist(id: string): Promise<PlaylistDetail> {
    return this.request<PlaylistDetail>(
      `/api/v1/playlists/${encodeURIComponent(id)}`
    );
  }

  public async updatePlaylist(
    id: string,
    payload: UpdatePlaylistPayload
  ): Promise<PlaylistSummary> {
    return this.request<PlaylistSummary>(
      `/api/v1/playlists/${encodeURIComponent(id)}`,
      {
        method: 'PATCH',
        body: JSON.stringify(payload),
      }
    );
  }

  public async deletePlaylist(id: string): Promise<void> {
    await this.request<void>(`/api/v1/playlists/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    });
  }

  public async addPlaylistItems(
    id: string,
    payload: AddPlaylistItemsPayload
  ): Promise<AddPlaylistItemsResponse> {
    return this.request<AddPlaylistItemsResponse>(
      `/api/v1/playlists/${encodeURIComponent(id)}/items`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  public async removePlaylistItem(
    id: string,
    mediaItemId: number
  ): Promise<PlaylistSummary> {
    return this.request<PlaylistSummary>(
      `/api/v1/playlists/${encodeURIComponent(id)}/items/${encodeURIComponent(String(mediaItemId))}`,
      {
        method: 'DELETE',
      }
    );
  }

  public async reorderPlaylistItems(
    id: string,
    itemIds: number[]
  ): Promise<PlaylistSummary> {
    return this.request<PlaylistSummary>(
      `/api/v1/playlists/${encodeURIComponent(id)}/items/order`,
      {
        method: 'PUT',
        body: JSON.stringify({ item_ids: itemIds }),
      }
    );
  }
```

- [ ] **Step 4: Run Vitest on `client.test.ts` to verify pass**

Run: `cd web && npm test -- --run src/api/client.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit Task 4**

```bash
git add web/src/types/index.ts web/src/api/client.ts web/src/api/client.test.ts
git commit -m "feat(web): add playlist TypeScript models and ApiClient methods"
```

---

### Task 5: Playlist Views, `AddToPlaylistModal` & Catalog/Folder/Studio Entry Points

**Files:**
- Create: `web/src/components/playlists/PlaylistFormModal.tsx`
- Create: `web/src/components/playlists/PlaylistDetailView.tsx`
- Create: `web/src/components/playlists/PlaylistsScreen.tsx`
- Create: `web/src/components/playlists/AddToPlaylistModal.tsx`
- Modify: `web/src/components/browse/ItemDetailsModal.tsx:30-155, 436-460, 520-566, 750-778`
- Modify: `web/src/components/browse/SpotlightWidget.tsx:101-120`
- Modify: `web/src/components/browse/FolderBrowser.tsx:174-215, 294-312, 375-397`
- Modify: `web/src/components/studio/WidgetConfigModal.tsx:13-20, 50-120, 243-280, 440-525`
- Create: `web/src/components/playlists/PlaylistsScreen.test.tsx`
- Create: `web/src/components/playlists/AddToPlaylistModal.test.tsx`

**Interfaces:**
- Consumes: `api.getPlaylists`, `api.createPlaylist`, `api.getPlaylist`, `api.updatePlaylist`, `api.deletePlaylist`, `api.addPlaylistItems`, `api.removePlaylistItem`, `api.reorderPlaylistItems` and types from Task 4 (`web/src/types/index.ts`).
- Produces:
  - `PlaylistsScreen` (`web/src/components/playlists/PlaylistsScreen.tsx`) with exact props consumed by `App.tsx` in Task 6:
    ```ts
    export interface PlaylistsScreenProps {
      currentUser: User;
      activePlaylistId?: string | null;
      onSelectPlaylist?: (playlistId: string | null) => void;
      onPlayItem: (itemId: number) => void;
      onPlayQueue: (items: PlaylistItemView[], startIndex?: number) => void;
      onNavigateToFolder?: (libraryId: string, folderPath: string) => void;
    }
    ```
  - `PlaylistDetailView` (`web/src/components/playlists/PlaylistDetailView.tsx`)
  - `PlaylistFormModal` (`web/src/components/playlists/PlaylistFormModal.tsx`)
  - `AddToPlaylistModal` (`web/src/components/playlists/AddToPlaylistModal.tsx`):
    ```ts
    export interface AddToPlaylistModalProps {
      isOpen: boolean;
      target: PlaylistAddTarget | null;
      onClose: () => void;
      onUpdated?: () => void;
    }
    ```

- [ ] **Step 1: Write failing Vitest suites `PlaylistsScreen.test.tsx` and `AddToPlaylistModal.test.tsx`**

Create `web/src/components/playlists/PlaylistsScreen.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PlaylistsScreen } from './PlaylistsScreen';
import { api } from '../../api/client';
import type { PlaylistDetail, PlaylistSummary, User } from '../../types';

const currentUser: User = { id: 'u-1', username: 'alex', role: 'standard' };

const mySummary: PlaylistSummary = {
  id: 'pl-mine',
  name: 'Sci-Fi Marathon',
  description: 'Director cuts',
  owner_user_id: 'u-1',
  owner_username: 'alex',
  is_public: false,
  is_editable: true,
  visible_item_count: 2,
  locked_item_count: 1,
  total_duration_seconds: 12600,
  cover_poster_urls: [
    '/api/v1/artwork/1/poster',
    '/api/v1/artwork/2/poster',
    '/api/v1/artwork/3/poster',
    '/api/v1/artwork/4/poster',
  ],
  created_at: 1000,
  updated_at: 2000,
};

const sharedSummary: PlaylistSummary = {
  id: 'pl-shared',
  name: 'Family Favorites',
  owner_user_id: 'u-2',
  owner_username: 'sam',
  is_public: true,
  is_editable: false,
  visible_item_count: 1,
  locked_item_count: 0,
  total_duration_seconds: 5400,
  cover_poster_urls: ['/api/v1/artwork/9/poster'],
  created_at: 1000,
  updated_at: 1900,
};

const detailPayload: PlaylistDetail = {
  summary: mySummary,
  items: [
    {
      position: 0,
      added_at: 1100,
      duration_seconds: 7000,
      card: { id: 10, title: 'Arrival', subtitle: '2016 • 4K', media_type: 'movie' },
    },
    {
      position: 1,
      added_at: 1200,
      duration_seconds: 5600,
      card: { id: 14, title: 'Blade Runner 2049', subtitle: '2017 • 4K', media_type: 'movie' },
    },
  ],
};

describe('PlaylistsScreen & PlaylistDetailView', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders My Playlists and Shared on Server sections, 2x2 mosaic, locked badge, and creates playlist', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [mySummary, sharedSummary] });
    const createSpy = vi.spyOn(api, 'createPlaylist').mockResolvedValue(detailPayload);

    render(
      <PlaylistsScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('My Playlists')).toBeInTheDocument();
      expect(screen.getByText('Shared on Server')).toBeInTheDocument();
      expect(screen.getByText('Sci-Fi Marathon')).toBeInTheDocument();
      expect(screen.getByText('Family Favorites')).toBeInTheDocument();
      expect(screen.getByText(/\+1 locked/i)).toBeInTheDocument();
      expect(screen.getByText(/by @sam/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /\+ New Playlist/i }));
    fireEvent.change(screen.getByLabelText(/Playlist Name/i), { target: { value: 'Late Night' } });
    fireEvent.click(screen.getByRole('button', { name: /^Create Playlist$/i }));

    await waitFor(() => {
      expect(createSpy).toHaveBeenCalledWith(
        expect.objectContaining({ name: 'Late Night', is_public: false })
      );
    });
  });

  it('opens detail view and supports Play All, Shuffle, Move Down reorder, and item removal', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [mySummary] });
    vi.spyOn(api, 'getPlaylist').mockResolvedValue(detailPayload);
    const reorderSpy = vi.spyOn(api, 'reorderPlaylistItems').mockResolvedValue(mySummary);
    const removeSpy = vi.spyOn(api, 'removePlaylistItem').mockResolvedValue({
      ...mySummary,
      visible_item_count: 1,
    });
    const onPlayQueue = vi.fn();

    render(
      <PlaylistsScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={onPlayQueue}
      />
    );

    await waitFor(() => expect(screen.getByText('Sci-Fi Marathon')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Sci-Fi Marathon'));

    await waitFor(() => {
      expect(screen.getByText('Arrival')).toBeInTheDocument();
      expect(screen.getByText('Blade Runner 2049')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /Play All/i }));
    expect(onPlayQueue).toHaveBeenCalledWith(detailPayload.items, 0);

    fireEvent.click(screen.getByRole('button', { name: /Shuffle/i }));
    expect(onPlayQueue).toHaveBeenCalledTimes(2);
    expect(onPlayQueue.mock.calls[1][0]).toHaveLength(2);
    expect(onPlayQueue.mock.calls[1][1]).toBe(0);

    // Move Arrival down -> [14, 10]
    fireEvent.click(screen.getByRole('button', { name: /Move Arrival down/i }));
    await waitFor(() => {
      expect(reorderSpy).toHaveBeenCalledWith('pl-mine', [14, 10]);
    });

    // Remove item
    fireEvent.click(screen.getByRole('button', { name: /Remove Arrival from playlist/i }));
    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith('pl-mine', 10);
    });
  });
});
```

Create `web/src/components/playlists/AddToPlaylistModal.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AddToPlaylistModal } from './AddToPlaylistModal';
import { api } from '../../api/client';
import type { PlaylistSummary } from '../../types';

const existingPlaylist: PlaylistSummary = {
  id: 'pl-1',
  name: 'Watchlist',
  owner_user_id: 'u-1',
  owner_username: 'alex',
  is_public: false,
  is_editable: true,
  visible_item_count: 3,
  locked_item_count: 0,
  total_duration_seconds: 9000,
  cover_poster_urls: [],
  created_at: 100,
  updated_at: 200,
  contains_queried_item: true,
};

const secondPlaylist: PlaylistSummary = {
  ...existingPlaylist,
  id: 'pl-2',
  name: 'Weekend Queue',
  contains_queried_item: false,
};

describe('AddToPlaylistModal', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('displays membership checkmarks, toggles single item membership, and supports 1-step Create & Add', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({
      playlists: [existingPlaylist, secondPlaylist],
    });
    const addSpy = vi.spyOn(api, 'addPlaylistItems').mockResolvedValue({
      added_count: 1,
      skipped_duplicate_count: 0,
      summary: secondPlaylist,
    });
    const removeSpy = vi.spyOn(api, 'removePlaylistItem').mockResolvedValue(existingPlaylist);
    const createSpy = vi.spyOn(api, 'createPlaylist').mockResolvedValue({
      summary: secondPlaylist,
      items: [],
    });

    render(
      <AddToPlaylistModal
        isOpen={true}
        target={{ kind: 'item', itemId: 42, title: 'Dune' }}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(api.getPlaylists).toHaveBeenCalledWith(42);
      expect(screen.getByText('Watchlist')).toBeInTheDocument();
      expect(screen.getByText('Weekend Queue')).toBeInTheDocument();
    });

    // Click unchecked playlist -> adds item
    fireEvent.click(screen.getByRole('button', { name: /Weekend Queue/i }));
    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith('pl-2', { item_ids: [42] });
    });

    // Click already-checked playlist -> removes item
    fireEvent.click(screen.getByRole('button', { name: /Watchlist/i }));
    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith('pl-1', 42);
    });

    // 1-step inline Create & Add
    fireEvent.change(screen.getByPlaceholderText(/New playlist name/i), {
      target: { value: 'Dune Double Feature' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Create & Add/i }));
    await waitFor(() => {
      expect(createSpy).toHaveBeenCalledWith({
        name: 'Dune Double Feature',
        is_public: false,
        items: { item_ids: [42] },
      });
    });
  });

  it('supports season and folder batch targets with recursive checkbox', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [secondPlaylist] });
    const addSpy = vi.spyOn(api, 'addPlaylistItems').mockResolvedValue({
      added_count: 6,
      skipped_duplicate_count: 0,
      summary: secondPlaylist,
    });

    const { rerender } = render(
      <AddToPlaylistModal
        isOpen={true}
        target={{ kind: 'season', showId: 77, seasonNumber: 2, title: 'Severance — Season 2' }}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => expect(screen.getByText('Weekend Queue')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Weekend Queue/i }));
    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith('pl-2', {
        show: { show_id: 77, season_number: 2 },
      });
    });

    rerender(
      <AddToPlaylistModal
        isOpen={true}
        target={{
          kind: 'folder',
          libraryId: 'lib-1',
          path: 'Sci-Fi/Collection',
          recursive: true,
          title: 'Collection',
        }}
        onClose={vi.fn()}
      />
    );

    const recursiveToggle = await screen.findByLabelText(/Include subfolders/i);
    expect(recursiveToggle).toBeChecked();
    fireEvent.click(recursiveToggle);

    fireEvent.click(screen.getByRole('button', { name: /Weekend Queue/i }));
    await waitFor(() => {
      expect(addSpy).toHaveBeenLastCalledWith('pl-2', {
        folder: { library_id: 'lib-1', path: 'Sci-Fi/Collection', recursive: false },
      });
    });
  });
});
```

- [ ] **Step 2: Run Vitest on the new playlist test files to verify failure**

Run: `cd web && npm test -- --run src/components/playlists/PlaylistsScreen.test.tsx src/components/playlists/AddToPlaylistModal.test.tsx`
Expected: FAIL because `./PlaylistsScreen` and `./AddToPlaylistModal` do not exist yet.

- [ ] **Step 3: Create `PlaylistFormModal.tsx`, `PlaylistDetailView.tsx`, `PlaylistsScreen.tsx`, and `AddToPlaylistModal.tsx`**

Implement all four components in `web/src/components/playlists/` strictly adhering to `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `bg-cta`, `text-accent`, `text-highlight`, `border-border-subtle`, `rounded-xl`, `font-mono` for counts/durations, and `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` on every interactive control):

1. **`PlaylistFormModal.tsx`**:
   - Controlled dialog (`role="dialog"`, `aria-modal="true"`) for both creating and editing playlists:
     - `Playlist Name` input (`aria-label="Playlist Name"`, `maxLength={120}`, required trimmed `1..=120`).
     - `Description` textarea (`aria-label="Description"`, `maxLength={500}`, optional; passes `""` when cleared in edit mode so `PATCH` clears `description` to `NULL`).
     - `Share with everyone on server` checkbox (`is_public`).
     - Submit button labeled `"Create Playlist"` or `"Save Changes"`.

2. **`PlaylistDetailView.tsx`**:
   - Props: `{ playlistId: string; onBack: () => void; onPlayItem: (itemId: number) => void; onPlayQueue: (items: PlaylistItemView[], startIndex?: number) => void; onNavigateToFolder?: (libraryId: string, folderPath: string) => void; onDeleted: () => void; }`.
   - Fetches `api.getPlaylist(playlistId)`.
   - Renders `"All Playlists"` back button, 2x2 poster mosaic (`cover_poster_urls`), title, description, `@owner_username`, `Globe`/`Lock` badge, `font-mono` item count + formatted total runtime (`Xh Ym`), and an informational notice banner when `summary.locked_item_count > 0` (`"${summary.locked_item_count} item(s) from locked private libraries are hidden. Unlock private libraries in the top bar to view and play them."`).
   - Primary action bar:
     - `Play All` (`Play` icon, `bg-cta`, disabled when `items.length === 0`, calls `onPlayQueue(detail.items, 0)`).
     - `Shuffle` (`Shuffle` icon, `bg-panel`, disabled when `items.length === 0`, performs Fisher-Yates shuffle on a copy `[...detail.items]` and calls `onPlayQueue(shuffled, 0)`).
     - When `summary.is_editable`: `Edit` (`Pencil` opening `PlaylistFormModal`) and `Delete` (`Trash2` with inline confirmation calling `api.deletePlaylist(playlistId)` -> `onDeleted()`).
   - Ordered track list:
     - Each row displays 1-based `font-mono` index, poster/backdrop thumbnail with `playback_progress` bar, `card.title`, `card.subtitle`, `font-mono` duration, click/Enter-to-play-from-index (`onPlayQueue(detail.items, index)`), and `Info` button opening `ItemDetailsModal`.
     - When `summary.is_editable`:
       - `ChevronUp` (`aria-label={`Move ${item.card.title} up`}`, disabled at `index === 0`) and `ChevronDown` (`aria-label={`Move ${item.card.title} down`}`, disabled at `index === items.length - 1`) + HTML5 drag-and-drop (`draggable`, `onDragStart`, `onDragOver`, `onDrop`), optimistically updating items and calling `api.reorderPlaylistItems(playlistId, nextItems.map((i) => i.card.id))` with rollback on error.
       - Remove button (`Trash2`, `aria-label={`Remove ${item.card.title} from playlist`}`) calling `api.removePlaylistItem(playlistId, item.card.id)`.

3. **`PlaylistsScreen.tsx`**:
   - Supports both controlled (`activePlaylistId` + `onSelectPlaylist` from `App.tsx`) and uncontrolled (`internalPlaylistId`) detail selection:
     ```tsx
     const [internalPlaylistId, setInternalPlaylistId] = useState<string | null>(null);
     const selectedId = activePlaylistId !== undefined ? activePlaylistId : internalPlaylistId;
     const selectPlaylist = (id: string | null) => {
       setInternalPlaylistId(id);
       onSelectPlaylist?.(id);
     };
     ```
   - When `selectedId` is non-null, renders `<PlaylistDetailView playlistId={selectedId} onBack={() => selectPlaylist(null)} onDeleted={() => { selectPlaylist(null); refresh(); }} ... />`.
   - Otherwise fetches `api.getPlaylists()` and renders:
     - Header `"Playlists"` + `"+ New Playlist"` CTA button.
     - `"My Playlists"` (`playlists.filter((p) => p.owner_user_id === currentUser.id)`) with empty state card when none exist.
     - `"Shared on Server"` (`playlists.filter((p) => p.is_public && p.owner_user_id !== currentUser.id)`), rendered when non-empty, showing `"by @${p.owner_username}"`.
     - Each card displays 2x2 mosaic (`4` images when `cover_poster_urls.length >= 4`, single cover when `1..3`, `ListVideo` icon placeholder when `0`), `Globe`/`Lock` badge, `font-mono` `${p.visible_item_count} items • ${formatRuntime(p.total_duration_seconds)}`, and `"+${p.locked_item_count} locked"` badge when `p.locked_item_count > 0`.

4. **`AddToPlaylistModal.tsx`**:
   - Accepts `isOpen`, `target: PlaylistAddTarget | null`, `onClose`, `onUpdated`.
   - Converts `target` (plus local `includeSubfolders` state initialized from `target.recursive` when `target.kind === 'folder'`) into `AddPlaylistItemsPayload`:
     - `item`: `{ item_ids: [target.itemId] }`
     - `season`: `{ show: { show_id: target.showId, season_number: target.seasonNumber } }`
     - `folder`: `{ folder: { library_id: target.libraryId, path: target.path, recursive: includeSubfolders } }`
   - Loads `api.getPlaylists(target.kind === 'item' ? target.itemId : undefined)` and filters to `p.is_editable`.
   - Clicking an editable playlist:
     - If `target.kind === 'item' && p.contains_queried_item === true`: calls `api.removePlaylistItem(p.id, target.itemId)` and flips `contains_queried_item` to `false`.
     - Otherwise calls `api.addPlaylistItems(p.id, payload)` and shows inline feedback (`"Added ${res.added_count} item(s)"`) and sets `contains_queried_item: true` for single items.
   - Renders 1-step inline `"Create new playlist & add"` form (`placeholder="New playlist name..."`, `Share on server` checkbox, `"Create & Add"` button calling `api.createPlaylist({ name: newName.trim(), is_public: newIsPublic, items: payload })`).

- [ ] **Step 4: Wire `AddToPlaylistModal` into `ItemDetailsModal.tsx`, `SpotlightWidget.tsx`, `FolderBrowser.tsx`, and `playlist_items` into `WidgetConfigModal.tsx`**

1. **`web/src/components/browse/ItemDetailsModal.tsx`**:
   - Import `ListPlus` from `lucide-react`, `PlaylistAddTarget` from `../../types`, and `AddToPlaylistModal` from `../playlists/AddToPlaylistModal`.
   - Add `playlistTarget` state (`useState<PlaylistAddTarget | null>(null)`).
   - In the header action row (`lines 437-458`, next to `Browse Folder` and `Play`), add `+ Playlist` button:
     ```tsx
     <button
       type="button"
       onClick={() =>
         setPlaylistTarget({
           kind: 'item',
           itemId,
           title: card?.title || `Item #${itemId}`,
         })
       }
       className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium bg-panel hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
     >
       <ListPlus className="w-4 h-4 text-accent" />
       <span>+ Playlist</span>
     </button>
     ```
   - In the Season Pill Selector row (`lines 531-552`), when `currentSeason !== null`, add an `Add Season` button (`ListPlus` icon) setting `playlistTarget = { kind: 'season', showId: itemId, seasonNumber: currentSeason, title: `${card?.title || 'Show'} — Season ${currentSeason}` }`.
   - In `EpisodeCard` (`lines 30-154`), pass `onAddToPlaylist: (ep: CardViewModel) => void` and render a compact icon button right next to `Play`:
     ```tsx
     <button
       type="button"
       aria-label={`Add ${episode.title} to playlist`}
       onClick={() => onAddToPlaylist(episode)}
       className="p-2 rounded-xl bg-panel hover:bg-panel-hover text-muted hover:text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
     >
       <ListPlus className="w-4 h-4 text-accent" />
     </button>
     ```
   - Render `<AddToPlaylistModal isOpen={playlistTarget !== null} target={playlistTarget} onClose={() => setPlaylistTarget(null)} />` inside the modal portal.

2. **`web/src/components/browse/SpotlightWidget.tsx`**:
   - Import `ListPlus` from `lucide-react` and `AddToPlaylistModal` from `../playlists/AddToPlaylistModal`.
   - Add `isAddPlaylistOpen` state (`useState(false)`).
   - In the CTA row (`lines 102-118`, right after `More Info`), render an `Add to Playlist` button (`ListPlus` icon) opening `<AddToPlaylistModal isOpen={isAddPlaylistOpen} target={{ kind: 'item', itemId: item.id, title: item.title }} onClose={() => setIsAddPlaylistOpen(false)} />`.

3. **`web/src/components/browse/FolderBrowser.tsx`**:
   - Import `ListPlus` from `lucide-react` and `AddToPlaylistModal` from `../playlists/AddToPlaylistModal`.
   - Add `isAddFolderOpen` state (`useState(false)`).
   - In the Breadcrumb / Folder action bar (`lines 175-214`), render an `Add Folder to Playlist` button (`ListPlus` icon) opening `<AddToPlaylistModal isOpen={isAddFolderOpen} target={{ kind: 'folder', libraryId, path: currentPath, recursive: true, title: currentPath || data?.library_name || 'Folder' }} onClose={() => setIsAddFolderOpen(false)} />`.

4. **`web/src/components/studio/WidgetConfigModal.tsx`**:
   - Extend `MacroTypeOption` with `| 'playlist_items'`.
   - Add `playlists` (`PlaylistSummary[]`) and `selectedPlaylistId` (`string`) state; when `isOpen && macroType === 'playlist_items'`, fetch `api.getPlaylists()` and populate `playlists` (defaulting `selectedPlaylistId` to the existing binding's `playlist_id` or `playlists[0]?.id || ''`).
   - Parse `{ playlist_items: { playlist_id } }` when editing an existing widget (`lines 103-118`) and serialize `case 'playlist_items': resolvedMacro = { playlist_items: { playlist_id: selectedPlaylistId } };` in `handleSubmit`.
   - Add `<option value="playlist_items">Playlist Items</option>` to `#widget-macro`, and when `macroType === 'playlist_items'`, render a `#widget-playlist` `<select>` listing `playlists` (`${p.name} (${p.is_public ? 'Public' : 'Private'})`) plus an inline warning banner when the selected playlist has `!p.is_public` explaining that items in a private playlist are only visible to its owner on shared screens.

- [ ] **Step 5: Run Vitest on playlist components and existing browse/studio suites**

Run:
```bash
cd web && npm test -- --run src/components/playlists/PlaylistsScreen.test.tsx src/components/playlists/AddToPlaylistModal.test.tsx src/components/browse/ItemDetailsModal.test.tsx src/components/browse/FolderBrowser.test.tsx src/components/studio/WidgetConfigModal.test.tsx
```
Expected: PASS across all 5 test suites.

- [ ] **Step 6: Commit Task 5**

```bash
git add web/src/components/playlists/ web/src/components/browse/ItemDetailsModal.tsx web/src/components/browse/SpotlightWidget.tsx web/src/components/browse/FolderBrowser.tsx web/src/components/studio/WidgetConfigModal.tsx
git commit -m "feat(web): add playlists screens, add-to-playlist modal, and catalog/studio bindings"
```

---

### Task 6: `CinemaPlayer` Queue Playback, `PlayerControls` Queue Popover & `App.tsx` Navigation

**Files:**
- Modify: `web/src/components/player/CinemaPlayer.tsx:6-65, 120-195, 222-297, 358-451`
- Modify: `web/src/components/player/PlayerControls.tsx:1-90, 159-186, 226-295, 525-552`
- Modify: `web/src/App.tsx:1-66, 104-155, 181-237, 290-322`
- Modify: `web/src/components/player/CinemaPlayer.test.tsx`
- Modify: `web/src/components/player/PlayerControls.test.tsx`
- Modify: `web/src/App.test.tsx`

**Interfaces:**
- Consumes:
  - `PlaylistItemView` (`web/src/types/index.ts`)
  - `PlaylistsScreen` (`web/src/components/playlists/PlaylistsScreen.tsx` from Task 5)
  - `api.createPlaybackSession`, `api.sendPlaybackHeartbeat`, `api.closePlaybackSession`, `api.getPlaybackState`
- Produces:
  - `CinemaPlayerProps`:
    ```ts
    export interface CinemaPlayerProps {
      itemId: number;
      onClose: () => void;
      returnLabel?: string;
      queue?: PlaylistItemView[];
      currentIndex?: number;
      onSelectQueueIndex?: (index: number) => void;
    }
    ```
  - `PlayerControlsProps` extended with `returnLabel?: string`, `queue?: PlaylistItemView[]`, `currentIndex?: number`, `onSelectQueueIndex?: (index: number) => void`, `onPrevTrack?: () => void`, `onNextTrack?: () => void`, `onQueueMenuToggle?: (isOpen: boolean) => void`.
  - `NavView` in `web/src/App.tsx`: `'home' | \`library-${string}\` | 'playlists' | 'player' | 'admin'`.

- [ ] **Step 1: Add failing queue & return-navigation tests in `PlayerControls.test.tsx`, `CinemaPlayer.test.tsx`, and `App.test.tsx`**

1. Append to `web/src/components/player/PlayerControls.test.tsx`:
```tsx
  it('renders Previous/Next track controls, track counter, and Queue popover drawer when queue has >1 items', () => {
    const queue = [
      {
        position: 0,
        added_at: 100,
        duration_seconds: 7200,
        card: { id: 10, title: 'Arrival', media_type: 'movie' },
      },
      {
        position: 1,
        added_at: 200,
        duration_seconds: 9800,
        card: { id: 14, title: 'Blade Runner 2049', media_type: 'movie' },
      },
    ];
    const onSelectQueueIndex = vi.fn();
    const onPrevTrack = vi.fn();
    const onNextTrack = vi.fn();
    const onQueueMenuToggle = vi.fn();

    render(
      <PlayerControls
        {...defaultProps}
        returnLabel="Back to Playlist"
        queue={queue}
        currentIndex={0}
        onSelectQueueIndex={onSelectQueueIndex}
        onPrevTrack={onPrevTrack}
        onNextTrack={onNextTrack}
        onQueueMenuToggle={onQueueMenuToggle}
      />
    );

    expect(screen.getByRole('button', { name: 'Back to Playlist' })).toBeInTheDocument();
    expect(screen.getByText('1 / 2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Next track' }));
    expect(onNextTrack).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: 'Playback Queue' }));
    expect(onQueueMenuToggle).toHaveBeenCalledWith(true);
    expect(screen.getByText('Blade Runner 2049')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Blade Runner 2049/i }));
    expect(onSelectQueueIndex).toHaveBeenCalledWith(1);
  });
```

2. Append to `web/src/components/player/CinemaPlayer.test.tsx`:
```tsx
  it('scrobbles final position, resets per-item state, and auto-advances queue on video ended', async () => {
    const queue = [
      {
        position: 0,
        added_at: 100,
        duration_seconds: 3600,
        card: { id: 10, title: 'Track One', media_type: 'movie' },
      },
      {
        position: 1,
        added_at: 200,
        duration_seconds: 4000,
        card: { id: 20, title: 'Track Two', media_type: 'movie' },
      },
    ];

    const createSessionSpy = vi
      .spyOn(api, 'createPlaybackSession')
      .mockImplementation(async (id) => ({
        session_id: `sess-${id}`,
        media_item_id: id,
        duration_seconds: 3600,
        resume_position_seconds: 0,
      }));
    const heartbeatSpy = vi.spyOn(api, 'sendPlaybackHeartbeat').mockResolvedValue();
    const closeSessionSpy = vi.spyOn(api, 'closePlaybackSession').mockResolvedValue();
    vi.spyOn(api, 'getPlaybackState').mockResolvedValue({
      user_id: 'u-1',
      media_item_id: 10,
      playback_position_seconds: 3500,
      watch_state: 'completed',
      last_watched_at: 100,
      play_count: 1,
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValue([]);
    vi.spyOn(api, 'getItemDetails').mockImplementation(async (id) => ({
      card: { id, title: id === 10 ? 'Track One' : 'Track Two', media_type: 'movie' },
      genres: [],
      stream_url: `/api/v1/stream/${id}`,
    }));

    const onSelectQueueIndex = vi.fn();
    const { container, rerender } = render(
      <CinemaPlayer
        itemId={10}
        onClose={mockOnClose}
        returnLabel="Back to Playlist"
        queue={queue}
        currentIndex={0}
        onSelectQueueIndex={onSelectQueueIndex}
      />
    );

    await waitFor(() => expect(createSessionSpy).toHaveBeenCalledWith(10));
    const video = container.querySelector('video') as HTMLVideoElement;
    Object.defineProperty(video, 'duration', { value: 3600, configurable: true });
    fireEvent.loadedMetadata(video);

    // Completed item starts at 0s rather than seeking to old position
    expect(video.currentTime).toBe(0);

    await act(async () => {
      fireEvent.ended(video);
    });

    await waitFor(() => {
      expect(heartbeatSpy).toHaveBeenCalledWith('sess-10', 3600);
      expect(onSelectQueueIndex).toHaveBeenCalledWith(1);
    });

    // Advance parent to item 20 -> closes sess-10 and opens sess-20
    rerender(
      <CinemaPlayer
        itemId={20}
        onClose={mockOnClose}
        returnLabel="Back to Playlist"
        queue={queue}
        currentIndex={1}
        onSelectQueueIndex={onSelectQueueIndex}
      />
    );

    await waitFor(() => {
      expect(closeSessionSpy).toHaveBeenCalledWith('sess-10');
      expect(createSessionSpy).toHaveBeenCalledWith(20);
    });
  });

  it('pauses the 3s control auto-hide timer while Playback Queue popover is open', async () => {
    vi.useFakeTimers();
    vi.spyOn(api, 'createPlaybackSession').mockResolvedValue(mockSession);
    vi.spyOn(api, 'getPlaybackState').mockResolvedValue(mockPlaybackState);
    vi.spyOn(api, 'getSubtitles').mockResolvedValue([]);
    vi.spyOn(api, 'getItemDetails').mockResolvedValue(mockDetails);

    const queue = [
      { position: 0, added_at: 1, duration_seconds: 100, card: { id: 10, title: 'A', media_type: 'movie' } },
      { position: 1, added_at: 2, duration_seconds: 100, card: { id: 11, title: 'B', media_type: 'movie' } },
    ];

    render(
      <CinemaPlayer
        itemId={10}
        onClose={mockOnClose}
        queue={queue}
        currentIndex={0}
        onSelectQueueIndex={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Playback Queue' }));
    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(screen.getByTestId('player-controls')).toHaveClass('opacity-100');
    vi.useRealTimers();
  });
```

3. Append to `web/src/App.test.tsx`:
```tsx
  it('shows Playlists tab for standard users, launches queue playback, and returns to playlist detail on close', async () => {
    api.setUser({ id: 'u-std', username: 'viewer', role: 'standard' });
    api.setToken('test-jwt');

    vi.spyOn(api, 'getLibraries').mockResolvedValue([]);
    vi.spyOn(api, 'getScreen').mockResolvedValue({ id: 'home', title: 'Home', widgets: [] });
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({
      playlists: [
        {
          id: 'pl-99',
          name: 'Road Trip Queue',
          owner_user_id: 'u-std',
          owner_username: 'viewer',
          is_public: false,
          is_editable: true,
          visible_item_count: 2,
          locked_item_count: 0,
          total_duration_seconds: 7200,
          cover_poster_urls: [],
          created_at: 100,
          updated_at: 200,
        },
      ],
    });
    vi.spyOn(api, 'getPlaylist').mockResolvedValue({
      summary: {
        id: 'pl-99',
        name: 'Road Trip Queue',
        owner_user_id: 'u-std',
        owner_username: 'viewer',
        is_public: false,
        is_editable: true,
        visible_item_count: 2,
        locked_item_count: 0,
        total_duration_seconds: 7200,
        cover_poster_urls: [],
        created_at: 100,
        updated_at: 200,
      },
      items: [
        {
          position: 0,
          added_at: 101,
          duration_seconds: 3600,
          card: { id: 501, title: 'First Movie', media_type: 'movie' },
        },
        {
          position: 1,
          added_at: 102,
          duration_seconds: 3600,
          card: { id: 502, title: 'Second Movie', media_type: 'movie' },
        },
      ],
    });

    render(<App />);

    const playlistsTab = screen.getByRole('button', { name: /^Playlists$/i });
    expect(playlistsTab).toBeInTheDocument();
    fireEvent.click(playlistsTab);

    await waitFor(() => expect(screen.getByText('Road Trip Queue')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Road Trip Queue'));

    await waitFor(() => expect(screen.getByText('First Movie')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Play All/i }));

    await waitFor(() => {
      expect(screen.getByText(/Cinema Player/i)).toBeInTheDocument();
      expect(screen.getByText('1 / 2')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /Back to Playlist/i }));

    // Restores exact PlaylistDetailView instead of resetting to Home
    await waitFor(() => {
      expect(screen.getByText('First Movie')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /All Playlists/i })).toBeInTheDocument();
    });
  });
```

- [ ] **Step 2: Run Vitest on the three updated test files to verify failure**

Run: `cd web && npm test -- --run src/components/player/PlayerControls.test.tsx src/components/player/CinemaPlayer.test.tsx src/App.test.tsx`
Expected: FAIL on missing queue controls, auto-advance, and `Playlists` top-nav button.

- [ ] **Step 3: Implement queue controls and Queue popover in `PlayerControls.tsx` and `CinemaPlayer.tsx`**

1. **`web/src/components/player/PlayerControls.tsx`**:
   - Import `SkipBack`, `SkipForward`, `ListVideo` from `lucide-react` and `PlaylistItemView` from `../../types`.
   - Accept optional props: `returnLabel = 'Back to Browse'`, `queue`, `currentIndex`, `onSelectQueueIndex`, `onPrevTrack`, `onNextTrack`, `onQueueMenuToggle`.
   - Add `isQueueMenuOpen` local state; toggling it closes `isSubtitlesMenuOpen` (and vice-versa) and invokes `onQueueMenuToggle?.(nextState)`.
   - Top bar Back button uses `aria-label={returnLabel}` and `<span>{returnLabel}</span>` (preserving `'Back to Browse'` by default for existing tests and `'Back to Playlist'` when launched from playlists), plus renders a `font-mono` track chip `${currentIndex + 1} / ${queue.length}` next to `title` whenever `queue && queue.length > 1 && currentIndex !== undefined`.
   - Transport bar (when `queue && queue.length > 1 && currentIndex !== undefined`):
     - `Previous` button (`SkipBack`, `aria-label="Previous track"`, disabled when `currentIndex === 0 && currentTime <= 5`, calls `onPrevTrack`).
     - `Next` button (`SkipForward`, `aria-label="Next track"`, disabled when `currentIndex >= queue.length - 1`, calls `onNextTrack`).
   - Right-side controls (next to `Subtitles` when `queue && queue.length > 1 && currentIndex !== undefined`):
     - `Queue` button (`ListVideo`, `aria-label="Playback Queue"`) toggling a `bg-panel/95 border border-border-subtle rounded-xl max-h-80 overflow-y-auto` popover drawer highlighting `idx === currentIndex` (`bg-accent/20 border-accent`), displaying poster/thumbnail, title, and `font-mono` formatted duration, and calling `onSelectQueueIndex?.(idx)` on click/Enter.

2. **`web/src/components/player/CinemaPlayer.tsx`**:
   - Extend `CinemaPlayerProps` with `returnLabel?: string`, `queue?: PlaylistItemView[]`, `currentIndex?: number`, `onSelectQueueIndex?: (index: number) => void`.
   - Track `sessionIdRef = useRef<string | null>(null)` and `isQueueMenuOpen` (`isQueueMenuOpenRef = useRef(false)`), pausing the 3-second inactivity hide timer whenever `isSubtitlesMenuOpenRef.current || isQueueMenuOpenRef.current` is `true`.
   - Add per-item reset inside `useEffect([itemId])`:
     ```ts
     isMetadataLoadedRef.current = false;
     hasSeekedResumeRef.current = false;
     resumePositionRef.current = null;
     setResumePosition(null);
     setCurrentTime(0);
     setDuration(0);
     setActiveSubtitleId(null);
     setSubtitles([]);
     setItemDetails(null);
     setRecentlyDownloadedId(null);
     ```
   - In `getPlaybackState(itemId)` effect (`line 156`), check `if (state?.watch_state === 'in_progress' && state.playback_position_seconds > 0)` so completed items (`watch_state === 'completed'`) start cleanly at `0s`.
   - Implement `handlePrevTrack` (`if (currentTime > 5) handleSeek(0); else if (queue && currentIndex !== undefined && currentIndex > 0) onSelectQueueIndex?.(currentIndex - 1);`) and `handleNextTrack` (`if (queue && currentIndex !== undefined && currentIndex + 1 < queue.length) onSelectQueueIndex?.(currentIndex + 1);`), plus keyboard shortcuts `Shift+P` (`MediaTrackPrevious`) and `Shift+N` (`MediaTrackNext`).
   - Attach `onEnded` handler to `<video>`:
     ```ts
     const handleVideoEnded = async () => {
       const finalSec = Math.floor(videoRef.current?.duration || duration || 0);
       if (sessionIdRef.current && finalSec > 0) {
         await api.sendPlaybackHeartbeat(sessionIdRef.current, finalSec).catch(() => {});
       }
       if (queue && currentIndex !== undefined && currentIndex + 1 < queue.length) {
         onSelectQueueIndex?.(currentIndex + 1);
       } else {
         setIsPlaying(false);
       }
     };
     ```

- [ ] **Step 4: Wire `Playlists` top-nav tab, queue state, and `previousView` return routing in `web/src/App.tsx`**

In `web/src/App.tsx`:
1. Import `ListVideo` from `lucide-react`, `PlaylistItemView` from `./types`, and `PlaylistsScreen` from `./components/playlists/PlaylistsScreen`.
2. Update `NavView`:
   ```ts
   export type NavView = 'home' | `library-${string}` | 'playlists' | 'player' | 'admin';
   ```
3. Add state hooks:
   ```ts
   const [previousView, setPreviousView] = useState<Exclude<NavView, 'player'>>('home');
   const [activePlaylistId, setActivePlaylistId] = useState<string | null>(null);
   const [playerQueue, setPlayerQueue] = useState<PlaylistItemView[] | null>(null);
   const [playerQueueIndex, setPlayerQueueIndex] = useState<number>(0);
   ```
   Reset `activePlaylistId(null)` and `playerQueue(null)` on `handleAuthSuccess` and `handleLogout`.
4. Update `handlePlayItem` and add `handlePlayQueue`:
   ```ts
   const handlePlayItem = (itemId: number) => {
     if (currentView !== 'player') {
       setPreviousView(currentView);
     }
     setPlayerQueue(null);
     setPlayerQueueIndex(0);
     setActivePlayingItemId(itemId);
     setCurrentView('player');
   };

   const handlePlayQueue = (items: PlaylistItemView[], startIndex = 0) => {
     if (items.length === 0) return;
     const clamped = Math.min(Math.max(0, startIndex), items.length - 1);
     if (currentView !== 'player') {
       setPreviousView(currentView);
     }
     setPlayerQueue(items);
     setPlayerQueueIndex(clamped);
     setActivePlayingItemId(items[clamped].card.id);
     setCurrentView('player');
   };
   ```
5. In top header `<nav>` (immediately after the `Home` button, visible to all authenticated users):
   ```tsx
   <button
     onClick={() => {
       setActivePlaylistId(null);
       handleNavigate('playlists');
     }}
     className={navItemClass(currentView === 'playlists')}
   >
     <ListVideo className="h-4 w-4" />
     Playlists
   </button>
   ```
6. Render `<PlaylistsScreen>` when `currentView === 'playlists'`, and update `<CinemaPlayer>` rendering:
   ```tsx
   {currentView === 'playlists' && (
     <PlaylistsScreen
       currentUser={currentUser}
       activePlaylistId={activePlaylistId}
       onSelectPlaylist={setActivePlaylistId}
       onPlayItem={handlePlayItem}
       onPlayQueue={handlePlayQueue}
       onNavigateToFolder={handleNavigateToFolder}
     />
   )}

   {currentView === 'player' && activePlayingItemId !== null && (
     <CinemaPlayer
       itemId={activePlayingItemId}
       returnLabel={previousView === 'playlists' ? 'Back to Playlist' : 'Back to Browse'}
       queue={playerQueue ?? undefined}
       currentIndex={playerQueue ? playerQueueIndex : undefined}
       onSelectQueueIndex={(idx) => {
         if (playerQueue && playerQueue[idx]) {
           setPlayerQueueIndex(idx);
           setActivePlayingItemId(playerQueue[idx].card.id);
         }
       }}
       onClose={() => {
         setActivePlayingItemId(null);
         setPlayerQueue(null);
         setCurrentView(previousView);
       }}
     />
   )}
   ```

- [ ] **Step 5: Run full Frontend Vitest suite and production Vite/TypeScript build**

Run: `cd web && npm test -- --run && npm run build`
Expected: 100% PASS across all Vitest suites (`client.test.ts`, `PlaylistsScreen.test.tsx`, `AddToPlaylistModal.test.tsx`, `CinemaPlayer.test.tsx`, `PlayerControls.test.tsx`, `App.test.tsx`, and all existing tests) and clean `tsc -b && vite build` output.

- [ ] **Step 6: Commit Task 6**

```bash
git add web/src/components/player/CinemaPlayer.tsx web/src/components/player/PlayerControls.tsx web/src/components/player/CinemaPlayer.test.tsx web/src/components/player/PlayerControls.test.tsx web/src/App.tsx web/src/App.test.tsx
git commit -m "feat(web): add CinemaPlayer queue playback and Playlists top-bar navigation"
```

---

## Track C: Client Documentation (`docs/CLIENT_GUIDE.md` & `README.md` ONLY)

### Task 7: External Client Playlist & Queue Playback Guide (`docs/CLIENT_GUIDE.md`, `README.md`)

**Files:**
- Modify: `docs/CLIENT_GUIDE.md:7-20, 320-380, 960-1188`
- Modify: `README.md:30-52, 420-445`

**Interfaces:**
- Consumes: Exact wire contracts from `docs/superpowers/specs/2026-11-12-playlists-design.md` (Sections 2.2–2.4, 3.2–3.5, 5.1).
- Produces: Complete external client developer documentation for `/api/v1/playlists/*`, private library headers & `locked_item_count`, `playlist_items` AST macro, `playlist:updated` SSE event, ExoPlayer/AVPlayer native queue recipe, error matrix updates, and `README.md` API index entries.

- [ ] **Step 1: Update `docs/CLIENT_GUIDE.md` Table of Contents, AST Macro table, SSE Event docs, new Playlists & Queue Playback section, and Error Matrix**

1. **Table of Contents (`docs/CLIENT_GUIDE.md:7-19`)**:
   Insert `9. [Playlists & Client Queue Playback](#9-playlists--client-queue-playback)` and renumber `Client Architecture Recipes & Best Practices` to `10` and `Error Handling & HTTP Status Matrix` to `11`.
2. **Section 4 (`Declarative Screen Layout Engine (AST)`)**:
   Document the `{"playlist_items": {"playlist_id": "<uuid>"}}` `QueryMacro` variant, noting that it supports `/api/v1/widgets/{widget_id}/data?offset=&limit=` pagination and gracefully resolves to an empty item list (`items: []`, `total_count: 0`) if the referenced playlist is deleted or private to another user.
3. **Section 8 (`Hierarchical Folder Browsing & Real-Time Events (SSE)`)**:
   Add `playlist:updated` to the SSE event reference:
   ```json
   {
     "type": "playlist:updated",
     "payload": {
       "playlist_id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
       "user_id": "u-123",
       "action": "items_changed",
       "timestamp": 1762942100
     }
   }
   ```
   Document that `action` is `"created" | "updated" | "deleted" | "items_changed"` and intentionally omits titles/item IDs because `/api/v1/events` is an unauthenticated broadcast stream.
4. **New Section 9 (`## 9. Playlists & Client Queue Playback`)**:
   - Document all 8 REST endpoints with exact HTTP methods, paths, query parameters, required headers (`Authorization: Bearer <jwt>` + optional `X-Kadr-Unlocked` / `X-Library-Unlock-Token`), and verbatim JSON request/response payloads matching `docs/superpowers/specs/2026-11-12-playlists-design.md`:
     - `GET /api/v1/playlists?contains_item_id=<optional i64>` -> `{ "playlists": [PlaylistSummary] }`
     - `POST /api/v1/playlists` -> `CreatePlaylistRequest` (including 1-step `items`) -> `201 Created` `PlaylistDetail`
     - `GET /api/v1/playlists/{id}` -> `200 OK` `PlaylistDetail` (`{ summary, items: PlaylistItemView[] }`)
     - `PATCH /api/v1/playlists/{id}` -> `UpdatePlaylistRequest` -> `200 OK` `PlaylistSummary`
     - `DELETE /api/v1/playlists/{id}` -> `204 No Content`
     - `POST /api/v1/playlists/{id}/items` -> `AddPlaylistItemsRequest` (`item_ids`, `show`, `folder`) -> `200 OK` `AddPlaylistItemsResponse` (`{ added_count, skipped_duplicate_count, summary }`)
     - `DELETE /api/v1/playlists/{id}/items/{media_item_id}` -> `200 OK` `PlaylistSummary`
     - `PUT /api/v1/playlists/{id}/items/order` -> `{ "item_ids": [...] }` -> `200 OK` `PlaylistSummary`
   - Document **Multi-User Permissions** (`is_public`, `is_editable`, per-viewer `resume_position_seconds` / `playback_progress`) and **Private PIN-Locked Library Isolation** (`locked_item_count`, zero metadata leakage when locked, `403 {"error": "LIBRARY_LOCKED"}` when adding/removing locked items without unlocking, and locked-preserving `PUT /items/order`).
   - Include **Native Client Queue Playback Recipes** (Kotlin AndroidX Media3 `ExoPlayer` and Swift `AVPlayer`) demonstrating the exact per-track lifecycle:
     1. Fetch `GET /api/v1/playlists/{id}` -> hold `items: PlaylistItemView[]` and `currentIndex` in client memory (optionally Fisher-Yates shuffled on a copy).
     2. For `active = items[currentIndex]`, inspect `GET /api/v1/items/{active.card.id}` `technical` codecs -> choose Direct Play (`/api/v1/stream/{id}?token=...&unlocked=...`) or HLS (`/api/v1/media/{id}/hls/master.m3u8?...`).
     3. Open `POST /api/v1/playback/sessions` (`{ "media_item_id": active.card.id }`), seek to `active.resume_position_seconds` if `> 0` (completed items have `resume_position_seconds` omitted and start at `0s`), and send 10-second `POST /api/v1/playback/{session_id}/progress` heartbeats.
     4. On natural EOF (`Player.STATE_ENDED` / `AVPlayerItemDidPlayToEndTime`) or manual Next/Previous track skip: send final `POST /api/v1/playback/{session_id}/progress` (`position_seconds = duration_seconds` on EOF so the item scrobbles `>= 90%` `completed`, or current position on skip), call `DELETE /api/v1/playback/sessions/{session_id}`, advance `currentIndex`, and open the next track's session.
5. **Section 11 (`Error Handling & HTTP Status Matrix`)**:
   Add playlist-specific rows (`400` invalid name/description length, empty batch add, invalid folder path, non-permutation `item_ids`; `403 Forbidden` mutating another user's public playlist; `403 LIBRARY_LOCKED` adding/removing locked items; `404 Playlist not found` for missing or another user's private playlist).

- [ ] **Step 2: Add Playlists endpoints to `README.md`**

In `README.md`:
1. Add a concise bullet under `Key Features` describing manual ordered playlists, server-wide sharing (`is_public`), batch Show/Season/Folder additions, `playlist_items` AST rails, and Cinema Player queue playback.
2. Under `## REST & Streaming API Reference` (right after `Playback & Scrobbling`), add a `### Playlists & Queue Playback` subsection listing all 8 `/api/v1/playlists/*` endpoints and add `playlist:updated` to the `GET /api/v1/events` SSE list.

- [ ] **Step 3: Verify exact field/route parity and link integrity across `docs/CLIENT_GUIDE.md` and `README.md`**

Run:
```bash
grep -n "/api/v1/playlists" docs/CLIENT_GUIDE.md README.md
grep -n "locked_item_count\|contains_queried_item\|skipped_duplicate_count\|playlist:updated\|playlist_items" docs/CLIENT_GUIDE.md
git diff --stat
```
Expected: Every route and JSON key matches `docs/superpowers/specs/2026-11-12-playlists-design.md` verbatim, and `git diff --stat` shows ONLY `docs/CLIENT_GUIDE.md` and `README.md` modified in Track C.

- [ ] **Step 4: Commit Task 7**

```bash
git add docs/CLIENT_GUIDE.md README.md
git commit -m "docs(client): document playlists REST API, AST macro, SSE event, and queue playback recipe"
```
