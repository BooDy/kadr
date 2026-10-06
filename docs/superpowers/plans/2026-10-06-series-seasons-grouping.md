# Series and Seasons Grouping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically group TV series into seasons based on filenames and folder cues during ingest, display clean unified Show cards in library grids, and provide an interactive Seasons and Episodes viewer in the Item Details Modal.

**Architecture:** Extend `FilenameParser` with regexes for `SxxExx`, `#x##`, and folder cue fallbacks. Ingest pipeline marks files as `MediaType::Episode` and creates/links parent `MediaType::Show` records. Storage catalog queries exclude loose episode records from main grids (`item_type != 'episode'`). `WidgetResolver` fetches and batch-hydrates episodes for a show, and the web `ItemDetailsModal` renders season tabs with a responsive episode list and 1-click playback.

**Tech Stack:** Rust (Axum, SQLite/rusqlite, regex, serde), React 19, TypeScript, Tailwind CSS, Lucide icons, Vitest.

## Global Constraints
- Pure-Rust baseline on server preserved; zero external native C dependencies (musl compatible).
- Existing media files and folders on disk remain completely unmodified.
- 100% backward compatibility for existing screen ASTs and `CardViewModel` serde.
- Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `border-border-subtle`).
- 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all interactive inputs, tabs, and buttons.
- Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
- All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.
- Clippy (`cargo clippy --workspace --all-targets -- -D warnings`) must remain clean with 0 warnings.

---

### Task 1: AST Models & Episode Filename/Folder Parser (`crates/kadr-core` & `crates/kadr-ingest`)

**Files:**
- Modify: `crates/kadr-core/src/ast.rs:111-130`
- Modify: `crates/kadr-ingest/src/parser/filename.rs:1-150`
- Test: `crates/kadr-ingest/tests/filename_parser_test.rs`
- Test: `crates/kadr-core/tests/ast_test.rs`

**Interfaces:**
- Consumes: `FilenameParser` in `crates/kadr-ingest/src/parser/filename.rs`
- Produces:
  ```rust
  // In crates/kadr-core/src/ast.rs
  pub struct CardViewModel {
      // ... existing fields ...
      pub season: Option<u32>,
      pub episode: Option<u32>,
  }

  // In crates/kadr-ingest/src/parser/filename.rs
  pub struct ParsedFilename {
      pub title: String,
      pub year: Option<i32>,
      pub resolution: Option<String>,
      pub video_codec: Option<String>,
      pub source: Option<String>,
      pub release_group: Option<String>,
      pub container: String,
      pub is_episode: bool,
      pub series_title: Option<String>,
      pub season: Option<u32>,
      pub episode: Option<u32>,
      pub episode_title: Option<String>,
  }

  impl FilenameParser {
      pub fn parse(&self, filename: &str) -> Option<ParsedFilename>;
      pub fn parse_with_path(&self, path: &std::path::Path) -> Option<ParsedFilename>;
  }
  ```

- [ ] **Step 1: Write the failing tests in `ast_test.rs` and `filename_parser_test.rs`**

```rust
// crates/kadr-core/tests/ast_test.rs
#[test]
fn test_card_view_model_season_episode_serde() {
    let card = CardViewModel {
        id: 101,
        title: "Reunited".to_string(),
        subtitle: Some("S04E01".to_string()),
        poster_url: None,
        backdrop_url: None,
        media_type: "episode".to_string(),
        playback_progress: Some(0.4),
        rating: None,
        release_year: Some(2022),
        badge: None,
        season: Some(4),
        episode: Some(1),
    };
    let json_str = serde_json::to_string(&card).unwrap();
    assert!(json_str.contains("\"season\":4"));
    assert!(json_str.contains("\"episode\":1"));

    let deserialized: CardViewModel = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.season, Some(4));
    assert_eq!(deserialized.episode, Some(1));
}

// crates/kadr-ingest/tests/filename_parser_test.rs
use kadr_ingest::parser::filename::FilenameParser;
use std::path::Path;

#[test]
fn test_parse_standard_sxx_exx_episode() {
    let parser = FilenameParser::new();
    let res = parser.parse("What.We.Do.in.the.Shadows.(2019).-S04E01.-.Reunited.(1080p.HULU.WEB-DL.x265.Ghost).mkv").unwrap();
    assert!(res.is_episode);
    assert_eq!(res.series_title.as_deref(), Some("What We Do in the Shadows"));
    assert_eq!(res.season, Some(4));
    assert_eq!(res.episode, Some(1));
    assert_eq!(res.episode_title.as_deref(), Some("Reunited"));
}

#[test]
fn test_parse_alt_1x02_episode() {
    let parser = FilenameParser::new();
    let res = parser.parse("Hacks.1x02.Prank.Call.720p.mkv").unwrap();
    assert!(res.is_episode);
    assert_eq!(res.series_title.as_deref(), Some("Hacks"));
    assert_eq!(res.season, Some(1));
    assert_eq!(res.episode, Some(2));
    assert_eq!(res.episode_title.as_deref(), Some("Prank Call"));
}

#[test]
fn test_parse_folder_cue_fallback() {
    let parser = FilenameParser::new();
    let path = Path::new("/media/shows/Dexter/Season 01/02 - Crocodile.mkv");
    let res = parser.parse_with_path(path).unwrap();
    assert!(res.is_episode);
    assert_eq!(res.series_title.as_deref(), Some("Dexter"));
    assert_eq!(res.season, Some(1));
    assert_eq!(res.episode, Some(2));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kadr_ingest --test filename_parser_test`
