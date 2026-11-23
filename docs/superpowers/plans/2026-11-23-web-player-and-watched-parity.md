# Web Player & Watch-State Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Kadr's server-side Mark Watched (single & batch), Multi-Audio Track discovery & `hls.js` HLS playback (including `video_codec=none` audio-only HLS and automatic Direct Play fallback), and 10×10 Storyboard seek-bar previews into the React Web UI (`web/`) and official documentation (`README.md`, `docs/CLIENT_GUIDE.md`).

**Architecture:** Keep `CinemaPlayer.tsx` and `PlayerControls.tsx` lean by extracting focused modules under `web/src/player/` (`compatibility.ts`, `useHlsPlayback.ts`, `useStoryboard.ts`), `web/src/components/player/SeekPreviewThumbnail.tsx`, `web/src/components/browse/WatchStateButton.tsx`, and `web/src/utils/watchStateEvents.ts`. Load `hls.js` (`^1.7.3`) lazily via dynamic `import('hls.js')` only when an HLS session is required, append media query authentication (`?token=<jwt>&unlocked=<tokens>`) to all sub-resource image/playlist URLs, and synchronize optimistic watch-state changes across carousels, grids, modals, and the Home `Continue Watching` rail.

**Tech Stack:** React 19, TypeScript 5.7, Vite 6, Vitest 3 + React Testing Library, Tailwind CSS 4, `lucide-react`, `hls.js` `^1.7.3`.

## Global Constraints

- **Zero Backend Modifications Required:** Consume the exact live Rust endpoints on `main` (`PUT /api/v1/playback/states/{item_id}`, `PUT /api/v1/playback/states`, `GET /api/v1/items/{item_id}/audio-tracks`, `GET /api/v1/media/{item_id}/hls/master.m3u8`, `DELETE /api/v1/media/{item_id}/hls/sessions/{session_id}`, `GET /api/v1/media/{item_id}/storyboard`, `GET /api/v1/media/{item_id}/storyboard/{index}.jpg`, `GET /api/v1/media/{item_id}/frame`).
- **Dynamic Code-Split `hls.js`:** Add `"hls.js": "^1.7.3"` to `web/package.json` and import it exclusively via `await import('hls.js')` inside `useHlsPlayback.ts` (plus `import type Hls from 'hls.js'` for TypeScript types) so non-player bundles stay lean.
- **Authenticated Media Sub-Resources:** CSS `background-image`, `<img>`, and `<video>` requests do not attach `Authorization` or `X-Kadr-Unlocked` headers; every HLS playlist URL, storyboard sprite-sheet URL, and `/frame` URL MUST pass through `api.appendMediaAuthParams(...)`.
- **Batch Watch-State Limit (`<= 500`):** `PUT /api/v1/playback/states` rejects `item_ids.len() > 500` with `400 Bad Request`; `api.setPlaybackStatesBatch` MUST deduplicate IDs and chunk requests into slices of at most `500` items.
- **Accessibility & Modern Web Menu Semantics:** Trigger buttons for Audio Tracks use `<button type="button" aria-haspopup="menu" aria-expanded={...}>`, menu choices use `<button type="button" role="menuitemradio" aria-checked={...}>`, `Escape` closes the open menu and restores focus to its trigger button, decorative seek thumbnails carry `aria-hidden="true"`, and repeated card Watch toggles disambiguate their accessible name with the item title (`Mark ${title} as watched` / `Mark ${title} as unwatched`) and `aria-pressed={watched}`.
- **Working Branch & Git Safety:** Commit directly on `main` in `/home/boody/work/personal/kadr` per user instruction; never run `git push` or create git tags unless explicitly asked.

---

## File Map

- **Modify:** `web/package.json` — add `"hls.js": "^1.7.3"` dependency.
- **Modify:** `web/src/types/index.ts` — add `AudioStreamInfo`, `StoryboardSheet`, `StoryboardManifest`, `BatchWatchStateResponse`, and `TechnicalInfo.audio_tracks`.
- **Modify:** `web/src/api/client.ts` — add `appendMediaAuthParams`, `setPlaybackState`, `setPlaybackStatesBatch`, `getAudioTracks`, `getHlsMasterUrl`, `deleteHlsSession`, `getStoryboard`, `getStoryboardSheetUrl`, and `getFramePreviewUrl`.
- **Modify:** `web/src/api/client.test.ts` — unit tests for all new `ApiClient` methods, auth query param preservation, `202` `Retry-After` parsing, and `> 500` batch chunking.
- **Create:** `web/src/player/compatibility.ts` — Direct Play vs HLS decision matrix (`isAudioOnlyMedia`, `shouldUseHls`) and audio track label/channel formatters (`formatAudioTrackLabel`, `formatChannelLayout`).
- **Create:** `web/src/player/compatibility.test.ts` — unit tests for container/codec/audio-track compatibility matrix and label formatting.
- **Create:** `web/src/player/useHlsPlayback.ts` — `<video>` playback lifecycle across Direct Play, lazy `hls.js`, and Safari native HLS, including `timeOffset` (`start_time`) seeking, WebVTT cue offset sync, Direct Play `onError` fallback, and `deleteHlsSession` cleanup.
- **Create:** `web/src/player/useHlsPlayback.test.tsx` — unit tests for `useHlsPlayback` with mocked `hls.js` and `HTMLVideoElement`.
- **Create:** `web/src/player/useStoryboard.ts` — storyboard manifest fetcher with `202 Accepted` `Retry-After` polling (max 12 retries) and `getTileAtSeconds(sec, displayWidth)` sprite CSS coordinate math.
- **Create:** `web/src/components/player/SeekPreviewThumbnail.tsx` — floating seek-bar thumbnail component (sprite mode, debounced `/frame` fallback while generating, timestamp pill fallback).
- **Create:** `web/src/components/player/SeekPreviewThumbnail.test.tsx` — unit tests for `useStoryboard` polling/math and `<SeekPreviewThumbnail />` rendering modes.
- **Modify:** `web/src/components/player/PlayerControls.tsx` — seek-bar pointer hover preview integration and accessible **Audio Tracks** radio-menu popover.
- **Modify:** `web/src/components/player/PlayerControls.test.tsx` — unit tests for Audio Tracks menu (`aria-checked`, `Escape` focus return, mutual exclusion) and seek-bar hover preview.
- **Modify:** `web/src/components/player/CinemaPlayer.tsx` — wire `useHlsPlayback`, `useStoryboard`, `api.getAudioTracks`, `effectiveCurrentTime` heartbeats, and keyboard seeking.
- **Modify:** `web/src/components/player/CinemaPlayer.test.tsx` — integration tests for multi-audio track switching, HLS fallback, and effective timeline heartbeats.
- **Create:** `web/src/utils/watchStateEvents.ts` — cross-component custom event bus (`emitWatchStateChanged`, `onWatchStateChanged`, `applyWatchStateToCard`).
- **Create:** `web/src/components/browse/WatchStateButton.tsx` — reusable `icon` (poster card overlay) and `pill` (modal action) Mark Watched / Unwatched toggle component.
- **Create:** `web/src/components/browse/WatchStateButton.test.tsx` — unit tests for optimistic toggle, `stopPropagation`, rollback on error, disambiguated `aria-label`, and `live_channel` hiding.
- **Modify:** `web/src/components/browse/CarouselWidget.tsx` — render `<WatchStateButton variant="icon" />` on playable poster cards and subscribe to `onWatchStateChanged`.
- **Modify:** `web/src/components/browse/GridWidget.tsx` — render `<WatchStateButton variant="icon" />` on playable poster cards and subscribe to `onWatchStateChanged`.
- **Modify:** `web/src/components/browse/BrowseScreen.tsx` — silently refresh `home` layout on `onWatchStateChanged` so `Continue Watching` updates immediately.
- **Modify:** `web/src/components/browse/ItemDetailsModal.tsx` — add single-item `Mark Watched`, TV `Mark Series Watched`, `Mark Season Watched`, per-episode watched toggle, and technical audio stream badges.
- **Modify:** `web/src/components/browse/ItemDetailsModal.test.tsx` — unit tests for single, episode, season, and series Mark Watched flows plus audio stream badges.
- **Modify:** `README.md` & `docs/CLIENT_GUIDE.md` — document all new endpoints, query parameters, and Web player capabilities.

---

### Task 1: `hls.js` Dependency, Wire Types, `ApiClient` Methods & Media Compatibility

**Files:**
- Modify: `web/package.json`
- Modify: `web/src/types/index.ts:123-131,278-295`
- Modify: `web/src/api/client.ts:1-54,280-310,537-575`
- Modify: `web/src/api/client.test.ts`
- Create: `web/src/player/compatibility.ts`
- Create: `web/src/player/compatibility.test.ts`

**Interfaces:**
- Consumes: Existing `ApiClient.request`, `ApiClient.getToken`, `ApiClient.getUnlockedTokens` (`web/src/api/client.ts`).
- Produces:
  - `AudioStreamInfo`, `StoryboardSheet`, `StoryboardManifest`, `BatchWatchStateResponse`, `StoryboardFetchResult` in `web/src/types/index.ts` (and `TechnicalInfo.audio_tracks?: AudioStreamInfo[]`).
  - `api.appendMediaAuthParams(pathOrUrl: string): string`
  - `api.setPlaybackState(itemId: number, watched: boolean): Promise<PlaybackState>`
  - `api.setPlaybackStatesBatch(itemIds: number[], watched: boolean): Promise<BatchWatchStateResponse>`
  - `api.getAudioTracks(itemId: number): Promise<AudioStreamInfo[]>`
  - `api.getHlsMasterUrl(itemId: number, opts?: HlsUrlOptions): string`
  - `api.deleteHlsSession(itemId: number, sessionId: string): Promise<void>`
  - `api.getStoryboard(itemId: number, signal?: AbortSignal): Promise<StoryboardFetchResult>`
  - `api.getStoryboardSheetUrl(sheetUrl: string): string`
  - `api.getFramePreviewUrl(itemId: number, seconds: number, width?: number): string`
  - `isAudioOnlyMedia(mediaType?: string, technical?: TechnicalInfo): boolean`
  - `shouldUseHls(opts: CompatibilityInput): CompatibilityDecision`
  - `formatChannelLayout(channels?: number): string | null`
  - `formatAudioTrackLabel(track: AudioStreamInfo): string`

