# Design Specification: Kadr Milestone 3 — Declarative Widget AST & Home Layout Engine

**Document Status:** Approved  
**Date:** 2026-10-01  
**Target Milestone:** Milestone 3 (Declarative Widget AST & Home Layout Engine)  
**Author:** Pair Programming Agent & User  

---

## 1. System Overview & Scope

Kadr (كادر) is a lightweight, zero-bloat, direct-play self-hosted media server written in Rust with an embedded SQLite database. This design specification covers **Milestone 3**: the **Declarative Widget AST & Home Layout Engine**.

Milestone 3 establishes a server-driven UI architecture where client applications (web, TV, mobile) render dynamic, configurable screens based on a strongly-typed Abstract Syntax Tree (AST) returned by the server. Clients do not hardcode media query waterfalls or layout hierarchies; instead, the server delivers pre-hydrated widget trees (Hero banners, Continue Watching shelves, Recently Added carousels, Top Rated carousels, Genre clusters, and Paginated grids) alongside pagination cursors.

### 1.1 Non-Goals for Milestone 3
- Subtitle fetching/extraction (deferred to a subsequent milestone).
- Full-Text Search (FTS5) library search queries (deferred to a dedicated catalog search milestone).
- Transcoding / child FFmpeg pipelines (retaining pure direct-play HTTP 206 architecture).

### 1.2 Performance & System Constraints
- **RSS Memory Budget:** $\le 30\text{ MB}$ under idle and active layout query workloads.
- **Latency / Response Budget:** $< 50\text{ ms}$ for fully hydrated screen layout responses against SQLite with WAL mode.
- **Zero-External Runtime Dependencies:** Pure Rust compilable against `x86_64-unknown-linux-musl`.
- **Database Safety:** All operations use pooled SQLite with `PRAGMA synchronous = NORMAL`, `PRAGMA foreign_keys = ON`, and `PRAGMA busy_timeout = 5000`.

---

## 2. Architecture & Crate Boundaries

Milestone 3 extends the existing Cargo workspace:

```
crates/
├── kadr-core/
│   └── src/
│       ├── models.rs            # Core entities (User, MediaItem, Library, etc.)
│       └── ast.rs               # [NEW] Widget AST, ScreenLayout, CardViewModel, QueryMacro
├── kadr-storage/
│   └── src/
│       ├── migrations/
│       │   └── 003_widget_query_indexes.sql # [NEW] Rating and filter indexes
│       └── repos/
│           └── media_repo.rs    # [EXTENDED] Queries for recently added, top rated, genres, spotlight
└── kadr-server/
    └── src/
        ├── layout/              # [NEW] LayoutRegistry, default screens, TOML layout overrides
        ├── resolver/            # [NEW] Concurrent WidgetResolver, CardViewModel normalization
        ├── api/
        │   ├── screen_routes.rs # [NEW] /api/v1/screens, /api/v1/screens/:id
        │   ├── widget_routes.rs # [NEW] /api/v1/widgets/:id/data
        │   ├── item_routes.rs   # [NEW] /api/v1/items/:id/details
        │   └── artwork_routes.rs# [NEW] /api/v1/artwork/:id/poster, /backdrop
        └── main.rs              # [EXTENDED] Mount layout routes and register LayoutRegistry
```

---

## 3. Data Models & AST Schema (`kadr-core`)

All AST structures implement `Serialize`, `Deserialize`, `Clone`, and `Debug`.

### 3.1 Identifiers & Normalized View Models

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenId {
    Home,
    Movies,
    Shows,
    Custom(String),
}

