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
  alwaysVisible?: boolean;
  onToggled?: (watched: boolean) => void;
}

export const WatchStateButton: FC<WatchStateButtonProps> = ({
  itemId,
  title,
  mediaType,
  watched: propWatched,
  variant = 'pill',
  alwaysVisible = false,
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
            : alwaysVisible
            ? 'bg-panel hover:bg-panel-hover text-muted hover:text-text-main border-border-subtle opacity-100'
            : 'bg-canvas/85 hover:bg-panel text-muted hover:text-white border-border-subtle opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100'
        }`}
      >
        {busy ? (
          <Loader2 aria-hidden="true" className="w-3.5 h-3.5 animate-spin" />
        ) : (
          <Check aria-hidden="true" className="w-3.5 h-3.5 stroke-[2.75]" />
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
        <Loader2 aria-hidden="true" className="w-4 h-4 animate-spin" />
      ) : (
        <CheckCircle2
          aria-hidden="true"
          className={`w-4 h-4 ${
            optimisticWatched ? 'text-emerald-400' : 'text-muted'
          }`}
        />
      )}
      <span>{optimisticWatched ? 'Watched' : 'Mark Watched'}</span>
    </button>
  );
};