- [ ] **Step 1: Install `hls.js` (`^1.7.3`) in `web/`**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npm install hls.js@^1.7.3
```
Expected: `web/package.json` lists `"hls.js": "^1.7.3"` under `dependencies` and `web/package-lock.json` updates cleanly.

- [ ] **Step 2: Write failing unit tests in `web/src/api/client.test.ts` and `web/src/player/compatibility.test.ts`**

Append a new `describe('Player, Storyboard & Watch-State API methods', ...)` block to `web/src/api/client.test.ts`:
```ts
describe('Player, Storyboard & Watch-State API methods', () => {
  it('appendMediaAuthParams preserves existing query params and appends token + unlocked', () => {
    const client = new ApiClient('http://localhost:8492');
    client.setToken('jwt-abc');
    client.storeUnlockToken('lib-priv', 'unlock-1');

    expect(client.appendMediaAuthParams('/api/v1/media/42/storyboard/0.jpg')).toBe(
      'http://localhost:8492/api/v1/media/42/storyboard/0.jpg?token=jwt-abc&unlocked=unlock-1'
    );
    expect(client.appendMediaAuthParams('/api/v1/media/42/frame?t=12&w=320')).toBe(
      'http://localhost:8492/api/v1/media/42/frame?t=12&w=320&token=jwt-abc&unlocked=unlock-1'
    );
  });

  it('builds HLS master URLs with codec, start_time, audio_track, and auth params', () => {
    const client = new ApiClient('');
    client.setToken('jwt-xyz');
    const url = client.getHlsMasterUrl(99, {
      videoCodec: 'none',
      audioCodec: 'aac',
      startTime: 125.8,
      audioTrack: 2,
    });
    expect(url).toBe(
      '/api/v1/media/99/hls/master.m3u8?video_codec=none&audio_codec=aac&start_time=125.8&audio_track=2&token=jwt-xyz'
    );
  });

  it('snaps getFramePreviewUrl timestamps down to 2-second increments', () => {
    const client = new ApiClient('');
    client.setToken('jwt-1');
    expect(client.getFramePreviewUrl(7, 15.9, 320)).toBe(
      '/api/v1/media/7/frame?t=14&w=320&token=jwt-1'
    );
  });

  it('chunks setPlaybackStatesBatch into <= 500 unique item IDs per PUT', async () => {
    const fetchMock = vi.fn().mockImplementation(async (_url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body));
      return new Response(
        JSON.stringify({ updated: body.item_ids.length, watched: body.watched }),
        { status: 200, headers: { 'Content-Type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const client = new ApiClient('');
    const ids = Array.from({ length: 620 }, (_, i) => i + 1);
    const res = await client.setPlaybackStatesBatch([...ids, 1, 2], true);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const firstBody = JSON.parse(String(fetchMock.mock.calls[0][1]?.body));
    const secondBody = JSON.parse(String(fetchMock.mock.calls[1][1]?.body));
    expect(firstBody.item_ids).toHaveLength(500);
    expect(secondBody.item_ids).toHaveLength(120);
    expect(res).toEqual({ updated: 620, watched: true });
  });

  it('parses getStoryboard 200 ready, 202 Retry-After generating, and 404 unavailable', async () => {
    const client = new ApiClient('');
    const manifest = {
      interval_ms: 10000,
      tile_width: 160,
      tile_height: 90,
      columns: 10,
      rows: 10,
      total_tiles: 12,
      sheets: [{ index: 0, url: '/api/v1/media/5/storyboard/0.jpg', first_tile: 0, tile_count: 12 }],
    };

    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ status: 'generating' }), {
          status: 202,
          headers: { 'Content-Type': 'application/json', 'Retry-After': '7' },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify(manifest), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: 'Not found' }), {
          status: 404,
          headers: { 'Content-Type': 'application/json' },
        })
      );
    vi.stubGlobal('fetch', fetchMock);

    expect(await client.getStoryboard(5)).toEqual({
      status: 'generating',
      retryAfterMs: 7000,
    });
    expect(await client.getStoryboard(5)).toEqual({
      status: 'ready',
      manifest,
    });
    expect(await client.getStoryboard(5)).toEqual({ status: 'unavailable' });
  });
});
```

Create `web/src/player/compatibility.test.ts`:
```ts
import { describe, it, expect } from 'vitest';
import {
  isAudioOnlyMedia,
  shouldUseHls,
  formatChannelLayout,
  formatAudioTrackLabel,
} from './compatibility';

describe('player compatibility helpers', () => {
  it('identifies audio-only media types and missing video streams', () => {
    expect(isAudioOnlyMedia('music', { duration_seconds: 180, container: 'flac' })).toBe(true);
    expect(isAudioOnlyMedia('audiobook', { duration_seconds: 600 })).toBe(true);
    expect(
      isAudioOnlyMedia('movie', { duration_seconds: 120, audio_codec: 'aac' })
    ).toBe(true);
    expect(
      isAudioOnlyMedia('movie', {
        duration_seconds: 120,
        video_codec: 'h264',
        audio_codec: 'aac',
      })
    ).toBe(false);
  });

  it('uses Direct Play for browser-native MP4/WebM at audioTrackIndex 0', () => {
    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'h264', audioCodec: 'aac' });
  });

  it('switches to HLS when audioTrackIndex > 0 or container/codec is non-native', () => {
    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        },
        audioTrackIndex: 1,
      })
    ).toEqual({ useHls: true, videoCodec: 'h264', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'mkv',
          video_codec: 'h264',
          audio_codec: 'eac3',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: true, videoCodec: 'h264', audioCodec: 'aac' });
  });

  it('uses video_codec=none for audio-only HLS transcoding (WMA/AC3 or non-zero track)', () => {
    expect(
      shouldUseHls({
        mediaType: 'music',
        technical: { duration_seconds: 210, container: 'mka', audio_codec: 'dts' },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: true, videoCodec: 'none', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'audiobook',
        technical: { duration_seconds: 900, container: 'm4b', audio_codec: 'aac' },
        audioTrackIndex: 1,
      })
    ).toEqual({ useHls: true, videoCodec: 'none', audioCodec: 'aac' });
  });

  it('formats channel layouts and human-readable audio track labels', () => {
    expect(formatChannelLayout(1)).toBe('Mono');
    expect(formatChannelLayout(2)).toBe('Stereo');
    expect(formatChannelLayout(6)).toBe('5.1');
    expect(formatChannelLayout(8)).toBe('7.1');

    expect(
      formatAudioTrackLabel({
        audio_index: 1,
        stream_index: 2,
        codec: 'eac3',
        channels: 6,
        language: 'jpn',
        title: 'Original Japanese',
        is_default: false,
        is_commentary: false,
      })
    ).toBe('Japanese — Original Japanese');
  });
});
```

- [ ] **Step 3: Run Vitest on the two test files to confirm failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/api/client.test.ts src/player/compatibility.test.ts
```
Expected: FAIL (`compatibility` module not found; `appendMediaAuthParams` / `getHlsMasterUrl` / `setPlaybackStatesBatch` / `getStoryboard` not defined).

- [ ] **Step 4: Implement types in `web/src/types/index.ts`, methods in `web/src/api/client.ts`, and `web/src/player/compatibility.ts`**

In `web/src/types/index.ts`:
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

export interface TechnicalInfo {
  duration_seconds: number;
  resolution?: string;
  video_codec?: string;
  audio_codec?: string;
  audio_channels?: number;
  container?: string;
  audio_tracks?: AudioStreamInfo[];
}

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

export type StoryboardFetchResult =
  | { status: 'ready'; manifest: StoryboardManifest }
  | { status: 'generating'; retryAfterMs: number }
  | { status: 'unavailable' };

export interface BatchWatchStateResponse {
  updated: number;
  watched: boolean;
}

export interface HlsUrlOptions {
  videoCodec?: 'h264' | 'hevc' | 'none' | string;
  audioCodec?: 'aac' | 'ac3' | 'eac3' | string;
  maxHeight?: number;
  startTime?: number;
  audioTrack?: number;
}
```

In `web/src/api/client.ts`:
```ts
  public appendMediaAuthParams(pathOrUrl: string, unlockedOverride?: string): string {
    const prefixed =
      pathOrUrl.startsWith('http://') || pathOrUrl.startsWith('https://')
        ? pathOrUrl
        : `${this.baseUrl}${pathOrUrl.startsWith('/') ? pathOrUrl : `/${pathOrUrl}`}`;

    const qIdx = prefixed.indexOf('?');
    const basePath = qIdx >= 0 ? prefixed.slice(0, qIdx) : prefixed;
    const params = new URLSearchParams(qIdx >= 0 ? prefixed.slice(qIdx + 1) : '');

    const token = this.getToken();
    if (token && !params.has('token')) {
      params.set('token', token);
    }

    const unlockedTokens = unlockedOverride
      ? [unlockedOverride]
      : Object.values(this.getUnlockedTokens()).filter(Boolean);
    if (unlockedTokens.length > 0 && !params.has('unlocked')) {
      params.set('unlocked', unlockedTokens.join(','));
    }

    const qs = params.toString();
    return qs ? `${basePath}?${qs}` : basePath;
  }

  public getHlsMasterUrl(itemId: number, opts: HlsUrlOptions = {}): string {
    const params = new URLSearchParams();
    params.set('video_codec', opts.videoCodec ?? 'h264');
    params.set('audio_codec', opts.audioCodec ?? 'aac');
    if (opts.maxHeight !== undefined && opts.maxHeight > 0) {
      params.set('max_height', String(Math.floor(opts.maxHeight)));
    }
    if (opts.startTime !== undefined && opts.startTime > 0) {
      params.set('start_time', String(Math.round(opts.startTime * 100) / 100));
    }
    if (opts.audioTrack !== undefined && opts.audioTrack >= 0) {
      params.set('audio_track', String(opts.audioTrack));
    }
    return this.appendMediaAuthParams(
      `/api/v1/media/${itemId}/hls/master.m3u8?${params.toString()}`
    );
  }

  public async deleteHlsSession(itemId: number, sessionId: string): Promise<void> {
    await this.request<void>(
      `/api/v1/media/${itemId}/hls/sessions/${encodeURIComponent(sessionId)}`,
      { method: 'DELETE' }
    );
  }

  public async getAudioTracks(itemId: number): Promise<AudioStreamInfo[]> {
    return this.request<AudioStreamInfo[]>(`/api/v1/items/${itemId}/audio-tracks`);
  }

  public async setPlaybackState(
    itemId: number,
    watched: boolean
  ): Promise<PlaybackState> {
    return this.request<PlaybackState>(`/api/v1/playback/states/${itemId}`, {
      method: 'PUT',
      body: JSON.stringify({ watched }),
    });
  }

  public async setPlaybackStatesBatch(
    itemIds: number[],
    watched: boolean
  ): Promise<BatchWatchStateResponse> {
    const uniqueIds = Array.from(
      new Set(itemIds.filter((id) => Number.isInteger(id) && id > 0))
    );
    if (uniqueIds.length === 0) {
      return { updated: 0, watched };
    }
    let totalUpdated = 0;
    for (let offset = 0; offset < uniqueIds.length; offset += 500) {
      const slice = uniqueIds.slice(offset, offset + 500);
      const res = await this.request<BatchWatchStateResponse>(
        '/api/v1/playback/states',
        {
          method: 'PUT',
          body: JSON.stringify({ item_ids: slice, watched }),
        }
      );
      totalUpdated += res?.updated ?? 0;
    }
    return { updated: totalUpdated, watched };
  }

  public async getStoryboard(
    itemId: number,
    signal?: AbortSignal
  ): Promise<StoryboardFetchResult> {
    const url = `${this.baseUrl}/api/v1/media/${itemId}/storyboard`;
    const headers: Record<string, string> = { Accept: 'application/json' };
    const token = this.getToken();
    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }
    const unlockedTokens = Object.values(this.getUnlockedTokens()).filter(Boolean);
    if (unlockedTokens.length > 0) {
      headers['X-Kadr-Unlocked'] = unlockedTokens.join(',');
    }

    const response = await fetch(url, { method: 'GET', headers, signal });
    if (response.status === 401) {
      this.logout();
      return { status: 'unavailable' };
    }
    if (response.status === 202) {
      const retryHeader = response.headers.get('Retry-After');
      const retrySecs = retryHeader ? Number(retryHeader) : NaN;
      const retryAfterMs =
        Number.isFinite(retrySecs) && retrySecs > 0 ? retrySecs * 1000 : 5000;
      return { status: 'generating', retryAfterMs };
    }
    if (response.status === 200) {
      const manifest = (await response.json()) as StoryboardManifest;
      return { status: 'ready', manifest };
    }
    return { status: 'unavailable' };
  }

  public getStoryboardSheetUrl(sheetUrl: string): string {
    return this.appendMediaAuthParams(sheetUrl);
  }

  public getFramePreviewUrl(
    itemId: number,
    seconds: number,
    width: number = 320
  ): string {
    const snapped = Math.max(0, Math.floor(seconds / 2) * 2);
    return this.appendMediaAuthParams(
      `/api/v1/media/${itemId}/frame?t=${snapped}&w=${width}`
    );
  }
