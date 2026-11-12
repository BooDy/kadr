# Manual Ordered Playlists & Client Playback Queue Design

**Date:** 2026-11-12  
**Status:** Approved

---

## 1. Overview & Goals

Enable users to create, curate, share, and play manual ordered playlists across both Kadr's React Web UI (`web/`) and external/native clients (Smart TVs, mobile, desktop) communicating with `kadr-server`.

### 1.1 Primary Goals
- **Full CRUD & Ordering API**: Provide REST endpoints under `/api/v1/playlists` supporting creation (including 1-step Create & Add), metadata/visibility updates, deletion, single and batch item addition (playable items, TV series/seasons, and library folders), item removal, and deterministic reordering.
- **Multi-User Ownership & Server Sharing**: Scope playlists to an `owner_user_id` with an `is_public` flag (`0` private to owner, `1` visible and playable by all authenticated profiles on the server while remaining editable only by the owner or an `admin`).
- **Declarative Screen & Real-Time Integration**: Expose playlists as a `QueryMacro::PlaylistItems { playlist_id }` data source in Kadr's Declarative Widget AST (`Carousel` and `Grid` rails on `Home` or custom screens, with `/api/v1/widgets/{widget_id}/data` pagination) and broadcast metadata-free `playlist:updated` Server-Sent Events (`SSE`).
- **Web UI & Queue Playback**: Add a top-level `Playlists` view, playlist detail screen, `AddToPlaylistModal` workflows across catalog and folder views, Layout Studio binding support, and multi-item queue playback (auto-advance, previous/next track controls, and an interactive queue drawer) inside `CinemaPlayer`.
- **External Client Parity**: Document all playlist REST contracts, authentication/unlock headers, AST bindings, SSE payloads, and the client-side queue playback recipe in `docs/CLIENT_GUIDE.md`.

### 1.2 Architectural Invariants Preserved
- **Memory & Portability Budget**: Strictly preserve Kadr's `<= 30 MB` idle RSS target and pure-Rust static `musl` portability (`deadpool-sqlite` + `rusqlite` WAL mode, zero new native C/C++ dependencies).
- **Existing Streaming & Scrobbling Pipeline**: Preserve existing direct-play (`GET /api/v1/stream/{item_id}` HTTP 206 byte-range) and universal HLS fallback (`GET /api/v1/media/{item_id}/hls/master.m3u8`) routes alongside per-item playback sessions (`POST /api/v1/playback/sessions`, 10s heartbeats, `DELETE /api/v1/playback/sessions/{session_id}`).
- **Fail-Closed Private Library Isolation**: Items belonging to PIN-locked private libraries (`libraries.is_private = 1`) are never leaked through playlist summaries, cover mosaics, detail item lists, AST widgets, or SSE events unless the caller supplies a valid signed `UnlockedLibraries` token (`X-Kadr-Unlocked`, `X-Library-Unlock-Token`, `?unlocked=`, or `?unlock_token=`).

### 1.3 Non-Goals (v1)
- Dynamic rule-based or SQL filter "smart playlists".
- Duplicate `media_item_id` entries within the same playlist (playlists enforce set-membership per `media_item_id` with explicit ordering).
- Server-persisted active playback queue state across devices (queue progression is orchestrated client-side while per-item watch progress continues to sync through `user_playback_states`).

---

## 2. Storage & Data Model (`crates/kadr-storage`, `crates/kadr-core`)

### 2.1 Database Migration (`009_playlists.sql`)

Register `009_playlists.sql` in `crates/kadr-storage/src/migrations.rs` immediately after `008_settings.sql`:

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

**Schema invariants:**
- `playlists.id`: UUID v4 string (`TEXT PRIMARY KEY`), matching `users.id` and `libraries.id`.
- `playlists.name`: Trimmed non-empty UTF-8 string of `1..=120` characters (`chars().count()`).
- `playlists.description`: Optional trimmed UTF-8 string; empty strings normalize to `None` (`NULL`); maximum `500` characters (`chars().count()`).
- `playlists.is_public`: `0` (private to owner) or `1` (shared with all authenticated server users).
- `playlists.created_at` / `updated_at` / `playlist_items.added_at`: Unix epoch seconds (`i64`). Any item addition, removal, reorder, or metadata edit updates `playlists.updated_at`.
- `playlist_items.position`: 0-based dense integer sequence `0..N-1` across all rows stored in `playlist_items` for a given `playlist_id`.
- `PRIMARY KEY (playlist_id, media_item_id)` prevents duplicate items in a playlist at the storage layer and enables `INSERT OR IGNORE` batch deduplication.

### 2.2 Domain Models (`crates/kadr-core`)

Add a `playlists` module (or structs re-exported from `crates/kadr-core/src/models.rs` and `lib.rs`) defining the core domain and wire types:

```rust
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
```

**Field semantics:**
- `cover_poster_urls`: Up to `4` poster URLs (`/api/v1/artwork/{id}/poster`) drawn in playlist `position ASC` order from *visible* items that have a non-empty `metadata.poster_path`. Locked items never contribute to `cover_poster_urls`.
- `total_duration_seconds`: Sum of `technical.duration_seconds.max(0)` across *visible* items only (so locked library runtimes are not leaked).
- `contains_queried_item`: Populated as `Some(bool)` only when `GET /api/v1/playlists?contains_item_id=<id>` is requested; `None` (omitted in JSON) otherwise. If `<id>` belongs to a locked private library that the caller has not unlocked, `contains_queried_item` evaluates to `Some(false)` so membership of locked items cannot be probed without unlocking.
- `PlaylistItemView`: Uses nested `card: CardViewModel` (matching `ItemDetailsPayload.card`) rather than `#[serde(flatten)]`, avoiding intermediate map buffering and field collisions.
  - `position`: Normalized 0-based index (`0..items.len()`) within the caller's visible `items` slice so locked item positions do not create index gaps or reveal where locked items sit.
  - `duration_seconds`: `item.technical.duration_seconds.max(0)`.
  - `resume_position_seconds`: `Some(pos)` when the caller's `PlaybackState` for `card.id` has `watch_state == WatchState::InProgress && playback_position_seconds > 0`, otherwise `None`.

