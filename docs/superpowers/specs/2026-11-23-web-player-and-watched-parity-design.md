# Web Player & Watch-State Parity Design (Mark Watched, Multi-Audio Tracks, HLS & Storyboards)

**Date:** 2026-11-23  
**Status:** Approved

---

## 1. Overview & Goals

Surface all recently landed Kadr server playback, audio, transcoding, and trickplay capabilities (`a83716b`, `1a39396`, `84b7bd2`, and storyboard routes) across the React Web Client (`web/`) and official documentation (`README.md`, `docs/CLIENT_GUIDE.md`), bringing `kadr-web` to full feature parity with `kadr-server` and `kadr-tizenos`.

### 1.1 Primary Goals
- **Universal HLS Fallback & Multi-Audio Track Switching (`hls.js` + `GET /api/v1/items/{id}/audio-tracks`):**
  - Add `"hls.js": "^1.7.3"` (dynamically imported via `import('hls.js')` so non-player views incur zero bundle overhead, with native Safari HLS fallback).
  - Default to zero-copy Direct Play (`/api/v1/stream/{id}`) when container + codecs are HTML5-native and `audioTrackIndex === 0`, and switch seamlessly to server-muxed HLS (`/api/v1/media/{id}/hls/master.m3u8?audio_track=N&start_time=T`) whenever:
    1. A non-zero audio track (`audio_index > 0`) is selected in the new in-player **Audio Tracks** menu,
    2. Container/codecs require remuxing or transcoding (`mkv`, `avi`, `ts`/`m2ts`, `dts`, `eac3`/`ac3`/`truehd`, `hevc` when unsupported),
    3. Audio-only media (`music`, `audiobook`, or missing video stream) needs HLS (`video_codec=none&audio_codec=aac` exercising `VideoAction::None`), OR
    4. Direct Play `<video>` fires an `error` event (e.g. `409 PLAYBACK_REQUIRES_TRANSCODE` or browser decode failure).
  - Track `hlsTimeOffset` (`start_time`) so playback timeline, scrobble heartbeats, and WebVTT subtitle cues stay 100% synchronized across server-side `-ss` seeks and track switches, and cleanly terminate superseded sessions via `DELETE /api/v1/media/{id}/hls/sessions/{session_id}`.
- **Storyboard Seek-Bar Hover Previews (`GET /api/v1/media/{id}/storyboard` & `/frame`):**
  - Poll `/api/v1/media/{id}/storyboard` across `202 Accepted` (`Retry-After`, max 12 retries), render 10×10 sprite-sheet tiles via authenticated CSS `background-image` + `background-position` on seek-bar hover/drag, and fall back smoothly to debounced single-frame `GET /api/v1/media/{id}/frame?t=<snapped_2s>&w=320` while generating.
- **Single & Batch Mark Watched / Unwatched (`PUT /api/v1/playback/states/{id}` & `PUT /api/v1/playback/states`):**
  - Provide instant optimistic watched/unwatched toggles on poster cards (`CarouselWidget`, `GridWidget`) and inside `ItemDetailsModal` (primary item button, per-episode row toggle, **`Mark Season Watched / Unwatched`**, and **`Mark Series Watched / Unwatched`** chunked in `<= 500`-item batches).
- **Complete Documentation Parity (`README.md` & `docs/CLIENT_GUIDE.md`):**
  - Document `PUT /api/v1/playback/states/{item_id}`, `PUT /api/v1/playback/states`, `GET /api/v1/items/{item_id}/audio-tracks`, `video_codec=none` audio-only HLS sessions, and the upgraded Web `CinemaPlayer` capabilities.

### 1.2 Architectural Invariants Preserved
- **Zero Backend Changes Needed**: Consumes the exact live server contracts already tested on `main` (`playback/handler.rs`, `audio_routes.rs`, `hls_routes.rs`, `storyboard_routes.rs`).
- **Code-Split Bundle Hygiene**: `hls.js` loads lazily via `import('hls.js')` inside `useHlsPlayback` only when an HLS stream is activated, keeping the primary SPA entry bundle lean.
- **Authenticated Media Sub-Resources**: Because `<img>`, CSS `background-image`, and `<video>` requests do not attach `Authorization` or `X-Kadr-Unlocked` headers automatically, all storyboard sheet URLs (`/api/v1/media/{id}/storyboard/{index}.jpg`), single-frame URLs (`/api/v1/media/{id}/frame`), and HLS playlist URLs append `?token=<jwt>&unlocked=<tokens>` via a shared `api.appendMediaAuthParams(url)` helper.