```

Create `web/src/player/compatibility.ts`:
```ts
import type { AudioStreamInfo, TechnicalInfo } from '../types';

export interface CompatibilityInput {
  mediaType?: string;
  technical?: TechnicalInfo;
  audioTrackIndex: number;
  forceHls?: boolean;
}

export interface CompatibilityDecision {
  useHls: boolean;
  videoCodec: 'h264' | 'none';
  audioCodec: 'aac';
}

const NATIVE_VIDEO_CONTAINERS = new Set(['mp4', 'm4v', 'mov', 'webm']);
const NATIVE_VIDEO_CODECS = new Set([
  'h264',
  'avc',
  'avc1',
  'vp8',
  'vp9',
  'av1',
  'av01',
]);
const NATIVE_AUDIO_CODECS = new Set([
  'aac',
  'mp4a',
  'mp3',
  'mp2',
  'opus',
  'vorbis',
  'flac',
  'pcm',
  'pcm_s16le',
  'pcm_s24le',
  'wav',
]);
const NATIVE_AUDIO_CONTAINERS = new Set([
  'mp3',
  'm4a',
  'm4b',
  'aac',
  'ogg',
  'oga',
  'opus',
  'flac',
  'wav',
  'webm',
]);

const LANGUAGE_NAMES: Record<string, string> = {
  en: 'English',
  eng: 'English',
  ar: 'Arabic',
  ara: 'Arabic',
  fr: 'French',
  fra: 'French',
  fre: 'French',
  es: 'Spanish',
  spa: 'Spanish',
  de: 'German',
  deu: 'German',
  ger: 'German',
  it: 'Italian',
  ita: 'Italian',
  ja: 'Japanese',
  jpn: 'Japanese',
  ko: 'Korean',
  kor: 'Korean',
  zh: 'Chinese',
  zho: 'Chinese',
  chi: 'Chinese',
  ru: 'Russian',
  rus: 'Russian',
  pt: 'Portuguese',
  por: 'Portuguese',
  tr: 'Turkish',
  tur: 'Turkish',
  hi: 'Hindi',
  hin: 'Hindi',
  und: 'Unknown',
};

function norm(value?: string): string {
  return (value ?? '').trim().toLowerCase();
}

export function isAudioOnlyMedia(
  mediaType?: string,
  technical?: TechnicalInfo
): boolean {
  const mt = norm(mediaType);
  if (mt === 'music' || mt === 'audiobook') {
    return true;
  }
  const vCodec = norm(technical?.video_codec);
  const aCodec = norm(technical?.audio_codec);
  const container = norm(technical?.container);
  if (!vCodec && aCodec) {
    return true;
  }
  if (!vCodec && NATIVE_AUDIO_CONTAINERS.has(container)) {
    return true;
  }
  return false;
}

export function shouldUseHls({
  mediaType,
  technical,
  audioTrackIndex,
  forceHls = false,
}: CompatibilityInput): CompatibilityDecision {
  if (norm(mediaType) === 'live_channel') {
    return {
      useHls: forceHls,
      videoCodec: 'h264',
      audioCodec: 'aac',
    };
  }

  const audioOnly = isAudioOnlyMedia(mediaType, technical);
  const targetVideoCodec: 'h264' | 'none' = audioOnly ? 'none' : 'h264';

  if (forceHls || audioTrackIndex > 0) {
    return {
      useHls: true,
      videoCodec: targetVideoCodec,
      audioCodec: 'aac',
    };
  }

  const container = norm(technical?.container);
  const vCodec = norm(technical?.video_codec);
  const aCodec = norm(technical?.audio_codec);

  if (audioOnly) {
    const containerOk = !container || NATIVE_AUDIO_CONTAINERS.has(container);
    const audioOk = !aCodec || NATIVE_AUDIO_CODECS.has(aCodec);
    return {
      useHls: !(containerOk && audioOk),
      videoCodec: 'none',
      audioCodec: 'aac',
    };
  }

  // Unprobed items try Direct Play first and rely on <video> onError -> forceHls fallback
  if (!container && !vCodec && !aCodec) {
    return { useHls: false, videoCodec: 'h264', audioCodec: 'aac' };
  }

  const containerOk = !container || NATIVE_VIDEO_CONTAINERS.has(container);
  const videoOk = !vCodec || NATIVE_VIDEO_CODECS.has(vCodec);
  const audioOk = !aCodec || NATIVE_AUDIO_CODECS.has(aCodec);

  return {
    useHls: !(containerOk && videoOk && audioOk),
    videoCodec: 'h264',
    audioCodec: 'aac',
  };
}

export function formatChannelLayout(channels?: number): string | null {
  if (!channels || channels <= 0) return null;
  if (channels === 1) return 'Mono';
  if (channels === 2) return 'Stereo';
  if (channels === 6) return '5.1';
  if (channels === 8) return '7.1';
  return `${channels}ch`;
}

export function formatLanguageCode(code?: string): string {
  const key = norm(code);
  if (!key || key === 'und') return '';
  return LANGUAGE_NAMES[key] ?? code!.toUpperCase();
}

export function formatAudioTrackLabel(track: AudioStreamInfo): string {
  const lang = formatLanguageCode(track.language);
  const cleanTitle = track.title?.trim() || '';
  if (lang && cleanTitle && cleanTitle.toLowerCase() !== lang.toLowerCase()) {
    return `${lang} — ${cleanTitle}`;
  }
  if (cleanTitle) return cleanTitle;
  if (lang) return lang;
  return `Track ${track.audio_index + 1}`;
}
```

- [ ] **Step 5: Run Vitest to confirm all Task 1 tests pass**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/api/client.test.ts src/player/compatibility.test.ts
```
Expected: PASS.

- [ ] **Step 6: Commit Task 1**

```bash
git add web/package.json web/package-lock.json web/src/types/index.ts web/src/api/client.ts web/src/api/client.test.ts web/src/player/compatibility.ts web/src/player/compatibility.test.ts
git commit -m "feat(web): add hls.js dependency, media API client helpers, and compatibility matrix"
```

---

### Task 2: `useHlsPlayback` Hook (Direct Play, Lazy `hls.js`, `timeOffset` Timeline, Cue Sync & Session Cleanup)

**Files:**
- Create: `web/src/player/useHlsPlayback.ts`
- Create: `web/src/player/useHlsPlayback.test.tsx`

**Interfaces:**
- Consumes: `api.getStreamUrl`, `api.getHlsMasterUrl`, `api.deleteHlsSession`, `shouldUseHls`, `TechnicalInfo`.
- Produces:
  - `extractHlsSessionId(url: string): string | null`
  - `syncCueOffsets(video: HTMLVideoElement | null, timeOffset: number): void`
  - `useHlsPlayback(params: UseHlsPlaybackParams): UseHlsPlaybackResult`
    - `params`: `{ itemId: number; videoRef: RefObject<HTMLVideoElement | null>; mediaType?: string; technical?: TechnicalInfo; knownDurationSeconds?: number; initialResumeSeconds?: number | null }`
    - `result`: `{ isUsingHls: boolean; timeOffset: number; activeAudioTrackIndex: number; switchAudioTrack: (audioIndex: number, currentEffectiveTime: number) => void; seekTo: (targetSeconds: number) => void; handleVideoError: (currentEffectiveTime: number) => void; getEffectiveCurrentTime: () => number; getEffectiveDuration: (rawVideoDuration?: number) => number }`

- [ ] **Step 1: Write failing tests in `web/src/player/useHlsPlayback.test.tsx`**

