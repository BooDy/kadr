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
    !Number.isFinite(seconds) ||
    !Number.isFinite(displayWidth) ||
    displayWidth <= 0 ||
    manifest.total_tiles <= 0 ||
    manifest.interval_ms <= 0 ||
    manifest.tile_width <= 0 ||
    manifest.tile_height <= 0 ||
    manifest.columns <= 0 ||
    manifest.rows <= 0 ||
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
    !Number.isFinite(durationSeconds) ||
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
