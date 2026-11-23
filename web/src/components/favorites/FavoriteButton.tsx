import {
  useState,
  useEffect,
  type FC,
  type MouseEvent,
  type KeyboardEvent,
} from 'react';
import { Heart, Loader2 } from 'lucide-react';
import { api } from '../../api/client';
import { emitFavoriteChanged } from '../../utils/favoriteEvents';

export interface FavoriteButtonProps {
  itemId: number;
  title: string;
  mediaType?: string;
  isFavorite: boolean;
  variant?: 'icon' | 'pill';
  alwaysVisible?: boolean;
  onToggled?: (isFavorite: boolean) => void;
}

export const FavoriteButton: FC<FavoriteButtonProps> = ({
  itemId,
  title,
  isFavorite: propIsFavorite,
  variant = 'pill',
  alwaysVisible = false,
  onToggled,
}) => {
  const [optimisticFavorite, setOptimisticFavorite] = useState(propIsFavorite);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setOptimisticFavorite(propIsFavorite);
  }, [itemId, propIsFavorite]);

  const label = optimisticFavorite
    ? `Remove ${title} from favorites`
    : `Add ${title} to favorites`;

  const handleToggle = async (e: MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (busy) return;
    const prev = optimisticFavorite;
    const next = !prev;
    setOptimisticFavorite(next);
    onToggled?.(next);
    setBusy(true);
    try {
      if (next) {
        if (typeof api.addFavorite === 'function') {
          await api.addFavorite(itemId);
        }
      } else {
        if (typeof api.removeFavorite === 'function') {
          await api.removeFavorite(itemId);
        }
      }
      emitFavoriteChanged(itemId, next);
    } catch {
      setOptimisticFavorite(prev);
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
        aria-pressed={optimisticFavorite}
        disabled={busy}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        className={`z-20 min-w-7 min-h-7 w-7 h-7 rounded-full flex items-center justify-center border transition-all duration-200 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shadow-md ${
          optimisticFavorite
            ? 'bg-canvas/90 hover:bg-panel text-cta border-cta/50 opacity-100'
            : alwaysVisible
            ? 'bg-panel hover:bg-panel-hover text-muted hover:text-text-main border-border-subtle opacity-100'
            : 'bg-canvas/85 hover:bg-panel text-muted hover:text-white border-border-subtle opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100'
        }`}
      >
        {busy ? (
          <Loader2 aria-hidden="true" className="w-3.5 h-3.5 animate-spin" />
        ) : (
          <Heart
            aria-hidden="true"
            className={
              optimisticFavorite ? 'w-3.5 h-3.5 fill-cta text-cta' : 'w-3.5 h-3.5'
            }
          />
        )}
      </button>
    );
  }

  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={optimisticFavorite}
      disabled={busy}
      onClick={handleToggle}
      onKeyDown={handleKeyDown}
      className={`min-w-7 min-h-7 flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium border transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
        optimisticFavorite
          ? 'bg-cta/15 hover:bg-cta/25 text-text-main border-cta/40'
          : 'bg-panel hover:bg-panel-hover text-text-main border-border-subtle'
      }`}
    >
      {busy ? (
        <Loader2 aria-hidden="true" className="w-4 h-4 animate-spin" />
      ) : (
        <Heart
          aria-hidden="true"
          className={
            optimisticFavorite ? 'w-4 h-4 fill-cta text-cta' : 'w-4 h-4 text-muted'
          }
        />
      )}
      <span>{optimisticFavorite ? 'Favorited' : 'Favorite'}</span>
    </button>
  );
};