---

## 2. TypeScript Types & API Client (`web/src/types/`, `web/src/api/client.ts`)

### 2.1 Types (`web/src/types/index.ts`)
1. Add `AudioStreamInfo` (matching `crates/kadr-core/src/models.rs:60-74`) and extend `TechnicalInfo`:
   ```ts
   export interface AudioStreamInfo {
     audio_index: number;
     stream_index: number;
     codec: string;
     channels?: number;
     language: string;
     title?: string;
     is_default: boolean;
     is_commentary: boolean;
   }
   ```
   Add `audio_tracks?: AudioStreamInfo[];` to `TechnicalInfo`.
2. Add Storyboard & Batch Watch-State wire types:
   ```ts
   export interface StoryboardSheet {
     index: number;
     url: string;
     first_tile: number;
     tile_count: number;
   }

   export interface StoryboardManifest {
     interval_ms: number;
     tile_width: number;
     tile_height: number;
     columns: number;
     rows: number;
     total_tiles: number;
     sheets: StoryboardSheet[];
   }

   export interface BatchWatchStateResponse {
     updated: number;
     watched: boolean;
   }
   ```

### 2.2 `ApiClient` Methods (`web/src/api/client.ts`)
- **Auth URL Helper:** `appendMediaAuthParams(pathOrUrl: string): string` — appends `token=<jwt>` and `unlocked=<comma_joined_unlock_tokens>` cleanly (preserving any existing query parameters).
- **Watch State:**
  - `setPlaybackState(itemId: number, watched: boolean): Promise<PlaybackState>` -> `PUT /api/v1/playback/states/${itemId}` with `{ watched }`.
  - `setPlaybackStatesBatch(itemIds: number[], watched: boolean): Promise<BatchWatchStateResponse>` -> deduplicates `itemIds` and chunks into slices of `<= 500` items (matching server `1..=500` limit in `handler.rs:378`), summing `updated`.
- **Audio Tracks & HLS:**
  - `getAudioTracks(itemId: number): Promise<AudioStreamInfo[]>` -> `GET /api/v1/items/${itemId}/audio-tracks`.
  - `getHlsMasterUrl(itemId: number, opts?: { videoCodec?: string; audioCodec?: string; maxHeight?: number; startTime?: number; audioTrack?: number }): string` -> builds `/api/v1/media/${itemId}/hls/master.m3u8?...` with `appendMediaAuthParams`.
  - `deleteHlsSession(itemId: number, sessionId: string): Promise<void>` -> `DELETE /api/v1/media/${itemId}/hls/sessions/${encodeURIComponent(sessionId)}`.
- **Storyboards & Single-Frame Preview:**
  - `getStoryboard(itemId: number, signal?: AbortSignal): Promise<{ status: 'ready'; manifest: StoryboardManifest } | { status: 'generating'; retryAfterMs: number } | { status: 'unavailable' }>` -> handles `200 OK` JSON manifest, `202 Accepted` + `Retry-After` header (default `5000 ms`), and `404`/`503` gracefully without throwing unhandled errors.
  - `getStoryboardSheetUrl(sheetUrl: string): string` -> wraps `sheet.url` with `appendMediaAuthParams`.
  - `getFramePreviewUrl(itemId: number, seconds: number, width = 320): string` -> snaps `seconds` to 2-second increments (`Math.max(0, Math.floor(seconds / 2) * 2)`) and returns authenticated `/api/v1/media/${itemId}/frame?t=${snapped}&w=${width}`.

---

## 3. HLS Playback, Multi-Audio Tracks & Timeline Synchronization (`web/src/player/`)