Expected: Compilation failure or missing fields/methods.

- [ ] **Step 3: Implement `CardViewModel` fields and `FilenameParser` regexes**

In `crates/kadr-core/src/ast.rs`:
Add `season: Option<u32>` and `episode: Option<u32>` to `CardViewModel` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

In `crates/kadr-ingest/src/parser/filename.rs`:
Add regexes for `SxxExx` and `#x##`:
- `sxx_exx_regex`: `r"(?i)^(?P<series>.+?)[._\s]+[sS](?P<season>\d{1,2})[eE](?P<episode>\d{1,3})(?:[._\s]+(?P<ep_title>.*?))?(?:[._\s]+(?:(?:19|20)\d{2}|2160p|4k|1080p|720p|bluray|web-dl|webdl|webrip|hdtv|x264|x265|hevc|av1|h\.?264|h\.?265))?.*?\.(?P<ext>mkv|mp4|webm|avi)$"`
- `alt_num_regex`: `r"(?i)^(?P<series>.+?)[._\s]+(?P<season>\d{1,2})x(?P<episode>\d{1,3})(?:[._\s]+(?P<ep_title>.*?))?(?:[._\s]+(?:(?:19|20)\d{2}|2160p|4k|1080p|720p|bluray|web-dl|webdl|webrip|hdtv|x264|x265|hevc|av1|h\.?264|h\.?265))?.*?\.(?P<ext>mkv|mp4|webm|avi)$"`
- In `parse_with_path(path)`: If `parse(filename)` does not identify an episode, inspect `path.parent()` for `Season (\d+)` or `S(\d+)` and `01 - Title.ext` or `E01.ext` in filename, extracting season and series title from grandparent directory.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kadr_core --test ast_test && cargo test -p kadr_ingest --test filename_parser_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-core crates/kadr-ingest
git commit -m "feat(ingest): add TV episode filename and folder cue parsing"
```

---

### Task 2: Ingest Pipeline & Parent Show Record Creation (`crates/kadr-ingest` & `crates/kadr-server`)

**Files:**
- Modify: `crates/kadr-ingest/src/watcher/pipeline.rs:50-135`
- Modify: `crates/kadr-ingest/src/watcher/worker.rs:30-100`
- Test: `crates/kadr-ingest/tests/pipeline_series_test.rs`

**Interfaces:**
- Consumes: `FilenameParser::parse_with_path`, `MediaItem`, `MediaType::Show`, `MediaType::Episode`
- Produces: Ingested `MediaItem` with `item_type = MediaType::Episode` when detected, plus automatically created/verified `MediaType::Show` parent item.

- [ ] **Step 1: Write failing test in `pipeline_series_test.rs`**

```rust
// crates/kadr-ingest/tests/pipeline_series_test.rs
use kadr_core::models::{Library, MediaType};
use kadr_ingest::watcher::pipeline::IngestPipeline;
use std::path::PathBuf;