Create `web/src/player/useHlsPlayback.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import { useRef, type FC } from 'react';
import {
  extractHlsSessionId,
  syncCueOffsets,
  useHlsPlayback,
  type UseHlsPlaybackParams,
} from './useHlsPlayback';
import { api } from '../api/client';

const mockHlsInstances: Array<{
  loadedSrc: string | null;
  attachedMedia: HTMLMediaElement | null;
  destroyed: boolean;
  handlers: Record<string, Array<(...args: unknown[]) => void>>;
  trigger: (event: string, data: unknown) => void;
}> = [];

vi.mock('hls.js', () => {
  class MockHls {
    static isSupported = vi.fn(() => true);
    static Events = {
      MANIFEST_PARSED: 'hlsManifestParsed',
      FRAG_LOADING: 'hlsFragLoading',
      LEVEL_LOADED: 'hlsLevelLoaded',
      ERROR: 'hlsError',
    };
    static ErrorTypes = {
      NETWORK_ERROR: 'networkError',
      MEDIA_ERROR: 'mediaError',
    };

    loadedSrc: string | null = null;
    attachedMedia: HTMLMediaElement | null = null;
    destroyed = false;
    handlers: Record<string, Array<(...args: unknown[]) => void>> = {};

    constructor() {
      mockHlsInstances.push(this);
    }

    on(event: string, cb: (...args: unknown[]) => void) {
      this.handlers[event] = this.handlers[event] || [];
      this.handlers[event].push(cb);
    }

    loadSource(src: string) {
      this.loadedSrc = src;
    }

    attachMedia(media: HTMLMediaElement) {
      this.attachedMedia = media;
    }

    destroy() {
      this.destroyed = true;
    }

    trigger(event: string, data: unknown) {
      for (const cb of this.handlers[event] || []) {
        cb(event, data);
      }
    }
  }

  return { default: MockHls };
});

const Harness: FC<Omit<UseHlsPlaybackParams, 'videoRef'>> = (props) => {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const hls = useHlsPlayback({ ...props, videoRef });
  return (
    <div>
      <video ref={videoRef} data-testid="video" />
      <span data-testid="mode">{hls.isUsingHls ? 'hls' : 'direct'}</span>
      <span data-testid="offset">{hls.timeOffset}</span>
      <span data-testid="track">{hls.activeAudioTrackIndex}</span>
      <button
        type="button"
        onClick={() => hls.switchAudioTrack(1, 45)}
        data-testid="switch-track"
      >
        Switch
      </button>
      <button
        type="button"
        onClick={() => hls.seekTo(300)}
        data-testid="seek-unbuffered"
      >
        Seek 300
      </button>
      <button
        type="button"
        onClick={() => hls.handleVideoError(18)}
        data-testid="trigger-error"
      >
        Error
      </button>
    </div>
  );
};

describe('useHlsPlayback', () => {
  beforeEach(() => {
    mockHlsInstances.length = 0;
    vi.restoreAllMocks();
    vi.spyOn(api, 'deleteHlsSession').mockResolvedValue(undefined);
    api.setToken('jwt-test');
  });

  it('extracts session UUIDs from rewritten segment URLs', () => {
    expect(
      extractHlsSessionId(
        '/api/v1/media/42/hls/sessions/e8b6238b-d72b-47e1-85b8-50fa973ff52c/segment_0000.ts?token=abc'
      )
    ).toBe('e8b6238b-d72b-47e1-85b8-50fa973ff52c');
    expect(extractHlsSessionId('/api/v1/stream/42')).toBeNull();
  });

  it('starts in Direct Play for MP4 H264/AAC, switches to HLS on audio track change, and cleans up session on unmount', async () => {
    const { unmount } = render(
      <Harness
        itemId={42}
        mediaType="movie"
        technical={{
          duration_seconds: 600,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        }}
        knownDurationSeconds={600}
      />
    );

    const video = screen.getByTestId('video') as HTMLVideoElement;
    expect(screen.getByTestId('mode').textContent).toBe('direct');
    expect(video.getAttribute('src')).toContain('/api/v1/stream/42');

    await act(async () => {
      screen.getByTestId('switch-track').click();
    });

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(1);
    });
    expect(screen.getByTestId('mode').textContent).toBe('hls');
    expect(screen.getByTestId('offset').textContent).toBe('45');
    expect(mockHlsInstances[0].loadedSrc).toContain('audio_track=1');
    expect(mockHlsInstances[0].loadedSrc).toContain('start_time=45');

    // Simulate segment load carrying session ID
    act(() => {
      mockHlsInstances[0].trigger('hlsFragLoading', {
        frag: {
          url: '/api/v1/media/42/hls/sessions/11111111-2222-3333-4444-555555555555/segment_0000.ts',
        },
      });
    });

    // Seek to unbuffered 300s restarts HLS and deletes previous session
    await act(async () => {
      screen.getByTestId('seek-unbuffered').click();
    });

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(2);
    });
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      42,
      '11111111-2222-3333-4444-555555555555'
    );
    expect(mockHlsInstances[1].loadedSrc).toContain('start_time=300');

    act(() => {
      mockHlsInstances[1].trigger('hlsFragLoading', {
        frag: {
          url: '/api/v1/media/42/hls/sessions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/segment_0000.ts',
        },
      });
    });

    unmount();
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      42,
      'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'
    );
  });

  it('automatically falls back from Direct Play to HLS on <video> error', async () => {
    render(
      <Harness
        itemId={55}
        mediaType="movie"
        technical={{ duration_seconds: 500 }}
        knownDurationSeconds={500}
      />
    );

    expect(screen.getByTestId('mode').textContent).toBe('direct');
    await act(async () => {
      screen.getByTestId('trigger-error').click();
    });

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(1);
    });
    expect(screen.getByTestId('mode').textContent).toBe('hls');
    expect(mockHlsInstances[0].loadedSrc).toContain('start_time=18');
  });

  it('shifts VTTCue startTime and endTime by -timeOffset idempotently', () => {
    const fakeCue = { startTime: 50, endTime: 54 };
    const fakeVideo = {
      textTracks: [{ cues: [fakeCue] }],
    } as unknown as HTMLVideoElement;

    syncCueOffsets(fakeVideo, 40);
    expect(fakeCue.startTime).toBe(10);
    expect(fakeCue.endTime).toBe(14);

    // Calling again with a new offset uses stored original timestamps!
    syncCueOffsets(fakeVideo, 20);
    expect(fakeCue.startTime).toBe(30);
    expect(fakeCue.endTime).toBe(34);
  });
});
```

- [ ] **Step 2: Run Vitest on `useHlsPlayback.test.tsx` to confirm failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/player/useHlsPlayback.test.tsx
```
Expected: FAIL (`./useHlsPlayback` cannot be resolved).

- [ ] **Step 3: Implement `web/src/player/useHlsPlayback.ts`**

Create `web/src/player/useHlsPlayback.ts`:
```ts
import { useState, useEffect, useRef, useCallback, type RefObject } from 'react';
import type Hls from 'hls.js';
import { api } from '../api/client';
import type { TechnicalInfo } from '../types';
import { shouldUseHls } from './compatibility';

export interface UseHlsPlaybackParams {
  itemId: number;
  videoRef: RefObject<HTMLVideoElement | null>;
  mediaType?: string;
  technical?: TechnicalInfo;
  knownDurationSeconds?: number;
  initialResumeSeconds?: number | null;
  defaultAudioTrackIndex?: number;
}

export interface UseHlsPlaybackResult {
  isUsingHls: boolean;
  timeOffset: number;
  activeAudioTrackIndex: number;
  switchAudioTrack: (audioIndex: number, currentEffectiveTime: number) => void;
  seekTo: (targetSeconds: number) => void;
  handleVideoError: (currentEffectiveTime: number) => void;
  getEffectiveCurrentTime: () => number;
  getEffectiveDuration: (rawVideoDuration?: number) => number;
}

interface OffsetAnnotatedCue {
  startTime: number;
  endTime: number;
  __kadrOrigStart?: number;
  __kadrOrigEnd?: number;
}

const HLS_SESSION_RE = /\/hls\/sessions\/([0-9a-fA-F-]{36})\//;

export function extractHlsSessionId(url: string): string | null {
  const m = HLS_SESSION_RE.exec(url);
  return m ? m[1] : null;
}

export function syncCueOffsets(
  video: HTMLVideoElement | null,
  timeOffset: number
): void {
  if (!video || !video.textTracks) return;
  const tracks = video.textTracks;
  for (let i = 0; i < tracks.length; i++) {
    const cues = tracks[i]?.cues;
    if (!cues) continue;
    for (let j = 0; j < cues.length; j++) {
      const cue = cues[j] as unknown as OffsetAnnotatedCue;
      if (cue.__kadrOrigStart === undefined) {
        cue.__kadrOrigStart = cue.startTime;
      }
      if (cue.__kadrOrigEnd === undefined) {
        cue.__kadrOrigEnd = cue.endTime;
      }
      if (timeOffset > 0) {
        cue.startTime = Math.max(0, cue.__kadrOrigStart - timeOffset);
        cue.endTime = Math.max(0, cue.__kadrOrigEnd - timeOffset);
      } else {
        cue.startTime = cue.__kadrOrigStart;
        cue.endTime = cue.__kadrOrigEnd;
      }
    }
  }
}

function isWithinBuffered(
  video: HTMLVideoElement,
  relativeSeconds: number
): boolean {
  if (relativeSeconds < 0) return false;
  const buffered = video.buffered;
  if (!buffered) return false;
  for (let i = 0; i < buffered.length; i++) {
    if (
      relativeSeconds >= buffered.start(i) - 0.5 &&
      relativeSeconds <= buffered.end(i) + 0.5
    ) {
      return true;
    }
  }
  return false;
}