/// Normalized card model sent to frontends to render media thumbnails
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardViewModel {
    pub id: i64,
    pub title: String,
    pub subtitle: Option<String>,         // e.g. "S1:E4 - The Beginning" or "2024 • 4K"
    pub poster_url: Option<String>,       // e.g. "/api/v1/artwork/{id}/poster"
    pub backdrop_url: Option<String>,     // e.g. "/api/v1/artwork/{id}/backdrop"
    pub media_type: String,               // "movie", "episode", "show"
    pub playback_progress: Option<f32>,   // 0.0 to 1.0 (for progress bars)
    pub rating: Option<f32>,              // e.g. 8.4
    pub release_year: Option<u32>,        // e.g. 2023
    pub badge: Option<String>,            // e.g. "4K HDR", "NEW", "RESUME"
}
```

### 3.2 Query Macros & Bindings

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMacro {
    ContinueWatching,
    RecentlyAdded,
    TopRated,
    LibraryItems { library_id: String },
    GenreShelf { genre: String },
    SpotlightItem { item_id: Option<i64> }, // None picks the latest top-rated featured item
    ItemDetails { item_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetQueryBinding {
    pub macro_type: QueryMacro,
    #[serde(default = "default_widget_limit")]
    pub limit: u32,
    #[serde(default)]
    pub sort: Option<String>,             // e.g. "created_at:desc", "rating:desc"
}

fn default_widget_limit() -> u32 {
    20
}
```