### 3.1 Browser Media Compatibility (`web/src/player/compatibility.ts`)
- **`isAudioOnlyMedia(mediaType?: string, technical?: TechnicalInfo): boolean`:** Returns `true` when `mediaType` is `'music'` or `'audiobook'`, or when `technical?.audio_codec` is present while `technical?.video_codec` is empty/missing.
- **`shouldUseHls(opts: { mediaType?: string; technical?: TechnicalInfo; audioTrackIndex: number; forceHls?: boolean }): { useHls: boolean; videoCodec: 'h264' | 'none'; audioCodec: 'aac' }`:**
  - If `opts.audioTrackIndex > 0` or `opts.forceHls`: `useHls = true` (browsers do not reliably expose multi-track switching on Direct Play MP4/MKV without MSE/HLS).
  - If `isAudioOnlyMedia`: Direct Play for `mp3`, `m4a`, `aac`, `ogg`, `opus`, `flac`, `wav` when `audioTrackIndex === 0`; otherwise `useHls = true` with `videoCodec: 'none'` (`VideoAction::None`).
  - For video items (`audioTrackIndex === 0`): Direct Play when container is `mp4`, `m4v`, `mov`, or `webm` AND video codec is `h264`/`avc1`/`vp8`/`vp9`/`av1` (or unprobed) AND audio codec is `aac`/`mp3`/`opus`/`vorbis`/`flac` (or unprobed); otherwise `useHls = true` with `videoCodec: 'h264'`, `audioCodec: 'aac'`.

### 3.2 `useHlsPlayback` Hook (`web/src/player/useHlsPlayback.ts`)
Manages the `<video>` source lifecycle across Direct Play, `hls.js`, and native Safari HLS:
1. **Session Tracking & Cleanup:**
   - Parses `session_id` out of rewritten segment URLs (`/api/v1/media/{id}/hls/sessions/{session_id}/segment_...`) via `hls.js` `FRAG_LOADING` / `LEVEL_LOADED` events (or initial `fetch` pre-check on Safari).
   - Whenever switching audio tracks, restarting HLS at a new `start_time`, changing `itemId`, or unmounting `CinemaPlayer`, destroys the previous `Hls` instance and calls `api.deleteHlsSession(itemId, prevSessionId).catch(() => {})` so server `ffmpeg` slots are freed immediately!
2. **Automatic Direct Play -> HLS Fallback:**
   - If Direct Play `<video>` fires `onError` and `!forceHls`, sets `forceHls = true` (at `startTime = resumePosition || currentTime`) once per item so `MKV`s with unprobed codecs or `409 PLAYBACK_REQUIRES_TRANSCODE` plugin items seamlessly recover via HLS.
3. **Unified Timeline (`timeOffset`) & Seeking:**
   - In Direct Play: `timeOffset = 0`, `effectiveCurrentTime = video.currentTime`, `effectiveDuration = video.duration || itemDetails.duration_seconds`.
   - In HLS started with `start_time = S`: `timeOffset = S`, `effectiveCurrentTime = S + video.currentTime`, and `effectiveDuration = itemDetails.duration_seconds || (S + video.duration)`.
   - When `handleSeek(targetSeconds)` is called in HLS mode:
     - If `targetSeconds >= timeOffset` and `targetSeconds - timeOffset` falls within `video.buffered` (with 1s tolerance), seek `video.currentTime = targetSeconds - timeOffset` directly with zero reload!
     - Otherwise, restart the HLS stream cleanly at `start_time = targetSeconds` (`setTimeOffset(targetSeconds)`), preserving play/pause state.
4. **WebVTT Subtitle Cue Synchronization Under `timeOffset`:**
   - When `timeOffset > 0` in HLS mode, `<track>` cues parsed by the browser have original media timestamps (`cue.startTime`), while `video.currentTime` starts at `0`. On `cue` load (storing original `_origStart` / `_origEnd` on each cue once), adjust `cue.startTime = Math.max(0, origStart - timeOffset)` and `cue.endTime = Math.max(0, origEnd - timeOffset)` whenever `timeOffset` changes so subtitles remain frame-accurate after any HLS seek or audio-track switch!

### 3.3 Audio Track Selector UI (`PlayerControls.tsx` & `ItemDetailsModal.tsx`)
- **`CinemaPlayer.tsx`:** Fetches `api.getAudioTracks(itemId)` on mount (falling back to `itemDetails?.technical?.audio_tracks ?? []`), defaulting `activeAudioTrackIndex` to `tracks.find(t => t.is_default)?.audio_index ?? 0`. Selecting a track calls `switchAudioTrack(audioIndex)` at `effectiveCurrentTime`.
- **`PlayerControls.tsx`:**
  - Adds an **Audio Tracks** button (`Headphones` / `Languages` icon, `aria-haspopup="menu"`, `aria-expanded`) next to the Subtitles button whenever `audioTracks.length >= 1` (keeping controls visible while open, closing Subtitles/Queue popovers mutually exclusively, and closing on `Escape` with focus restored to the trigger button).
  - Menu items use `<button type="button" role="menuitemradio" aria-checked={track.audio_index === activeAudioTrackIndex}>` displaying:
    - Human-readable language/title (`English — Director Commentary` or `Arabic` / `Track 1`),
    - Technical pills (`EAC3`, `5.1` / `7.1` / `Stereo` / `Mono`, `Default`, `Commentary`).