export function useHlsPlayback({
  itemId,
  videoRef,
  mediaType,
  technical,
  knownDurationSeconds,
  initialResumeSeconds,
  defaultAudioTrackIndex = 0,
}: UseHlsPlaybackParams): UseHlsPlaybackResult {
  const [activeAudioTrackIndex, setActiveAudioTrackIndex] = useState<number>(
    defaultAudioTrackIndex
  );
  const [forceHls, setForceHls] = useState(false);
  const [hlsStartTime, setHlsStartTime] = useState(0);
  const [hlsReloadNonce, setHlsReloadNonce] = useState(0);

  const hlsInstanceRef = useRef<Hls | null>(null);
  const activeHlsSessionRef = useRef<{ itemId: number; sessionId: string } | null>(
    null
  );
  const hasAppliedDefaultTrackRef = useRef(false);
  const hasAppliedInitialResumeRef = useRef(false);

  // Reset per-item state when itemId changes
  useEffect(() => {
    hasAppliedDefaultTrackRef.current = false;
    hasAppliedInitialResumeRef.current = false;
    setActiveAudioTrackIndex(0);
    setForceHls(false);
    setHlsStartTime(0);
    setHlsReloadNonce(0);
  }, [itemId]);

  // Pick up non-zero default audio track once discovered
  useEffect(() => {
    if (!hasAppliedDefaultTrackRef.current && defaultAudioTrackIndex > 0) {
      hasAppliedDefaultTrackRef.current = true;
      setActiveAudioTrackIndex(defaultAudioTrackIndex);
    }
  }, [defaultAudioTrackIndex]);

  const decision = shouldUseHls({
    mediaType,
    technical,
    audioTrackIndex: activeAudioTrackIndex,
    forceHls,
  });

  // Apply initial resume position to HLS start_time before first stream start if HLS is active
  useEffect(() => {
    if (
      decision.useHls &&
      !hasAppliedInitialResumeRef.current &&
      initialResumeSeconds &&
      initialResumeSeconds > 0 &&
      hlsStartTime === 0
    ) {
      hasAppliedInitialResumeRef.current = true;
      setHlsStartTime(Math.floor(initialResumeSeconds));
    }
  }, [decision.useHls, initialResumeSeconds, hlsStartTime]);

  const timeOffset = decision.useHls ? hlsStartTime : 0;

  const teardownActiveHls = useCallback(() => {
    if (hlsInstanceRef.current) {
      hlsInstanceRef.current.destroy();
      hlsInstanceRef.current = null;
    }
    if (activeHlsSessionRef.current) {
      const { itemId: prevItemId, sessionId } = activeHlsSessionRef.current;
      activeHlsSessionRef.current = null;
      api.deleteHlsSession(prevItemId, sessionId).catch(() => {});
    }
  }, []);

  // Attach either Direct Play src or HLS pipeline
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    let cancelled = false;
    teardownActiveHls();

    if (!decision.useHls) {
      const directUrl = api.getStreamUrl(itemId);
      if (video.getAttribute('src') !== directUrl) {
        video.src = directUrl;
      }
      return () => {
        cancelled = true;
      };
    }

    const masterUrl = api.getHlsMasterUrl(itemId, {
      videoCodec: decision.videoCodec,
      audioCodec: decision.audioCodec,
      startTime: hlsStartTime,
      audioTrack: activeAudioTrackIndex,
    });

    (async () => {
      try {
        const mod = await import('hls.js');
        if (cancelled) return;
        const HlsCtor = mod.default;

        if (HlsCtor.isSupported()) {
          video.removeAttribute('src');
          const hls = new HlsCtor({
            enableWorker: true,
            backBufferLength: 60,
          });
          hlsInstanceRef.current = hls;

          const recordSessionFromUrl = (candidate?: string) => {
            if (!candidate) return;
            const sid = extractHlsSessionId(candidate);
            if (sid) {
              activeHlsSessionRef.current = { itemId, sessionId: sid };
            }
          };

          hls.on(HlsCtor.Events.FRAG_LOADING, (_evt, data: { frag?: { url?: string } }) => {
            recordSessionFromUrl(data?.frag?.url);
          });

          hls.on(
            HlsCtor.Events.LEVEL_LOADED,
            (_evt, data: { details?: { fragments?: Array<{ url?: string }> } }) => {
              recordSessionFromUrl(data?.details?.fragments?.[0]?.url);
            }
          );

          hls.on(HlsCtor.Events.MANIFEST_PARSED, () => {
            if (cancelled) return;
            try {
              const p = video.play();
              if (p && typeof p.catch === 'function') p.catch(() => {});
            } catch {
              // Ignore autoplay block in headless/restricted browsers
            }
          });

          hls.loadSource(masterUrl);
          hls.attachMedia(video);
          return;
        }

        // Native Safari HLS fallback
        if (video.canPlayType('application/vnd.apple.mpegurl')) {
          try {
            const resp = await fetch(masterUrl);
            if (!cancelled && resp.ok) {
              const playlistText = await resp.text();
              const sid = extractHlsSessionId(playlistText);
              if (sid) {
                activeHlsSessionRef.current = { itemId, sessionId: sid };
              }
            }
          } catch {
            // Best-effort session extraction for Safari
          }
          if (!cancelled) {
            video.src = masterUrl;
          }
        }
      } catch {
        // Ignore dynamic import cancellation
      }
    })();

    return () => {
      cancelled = true;
      teardownActiveHls();
    };
  }, [
    itemId,
    videoRef,
    decision.useHls,
    decision.videoCodec,
    decision.audioCodec,
    hlsStartTime,
    activeAudioTrackIndex,
    hlsReloadNonce,
    teardownActiveHls,
  ]);

  // Keep WebVTT cues shifted by -timeOffset
  useEffect(() => {
    syncCueOffsets(videoRef.current, timeOffset);
  }, [videoRef, timeOffset]);

  const getEffectiveCurrentTime = useCallback((): number => {
    const raw = videoRef.current?.currentTime ?? 0;
    return timeOffset + (Number.isFinite(raw) ? raw : 0);
  }, [videoRef, timeOffset]);

  const getEffectiveDuration = useCallback(
    (rawVideoDuration?: number): number => {
      if (knownDurationSeconds && knownDurationSeconds > 0) {
        return knownDurationSeconds;
      }
      const raw = rawVideoDuration ?? videoRef.current?.duration ?? 0;
      if (Number.isFinite(raw) && raw > 0) {
        return decision.useHls ? timeOffset + raw : raw;
      }
      return 0;
    },
    [knownDurationSeconds, videoRef, decision.useHls, timeOffset]
  );

  const switchAudioTrack = useCallback(
    (audioIndex: number, currentEffectiveTime: number) => {
      hasAppliedDefaultTrackRef.current = true;
      hasAppliedInitialResumeRef.current = true;
      const safeStart = Math.max(0, Math.floor(currentEffectiveTime));
      setActiveAudioTrackIndex(audioIndex);
      if (audioIndex > 0 || forceHls || decision.useHls) {
        setHlsStartTime(safeStart);
      } else {
        setHlsStartTime(0);
      }
    },
    [forceHls, decision.useHls]
  );

  const seekTo = useCallback(
    (targetSeconds: number) => {
      const clamped = Math.max(0, targetSeconds);
      const video = videoRef.current;
      if (!decision.useHls) {
        if (video) {
          video.currentTime = clamped;
        }
        return;
      }

      if (video) {
        const relative = clamped - timeOffset;
        if (isWithinBuffered(video, relative)) {
          video.currentTime = Math.max(0, relative);
          return;
        }
      }

      hasAppliedInitialResumeRef.current = true;
      const nextStart = Math.floor(clamped);
      if (nextStart === hlsStartTime) {
        setHlsReloadNonce((n) => n + 1);
      } else {
        setHlsStartTime(nextStart);
      }
    },
    [decision.useHls, timeOffset, hlsStartTime, videoRef]
  );

  const handleVideoError = useCallback(
    (currentEffectiveTime: number) => {
      if (!decision.useHls && !forceHls) {
        hasAppliedInitialResumeRef.current = true;
        setHlsStartTime(Math.max(0, Math.floor(currentEffectiveTime)));
        setForceHls(true);
      }
    },
    [decision.useHls, forceHls]
  );

  return {
    isUsingHls: decision.useHls,
    timeOffset,
    activeAudioTrackIndex,
    switchAudioTrack,
    seekTo,
    handleVideoError,
    getEffectiveCurrentTime,
    getEffectiveDuration,
  };
}
```

- [ ] **Step 4: Run Vitest on `useHlsPlayback.test.tsx` to verify all tests pass**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/player/useHlsPlayback.test.tsx
```
Expected: PASS.

- [ ] **Step 5: Commit Task 2**

```bash
git add web/src/player/useHlsPlayback.ts web/src/player/useHlsPlayback.test.tsx
git commit -m "feat(web): add useHlsPlayback hook with dynamic hls.js, timeline offset, and cue sync"
```

---

### Task 3: Storyboard Seek-Bar Preview, Audio Track Radio-Menu & `CinemaPlayer` Integration

**Files:**
- Create: `web/src/player/useStoryboard.ts`
- Create: `web/src/components/player/SeekPreviewThumbnail.tsx`
- Create: `web/src/components/player/SeekPreviewThumbnail.test.tsx`
- Modify: `web/src/components/player/PlayerControls.tsx`
- Modify: `web/src/components/player/PlayerControls.test.tsx`
- Modify: `web/src/components/player/CinemaPlayer.tsx`
- Modify: `web/src/components/player/CinemaPlayer.test.tsx`

**Interfaces:**
- Consumes: `useHlsPlayback`, `isAudioOnlyMedia`, `formatAudioTrackLabel`, `formatChannelLayout`, `api.getAudioTracks`, `api.getStoryboard`, `api.getStoryboardSheetUrl`, `api.getFramePreviewUrl`.
- Produces:
  - `computeStoryboardTile(manifest: StoryboardManifest, seconds: number, displayWidth?: number): StoryboardTileSlice | null`
  - `useStoryboard(params: { itemId: number; mediaType?: string; technical?: TechnicalInfo; durationSeconds: number }): UseStoryboardResult`
  - `<SeekPreviewThumbnail />` (`web/src/components/player/SeekPreviewThumbnail.tsx`)
  - Extended `PlayerControlsProps`: `audioTracks?: AudioStreamInfo[]`, `activeAudioTrackIndex?: number`, `onSelectAudioTrack?: (audioIndex: number) => void`, `onAudioMenuToggle?: (isOpen: boolean) => void`, `storyboard?: UseStoryboardResult`, `itemId?: number`.

- [ ] **Step 1: Write failing tests in `SeekPreviewThumbnail.test.tsx`, `PlayerControls.test.tsx`, and `CinemaPlayer.test.tsx`**

Create `web/src/components/player/SeekPreviewThumbnail.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import { computeStoryboardTile, useStoryboard } from '../../player/useStoryboard';
import { SeekPreviewThumbnail } from './SeekPreviewThumbnail';
import { api } from '../../api/client';
import type { StoryboardManifest } from '../../types';

const sampleManifest: StoryboardManifest = {
  interval_ms: 10000,
  tile_width: 320,
  tile_height: 180,
  columns: 10,
  rows: 10,
  total_tiles: 125,
  sheets: [
    { index: 0, url: '/api/v1/media/10/storyboard/0.jpg', first_tile: 0, tile_count: 100 },
    { index: 1, url: '/api/v1/media/10/storyboard/1.jpg', first_tile: 100, tile_count: 25 },
  ],
};

describe('Storyboard math & SeekPreviewThumbnail', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    api.setToken('jwt-sb');
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('computes scaled CSS sprite sheet coordinates across multi-sheet boundaries', () => {
    // At 1015s -> tileIndex = floor(1015000 / 10000) = 101 -> sheet 1, localIndex 1 -> row 0, col 1
    // Scaled from 320x180 to displayWidth 160 -> height 90
    const slice = computeStoryboardTile(sampleManifest, 1015, 160);
    expect(slice).toEqual({
      sheetUrl: '/api/v1/media/10/storyboard/1.jpg?token=jwt-sb',
      width: 160,
      height: 90,
      backgroundPosition: '-160px -0px',
      backgroundSize: '1600px 900px',
    });
  });

  it('polls 202 generating until 200 ready in useStoryboard', async () => {
    const spy = vi
      .spyOn(api, 'getStoryboard')
      .mockResolvedValueOnce({ status: 'generating', retryAfterMs: 5000 })
      .mockResolvedValueOnce({ status: 'ready', manifest: sampleManifest });

    const Probe = () => {
      const sb = useStoryboard({
        itemId: 10,
        mediaType: 'movie',
        durationSeconds: 1250,
      });
      return <div data-testid="sb-status">{sb.status}</div>;
    };

    render(<Probe />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByTestId('sb-status').textContent).toBe('generating');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(spy).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('sb-status').textContent).toBe('ready');
  });

  it('renders sprite tile when ready and debounced /frame fallback when generating', async () => {
    const { rerender } = render(
      <SeekPreviewThumbnail
        itemId={10}
        seconds={25}
        percent={50}
        status="ready"
        manifest={sampleManifest}
      />
    );
    const sprite = screen.getByTestId('seek-preview-sprite');
    expect(sprite.style.backgroundImage).toContain('/api/v1/media/10/storyboard/0.jpg');
    expect(screen.getByText('00:25')).toBeInTheDocument();

    rerender(
      <SeekPreviewThumbnail
        itemId={10}
        seconds={33}
        percent={50}
        status="generating"
        manifest={null}
      />
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    const img = screen.getByTestId('seek-preview-frame') as HTMLImageElement;
    expect(img.src).toContain('/api/v1/media/10/frame?t=32&w=320');
  });
});
```

