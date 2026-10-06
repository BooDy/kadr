# Series and Seasons Grouping Design Specification

**Feature:** Series & Seasons Grouping by Name  
**Status:** Approved  
**Date:** 2026-10-06  
**Target:** Ingest Pipeline, Media Storage, Layout Resolver, Web UI  

---

## 1. Problem Statement & Goals

Currently, when TV series are ingested into Kadr, individual video files (e.g. `What.We.Do.in.the.Shadows.S04E01.mkv`, `Hacks.S05E02.mkv`) are treated as separate standalone media items in the database. When viewing a TV show library or catalog grid:
1. Every individual episode is rendered as a standalone card, cluttering library screens and carousels with dozens or hundreds of items.
2. There is no hierarchical navigation from a TV show down to its seasons and constituent episodes.
3. Users cannot easily browse episodes by season or see season-level progress.

### Goals
* **Smart Filename & Folder Parsing:** Automatically parse series name, season number, episode number, and episode title from standard scene conventions (`S01E02`, `1x04`, `Season X Episode Y`) and folder structures (e.g. `Season 04/`).
* **Clean Catalog Grids:** Ensure library grids and catalog queries display a single parent `MediaType::Show` card per series rather than loose episode cards.
* **Seasons & Episodes UI Navigation:** When clicking a Show card, the enhanced Item Details Modal presents an interactive Season selector (e.g. `Season 1`, `Season 2`) and a responsive episode list with thumbnails, durations, playback progress, and 1-click play.

---

## 2. Architecture & Ingest Pipeline

### 2.1 Filename & Folder Parsing (`crates/kadr-ingest/src/parser/filename.rs`)
Expand `FilenameParser` with dedicated episode pattern detection:
1. **Standard `SxxExx` Regex:**
   - Detects `(?i)^(?P<series>.+?)[._\s]+[sS](?P<season>\d{1,2})[eE](?P<episode>\d{1,3})(?:[._\s]+(?P<title>.*?))?(?:[._\s]*(?:(?:19|20)\d{2}|2160p|4k|1080p|720p|bluray|web-dl|x264|x265|hevc|av1|h\.?26[45]))?.*\.(?P<ext>mkv|mp4|webm|avi)$`
   - Extracts:
     - `series_title`: normalized series name.
     - `season_number`: `u32` integer.
     - `episode_number`: `u32` integer.
     - `episode_title`: optional episode name (e.g., `Reunited`).
2. **Standard `#x##` Regex:**
   - Detects `(?i)^(?P<series>.+?)[._\s]+(?P<season>\d{1,2})x(?P<episode>\d{1,3})(?:[._\s]+(?P<title>.*?))?(?:[._\s]*(?:(?:19|20)\d{2}|2160p|4k|1080p|720p|bluray|web-dl|x264|x265|hevc|av1|h\.?26[45]))?.*\.(?P<ext>mkv|mp4|webm|avi)$`
3. **Folder Cues Fallback:**
   - If filename only contains an episode indicator (e.g. `01 - Pilot.mkv` or `Episode 2.mkv`):
     - Parent directory is checked for `(?i)^Season[._\s]*(?P<season>\d+)$` or `(?i)^S(?P<season>\d+)$`.
     - Grandparent directory is used for `series_title`.
4. **Title Normalization:**
   - Strips trailing release tags, release groups, and converts dots/underscores to spaces (e.g. `"What.We.Do.in.the.Shadows (2019)"` -> `"What We Do in the Shadows"`).

### 2.2 Ingest Pipeline Integration (`crates/kadr-ingest/src/watcher/pipeline.rs` & `worker.rs`)
* If a file parses as an episode (or belongs to a library with `media_type: MediaType::Show`):
  - Ingests the media file as `item_type = MediaType::Episode`.
  - Sets `metadata.series_title`, `metadata.season`, `metadata.episode`.
  - Sets `title` to episode title if detected, or fallback `S{season}E{episode}`.
