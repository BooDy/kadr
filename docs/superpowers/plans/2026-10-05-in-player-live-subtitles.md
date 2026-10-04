# In-Player Live Subtitle Search & Download Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable users to search OpenSubtitles and download subtitles directly inside the video player HUD during playback without interrupting video, prompting users to enable newly downloaded tracks.

**Architecture:**
* `PlayerControls.tsx`: Expand subtitle popover into a multi-view drawer (`list` vs `search`). When no subtitles are present, show a "Search OpenSubtitles Online" CTA; when subtitles are present, show existing tracks plus "+ Search & Download Online". Provide language filter, search results list, and 1-click download.
* `CinemaPlayer.tsx`: Implement `onSearchSubtitles` and `onDownloadSubtitle` handlers using existing `api.searchSubtitles` and `api.downloadSubtitle` client methods, dynamically re-fetching tracks and injecting `<track>` elements into the video DOM. Suspend HUD auto-hide while the subtitle menu/search is open.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4, Lucide React, Vitest.

## Global Constraints
* Design tokens strictly match `theme.md` (`bg-canvas`, `bg-panel`, `bg-panel-hover`, `text-accent`, `bg-accent`, `ring-highlight`, `bg-cta`, `text-text-main`, `text-muted`, `border-border-subtle`).
* 10-foot TV UI focus ring: `focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none` across all buttons and inputs.
* Production bundle in `web/dist` must compile with `npm run build` with zero TypeScript or build errors.
* All workspace Rust tests (`cargo test --workspace`) and frontend tests (`npm test -- --run`) must remain 100% passing.

---

### Task 1: PlayerControls Subtitles Popover Multi-View & Live Search Drawer

**Files:**
* Modify: `web/src/components/player/PlayerControls.tsx:1-279`
* Modify: `web/src/components/player/PlayerControls.test.tsx`

**Interfaces:**
* Produces:
  * Expanded `PlayerControlsProps`:
    ```typescript
    export interface PlayerControlsProps {
      // ... existing props ...
      onSearchSubtitles?: (language: string) => Promise<SubtitleSearchResult[]>;
      onDownloadSubtitle?: (result: SubtitleSearchResult) => Promise<void>;
      recentlyDownloadedId?: number | null;
      onSubtitlesMenuToggle?: (isOpen: boolean) => void;
    }
    ```
  * Multi-view popover in `PlayerControls`:
    * List View: Shows tracks, empty state notice, "+ Search & Download Online" button, and downloaded prompt badge.
    * Search View: Back button, language input, search button, results list, downloading indicator, error banner.

- [ ] **Step 1: Write failing tests in `PlayerControls.test.tsx`**

Test scenarios:
1. When `subtitles` is empty, opening subtitle menu displays "Search OpenSubtitles Online" button.
2. Clicking "Search OpenSubtitles Online" switches popover to search view.
3. Submitting search calls `onSearchSubtitles` and displays results.
4. Clicking download on a match calls `onDownloadSubtitle` and shows loading spinner.
5. Displays prompt banner when `recentlyDownloadedId` is provided.

```typescript
it('renders search online button when no subtitles and switches to search view', async () => {
  const onSearch = vi.fn().mockResolvedValue([
    { id: 'sub-1', language: 'en', format: 'srt', release_name: 'Interstellar.1080p' }
  ]);
  render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
  fireEvent.click(screen.getByRole('button', { name: /Subtitles/i }));
  expect(screen.getByRole('button', { name: /Search OpenSubtitles Online/i })).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
  expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /Search/i }));
  expect(onSearch).toHaveBeenCalledWith('en');
  expect(await screen.findByText(/Interstellar.1080p/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run PlayerControls.test.tsx`
Expected: FAIL (missing search elements).

- [ ] **Step 3: Implement multi-view popover & search drawer in `PlayerControls.tsx`**

1. Add state: `popoverView: 'list' | 'search'`, `searchLang: 'en'`, `searchResults: SubtitleSearchResult[]`, `isSearching`, `searchError`, `downloadingId`, `downloadError`.
2. Implement Search View with `ArrowLeft` back button, language text input, and results with `Download` button.
3. Implement List View with empty state CTA, bottom CTA, and downloaded track prompt.
4. Notify parent when subtitles popover opens/closes via `onSubtitlesMenuToggle?.(isOpen)`.

- [ ] **Step 4: Run tests and verify passing**

Run: `cd web && npm test -- --run PlayerControls.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/player/PlayerControls.tsx web/src/components/player/PlayerControls.test.tsx
git commit -m "feat(web): add live online subtitle search and download drawer to PlayerControls"
```

---

### Task 2: CinemaPlayer Subtitle Integration & Live DOM Track Injection

**Files:**
* Modify: `web/src/components/player/CinemaPlayer.tsx:50-250`
* Modify: `web/src/components/player/CinemaPlayer.test.tsx`

**Interfaces:**
* Produces:
  * In `CinemaPlayer.tsx`:
    * `handleSearchSubtitles`: Calls `api.searchSubtitles(itemId, undefined, language)`.
    * `handleDownloadSubtitle`: Calls `api.downloadSubtitle(itemId, result)`, re-fetches `api.getSubtitles(itemId)`, updates `subtitles`, and sets `recentlyDownloadedId`.
    * Suspend HUD auto-hide timeout when subtitles menu is open.
    * Injects new `<track>` tags dynamically as `subtitles` array updates.

- [ ] **Step 1: Write failing tests in `CinemaPlayer.test.tsx`**

Add test verifying:
1. `CinemaPlayer` provides subtitle search and download handlers to `PlayerControls`.
2. When a subtitle is downloaded, `api.downloadSubtitle` and `api.getSubtitles` are invoked, and the new track is passed to controls.

- [ ] **Step 2: Run test to verify failure**

Run: `cd web && npm test -- --run CinemaPlayer.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement handlers and track injection in `CinemaPlayer.tsx`**

1. Add `recentlyDownloadedId` state and `isSubtitlesMenuOpen` state.
2. In `showControls()`, do not set auto-hide timer if `isSubtitlesMenuOpen` is true.
3. Pass `onSearchSubtitles`, `onDownloadSubtitle`, `recentlyDownloadedId`, and `onSubtitlesMenuToggle={(isOpen) => setIsSubtitlesMenuOpen(isOpen)}` to `<PlayerControls />`.
4. Render `<track>` elements dynamically for all tracks in `subtitles`.

- [ ] **Step 4: Run tests and verify clean build**

Run: `cd web && npm test -- --run CinemaPlayer.test.tsx && npm test -- --run`
Run: `npm run build` in `web/`
Run: `cargo test --workspace` from repo root
Expected: 100% PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/player/CinemaPlayer.tsx web/src/components/player/CinemaPlayer.test.tsx
git commit -m "feat(web): connect in-player live subtitle search, download, and track injection in CinemaPlayer"
```