Add Audio Tracks & seek preview tests to `web/src/components/player/PlayerControls.test.tsx`:
```tsx
  it('renders Audio Tracks menu with role="menuitemradio", aria-checked, and Escape focus restore', () => {
    const onSelectAudioTrack = vi.fn();
    render(
      <PlayerControls
        {...defaultProps}
        audioTracks={[
          {
            audio_index: 0,
            stream_index: 1,
            codec: 'aac',
            channels: 2,
            language: 'eng',
            title: 'English Stereo',
            is_default: true,
            is_commentary: false,
          },
          {
            audio_index: 1,
            stream_index: 2,
            codec: 'eac3',
            channels: 6,
            language: 'jpn',
            title: 'Japanese Original',
            is_default: false,
            is_commentary: false,
          },
        ]}
        activeAudioTrackIndex={0}
        onSelectAudioTrack={onSelectAudioTrack}
      />
    );

    const trigger = screen.getByRole('button', { name: /audio tracks/i });
    expect(trigger).toHaveAttribute('aria-haspopup', 'menu');
    expect(trigger).toHaveAttribute('aria-expanded', 'false');

    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');

    const radios = screen.getAllByRole('menuitemradio');
    expect(radios).toHaveLength(2);
    expect(radios[0]).toHaveAttribute('aria-checked', 'true');
    expect(radios[1]).toHaveAttribute('aria-checked', 'false');

    fireEvent.keyDown(screen.getByRole('menu', { name: /audio tracks/i }), {
      key: 'Escape',
    });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(document.activeElement).toBe(trigger);

    fireEvent.click(trigger);
    fireEvent.click(screen.getAllByRole('menuitemradio')[1]);
    expect(onSelectAudioTrack).toHaveBeenCalledWith(1);
  });
```

Add an integration test to `web/src/components/player/CinemaPlayer.test.tsx` verifying `api.getAudioTracks(itemId)` and `api.getStoryboard(itemId)` are wired cleanly (and add default mocks `vi.spyOn(api, 'getAudioTracks').mockResolvedValue([])` + `vi.spyOn(api, 'getStoryboard').mockResolvedValue({ status: 'unavailable' })` in `beforeEach` so all existing `CinemaPlayer` tests run cleanly).

- [ ] **Step 2: Run Vitest on the three player test files to confirm failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/components/player/SeekPreviewThumbnail.test.tsx src/components/player/PlayerControls.test.tsx src/components/player/CinemaPlayer.test.tsx
```
Expected: FAIL (`useStoryboard` / `SeekPreviewThumbnail` missing; `Audio Tracks` button missing).

- [ ] **Step 3: Implement `web/src/player/useStoryboard.ts` and `web/src/components/player/SeekPreviewThumbnail.tsx`**

Create `web/src/player/useStoryboard.ts`:
```ts
import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import type { StoryboardManifest, TechnicalInfo } from '../types';
import { isAudioOnlyMedia } from './compatibility';

export type StoryboardUiStatus = 'idle' | 'generating' | 'ready' | 'unavailable';

export interface StoryboardTileSlice {
  sheetUrl: string;
  width: number;
  height: number;
  backgroundPosition: string;
  backgroundSize: string;
}

export interface UseStoryboardResult {
  status: StoryboardUiStatus;
  manifest: StoryboardManifest | null;
  getTileAtSeconds: (
    seconds: number,
    displayWidth?: number
  ) => StoryboardTileSlice | null;
}

const MAX_STORYBOARD_POLLS = 12;

export function computeStoryboardTile(
  manifest: StoryboardManifest,
  seconds: number,
  displayWidth: number = 160
): StoryboardTileSlice | null {
  if (
    !manifest ||
    manifest.total_tiles <= 0 ||
    manifest.interval_ms <= 0 ||
    manifest.tile_width <= 0 ||
    manifest.tile_height <= 0 ||
    manifest.sheets.length === 0
  ) {
    return null;
  }

  const clampedSecs = Math.max(0, seconds);
  const tileIndex = Math.min(
    manifest.total_tiles - 1,
    Math.max(0, Math.floor((clampedSecs * 1000) / manifest.interval_ms))
  );

  const sheet =
    manifest.sheets.find(
      (s) => tileIndex >= s.first_tile && tileIndex < s.first_tile + s.tile_count
    ) ?? manifest.sheets[manifest.sheets.length - 1];

  if (!sheet) return null;

  const scale = displayWidth / manifest.tile_width;
  const displayHeight = Math.max(1, Math.round(manifest.tile_height * scale));
  const localIndex = Math.max(0, tileIndex - sheet.first_tile);
  const col = localIndex % manifest.columns;
  const row = Math.floor(localIndex / manifest.columns);

  return {
    sheetUrl: api.getStoryboardSheetUrl(sheet.url),
    width: displayWidth,
    height: displayHeight,
    backgroundPosition: `-${col * displayWidth}px -${row * displayHeight}px`,
    backgroundSize: `${manifest.columns * displayWidth}px ${manifest.rows * displayHeight}px`,
  };
}

export function useStoryboard({
  itemId,
  mediaType,
  technical,
  durationSeconds,
}: {
  itemId: number;
  mediaType?: string;
  technical?: TechnicalInfo;
  durationSeconds: number;
}): UseStoryboardResult {
  const [status, setStatus] = useState<StoryboardUiStatus>('idle');
  const [manifest, setManifest] = useState<StoryboardManifest | null>(null);

  const ineligible =
    mediaType === 'live_channel' ||
    mediaType === 'show' ||
    mediaType === 'season' ||
    isAudioOnlyMedia(mediaType, technical) ||
    durationSeconds <= 0;

  useEffect(() => {
    if (ineligible) {
      setStatus('unavailable');
      setManifest(null);
      return;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const abortController = new AbortController();
    let attempts = 0;

    setStatus('idle');
    setManifest(null);

    const poll = async () => {
      try {
        const res = await api.getStoryboard(itemId, abortController.signal);
        if (cancelled) return;
        if (res.status === 'ready') {
          setManifest(res.manifest);
          setStatus('ready');
          return;
        }
        if (res.status === 'generating' && attempts < MAX_STORYBOARD_POLLS) {
          attempts += 1;
          setStatus('generating');
          timer = setTimeout(poll, res.retryAfterMs);
          return;
        }
        setStatus('unavailable');
      } catch {
        if (!cancelled) {
          setStatus('unavailable');
        }
      }
    };

    void poll();

    return () => {
      cancelled = true;
      abortController.abort();
      if (timer) clearTimeout(timer);
    };
  }, [itemId, ineligible]);

  const getTileAtSeconds = useCallback(
    (seconds: number, displayWidth: number = 160): StoryboardTileSlice | null => {
      if (!manifest) return null;
      return computeStoryboardTile(manifest, seconds, displayWidth);
    },
    [manifest]
  );

  return { status, manifest, getTileAtSeconds };
}
```

Create `web/src/components/player/SeekPreviewThumbnail.tsx`:
```tsx
import { useState, useEffect, type FC } from 'react';
import { api } from '../../api/client';
import type { StoryboardManifest } from '../../types';
import {
  computeStoryboardTile,
  type StoryboardUiStatus,
} from '../../player/useStoryboard';
import { formatPlaybackTime } from './PlayerControls';

export interface SeekPreviewThumbnailProps {
  itemId?: number;
  seconds: number;
  percent: number;
  status: StoryboardUiStatus;
  manifest: StoryboardManifest | null;
}

export const SeekPreviewThumbnail: FC<SeekPreviewThumbnailProps> = ({
  itemId,
  seconds,
  percent,
  status,
  manifest,
}) => {
  const [debouncedFrameUrl, setDebouncedFrameUrl] = useState<string | null>(null);
  const [frameFailed, setFrameFailed] = useState(false);

  const clampedPercent = Math.min(92, Math.max(8, percent));
  const tile =
    status === 'ready' && manifest
      ? computeStoryboardTile(manifest, seconds, 160)
      : null;

  useEffect(() => {
    if (status !== 'generating' || !itemId) {
      setDebouncedFrameUrl(null);
      setFrameFailed(false);
      return;
    }
    const nextUrl = api.getFramePreviewUrl(itemId, seconds, 320);
    const timer = setTimeout(() => {
      setFrameFailed(false);
      setDebouncedFrameUrl(nextUrl);
    }, 120);
    return () => clearTimeout(timer);
  }, [status, itemId, seconds]);

  return (
    <div
      aria-hidden="true"
      style={{ left: `${clampedPercent}%` }}
      className="absolute bottom-full mb-3 -translate-x-1/2 pointer-events-none z-40 flex flex-col items-center"
    >
      {tile ? (
        <div className="rounded-xl overflow-hidden border border-border-subtle bg-canvas shadow-2xl">
          <div
            data-testid="seek-preview-sprite"
            style={{
              width: `${tile.width}px`,
              height: `${tile.height}px`,
              backgroundImage: `url("${tile.sheetUrl}")`,
              backgroundPosition: tile.backgroundPosition,
              backgroundSize: tile.backgroundSize,
              backgroundRepeat: 'no-repeat',
            }}
          />
          <div className="px-2 py-1 text-center font-mono text-[11px] font-semibold text-text-main bg-panel/95 border-t border-border-subtle">
            {formatPlaybackTime(seconds)}
          </div>
        </div>
      ) : debouncedFrameUrl && !frameFailed ? (
        <div className="rounded-xl overflow-hidden border border-border-subtle bg-canvas shadow-2xl w-40">
          <img
            data-testid="seek-preview-frame"
            src={debouncedFrameUrl}
            alt=""
            onError={() => setFrameFailed(true)}
            className="w-40 h-[90px] object-cover block"
          />
          <div className="px-2 py-1 text-center font-mono text-[11px] font-semibold text-text-main bg-panel/95 border-t border-border-subtle">
            {formatPlaybackTime(seconds)}
          </div>
        </div>
      ) : (
        <div className="px-2.5 py-1 rounded-lg bg-panel/95 border border-border-subtle shadow-xl font-mono text-xs font-semibold text-text-main">
          {formatPlaybackTime(seconds)}
        </div>
      )}
    </div>
  );
};
```

- [ ] **Step 4: Update `PlayerControls.tsx` and `CinemaPlayer.tsx`**

In `web/src/components/player/PlayerControls.tsx`:
- Add optional props `itemId?: number`, `audioTracks?: AudioStreamInfo[]`, `activeAudioTrackIndex?: number`, `onSelectAudioTrack?: (audioIndex: number) => void`, `onAudioMenuToggle?: (isOpen: boolean) => void`, and `storyboard?: { status: StoryboardUiStatus; manifest: StoryboardManifest | null }`.
- Track `hoverPreview: { seconds: number; percent: number } | null` on the seek-bar container via `onPointerMove` (computing `(e.clientX - rect.left) / rect.width` clamped `0..1` and `seconds = ratio * duration`) and clear on `onPointerLeave`. Render `<SeekPreviewThumbnail />` when `hoverPreview && duration > 0`.
- Add the **Audio Tracks** button (`Headphones` icon from `lucide-react`, `type="button"`, `aria-label="Audio Tracks"`, `aria-haspopup="menu"`, `aria-expanded={isAudioMenuOpen}`) before the Subtitles button whenever `audioTracks && audioTracks.length > 0`.
- Render `<div role="menu" aria-label="Audio Tracks" onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); closeAudioMenuAndRestoreFocus(); } }}>` containing `<button type="button" role="menuitemradio" aria-checked={isSelected}>` rows with `formatAudioTrackLabel(track)` plus technical badges (`track.codec.toUpperCase()`, `formatChannelLayout(track.channels)`, `Commentary` if `track.is_commentary`). Opening Audio Tracks closes Queue and Subtitles menus mutually exclusively (and vice versa).

In `web/src/components/player/CinemaPlayer.tsx`:
- Fetch `api.getAudioTracks(itemId)` alongside `getSubtitles` & `getItemDetails` (falling back to `details.technical?.audio_tracks ?? []`), storing `audioTracks` state and computing `defaultAudioTrackIndex = audioTracks.find((t) => t.is_default)?.audio_index ?? 0`.
- Instantiate `useHlsPlayback` with `{ itemId, videoRef, mediaType: itemDetails?.card?.media_type, technical: itemDetails?.technical, knownDurationSeconds: itemDetails?.duration_seconds || itemDetails?.technical?.duration_seconds, initialResumeSeconds: resumePosition, defaultAudioTrackIndex }`.
- Instantiate `useStoryboard` with `{ itemId, mediaType: itemDetails?.card?.media_type, technical: itemDetails?.technical, durationSeconds: duration }`.
- Update `handleLoadedMetadata`, `handleTimeUpdate`, `handleSeek`, `handleVideoEnded`, `ArrowLeft`/`ArrowRight`, `Start Over`, and the 10s `sendPlaybackHeartbeat` interval to use `getEffectiveCurrentTime()`, `getEffectiveDuration(...)`, `seekTo(...)`, `handleVideoError(getEffectiveCurrentTime())` on `<video onError={...} />`, and call `syncCueOffsets(videoRef.current, timeOffset)` when `<track>` `onLoad` fires or `activeSubtitleId` changes.
- Keep controls visible while `isAudioMenuOpen` is open (matching `isSubtitlesMenuOpen` and `isQueueMenuOpen`), and prevent `CinemaPlayer`'s global `Escape` listener from exiting the player if a popover already handled `Escape` (`e.defaultPrevented`).

- [ ] **Step 5: Run Vitest on all player tests**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/components/player/SeekPreviewThumbnail.test.tsx src/components/player/PlayerControls.test.tsx src/components/player/CinemaPlayer.test.tsx
```
Expected: PASS.