* Ingest Worker maintains the parent `MediaType::Show` record:
  - Checks if a `MediaType::Show` item exists with `library_id` and matching `title = series_title`.
  - If missing, creates the parent `MediaType::Show` item inheriting the series folder artwork (or first episode's artwork) and year.

---

## 3. Storage & AST Models

### 3.1 `CardViewModel` Extension (`crates/kadr-core/src/ast.rs`)
Add optional fields for backward compatibility:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardViewModel {
    pub id: i64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback_progress: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_year: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
}
```

### 3.2 Storage Queries (`crates/kadr-storage/src/repos/widget_queries.rs`)
* Catalog queries (`find_by_library_paginated`, `find_recently_added_paginated`, `find_top_rated_paginated`, `find_by_genre_paginated`):
  - Push down condition: `m.item_type != 'episode'` (or `m.item_type IN ('movie', 'show', 'anime')`).
  - Loose episodes are excluded from main library grids and shelves.
* Series episode query (`find_episodes_by_series`):
  - Fetches all episodes where `metadata.series_title = ?` or `item_type = 'episode' AND title LIKE ?%`.
  - Orders by `season ASC`, `episode ASC`, `id ASC`.

---

## 4. Resolver Logic (`crates/kadr-server/src/resolver/resolver.rs`)

* In `to_card_view_model`:
  - When `item.item_type == MediaType::Episode`, populates `season: item.metadata.season`, `episode: item.metadata.episode`.
  - Formats `subtitle` as `format!("S{:02}E{:02}", s, e)` if both are present.
* In `resolve_item_details_with_unlocked`:
  - When requested item is `MediaType::Show`:
    - Queries all episodes via `self.media_repo.find_episodes_by_series(&item.title)`.
    - Batch-hydrates episodes with playback states for the requesting user (`playback_progress`, `watch_state`).
    - Returns hydrated episode cards in `ItemDetailsPayload.episodes`.

---

## 5. Web UI (`web/src/components/browse/ItemDetailsModal.tsx`)

### 5.1 Season Grouping
* Groups `details.episodes` into a dictionary: `Record<number, CardViewModel[]>`.
* Keys are sorted numerically: Season 1, Season 2, etc. Season 0 is labeled `"Specials"`.
* Default active season is the first available season, or the season containing an in-progress episode.

### 5.2 Season Selector UI
* Rendered above the episode list:
  - Horizontal scrollable pill buttons: `Season 1`, `Season 2`, etc.
  - Active button: `bg-accent text-white font-semibold shadow-sm`.
  - Inactive button: `bg-canvas/60 text-muted hover:text-text-main border border-border-subtle`.
  - 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`.

### 5.3 Episode List Component
* For the selected season, renders a responsive episode list:
  - Episode thumbnail image with duration overlay and fallback `Film` icon.
  - Episode index chip (`E01`, `E02`).
  - Episode title and formatted duration (`34m`, `1h 12m`).
  - Inline progress bar if partially watched.
  - 1-Click play button invoking `onPlayItem(episode.id)` directly into `CinemaPlayer`.

---

## 6. Testing & Quality Strategy

1. **Ingest & Parser Tests (`kadr-ingest`):**
   - Unit tests for `FilenameParser` with scene patterns (`S01E02`, `1x04`), folder cue fallbacks, and multi-episode formats.
   - Pipeline integration tests verifying episode ingest and parent Show record creation.
2. **Storage Tests (`kadr-storage`):**
   - Verify `find_by_library_paginated` excludes `MediaType::Episode` records.
   - Verify `find_episodes_by_series` returns episodes sorted by `season` and `episode`.
3. **Server Route Tests (`kadr-server`):**
   - Verify `GET /api/v1/items/{show_id}/details` returns `episodes` with populated `season` and `episode`.
4. **Web Client Tests (`web`):**
   - Unit tests in `ItemDetailsModal.test.tsx` verifying:
     - Grouping episodes into seasons.
     - Switching between season tabs updates the visible episode list.
     - Clicking play on an episode card triggers `onPlayItem` with the episode's ID.
5. **Quality Gates:**
   - Pure-Rust baseline preserved (musl compatible).
   - Zero test failures across `cargo test --workspace` and `npm test -- --run`.
   - `cargo clippy --workspace --all-targets -- -D warnings` clean with zero warnings.