### 3.3 Declarative Widget AST Nodes

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetNode {
    /// Hero Banner / Featured Spotlight at the top of a screen
    HeroBanner {
        id: String,
        binding: WidgetQueryBinding,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<CardViewModel>,
    },
    /// Horizontally scrolling carousel row of cards
    Carousel {
        id: String,
        title: String,
        binding: WidgetQueryBinding,
        #[serde(skip_serializing_if = "Option::is_none")]
        items: Option<Vec<CardViewModel>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
    },
    /// Paginated vertical grid of cards for browsing
    Grid {
        id: String,
        title: String,
        binding: WidgetQueryBinding,
        columns: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        items: Option<Vec<CardViewModel>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_count: Option<u64>,
    },
    /// Comprehensive details view for a selected item (episodes, cast, specs)
    ItemDetails {
        id: String,
        item_id: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<ItemDetailsPayload>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDetailsPayload {
    pub card: CardViewModel,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub duration_seconds: Option<u64>,
    pub technical: Option<crate::models::TechnicalInfo>,
    pub stream_url: String,               // "/api/v1/stream/{id}"
    pub resume_position_seconds: Option<u64>,
    pub episodes: Option<Vec<CardViewModel>>, // For TV shows / seasons
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenLayout {
    pub id: ScreenId,
    pub title: String,
    pub widgets: Vec<WidgetNode>,
}
```

---

## 4. Storage Engine & Query Repositories (`kadr-storage`)

### 4.1 Schema Migration `003_widget_query_indexes.sql`
```sql
-- Fast sorting and filtering by user/critic rating stored in JSON metadata
CREATE INDEX IF NOT EXISTS idx_media_rating 
ON media_items(CAST(json_extract(metadata, '$.rating') AS REAL) DESC);
```

### 4.2 Query Methods on `MediaItemRepository`
```rust
impl MediaItemRepository {
    /// Recently added items across all libraries (or scoped to a specific library)
    pub async fn find_recently_added(
        &self, 
        library_id: Option<&str>, 
        limit: u32, 
        offset: u32
    ) -> Result<Vec<MediaItem>, StorageError>;

    /// Top-rated items for spotlight recommendations or curated rows
    pub async fn find_top_rated(
        &self, 
        limit: u32, 
        offset: u32
    ) -> Result<Vec<MediaItem>, StorageError>;

    /// Filter items by genre using SQLite json_each()
    pub async fn find_by_genre(
        &self, 
        genre: &str, 
        limit: u32, 
        offset: u32
    ) -> Result<Vec<MediaItem>, StorageError>;

    /// Paginated browsing of library items with custom sort
    pub async fn find_by_library_paginated(
        &self, 
        library_id: &str, 
        limit: u32, 
        offset: u32, 
        sort_by: Option<&str>
    ) -> Result<(Vec<MediaItem>, u64), StorageError>;

    /// Finds candidate item for Hero Spotlight (recent high-rated item with backdrop)
    pub async fn find_spotlight_candidate(&self) -> Result<Option<MediaItem>, StorageError>;

    /// Finds all episodes belonging to a specific series / show title
    pub async fn find_episodes_by_series(&self, series_title: &str) -> Result<Vec<MediaItem>, StorageError>;
}
```

---

## 5. Layout Registry & Hydration Resolver (`kadr-server`)

### 5.1 In-Memory `LayoutRegistry`
- Holds default built-in layouts:
  - **`Home`**:
    1. `HeroBanner`: `SpotlightItem { item_id: None }`
    2. `Carousel` (`continue_watching`): User's in-progress watch sessions. Omitted or empty if no active sessions.
    3. `Carousel` (`recently_added`): Latest additions across libraries.
    4. `Carousel` (`top_rated`): Highest-rated media items.
    5. `Carousel` (`genre_shelf`): Curated genre shelves (Action, Drama, Sci-Fi).
  - **`Movies`**:
    1. `Grid`: 6 columns, paginated by 30 movies, sorted by `title:asc` or `added_at:desc`.
  - **`Shows`**:
    1. `Grid`: 6 columns, paginated by 30 series.
- Can optionally load TOML overrides from `config/screens/*.toml` if provided.

### 5.2 Concurrent `WidgetResolver`
- Implements `WidgetResolver::resolve_screen(screen: ScreenLayout, user_id: &str) -> ScreenLayout`.
- Uses `futures::future::join_all` to execute each widget's database query asynchronously across the deadpool connection pool in parallel.
- Normalizes `MediaItem` rows into `CardViewModel`:
  - Cross-references user's watch progress via `PlaybackRepository::get_watch_state(user_id, item_id)`.
  - Attaches poster and backdrop URL paths (`/api/v1/artwork/{id}/poster`).
  - Sets visual badges (`"RESUME"`, `"4K"`, `"NEW"` if added within 14 days).

---

## 6. REST API Endpoints & Contract (`kadr-server`)

All endpoints are mounted under `/api/v1`.

| Method | Path | Auth | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/screens` | `AuthUser` | Returns list of available screens (`home`, `movies`, `shows`). |
| `GET` | `/api/v1/screens/:screen_id` | `AuthUser` | Returns fully hydrated `ScreenLayout` AST (or unhydrated if `?unhydrated=true`). |
| `GET` | `/api/v1/widgets/:widget_id/data` | `AuthUser` | Returns paginated items for infinite scrolling/paging (`items`, `next_cursor`, `total_count`). |
| `GET` | `/api/v1/items/:item_id/details` | `AuthUser` | Returns `ItemDetailsPayload` (overview, cast, technical specs, stream URL, episodes). |
| `GET` | `/api/v1/artwork/:item_id/poster` | Public / Token | Streams local poster image file with `Cache-Control: public, max-age=86400`. |
| `GET` | `/api/v1/artwork/:item_id/backdrop` | Public / Token | Streams local backdrop image file with `Cache-Control: public, max-age=86400`. |

---

## 7. Testing & Verification Strategy

1. **Unit Tests:**
   - AST serialization/deserialization verification in `kadr-core`.
   - SQLite queries and index performance in `kadr-storage`.
   - CardViewModel conversion and badge generation in `kadr-server`.
2. **Integration Tests:**
   - Screen resolution endpoint testing (`GET /api/v1/screens/home`) returning hydrated widgets.
   - Continue watching row isolation between different users.
   - Cursor-based widget pagination (`GET /api/v1/widgets/recently_added/data?offset=20&limit=20`).
   - Artwork streaming with correct MIME headers and 404 on missing images.
3. **End-to-End Test (`tests/e2e_widget_ast_test.rs`):**
   - Full flow: Ingest sample media items $\to$ Authenticate user $\to$ Fetch `/api/v1/screens/home` $\to$ Inspect HeroBanner and Carousels $\to$ Post playback progress $\to$ Verify ContinueWatching shelf appears $\to$ Inspect item details and stream artwork.