#[tokio::test]
async fn test_pipeline_ingests_episode_and_populates_series_meta() {
    let pipeline = IngestPipeline::new(false, None);
    let lib = Library {
        id: "shows".to_string(),
        name: "TV Shows".to_string(),
        path: PathBuf::from("/media/shows"),
        media_type: MediaType::Show,
        ..Default::default()
    };
    let file = PathBuf::from("/media/shows/What We Do in the Shadows/Season 04/What We Do in the Shadows - S04E01 - Reunited.mkv");
    // Create dummy file for metadata
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_file = temp_dir.path().join("What We Do in the Shadows - S04E01 - Reunited.mkv");
    std::fs::write(&temp_file, b"dummy").unwrap();

    let res = pipeline.process_file(&temp_file, &lib).await.unwrap();
    assert!(res.is_some());
    let (item, _) = res.unwrap();
    assert_eq!(item.item_type, MediaType::Episode);
    assert_eq!(item.metadata.series_title.as_deref(), Some("What We Do in the Shadows"));
    assert_eq!(item.metadata.season, Some(4));
    assert_eq!(item.metadata.episode, Some(1));
    assert_eq!(item.title, "Reunited");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr_ingest --test pipeline_series_test`
Expected: FAIL (item_type is Show, metadata.series_title is None).

- [ ] **Step 3: Update `IngestPipeline::process_file` and `IngestWorker`**

In `crates/kadr-ingest/src/watcher/pipeline.rs`:
- Use `self.filename_parser.parse_with_path(path)`.
- If `parsed.is_episode` or `library.media_type == MediaType::Show`:
  - `item_type = MediaType::Episode`
  - `meta.series_title = parsed.series_title.clone()`
  - `meta.season = parsed.season`
  - `meta.episode = parsed.episode`
  - `final_title = parsed.episode_title.unwrap_or_else(|| format!("Episode {}", parsed.episode.unwrap_or(1)))`

In `crates/kadr-ingest/src/watcher/worker.rs`:
- When batch-inserting items into `MediaItemRepository`, collect all distinct `series_title`s from episodes.
- For each `series_title`, check if a `MediaType::Show` record exists in that library. If not, insert a parent `MediaItem`:
  - `item_type = MediaType::Show`
  - `library_id = library.id`
  - `title = series_title`
  - `release_year = episode.release_year`
  - `file_path = series_folder_or_empty`
  - `file_name = String::new()`
  - `metadata = default with poster_path from episode/folder`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kadr_ingest --test pipeline_series_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-ingest
git commit -m "feat(ingest): populate episode metadata and ensure parent show records during ingest"
```

---

### Task 3: Storage Queries Filtering & Resolver Hydration (`crates/kadr-storage` & `crates/kadr-server`)

**Files:**
- Modify: `crates/kadr-storage/src/repos/widget_queries.rs:1-120`
- Modify: `crates/kadr-storage/src/repos/media_item_repo.rs:215-245`
- Modify: `crates/kadr-server/src/resolver/resolver.rs:1-50, 520-575`
- Test: `crates/kadr-storage/tests/widget_queries_test.rs`
- Test: `crates/kadr-server/tests/screen_routes_test.rs`

**Interfaces:**
- Consumes: `CardViewModel`, `ItemDetailsPayload`, `MediaItemRepository::find_episodes_by_series`
- Produces: Clean catalog queries filtering out loose episodes, and `ItemDetailsPayload.episodes` with hydrated user playback progress and `season`/`episode` fields.

- [ ] **Step 1: Write failing tests in `widget_queries_test.rs` and `screen_routes_test.rs`**

```rust
// crates/kadr-storage/tests/widget_queries_test.rs
#[tokio::test]
async fn test_catalog_queries_exclude_loose_episodes() {
    let pool = create_in_memory_pool().unwrap();
    initialize_database(&pool).await.unwrap();
    let repo = MediaItemRepository::new(pool);

    // Insert 1 Movie, 1 Show, and 3 Episodes
    // Assert find_by_library_paginated returns only the Show, not the 3 Episodes
}

// crates/kadr-server/tests/screen_routes_test.rs
#[tokio::test]
async fn test_show_details_returns_hydrated_episodes_with_season() {
    let ctx = setup_test_app().await;
    // Request GET /api/v1/items/{show_id}/details
    // Assert json["episodes"] is array with season=Some(x), episode=Some(y)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kadr_storage --test widget_queries_test`
Expected: FAIL.

- [ ] **Step 3: Implement query filters and resolver hydration**

In `crates/kadr-storage/src/repos/widget_queries.rs` and `media_item_repo.rs`:
- Add `m.item_type != 'episode'` to `build_filter_clauses` or default `WHERE` conditions in `find_by_library_paginated`, `find_recently_added_paginated`, `find_top_rated_paginated`, and `find_by_genre_paginated`.

In `crates/kadr-server/src/resolver/resolver.rs`:
- In `to_card_view_model`:
  - Map `season = item.metadata.season`
  - Map `episode = item.metadata.episode`
  - If `season` and `episode` are present:
    - Set `subtitle = Some(format!("S{:02}E{:02}", s, e))`
- In `resolve_item_details_with_unlocked`:
  - When `item.item_type == MediaType::Show`:
    - Call `self.media_repo.find_episodes_by_series(&item.title).await?`
    - Batch hydrate with `self.hydrate_items_to_cards(episode_items, user_id).await`
    - Assign to `episodes: Some(ep_cards)`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kadr_storage --test widget_queries_test && cargo test -p kadr_server --test screen_routes_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/kadr-storage crates/kadr-server
git commit -m "feat(server): exclude loose episodes from catalog grids and hydrate show episodes with seasons"
```

---

### Task 4: Web UI Season Tabs & Episode List in Item Details Modal (`web/`)

**Files:**
- Modify: `web/src/types/index.ts:1-50`
- Modify: `web/src/components/browse/ItemDetailsModal.tsx:1-500`
- Test: `web/src/components/browse/ItemDetailsModal.test.tsx`

**Interfaces:**
- Consumes: `ItemDetailsPayload.episodes`, `CardViewModel.season`, `CardViewModel.episode`, `onPlayItem(id: number)`
- Produces: Season tabs selector, responsive episode cards with thumbnails, durations, progress bars, and 1-click play.

- [ ] **Step 1: Write failing component tests in `ItemDetailsModal.test.tsx`**

```tsx
// web/src/components/browse/ItemDetailsModal.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { ItemDetailsModal } from './ItemDetailsModal';
import { describe, it, expect, vi } from 'vitest';
import type { ItemDetailsPayload } from '../../types';

describe('ItemDetailsModal Seasons & Episodes', () => {
  const mockShowDetails: ItemDetailsPayload = {
    card: {
      id: 10,
      title: 'What We Do in the Shadows',
      media_type: 'show',
    },
    overview: 'Vampire roommates in Staten Island.',
    genres: ['Comedy'],
    stream_url: '/api/v1/stream/10',
    episodes: [
      {
        id: 101,
        title: 'Reunited',
        subtitle: 'S04E01',
        media_type: 'episode',
        season: 4,
        episode: 1,
      },
      {
        id: 102,
        title: 'The Lamp',
        subtitle: 'S04E02',
        media_type: 'episode',
        season: 4,
        episode: 2,
      },
      {
        id: 201,
        title: 'Mall Stories',
        subtitle: 'S05E01',
        media_type: 'episode',
        season: 5,
        episode: 1,
      },
    ],
  };

  it('renders season selector tabs and filters episodes by selected season', async () => {
    const onPlayItem = vi.fn();
    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={onPlayItem}
      />
    );

    // Verify season tabs appear
    expect(screen.getByRole('button', { name: /Season 4/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Season 5/i })).toBeInTheDocument();

    // Default season 4 episodes visible
    expect(screen.getByText('Reunited')).toBeInTheDocument();
    expect(screen.getByText('The Lamp')).toBeInTheDocument();
    expect(screen.queryByText('Mall Stories')).not.toBeInTheDocument();

    // Click Season 5
    fireEvent.click(screen.getByRole('button', { name: /Season 5/i }));
    expect(screen.getByText('Mall Stories')).toBeInTheDocument();
    expect(screen.queryByText('Reunited')).not.toBeInTheDocument();
  });

  it('clicking an episode play button triggers onPlayItem with episode id', async () => {
    const onPlayItem = vi.fn();
    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={onPlayItem}
      />
    );

    const playBtn = screen.getByRole('button', { name: /Play Reunited/i });
    fireEvent.click(playBtn);
    expect(onPlayItem).toHaveBeenCalledWith(101);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web && npm test -- --run ItemDetailsModal.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement Season Tabs and Episode List in `ItemDetailsModal.tsx`**

In `web/src/types/index.ts`:
- Add `season?: number; episode?: number;` to `CardViewModel`.

In `web/src/components/browse/ItemDetailsModal.tsx`:
- Group `details.episodes` by `card.season ?? 1`.
- Maintain `selectedSeason: number` state.
- Render Season pill tabs with `theme.md` tokens and 10-foot TV UI focus rings (`focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`).
- Render episode cards with:
  - Thumbnail with play hover overlay and duration badge.
  - Episode number badge (`E01`).
  - Episode title and duration.
  - Progress bar (`card.playback_progress`).
  - Play button triggering `onPlayItem(ep.id)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web && npm test -- --run ItemDetailsModal.test.tsx`
Expected: PASS.

- [ ] **Step 5: Run full workspace test suite, clippy, and build**

Run:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd web && npm test -- --run
npm run build
```
Expected: All tests pass, 0 clippy warnings, production bundle builds cleanly.

- [ ] **Step 6: Commit**

```bash
git add web/
git commit -m "feat(web): add season tabs selector and episode list with 1-click play in ItemDetailsModal"
```