- [ ] **Step 6: Commit Task 3**

```bash
git add web/src/player/useStoryboard.ts web/src/components/player/SeekPreviewThumbnail.tsx web/src/components/player/SeekPreviewThumbnail.test.tsx web/src/components/player/PlayerControls.tsx web/src/components/player/PlayerControls.test.tsx web/src/components/player/CinemaPlayer.tsx web/src/components/player/CinemaPlayer.test.tsx
git commit -m "feat(web): wire storyboard seek previews, multi-audio track menu, and HLS playback into CinemaPlayer"
```

---

### Task 4: Single & Batch Mark Watched / Unwatched Across Cards, `ItemDetailsModal` & `BrowseScreen`

**Files:**
- Create: `web/src/utils/watchStateEvents.ts`
- Create: `web/src/components/browse/WatchStateButton.tsx`
- Create: `web/src/components/browse/WatchStateButton.test.tsx`
- Modify: `web/src/components/browse/CarouselWidget.tsx`
- Modify: `web/src/components/browse/GridWidget.tsx`
- Modify: `web/src/components/browse/BrowseScreen.tsx`
- Modify: `web/src/components/browse/ItemDetailsModal.tsx`
- Modify: `web/src/components/browse/ItemDetailsModal.test.tsx`

**Interfaces:**
- Consumes: `api.setPlaybackState`, `api.setPlaybackStatesBatch`, `formatLanguageCode`, `formatChannelLayout`, `CardViewModel`, `ItemDetailsPayload`.
- Produces:
  - `isCardWatched(card?: Pick<CardViewModel, 'playback_progress'> | null): boolean`
  - `applyWatchStateToCard(card: CardViewModel, watched: boolean): CardViewModel`
  - `emitWatchStateChanged(itemIds: number[], watched: boolean): void`
  - `onWatchStateChanged(handler: (detail: { itemIds: Set<number>; watched: boolean }) => void): () => void`
  - `<WatchStateButton />` (`web/src/components/browse/WatchStateButton.tsx`)

- [ ] **Step 1: Write failing tests in `WatchStateButton.test.tsx` and `ItemDetailsModal.test.tsx`**

Create `web/src/components/browse/WatchStateButton.test.tsx`:
```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WatchStateButton } from './WatchStateButton';
import { api } from '../../api/client';

describe('WatchStateButton', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('hides completely for live_channel items', () => {
    const { container } = render(
      <WatchStateButton
        itemId={10}
        title="Live News"
        mediaType="live_channel"
        watched={false}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it('toggles watched state optimistically, stops card click propagation, and disambiguates aria-label', async () => {
    const setSpy = vi.spyOn(api, 'setPlaybackState').mockResolvedValue({
      user_id: 'u1',
      media_item_id: 42,
      playback_position_seconds: 0,
      watch_state: 'completed',
      last_watched_at: 1000,
      play_count: 1,
    });
    const parentClick = vi.fn();
    const onToggled = vi.fn();

    render(
      <div onClick={parentClick}>
        <WatchStateButton
          itemId={42}
          title="Inception"
          mediaType="movie"
          watched={false}
          variant="icon"
          onToggled={onToggled}
        />
      </div>
    );

    const btn = screen.getByRole('button', { name: 'Mark Inception as watched' });
    expect(btn).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(btn);
    expect(parentClick).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(setSpy).toHaveBeenCalledWith(42, true);
    });
    expect(onToggled).toHaveBeenCalledWith(true);
  });

  it('rolls back optimistic state when API call fails', async () => {
    vi.spyOn(api, 'setPlaybackState').mockRejectedValue(new Error('network error'));
    const onToggled = vi.fn();

    render(
      <WatchStateButton
        itemId={42}
        title="Inception"
        mediaType="movie"
        watched={false}
        onToggled={onToggled}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Mark Inception as watched' }));
    await waitFor(() => {
      expect(onToggled).toHaveBeenLastCalledWith(false);
    });
  });
});
```

Append tests to `web/src/components/browse/ItemDetailsModal.test.tsx` testing:
1. Single movie `Mark Watched` button calling `api.setPlaybackState(itemId, true)` and flipping to `Watched`.
2. TV Show `Mark Season Watched` and `Mark Series Watched` calling `api.setPlaybackStatesBatch` with the exact episode IDs for the selected season vs all seasons, plus per-episode row toggle calling `api.setPlaybackState(episodeId, true)`.
3. Multiple audio streams on `details.technical.audio_tracks` rendering technical audio stream badges (e.g., `English • AAC Stereo`, `Japanese • EAC3 5.1`).

- [ ] **Step 2: Run Vitest on `WatchStateButton.test.tsx` and `ItemDetailsModal.test.tsx` to confirm failure**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/components/browse/WatchStateButton.test.tsx src/components/browse/ItemDetailsModal.test.tsx
```
Expected: FAIL (`WatchStateButton` module not found; `Mark Season Watched` / `Mark Series Watched` not rendered).

- [ ] **Step 3: Implement `watchStateEvents.ts` and `WatchStateButton.tsx`**

Create `web/src/utils/watchStateEvents.ts`:
```ts
import type { CardViewModel } from '../types';

const WATCH_STATE_EVENT = 'kadr:watch-state-changed';

export interface WatchStateEventPayload {
  itemIds: number[];
  watched: boolean;
}

export function isCardWatched(
  card?: Pick<CardViewModel, 'playback_progress'> | null
): boolean {
  return (
    typeof card?.playback_progress === 'number' && card.playback_progress >= 1
  );
}

export function applyWatchStateToCard(
  card: CardViewModel,
  watched: boolean
): CardViewModel {
  const nextBadge =
    card.badge === 'RESUME' ? undefined : card.badge;
  return {
    ...card,
    playback_progress: watched ? 1 : undefined,
    badge: nextBadge,
  };
}

export function emitWatchStateChanged(
  itemIds: number[],
  watched: boolean
): void {
  if (typeof window === 'undefined' || itemIds.length === 0) return;
  window.dispatchEvent(
    new CustomEvent<WatchStateEventPayload>(WATCH_STATE_EVENT, {
      detail: { itemIds, watched },
    })
  );
}

export function onWatchStateChanged(
  handler: (detail: { itemIds: Set<number>; watched: boolean }) => void
): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (evt: Event) => {
    const custom = evt as CustomEvent<WatchStateEventPayload>;
    if (!custom.detail) return;
    handler({
      itemIds: new Set(custom.detail.itemIds),
      watched: custom.detail.watched,
    });
  };
  window.addEventListener(WATCH_STATE_EVENT, listener);
  return () => window.removeEventListener(WATCH_STATE_EVENT, listener);
}
```

Create `web/src/components/browse/WatchStateButton.tsx`:
```tsx
import { useState, useEffect, type FC, type MouseEvent, type KeyboardEvent } from 'react';
import { Check, CheckCircle2, Loader2 } from 'lucide-react';
import { api } from '../../api/client';
import { emitWatchStateChanged } from '../../utils/watchStateEvents';

export interface WatchStateButtonProps {
  itemId: number;
  title: string;
  mediaType?: string;
  watched: boolean;
  variant?: 'icon' | 'pill';
  onToggled?: (watched: boolean) => void;
}