- **`ItemDetailsModal.tsx`:** Displays concise audio track badges (e.g., `AR • AAC Stereo`, `EN • EAC3 5.1`) in the technical metadata row when `audio_tracks` has multiple streams.

---

## 4. Storyboard Seek-Bar Hover Preview (`web/src/components/player/`)

### 4.1 `useStoryboard` Hook (`web/src/player/useStoryboard.ts`)
- Skips ineligible items (`live_channel`, `music`, `audiobook`, `show`, `season`, or `duration <= 0`).
- Calls `api.getStoryboard(itemId, signal)`:
  - On `ready`: stores `StoryboardManifest` and exposes tile lookup helper `getTileAtSeconds(sec, displayWidth = 160)` -> `{ sheetUrl, width, height, backgroundSize, backgroundPosition }`:
    - `tileIndex = Math.min(manifest.total_tiles - 1, Math.max(0, Math.floor((sec * 1000) / manifest.interval_ms)))`
    - Finds matching `sheet` where `tileIndex >= sheet.first_tile && tileIndex < sheet.first_tile + sheet.tile_count`
    - `scale = displayWidth / manifest.tile_width`, `displayHeight = Math.round(manifest.tile_height * scale)`
    - `localIndex = tileIndex - sheet.first_tile`, `col = localIndex % manifest.columns`, `row = Math.floor(localIndex / manifest.columns)`
    - `backgroundPosition = -${col * displayWidth}px -${row * displayHeight}px`
    - `backgroundSize = ${manifest.columns * displayWidth}px ${manifest.rows * displayHeight}px`
  - On `generating`: schedules retry after `retryAfterMs` (`5000ms` default, capped at `12` retries, cleaned up on unmount/`itemId` change).

### 4.2 `<SeekPreviewThumbnail />` (`web/src/components/player/SeekPreviewThumbnail.tsx`) & `PlayerControls.tsx`
- On `onPointerMove` / `onPointerLeave` over the seek bar wrapper in `PlayerControls.tsx`, compute hovered `previewSeconds` and horizontal percentage `previewPercent` (clamped so the `160px`-wide preview card never clips off the left/right edges of the player).
- Render `<SeekPreviewThumbnail />` (`aria-hidden="true"`, `pointer-events-none`) floating above the hovered position:
  1. **Sprite mode (`manifest` ready):** Renders scaled `160px`-wide tile (`scale = 160 / tile_width`) using CSS `background-image` + `background-position` + `background-size` for zero-latency instant scrubbing, with crisp `formatPlaybackTime(previewSeconds)` pill overlaid at the bottom.
  2. **Generating fallback (`status === 'generating'`):** Debounces (`120ms`) a single-frame `<img src={api.getFramePreviewUrl(itemId, previewSeconds, 320)} alt="" />` preview while background sprite-sheet generation runs.
  3. **Unavailable / Audio-only:** Shows the compact floating `formatPlaybackTime(previewSeconds)` timestamp pill cleanly without broken image icons.

---

## 5. Mark Watched / Unwatched Surfaces (`web/src/components/browse/`, `web/src/utils/watchStateEvents.ts`)

### 5.1 Cross-View Optimistic Sync (`web/src/utils/watchStateEvents.ts`)
- `emitWatchStateChanged(itemIds: number[], watched: boolean)` dispatches a `CustomEvent` on `window`.
- `CarouselWidget.tsx`, `GridWidget.tsx`, and `BrowseScreen.tsx` subscribe via `onWatchStateChanged`:
  - Cards matching `itemIds` update `playback_progress` (`1.0` when `watched`, `undefined` when `!watched`) and clear `"RESUME"` `badge` immediately.
  - When `screenId === 'home'`, `BrowseScreen` silently re-fetches `/api/v1/screens/home` so the `Continue Watching` rail reflects newly completed/unwatched items without page reload!