### 2.3 Access Control Matrix

| Operation | Allowed Caller | Private Playlist (`is_public = false`) Not Owned by Caller | Public Playlist (`is_public = true`) Not Owned by Caller |
| :--- | :--- | :--- | :--- |
| **Read / Play / Hydrate** (`GET`, Widget AST) | `owner_user_id == auth_user.id \|\| is_public` | `404 Not Found` | `200 OK` (`is_editable: auth_user.role == Admin`) |
| **Mutate Metadata / Items / Delete** (`PATCH`, `POST .../items`, `PUT .../order`, `DELETE`) | `owner_user_id == auth_user.id \|\| auth_user.role == UserRole::Admin` | `404 Not Found` (do not leak existence) | `403 Forbidden` (unless caller is `Admin`) |

### 2.4 Private PIN-Locked Library Isolation

All repository queries reading playlist items join `playlist_items pi JOIN media_items m ON pi.media_item_id = m.id JOIN libraries l ON m.library_id = l.id`:
1. Each joined row evaluates `is_visible = !l.is_private || unlocked_ids.contains(&l.id)` (fail-closed if library lookup fails).
2. **Visible rows**:
   - Counted in `visible_item_count`.
   - Contribute to `total_duration_seconds` and (first 4 with posters) `cover_poster_urls`.
   - Hydrated into `CardViewModel` using `to_card_view_model(&item, caller_playback_state)` where `PlaybackState` is batch-fetched via `PlaybackRepository::get_states_for_items(&auth_user.id, &visible_ids)` — always reflecting the *viewer's* watch progress, even on a shared public playlist.
3. **Locked rows**:
   - Excluded completely from `PlaylistDetail.items`, `cover_poster_urls`, `total_duration_seconds`, and `WidgetResolver` card lists.
   - Counted strictly as an aggregate integer in `PlaylistSummary.locked_item_count` — leaking zero titles, filenames, paths, durations, item IDs, or artwork URLs.

### 2.5 Startup Deduplication Safety (`MediaItemRepository::deduplicate_media_items`)

`MediaItemRepository::deduplicate_media_items` (`crates/kadr-storage/src/repos/media_item_repo.rs`) runs on server startup and deletes duplicate `media_items` rows (`del_id`) that canonicalize to the same physical file as `keep_id`. Because `PRAGMA foreign_keys = ON` and `playlist_items.media_item_id` declares `ON DELETE CASCADE`, deleting `del_id` directly would silently drop merged items from user playlists.

Update the transaction in `deduplicate_media_items` so that before `DELETE FROM media_items WHERE id = ?1`:
1. Record any `playlist_id`s referencing `del_id`:
   `SELECT playlist_id FROM playlist_items WHERE media_item_id = ?1`.
2. Migrate references to `keep_id` while respecting `PRIMARY KEY (playlist_id, media_item_id)` if both `keep_id` and `del_id` were in the same playlist:
   ```sql
   UPDATE OR IGNORE playlist_items SET media_item_id = ?1 WHERE media_item_id = ?2;
   DELETE FROM playlist_items WHERE media_item_id = ?1;
   ```
   (passing `(keep_id, del_id)` to `UPDATE OR IGNORE` and `(del_id,)` to `DELETE`).
3. After processing `duplicates_to_delete`, re-compact `position` (`0..N-1` ordered by `position ASC, added_at ASC`) for every affected `playlist_id` before committing the transaction.

### 2.6 `PlaylistRepository` (`crates/kadr-storage/src/repos/playlist_repo.rs`)