export const WatchStateButton: FC<WatchStateButtonProps> = ({
  itemId,
  title,
  mediaType,
  watched: propWatched,
  variant = 'pill',
  onToggled,
}) => {
  const [optimisticWatched, setOptimisticWatched] = useState(propWatched);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setOptimisticWatched(propWatched);
  }, [propWatched]);

  if (mediaType === 'live_channel') {
    return null;
  }

  const label = optimisticWatched
    ? `Mark ${title} as unwatched`
    : `Mark ${title} as watched`;

  const handleToggle = async (e: MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (busy) return;
    const prev = optimisticWatched;
    const next = !prev;
    setOptimisticWatched(next);
    onToggled?.(next);
    setBusy(true);
    try {
      await api.setPlaybackState(itemId, next);
      emitWatchStateChanged([itemId], next);
    } catch {
      setOptimisticWatched(prev);
      onToggled?.(prev);
    } finally {
      setBusy(false);
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.stopPropagation();
    }
  };

  if (variant === 'icon') {
    return (
      <button
        type="button"
        aria-label={label}
        aria-pressed={optimisticWatched}
        disabled={busy}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        className={`z-20 min-w-7 min-h-7 w-7 h-7 rounded-full flex items-center justify-center border transition-all duration-200 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shadow-md ${
          optimisticWatched
            ? 'bg-emerald-500/95 text-white border-emerald-300/60 opacity-100'
            : 'bg-canvas/85 hover:bg-panel text-muted hover:text-white border-border-subtle opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100'
        }`}
      >
        {busy ? (
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
        ) : (
          <Check className="w-3.5 h-3.5 stroke-[2.75]" />
        )}
      </button>
    );
  }

  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={optimisticWatched}
      disabled={busy}
      onClick={handleToggle}
      onKeyDown={handleKeyDown}
      className={`flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium border transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
        optimisticWatched
          ? 'bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border-emerald-500/40'
          : 'bg-panel hover:bg-panel-hover text-text-main border-border-subtle'
      }`}
    >
      {busy ? (
        <Loader2 className="w-4 h-4 animate-spin" />
      ) : (
        <CheckCircle2
          className={`w-4 h-4 ${
            optimisticWatched ? 'text-emerald-400' : 'text-muted'
          }`}
        />
      )}
      <span>{optimisticWatched ? 'Watched' : 'Mark Watched'}</span>
    </button>
  );
};
```

- [ ] **Step 4: Wire `WatchStateButton` & `onWatchStateChanged` into `CarouselWidget.tsx`, `GridWidget.tsx`, `BrowseScreen.tsx`, and `ItemDetailsModal.tsx`**

1. **`CarouselWidget.tsx` & `GridWidget.tsx`:**
   - Subscribe to `onWatchStateChanged(({ itemIds, watched }) => setItems((prev) => prev.map((it) => itemIds.has(it.id) ? applyWatchStateToCard(it, watched) : it)))`.
   - Inside `MediaCard` and `GridCard` (when `item.media_type !== 'show' && item.media_type !== 'live_channel'`), render `<div className="absolute top-2 right-2 z-20"><WatchStateButton itemId={item.id} title={item.title} mediaType={item.media_type} watched={isCardWatched(item)} variant="icon" /></div>` inside the poster container.
   - Only render the bottom resume bar when `hasProgress && !isCardWatched(item)` so completed items show the crisp top-right emerald check badge instead of a permanent 100% red bar.
2. **`BrowseScreen.tsx`:**
   - Subscribe to `onWatchStateChanged`: when `screenId === 'home'`, silently call `api.getScreen('home').then((data) => setLayout(data)).catch(() => {})` (without toggling the full-page `loading` skeleton) so the `Continue Watching` rail refreshes seamlessly.
3. **`ItemDetailsModal.tsx`:**
   - For non-`show`, non-`live_channel` items: render `<WatchStateButton itemId={itemId} title={card?.title || 'Item'} mediaType={card?.media_type} watched={isCardWatched(card)} variant="pill" onToggled={(w) => setDetails((prev) => prev ? { ...prev, resume_position_seconds: undefined, card: applyWatchStateToCard(prev.card, w) } : prev)} />` in the quick actions row.
   - For TV `show` items (`details.episodes && details.episodes.length > 0`):
     - Compute `allSeriesWatched = details.episodes.every((ep) => isCardWatched(ep))` and `allSeasonWatched = episodesForCurrentSeason.length > 0 && episodesForCurrentSeason.every((ep) => isCardWatched(ep))`.
     - Add **`Series Watched` / `Mark Series Watched`** button (`aria-pressed={allSeriesWatched}`) in the quick actions row calling `api.setPlaybackStatesBatch(details.episodes.map((e) => e.id), !allSeriesWatched)` + `emitWatchStateChanged(...)` with optimistic update and rollback on error.
     - Add **`Season Watched` / `Mark Season Watched`** button (`aria-pressed={allSeasonWatched}`) next to `Add Season` in the Season header row calling `api.setPlaybackStatesBatch(episodesForCurrentSeason.map((e) => e.id), !allSeasonWatched)` + `emitWatchStateChanged(...)`.
     - Inside `EpisodeCard`, render `<WatchStateButton itemId={episode.id} title={episode.title} mediaType={episode.media_type} watched={isCardWatched(episode)} variant="icon" onToggled={(w) => onToggleEpisodeWatched(episode.id, w)} />` next to the `+ Playlist` and `Play` buttons (using an always-visible compact circular button style in `EpisodeCard` or `onToggled` updating `details.episodes`).
   - Also fetch `api.getAudioTracks(itemId).catch(() => [])` when `details` loads (if `details.technical?.audio_tracks` isn't already populated) and render a **Audio Streams** row inside `Technical Specifications` whenever `audioTracks.length > 0`, displaying pills like `English • AAC Stereo` / `Japanese • EAC3 5.1`.

- [ ] **Step 5: Run Vitest on all browse & modal test suites**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npx vitest run src/components/browse/WatchStateButton.test.tsx src/components/browse/ItemDetailsModal.test.tsx src/components/browse/CarouselWidget.test.tsx src/components/browse/GridWidget.test.tsx src/components/browse/BrowseScreen.test.tsx
```
Expected: PASS (if any of `CarouselWidget.test.tsx` / `GridWidget.test.tsx` don't exist, run `npx vitest run` across the entire suite to verify zero regressions!).

- [ ] **Step 6: Commit Task 4**

```bash
git add web/src/utils/watchStateEvents.ts web/src/components/browse/WatchStateButton.tsx web/src/components/browse/WatchStateButton.test.tsx web/src/components/browse/CarouselWidget.tsx web/src/components/browse/GridWidget.tsx web/src/components/browse/BrowseScreen.tsx web/src/components/browse/ItemDetailsModal.tsx web/src/components/browse/ItemDetailsModal.test.tsx
git commit -m "feat(web): add single, season, and series mark watched controls across cards and item details"
```

---

### Task 5: Documentation Parity (`README.md` & `docs/CLIENT_GUIDE.md`) & Full-Stack Verification

**Files:**
- Modify: `README.md:35-58,88-120,452-514`
- Modify: `docs/CLIENT_GUIDE.md:7-22,1133-1145,1324-1395`

**Interfaces:**
- Consumes: Exact live route contracts in `crates/kadr-server/src/playback/handler.rs`, `crates/kadr-server/src/api/audio_routes.rs`, and `crates/kadr-server/src/api/hls_routes.rs`.
- Produces: Synchronized `README.md` and `docs/CLIENT_GUIDE.md` documentation + verified green build & test suites across Rust and Web.

- [ ] **Step 1: Update `README.md`**

Update `README.md`:
1. In **Key Features**:
   - Under `Universal On-Demand HLS Transcoding & Smart Stream Copy`, note multi-audio stream selection (`audio_track`) and audio-only HLS (`video_codec=none` / `-vn`) for `music` & `audiobook` streams.
   - Under `Integrated Cinema Web App & Studio > Cinema Player`, document `hls.js` automatic HLS fallback, multi-audio track selector, 10×10 storyboard seek-bar hover previews (with `/frame` fallback), and single / season / series Mark Watched controls across catalog cards and Item Details.
   - Update SQLite migrations count from `001–012` to `001–014`.
2. In **REST & Streaming API Reference**:
   - Under `Video Streaming, HLS Transcoding, Storyboards & Artwork`: note `video_codec=none` audio-only HLS support on `GET /api/v1/media/:item_id/hls/master.m3u8`.
   - Under `Playback & Scrobbling`: add:
     - `PUT /api/v1/playback/states/:item_id` — Manually mark a playable VOD item watched or unwatched (`{"watched": true|false}`, returning `PlaybackState` and broadcasting `layout:changed` for `home`).
     - `PUT /api/v1/playback/states` — Batch mark `1..=500` items watched or unwatched (`{"item_ids": [...], "watched": true|false}` -> `{"updated": N, "watched": bool}`), ideal for marking entire TV seasons or series.
   - Under `Subtitle Management` (rename heading to `Audio Tracks & Subtitle Management`): add:
     - `GET /api/v1/items/:item_id/audio-tracks` — List container audio streams (`AudioStreamInfo[]`: `audio_index`, `stream_index`, `codec`, `channels`, `language`, `title`, `is_default`, `is_commentary`), lazily backfilling via singleflight `ffprobe` when not yet indexed.

- [ ] **Step 2: Update `docs/CLIENT_GUIDE.md`**

Update `docs/CLIENT_GUIDE.md`:
1. Update Table of Contents if adding a new subsection (`7.1b Audio Track Discovery`).
2. In **Section `6.3 Universal HLS Transcoding & Remuxing Fallback`**:
   - Update the `video_codec` row in the `Query Parameters` table to document `"h264"`, `"hevc"`, or `"none"` (`VideoAction::None` / `-vn` audio-only MPEG-TS HLS for `music` and `audiobook` items).
   - Add a client note explaining that because server-side `-ss <start_time>` resets segment PTS to `0`, clients starting HLS at `start_time = S` compute `effectiveCurrentTime = S + player.currentTime` and shift external WebVTT cues by `-S`.
3. In **Section `6.5 Playback Session Lifecycle & Scrobble Synchronization`**:
   - Add **`Step 5: Manual Mark Watched / Unwatched (Single & Batch)`** documenting:
     - `PUT /api/v1/playback/states/{item_id}` with request `{"watched": true}`, `200 OK` `PlaybackState` response, `400` on `live_channel`, and `403 LIBRARY_LOCKED` on locked private libraries.
     - `PUT /api/v1/playback/states` with request `{"item_ids": [201, 202, 203], "watched": true}`, `1..=500` unique ID limit (`400` if empty or `> 500`, chunking guidance for large multi-season shows), and `200 OK` `{"updated": 3, "watched": true}` response.
4. In **Section `7` (`Subtitles, Storyboard Trickplay & Media Assets`)**:
   - Add **`7.1b Audio Track Discovery (`GET /api/v1/items/{item_id}/audio-tracks`)`** with full `AudioStreamInfo[]` JSON example (`audio_index`, `stream_index`, `codec`, `channels`, `language`, `title`, `is_default`, `is_commentary`), lazy singleflight `ffprobe` persistence behavior, `403 LIBRARY_LOCKED` enforcement, and how `audio_index` maps directly to `?audio_track=<audio_index>` on `master.m3u8`.

- [ ] **Step 3: Run full Web & Rust workspace verification**

Run:
```bash
cd /home/boody/work/personal/kadr/web && npm test -- --run && npm run build
```
Expected: All Vitest suites PASS, `tsc -b && vite build` succeeds with zero errors, and `hls` is emitted as a separate code-split chunk in `web/dist/assets/`.

Run:
```bash
cd /home/boody/work/personal/kadr && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
```
Expected: All Rust workspace tests PASS and Clippy/rustfmt exit `0` with zero warnings.

- [ ] **Step 4: Commit Task 5**

```bash
git add README.md docs/CLIENT_GUIDE.md
git commit -m "docs: document audio-tracks, audio-only HLS, mark watched endpoints, and web player parity"
```