### 5.2 `<WatchStateButton />` (`web/src/components/browse/WatchStateButton.tsx`)
- Hidden when `mediaType === 'live_channel'`.
- **`variant="icon"` (Poster Cards in `CarouselWidget.tsx` & `GridWidget.tsx` for non-`show`, non-`live_channel` items):**
  - Top-right circular `<button type="button">` (`min-w-7 min-h-7`, `e.stopPropagation()` on click/keydown so activating never opens `ItemDetailsModal`).
  - Accessible disambiguated label (`aria-label={watched ? `Mark ${title} as unwatched` : `Mark ${title} as watched`}`, `aria-pressed={watched}`).
  - When `watched` (`playback_progress !== undefined && playback_progress >= 1`): always visible with crisp emerald/gold check badge (`CheckCircle2` / `Check`). When unwatched: visible on card hover or keyboard focus (`opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100`).
- **`variant="pill"` (`ItemDetailsModal.tsx`):**
  - **Single playable items (`movie`, `episode`, `anime`, `home_videos`, `music`, `audiobook`):** `Watched` / `Mark Watched` button in the primary action bar next to `Play` and `+ Playlist`; toggling clears `resume_position_seconds` locally and calls `api.setPlaybackState(id, nextWatched)`.
  - **TV `Show` items (`episodes.length > 0`):**
    - Primary action bar: **`Series Watched` / `Mark Series Watched`** (toggles `!allEpisodesWatched` across all episode IDs via `api.setPlaybackStatesBatch`).
    - Season header bar: **`Season Watched` / `Mark Season Watched`** (toggles `!allSeasonWatched` across the selected season's episode IDs via `api.setPlaybackStatesBatch`).
    - Episode list rows: compact per-episode Watched check button updating that episode's `playback_progress` optimistically via `api.setPlaybackState`.

---

## 6. Documentation Updates (`README.md` & `docs/CLIENT_GUIDE.md`)

- **`README.md`:**
  - Update `Key Features` (`Cinema Player` & `Playback & Scrobbling` API reference) to list `PUT /api/v1/playback/states/:item_id`, `PUT /api/v1/playback/states` (batch up to 500 items), `GET /api/v1/items/:item_id/audio-tracks`, `video_codec=none` audio-only HLS sessions, and Web player HLS fallback, multi-audio track switching, storyboard seek previews, and single/season/series Mark Watched controls.
- **`docs/CLIENT_GUIDE.md`:**
  - Document `GET /api/v1/items/{item_id}/audio-tracks` (`AudioStreamInfo` schema, lazy `ffprobe` singleflight persistence, `403 LIBRARY_LOCKED`), `video_codec=none` parameter on `GET /api/v1/media/{item_id}/hls/master.m3u8`, `PUT /api/v1/playback/states/{item_id}` (`{"watched": true|false}` -> `PlaybackState`), and `PUT /api/v1/playback/states` (`{"item_ids": [...], "watched": true|false}` -> `{"updated": N, "watched": bool}`).

---

## 7. Testing & Verification Strategy

1. **HLS & Compatibility Unit Tests (`web/src/player/compatibility.test.ts`, `useHlsPlayback.test.tsx`):**
   - Direct Play vs HLS selection matrix across `mp4`/`mkv`/`avi`/`webm`, `h264`/`hevc`/`dts`/`eac3`, `music`/`audiobook` (`video_codec=none`), and `audioTrackIndex > 0`.
   - `timeOffset` (`start_time`) effective timeline calculation, buffered vs unbuffered HLS seeking, WebVTT cue timestamp shifting, Direct Play error -> HLS auto-fallback, and `deleteHlsSession` cleanup on track switch & unmount.
2. **Storyboard & Player Controls Tests (`web/src/components/player/SeekPreviewThumbnail.test.tsx`, `PlayerControls.test.tsx`, `CinemaPlayer.test.tsx`):**
   - `202 Accepted` (`Retry-After`) -> `200 OK` manifest polling, exact tile `backgroundPosition` & `backgroundSize` calculation across multi-sheet boundaries, `/frame` fallback while generating, and Audio Track radio-menu selection (`aria-checked`, `Escape` dismissal).
3. **Watch-State UI Tests (`web/src/components/browse/WatchStateButton.test.tsx`, `ItemDetailsModal.test.tsx`):**
   - Card hover toggle (`stopPropagation`, optimistic state + rollback on error, `live_channel` hidden), `ItemDetailsModal` single-item toggle, per-episode row toggle, and Season/Series batch `setPlaybackStatesBatch` (including `> 500`-item chunking).
4. **Full Build & Workspace Verification:**
   - `cd web && npm test -- --run && npm run build` (verifying `hls.js` code-splits into its own async chunk!) and `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`.
