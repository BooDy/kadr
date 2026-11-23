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

  useEffect(() => {
    if (status !== 'generating' || !itemId || !Number.isFinite(seconds)) {
      setDebouncedFrameUrl(null);
      setFrameFailed(false);
      return;
    }
    const safeSecs = Math.max(0, seconds);
    const nextUrl = api.getFramePreviewUrl(itemId, safeSecs, 320);
    const timer = setTimeout(() => {
      setFrameFailed(false);
      setDebouncedFrameUrl(nextUrl);
    }, 120);
    return () => clearTimeout(timer);
  }, [status, itemId, seconds]);

  if (!Number.isFinite(seconds)) {
    return null;
  }

  const safePercent = Number.isFinite(percent) ? percent : 50;
  const clampedPercent = Math.min(92, Math.max(8, safePercent));
  const tile =
    status === 'ready' && manifest
      ? computeStoryboardTile(manifest, seconds, 160)
      : null;

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