Expose `#[derive(Clone)] pub struct PlaylistRepository { pool: Pool }` constructed via `PlaylistRepository::new(pool)` and re-exported from `crates/kadr-storage/src/repos/mod.rs` and `crates/kadr-storage/src/lib.rs`:
- `create_playlist(&self, id: &str, owner_user_id: &str, name: &str, description: Option<&str>, is_public: bool) -> Result<Playlist>`
- `get_playlist_raw(&self, playlist_id: &str) -> Result<Option<(Playlist, String)>>` (joins `users` to fetch `owner_username`)
- `update_playlist(&self, playlist_id: &str, name: Option<&str>, description: Option<Option<&str>>, is_public: Option<bool>) -> Result<Option<Playlist>>`
- `delete_playlist(&self, playlist_id: &str) -> Result<bool>`
- `list_playlists_for_user(&self, user_id: &str, is_admin: bool, unlocked_ids: &[String], contains_item_id: Option<i64>) -> Result<Vec<PlaylistSummary>>` (returns playlists where `owner_user_id = user_id OR is_public = 1`, ordered by `(owner_user_id = user_id) DESC, updated_at DESC`)
- `get_playlist_items_with_privacy(&self, playlist_id: &str, unlocked_ids: &[String]) -> Result<(Vec<(i64, MediaItem)>, usize)>` (returns ordered `(added_at, MediaItem)` for visible items plus `locked_item_count`)
- `add_items(&self, playlist_id: &str, media_item_ids: &[i64]) -> Result<(usize, usize)>` (transactional batch insert appending new items starting at `COALESCE(MAX(position) + 1, 0)` via `INSERT OR IGNORE`, updating `playlists.updated_at`, returning `(added_count, skipped_duplicate_count)`)
- `remove_item(&self, playlist_id: &str, media_item_id: i64) -> Result<bool>` (transactional delete + dense `0..N-1` position compaction + `updated_at` bump; returns `Ok(false)` when `(playlist_id, media_item_id)` is absent)
- `reorder_visible_items(&self, playlist_id: &str, ordered_visible_ids: &[i64], unlocked_ids: &[String]) -> Result<()>` (transactional validation & dense `0..N-1` update; returns `StorageError::Validation` if `ordered_visible_ids` is not an exact permutation of the playlist's currently visible `media_item_id`s)

---

## 3. REST API, Batch Expansion, Widget AST & SSE (`crates/kadr-server`, `docs/CLIENT_GUIDE.md`)

### 3.1 Router Wiring (`crates/kadr-server/src/api/playlist_routes.rs` & `api/mod.rs`)

- Implement handlers in `crates/kadr-server/src/api/playlist_routes.rs` exposing `pub fn playlist_routes() -> Router`.
- Inside `create_router_with_ingest` (`crates/kadr-server/src/api/mod.rs`), construct `let playlist_repo = PlaylistRepository::new(media_repo.pool().clone());`, merge `.merge(playlist_routes::playlist_routes())`, and inject `.layer(Extension(playlist_repo))`.
- Deriving `PlaylistRepository` from `media_repo.pool().clone()` preserves the exact parameter lists of `create_router`, `create_router_with_layout`, `create_router_with_subtitles`, `create_router_with_events`, and `create_router_with_ingest` so no existing integration tests break.
- Every endpoint under `/api/v1/playlists` requires `AuthUser` (`401 Unauthorized` if missing/invalid) and extracts `UnlockedLibraries`.

### 3.2 Endpoint Specifications

#### 3.2.1 `GET /api/v1/playlists?contains_item_id=<optional i64>`
Lists all playlists owned by `auth_user` plus all public playlists (`is_public == true`) created by other users on the server.
- **Query params**:
  - `contains_item_id: Option<i64>` — When supplied, sets `contains_queried_item: Some(true | false)` on each `PlaylistSummary` (evaluating to `Some(false)` if the item does not exist or resides in a locked private library not in `UnlockedLibraries`).
- **Response (`200 OK`)**:
  ```json
  {
    "playlists": [
      {
        "id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
        "name": "Sci-Fi Marathon",
        "description": "Weekend director's cuts",
        "owner_user_id": "u-123",
        "owner_username": "alex",
        "is_public": true,
        "is_editable": true,
        "visible_item_count": 4,
        "locked_item_count": 1,
        "total_duration_seconds": 34200,
        "cover_poster_urls": [
          "/api/v1/artwork/10/poster",
          "/api/v1/artwork/14/poster",
          "/api/v1/artwork/22/poster",
          "/api/v1/artwork/31/poster"
        ],
        "created_at": 1762940000,
        "updated_at": 1762941800,
        "contains_queried_item": false
      }
    ]
  }
  ```

#### 3.2.2 `POST /api/v1/playlists`
Creates a new playlist owned by `auth_user.id`, optionally seeding initial items in a single step.
- **Request body (`CreatePlaylistRequest`)**:
  ```json
  {
    "name": "Sci-Fi Marathon",
    "description": "Weekend director's cuts",
    "is_public": false,
    "items": {
      "item_ids": [10, 14]
    }
  }
  ```
  - `name`: required; trimmed length `1..=120` chars (`400 Bad Request` otherwise).
  - `description`: optional; trimmed length `<= 500` chars (`400 Bad Request` if `> 500`); empty string normalized to `None`.
  - `is_public`: optional `bool` (defaults to `false`).
  - `items`: optional `AddPlaylistItemsRequest` (same expansion, validation, and `403 LIBRARY_LOCKED` checks as `POST /api/v1/playlists/{id}/items`; validated *before* committing playlist creation so a failed initial item check does not leave an orphan empty playlist).
- **Side effects**: Emits `SystemEvent::PlaylistUpdated` (`action: "created"`).
- **Response (`201 Created`)**: Full `PlaylistDetail` (`{ summary, items }`).

#### 3.2.3 `GET /api/v1/playlists/{id}`
Returns playlist metadata summary and hydrated visible items in playback order.
- **Permissions**: `owner_user_id == auth_user.id || is_public`; otherwise `404 Not Found`.
- **Response (`200 OK`)**: `PlaylistDetail`:
  ```json
  {
    "summary": {
      "id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
      "name": "Sci-Fi Marathon",
      "description": "Weekend director's cuts",
      "owner_user_id": "u-123",
      "owner_username": "alex",
      "is_public": true,
      "is_editable": true,
      "visible_item_count": 2,
      "locked_item_count": 0,
      "total_duration_seconds": 16800,
      "cover_poster_urls": [
        "/api/v1/artwork/10/poster",
        "/api/v1/artwork/14/poster"
      ],
      "created_at": 1762940000,
      "updated_at": 1762941800
    },
    "items": [
      {
        "position": 0,
        "added_at": 1762940010,
        "duration_seconds": 8880,
        "resume_position_seconds": 1240,
        "card": {
          "id": 10,
          "title": "Arrival",
          "subtitle": "2016 • 4K",
          "poster_url": "/api/v1/artwork/10/poster",
          "backdrop_url": "/api/v1/artwork/10/backdrop",
          "media_type": "movie",
          "playback_progress": 0.1396,
          "rating": 7.9,
          "release_year": 2016,
          "badge": "RESUME"
        }
      }
    ]
  }
  ```

#### 3.2.4 `PATCH /api/v1/playlists/{id}`
Updates playlist metadata and/or sharing visibility.
- **Permissions**: `owner_user_id == auth_user.id || auth_user.role == UserRole::Admin` (`404` if invisible to caller, `403` if public but caller is neither owner nor admin).
- **Request body (`UpdatePlaylistRequest`)**:
  ```json
  {
    "name": "Updated Name",
    "description": "Updated description or empty string to clear",
    "is_public": true
  }
  ```
  - All fields optional; at least one field may be provided (no-op returns current summary).
  - If `name` is `Some(s)`, trimmed length must be `1..=120` (`400 Bad Request` otherwise).
  - If `description` is `Some(s)`, trimmed empty string clears `description` to `NULL`; length `> 500` returns `400 Bad Request`.
- **Side effects**: Updates `updated_at` and emits `SystemEvent::PlaylistUpdated` (`action: "updated"`).
- **Response (`200 OK`)**: Updated `PlaylistSummary`.

#### 3.2.5 `DELETE /api/v1/playlists/{id}`
Deletes a playlist and cascades deletion to `playlist_items`.
- **Permissions**: `owner_user_id == auth_user.id || auth_user.role == UserRole::Admin` (`404` if invisible, `403` if public non-owner/non-admin).
- **Side effects**: Emits `SystemEvent::PlaylistUpdated` (`action: "deleted"`).
- **Response**: `204 No Content`.

#### 3.2.6 `POST /api/v1/playlists/{id}/items`
Expands, validates, and appends one or more playable items to a playlist.
- **Permissions**: Playlist edit permission (`owner` or `Admin`; `404` if invisible, `403` if public non-editable).
- **Request body (`AddPlaylistItemsRequest`)**:
  ```json
  {
    "item_ids": [10, 14],
    "show": {
      "show_id": 50,
      "season_number": 1
    },
    "folder": {
      "library_id": "lib-uuid",
      "path": "Season 01",
      "recursive": true
    }
  }
  ```
  At least one of `item_ids` (non-empty), `show`, or `folder` must be supplied (`400 Bad Request` if all are omitted/empty). When multiple sources are provided in one request, they expand in order: `item_ids`, then `show`, then `folder`.

- **Expansion & Validation Rules**:
  1. **`item_ids: Option<Vec<i64>>`**:
     - Fetch each `MediaItem` in order (`404 Not Found` if any ID does not exist).
     - Check library lock status via `UnlockedLibraries`: if `lib.is_private && !unlocked.is_unlocked(&lib.id)` -> `403 Forbidden` `{"error": "LIBRARY_LOCKED"}`.
     - If `item.item_type == MediaType::Season` -> `400 Bad Request` `{"error": "Non-playable media item type"}`.
     - If `item.item_type == MediaType::Show` -> expand to all episodes belonging to that series (`media_repo.find_episodes_by_series(&item.title)`) filtered to unlocked/public libraries (and matching `item.library_id` when episodes reside in the same library), sorted deterministically by `(metadata.season.unwrap_or(u32::MAX), metadata.episode.unwrap_or(u32::MAX), title.as_str(), id)`. If the show has zero episodes, `0` items are expanded (`400 Bad Request` `{"error": "Show has no playable episodes"}` if the entire request yields zero candidate items).
     - All other media types (`Movie`, `Episode`, `Anime`, `Music`, `HomeVideos`, `Audiobook`, `LiveChannel`, `Unknown`) append `item.id` directly.
  2. **`show: Option<AddShowItemsSpec>` (`{ show_id: i64, season_number: Option<u32> }`)**:
     - Fetch `show_id` (`404` if missing, `400` if `item_type != MediaType::Show`).
     - Verify `show.library_id` is unlocked (`403 {"error": "LIBRARY_LOCKED"}` if private & locked).
     - Fetch episodes via `media_repo.find_episodes_by_series(&show.title)`, filter to unlocked/public libraries (and `metadata.season == Some(season_number)` when `season_number` is specified), and sort by `(metadata.season.unwrap_or(u32::MAX), metadata.episode.unwrap_or(u32::MAX), title.as_str(), id)`.
     - If no matching episodes exist -> `400 Bad Request` `{"error": "No playable episodes found"}`.
  3. **`folder: Option<AddFolderItemsSpec>` (`{ library_id: String, path: String, recursive: Option<bool> }`)**:
     - Fetch `Library` by `library_id` (`404` if missing).
     - Verify library privacy: if `lib.is_private && !unlocked.is_unlocked(&lib.id)` -> `403 Forbidden` `{"error": "LIBRARY_LOCKED"}`.
     - Validate and sandbox `path` identically to `browse_library_folder` (`crates/kadr-server/src/api/library_routes.rs`): reject `..`, null bytes, or absolute paths (`400 Bad Request`), resolve against `lib.paths` (canonicalizing and verifying the resolved directory stays strictly inside a configured library root; `404` if the relative directory does not exist in any root).
     - Discover playable media files inside the matched directory (single-level when `recursive.unwrap_or(false) == false`, recursive depth-first walk skipping hidden `.` entries when `recursive == true`), sort paths naturally/lexicographically by relative path, resolve ingested `MediaItem` rows via `media_repo.find_by_paths(...)` (filtering to `item.library_id == library_id` and playable non-`Show`/non-`Season` items), and collect their `id`s in sorted path order.
     - If zero ingested playable items exist in the folder -> `400 Bad Request` `{"error": "No playable media items in folder"}`.
  4. **Deduplication & Insertion**:
     - Deduplicate candidate IDs preserving first-seen order, insert via `PlaylistRepository::add_items` (`INSERT OR IGNORE` at `max(position) + 1`), and emit `SystemEvent::PlaylistUpdated` (`action: "items_changed"`).
- **Response (`200 OK`)**:
  ```json
  {
    "added_count": 8,
    "skipped_duplicate_count": 2,
    "summary": {
      "id": "6f9d8c12-3a4b-4e5f-8c9d-0e1f2a3b4c5d",
      "name": "Sci-Fi Marathon",
      "description": "Weekend director's cuts",
      "owner_user_id": "u-123",
      "owner_username": "alex",
      "is_public": true,
      "is_editable": true,
      "visible_item_count": 10,
      "locked_item_count": 0,
      "total_duration_seconds": 54600,
      "cover_poster_urls": [
        "/api/v1/artwork/10/poster",
        "/api/v1/artwork/14/poster",
        "/api/v1/artwork/51/poster",
        "/api/v1/artwork/52/poster"
      ],
      "created_at": 1762940000,
      "updated_at": 1762942100
    }
  }
  ```

#### 3.2.7 `DELETE /api/v1/playlists/{id}/items/{media_item_id}`
Removes a single media item from a playlist and compacts remaining item positions to `0..N-1`.
- **Permissions & Locked-Library Rule**:
  1. Check playlist edit permission (`owner` or `Admin`; `404` if playlist is missing or private to another user, `403` if public and caller is not editable).
  2. Look up `media_item_id` via `media_repo.get_by_id(media_item_id)`: if the item exists in `media_items` and its parent `Library` has `is_private && !unlocked.is_unlocked(&lib.id)`, return `403 Forbidden` `{"error": "LIBRARY_LOCKED"}` immediately (preventing locked-item membership probing without unlocking the library first).
  3. Execute `PlaylistRepository::remove_item(id, media_item_id)`, which deletes `(id, media_item_id)` and re-compacts remaining `playlist_items.position` values to `0..N-1` inside a single SQLite transaction. If `remove_item` returns `Ok(false)` (item was not in the playlist) -> `404 Not Found` `{"error": "Item not found in playlist"}`.
- **Side effects**: Emits `SystemEvent::PlaylistUpdated` (`action: "items_changed"`).
- **Response (`200 OK`)**: Updated `PlaylistSummary`.

#### 3.2.8 `PUT /api/v1/playlists/{id}/items/order`
Reorders the caller's currently visible items while preserving any locked private-library items untouched.
- **Permissions**: Playlist edit permission (`owner` or `Admin`; `404` if invisible, `403` if public non-editable).
- **Request body (`ReorderPlaylistItemsRequest`)**:
  ```json
  {
    "item_ids": [14, 10, 31, 22]
  }
  ```
- **Ordering Algorithm**:
  1. Load all `playlist_items` rows for `id` ordered by `position ASC`, partitioned by `UnlockedLibraries` into `visible_ids: Vec<i64>` and `locked_ids: Vec<i64>` (preserving `locked_ids`'s existing relative order).
  2. Validate that `payload.item_ids.len() == visible_ids.len()`, `payload.item_ids` contains no duplicates, and its set of IDs equals the set of `visible_ids`. If validation fails -> `400 Bad Request` `{"error": "item_ids must be an exact permutation of visible playlist items"}`.
  3. Write new dense positions inside a single SQLite transaction:
     - `payload.item_ids[0..visible_ids.len()]` receive `position = 0..visible_ids.len() - 1`.
     - `locked_ids[0..locked_ids.len()]` receive `position = visible_ids.len()..(visible_ids.len() + locked_ids.len() - 1)`.
  4. Update `playlists.updated_at` and emit `SystemEvent::PlaylistUpdated` (`action: "items_changed"`).
- **Response (`200 OK`)**: Updated `PlaylistSummary`.

### 3.3 Declarative Widget AST (`crates/kadr-core/src/ast.rs` & `WidgetResolver`)

1. **`QueryMacro` Extension**:
   Add `PlaylistItems { playlist_id: String }` to `QueryMacro` in `crates/kadr-core/src/ast.rs` (`#[serde(rename_all = "snake_case")]` serializes this as `{"playlist_items": {"playlist_id": "<uuid>"}}`):
   ```rust
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
2. **`WidgetResolver` Hydration & Pagination (`crates/kadr-server/src/resolver/resolver.rs`)**:
   - Store `playlist_repo: PlaylistRepository` on `WidgetResolver` (constructed via `PlaylistRepository::new(media_repo.pool().clone())` inside `WidgetResolver::new` so `WidgetResolver::new` signature is unchanged).
   - In `determine_effective_library_id`: return `None` when `binding.macro_type` is `QueryMacro::PlaylistItems { .. }` (unless `binding.filters.library_id` is explicitly set), since playlists span libraries.
   - When resolving `QueryMacro::PlaylistItems { playlist_id }` (for both full-screen hydration in `GET /api/v1/screens/{screen_id}` and paginated fetches in `GET /api/v1/widgets/{widget_id}/data?offset=&limit=`):
     - Load playlist via `playlist_repo.get_playlist_raw(playlist_id)`.
     - **Graceful fallback**: If the playlist was deleted (`None`) or is private to another user (`playlist.owner_user_id != user_id && !playlist.is_public`), return `Ok((Vec::new(), None, Some(0)))` rather than failing the screen layout resolution.
     - Load visible items via `playlist_repo.get_playlist_items_with_privacy(playlist_id, unlocked_ids)`, apply optional `WidgetFilterConfig` (`is_item_matching_filters`), compute `total = filtered_items.len() as u64`, slice `offset..min(offset + limit, total)`, hydrate `CardViewModel`s with caller `PlaybackState`, and compute `next_cursor = if (offset as u64 + page.len() as u64) < total { Some((offset + page.len() as u32).to_string()) } else { None }`.

### 3.4 SSE Event Bus (`crates/kadr-core/src/events.rs`)

Add `PlaylistUpdated` to `SystemEvent` in `crates/kadr-core/src/events.rs`:

```rust
#[serde(rename = "playlist:updated")]
PlaylistUpdated {
    playlist_id: String,
    user_id: String,
    action: String,
    timestamp: i64,
},
```
- `SystemEvent::event_type(&self)` returns `"playlist:updated"`.
- `action` is one of `"created"`, `"updated"`, `"deleted"`, or `"items_changed"`.
- Because `GET /api/v1/events` is an unauthenticated broadcast stream, the payload intentionally excludes playlist names, descriptions, media item IDs, titles, and poster URLs.

### 3.5 External Client Documentation (`docs/CLIENT_GUIDE.md`)

Add a dedicated **Playlists & Client Queue Playback** section to `docs/CLIENT_GUIDE.md` (and update the Table of Contents and Error Matrix) covering:
1. All `/api/v1/playlists/*` endpoints, request/response JSON schemas, and required headers (`Authorization: Bearer <jwt>` + `X-Kadr-Unlocked` / `X-Library-Unlock-Token`).
2. Private library behavior (`locked_item_count`, `403 {"error": "LIBRARY_LOCKED"}` on adding/deleting locked items, safe reorder semantics).
3. `playlist_items` Widget AST binding format and `playlist:updated` SSE event payload.
4. **Native TV / Mobile Queue Playback Recipe**:
   - Fetch `GET /api/v1/playlists/{id}` -> hold `items: PlaylistItemView[]` and `currentIndex` in client memory (optionally shuffled via Fisher-Yates on a copy).
   - For `activeItem = items[currentIndex]`: inspect `/api/v1/items/{activeItem.card.id}` for `technical` codecs -> choose Direct Play (`/api/v1/stream/{id}`) or HLS (`/api/v1/media/{id}/hls/master.m3u8`) -> `POST /api/v1/playback/sessions` -> 10s `POST /api/v1/playback/{session_id}/progress` heartbeats -> on natural EOF or manual skip, `DELETE /api/v1/playback/sessions/{session_id}` with final position, increment `currentIndex`, and start the next item's session.

---

## 4. Web UI & Cinema Player Queue (`web/`)

### 4.1 TypeScript Types & API Client (`web/src/types/index.ts`, `web/src/api/client.ts`)

1. **`web/src/types/index.ts`**:
   - Add `PlaylistSummary`, `PlaylistItemView`, `PlaylistDetail`, `CreatePlaylistPayload`, `UpdatePlaylistPayload`, `AddPlaylistItemsPayload`, `AddPlaylistItemsResponse`, and `QueueItem` interfaces matching Section 3.2 exact JSON fields.
   - Extend `QueryMacro` union with `| { playlist_items: { playlist_id: string } }`.
   - Extend `SystemEvent` union with `| { type: 'playlist:updated'; payload: { playlist_id: string; user_id: string; action: string; timestamp: number } }`.
2. **`web/src/api/client.ts`**:
   - `getPlaylists(containsItemId?: number): Promise<{ playlists: PlaylistSummary[] }>`
   - `createPlaylist(payload: CreatePlaylistPayload): Promise<PlaylistDetail>`
   - `getPlaylist(id: string): Promise<PlaylistDetail>`
   - `updatePlaylist(id: string, payload: UpdatePlaylistPayload): Promise<PlaylistSummary>`
   - `deletePlaylist(id: string): Promise<void>`
   - `addPlaylistItems(id: string, payload: AddPlaylistItemsPayload): Promise<AddPlaylistItemsResponse>`
   - `removePlaylistItem(id: string, mediaItemId: number): Promise<PlaylistSummary>`
   - `reorderPlaylistItems(id: string, itemIds: number[]): Promise<PlaylistSummary>`

### 4.2 Top Navigation & Return-View Routing (`web/src/App.tsx`)

- Extend ``NavView`` to ``'home' | `library-${string}` | 'playlists' | 'player' | 'admin'``.
- Add a **`Playlists`** button (`ListVideo` icon from `lucide-react`) in the top header bar right after `Home` (and in mobile navigation if present), visible to **all authenticated profiles** (`standard` and `admin`), styled per `theme.md` (`text-accent bg-panel` when active, `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`).
- **Player Launch & Return Navigation State**:
  - Store:
    - `previousView: Exclude<NavView, 'player'>` (defaults to `'home'`).
    - `activePlaylistId: string | null` (persisted across `'playlists'` <-> `'player'` transitions so returning from `CinemaPlayer` restores the exact open `PlaylistDetailView`).
    - `playerQueue: PlaylistItemView[] | null` and `playerQueueIndex: number` alongside `activePlayingItemId: number | null`.
  - Provide `handlePlayItem(itemId: number)` (single-item playback: clears `playerQueue`, saves `previousView = currentView`, sets `activePlayingItemId = itemId`, `currentView = 'player'`) and `handlePlayQueue(items: PlaylistItemView[], startIndex = 0)` (sets `playerQueue = items`, `playerQueueIndex = startIndex`, `activePlayingItemId = items[startIndex].card.id`, saves `previousView = currentView`, `currentView = 'player'`).
  - Closing `CinemaPlayer` (`Escape` or Back button) restores `setCurrentView(previousView)` instead of hardcoding `'home'`.

### 4.3 `PlaylistsScreen` & `PlaylistDetailView` (`web/src/components/playlists/`)

1. **`PlaylistsScreen.tsx`**:
   - Header with title `"Playlists"` and primary CTA `"+ New Playlist"` (`bg-cta hover:bg-cta-hover text-text-main rounded-xl`).
   - Two distinct sections:
     - **"My Playlists"**: Playlists where `owner_user_id === currentUser.id` (with friendly empty state prompting creation).
     - **"Shared on Server"**: Public playlists owned by other profiles (`is_public && owner_user_id !== currentUser.id`), showing `"by @owner_username"`. Hidden when empty.
   - **Playlist Card**:
     - `2x2` poster mosaic (`aspect-video` or square `bg-panel border border-border-subtle rounded-xl overflow-hidden`): renders 4 quadrant thumbnails when `cover_poster_urls.length === 4`, single cover when `1..3`, or `ListVideo` placeholder icon when `0`.
     - Metadata footer: `truncate` title, public (`Globe`) / private (`Lock`) badge, `font-mono` item count & formatted total runtime (`e.g. 12 items • 4h 35m`), and a subtle `Lock` indicator badge (`"+N locked"`) when `locked_item_count > 0`.
   - **Create / Edit Playlist Modal (`PlaylistFormModal.tsx`)**:
     - Fields: `Name` (required, max 120), `Description` (optional textarea, max 500), `Share with everyone on server` toggle (`is_public`).

2. **`PlaylistDetailView.tsx`**:
   - Back button (`"All Playlists"`), 2x2 mosaic header, title, description, owner chip, visibility badge, `font-mono` stats (`visible_item_count`, `total_duration_seconds`, and `locked_item_count` notice banner if `> 0` explaining that unlocking private libraries reveals hidden items).
   - **Primary Controls**:
     - **`Play All`** (`Play` icon, `bg-cta`): launches `onPlayQueue(detail.items, 0)` (disabled if `items.length === 0`).
     - **`Shuffle`** (`Shuffle` icon, `bg-panel`): creates a Fisher-Yates shuffled copy of `detail.items` and calls `onPlayQueue(shuffled, 0)`.
     - **`Edit`** (`Pencil`) & **`Delete`** (`Trash2`, with confirmation step): rendered only when `detail.summary.is_editable` is `true`.
   - **Ordered Track / Item List**:
     - Each row displays 1-based index (`font-mono`), poster/thumbnail with `playback_progress` bar, title, `card.subtitle`, `font-mono` duration, click/Enter-to-play-from-index (`onPlayQueue(detail.items, index)`), and info button opening `ItemDetailsModal`.
     - When `is_editable` is `true`, each row exposes:
       - **Keyboard/10-foot accessible reorder buttons**: `Move Up` (`ChevronUp`, disabled at `0`) and `Move Down` (`ChevronDown`, disabled at `items.length - 1`) + **HTML5 drag-and-drop** (`draggable`, `onDragStart`, `onDragOver`, `onDrop`), optimistically updating local order and persisting via `api.reorderPlaylistItems(playlist.id, nextIds)` (rolling back on error).
       - **Remove button** (`X` / `Trash2`): calls `api.removePlaylistItem(playlist.id, item.card.id)`.

### 4.4 `AddToPlaylistModal` & Catalog Entry Points (`web/src/components/playlists/AddToPlaylistModal.tsx`)

- **Target Descriptor (`PlaylistAddTarget`)**:
  - `{ kind: 'item'; itemId: number; title: string }` (single movie, episode, or full show)
  - `{ kind: 'season'; showId: number; seasonNumber: number; title: string }`
  - `{ kind: 'folder'; libraryId: string; path: string; recursive: boolean; title: string }`
- **Modal UX**:
  - Fetches `api.getPlaylists(target.kind === 'item' ? target.itemId : undefined)` and filters to editable playlists (`p.is_editable`).
  - Displays each playlist with a membership checkmark (`Check` badge when `contains_queried_item === true`); clicking a playlist calls `api.addPlaylistItems(playlist.id, payload)` (or removes if toggling off a single already-present item, or shows `"Added (N)"` toast/inline feedback for batch targets).
  - Includes a **1-step inline `"Create new playlist & add"`** form (`name` input + `is_public` checkbox + `"Create & Add"` button calling `api.createPlaylist({ name, is_public, items: payload })`).
  - For `{ kind: 'folder' }`, includes an `"Include subfolders"` (`recursive`) checkbox (default `true`).
- **Entry Points**:
  1. **`ItemDetailsModal.tsx`**:
     - Header action row (next to `Browse Folder` and `Play`): **`+ Playlist`** (`ListPlus` icon) button adding the current item (or all episodes when `media_type === 'show'`).
     - Season header / pill bar (when viewing a `show` with seasons): **`Add Season`** (`ListPlus`) button opening `AddToPlaylistModal` with `{ kind: 'season', showId, seasonNumber, title }`.
     - Each `EpisodeCard` row (next to `Play`): compact icon button (`aria-label="Add <episode title> to playlist"`, `ListPlus` icon) opening `AddToPlaylistModal` for that single episode `id`.
  2. **`SpotlightWidget.tsx`**:
     - Hero CTA row (next to `More Info`): **`Add to Playlist`** (`ListPlus`) button for `data.id`.
  3. **`FolderBrowser.tsx`**:
     - Folder header / media section action bar: **`Add Folder to Playlist`** (`ListPlus`) button opening `AddToPlaylistModal` with `{ kind: 'folder', libraryId, path: currentPath, recursive: true, title }`.

### 4.5 `WidgetConfigModal` (`web/src/components/studio/WidgetConfigModal.tsx`)

- Add `"Playlist Items"` (`playlist_items`) to the data source macro selector in `WidgetConfigModal` (alongside `recently_added`, `continue_watching`, `top_rated`, `library_items`, `genre_shelf`).
- When selected, load `api.getPlaylists()` and present a `<select>` dropdown of playlists (highlighting public playlists and warning with an inline note if a private playlist is bound to a shared screen, since other users won't see items from a private playlist).

### 4.6 `CinemaPlayer` & `PlayerControls` Queue Support (`web/src/components/player/`)

1. **`CinemaPlayer.tsx` Props & State Reset**:
   - Extend props:
     ```ts
     interface CinemaPlayerProps {
       itemId: number;
       onClose: () => void;
       returnLabel?: string; // e.g. "Back to Playlist" or "Back to Browse"
       queue?: PlaylistItemView[];
       currentIndex?: number;
       onSelectQueueIndex?: (index: number) => void;
     }
     ```
   - **Per-Item State Reset on `itemId` Change**:
     - When `itemId` changes (advancing or jumping in queue), explicitly reset `isMetadataLoadedRef.current = false`, `hasSeekedResumeRef.current = false`, `resumePositionRef.current = 0`, `currentTime = 0`, `duration = 0`, `activeSubtitleId = null`, `subtitles = []`, `error = null`, and tear down any active HLS instance before opening the new item's playback session.
     - Preserve the existing `useEffect([itemId])` lifecycle: `closePlaybackSession(prevSessionId)` runs in the effect cleanup (flushing the final position of the finished/skipped track), and `createPlaybackSession(nextItemId)` starts immediately for the new track.
     - Resume behavior in queue mode: if an item has `resume_position_seconds > 0` (i.e. `in_progress`, `< 90%`), honor normal resume seek; completed items (`watch_state === 'completed'`, `resume_position_seconds` absent/`0`) start cleanly from `0s`.
   - **Auto-Advance on `<video onEnded>`**:
     - Attach `onEnded` handler to `<video>`: when `queue && currentIndex !== undefined && currentIndex + 1 < queue.length`, first send a final heartbeat / close session at `Math.floor(duration)` so the completed item scrobbles at `100%`, then invoke `onSelectQueueIndex(currentIndex + 1)`. When the final item in the queue ends, pause at end without closing the player prematurely.

2. **`PlayerControls.tsx` Transport & Queue Popover**:
   - When `queue && queue.length > 1` and `currentIndex !== undefined`:
     - Render **`Previous`** (`SkipBack`, `aria-label="Previous track"`, disabled when `currentIndex === 0` unless `currentTime > 5` which seeks to `0`) and **`Next`** (`SkipForward`, `aria-label="Next track"`, disabled when `currentIndex >= queue.length - 1`) buttons in the transport bar, plus keyboard shortcuts (`Shift+P` / `Shift+N` or media track keys).
     - Display track counter chip (`font-mono`: `${currentIndex + 1} / ${queue.length}`) in the top bar next to the item title.
     - Render a **`Queue`** button (`ListVideo` icon, `aria-label="Playback Queue"`) next to the Subtitles button that toggles a scrollable **Queue Popover Drawer** (`bg-panel/95 border border-border-subtle rounded-xl max-h-80 overflow-y-auto`):
       - Highlights the currently playing item (`bg-accent/20 border-accent`), shows item thumbnails, titles, durations, and lets the user click/Enter any row to jump directly via `onSelectQueueIndex(idx)`.
       - **Auto-hide timer coordination**: Pause the 3-second player controls inactivity hide timer whenever `isQueueMenuOpen` is `true` (identical to `isSubtitlesMenuOpen`).

---

## 5. Error Handling & Testing Plan

### 5.1 HTTP Status & Error Code Matrix

| Scenario | HTTP Status | Response Body |
| :--- | :---: | :--- |
| Missing or expired JWT | `401 Unauthorized` | Standard `AuthUser` rejection |
| Empty name, `name > 120` chars, `description > 500` chars | `400 Bad Request` | `{"error": "Playlist name must be between 1 and 120 characters"}` (or description length error) |
| Empty batch add request, non-playable `Season` item, empty show/folder | `400 Bad Request` | `{"error": "No playable media items to add"}` |
| Folder path traversal (`..`, null byte, outside library root) | `400 Bad Request` | `{"error": "Invalid folder path"}` |
| Reorder `item_ids` not an exact permutation of visible playlist items | `400 Bad Request` | `{"error": "item_ids must be an exact permutation of visible playlist items"}` |
| Mutating/deleting a public playlist owned by another user (non-admin) | `403 Forbidden` | `{"error": "Forbidden"}` |
| Adding or removing an item/show/folder in a locked private library | `403 Forbidden` | `{"error": "LIBRARY_LOCKED"}` |
| Accessing or mutating a missing playlist or another user's private playlist | `404 Not Found` | `{"error": "Playlist not found"}` |
| Referencing a non-existent `media_item_id`, `library_id`, or missing folder | `404 Not Found` | `{"error": "Media item not found"}` (or library/folder not found) |

### 5.2 Backend Verification (`cargo test` & `cargo clippy`)

1. **Storage Unit Tests (`crates/kadr-storage/src/repos/playlist_repo.rs` & `media_item_repo.rs`)**:
   - Migration `009_playlists.sql` applies cleanly on fresh and upgraded databases.
   - Playlist CRUD, cascade delete on user deletion and media item deletion, `INSERT OR IGNORE` deduplication, dense `0..N-1` position assignment on add/remove.
   - Privacy partitioning: `visible_item_count`, `locked_item_count`, `cover_poster_urls` (excluding locked items), `contains_queried_item` (returning `Some(false)` when target item is locked), and `reorder_visible_items` preserving locked items at `visible.len()..total-1`.
   - `MediaItemRepository::deduplicate_media_items` preserves playlist items when `del_id` merges into `keep_id` (both when `keep_id` wasn't in the playlist and when both `keep_id` and `del_id` were in the same playlist, re-compacting `position`).
2. **Server E2E Integration Suite (`crates/kadr-server/tests/e2e_playlists_test.rs`)**:
   - Multi-user permissions: User A creates private playlist (`404` to User B), flips `is_public = true` (`200` to User B with User B's own `PlaybackState`, `403` when User B attempts `PATCH`/`DELETE`/reorder/add, `200`/`204` when Admin mutates).
   - 1-step Create & Add (`POST /api/v1/playlists`), Show/Season episode expansion ordering (`S01E01`, `S01E02`, ...), sandboxed Folder batch expansion (non-recursive & recursive, path traversal `400`).
   - Private PIN-locked library isolation: `403 LIBRARY_LOCKED` when adding/removing locked items without token; unlocked token reveals items; subsequent requests without token hide locked items into `locked_item_count` with zero metadata leakage; reordering while locked preserves locked items once unlocked again.
   - Widget AST `QueryMacro::PlaylistItems` hydration on `Home`, pagination via `/api/v1/widgets/{id}/data`, and graceful empty fallback when playlist is deleted or made private.
   - SSE `playlist:updated` event emission verified on create/update/items-changed/delete.
3. **Static Analysis**:
   - `cargo clippy --workspace --all-targets -- -D warnings` passes with zero warnings.

### 5.3 Frontend Verification (`cd web && npm test && npm run build`)

- **`web/src/api/client.test.ts`**: Unit tests for all 8 playlist API client methods, query parameter encoding (`contains_item_id`), and `Authorization` / `X-Kadr-Unlocked` header forwarding.
- **`web/src/components/playlists/PlaylistsScreen.test.tsx`**: Renders "My Playlists" and "Shared on Server", 2x2 mosaic, locked item count badge, create/edit/delete flows, detail view `Play All`, `Shuffle`, Move Up / Move Down reorder, and item removal.
- **`web/src/components/playlists/AddToPlaylistModal.test.tsx`**: Membership checkmarks (`contains_queried_item`), adding single item/season/folder, and 1-step inline "Create new playlist & add".
- **`web/src/components/player/CinemaPlayer.test.tsx` & `PlayerControls.test.tsx`**: Queue playback auto-advance on `<video onEnded>`, per-item playback session close/open lifecycle and state reset across `itemId` transitions, `SkipBack`/`SkipForward` buttons, and Queue popover drawer (including pausing the 3s control auto-hide timer).
- **`web/src/App.test.tsx`**: Top-bar `Playlists` tab visibility for standard and admin users, launching queue playback, and returning to `previousView` (`playlists`, library, or `home`) on player close.
