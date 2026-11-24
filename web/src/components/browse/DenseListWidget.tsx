import { useState, useEffect, type FC } from 'react';
import { Check, Film, Loader2, Play, Star } from 'lucide-react';
import type { CardViewModel, TargetDevice } from '../../types';
import { api } from '../../api/client';
import { WatchStateButton } from './WatchStateButton';
import { FavoriteButton } from '../favorites/FavoriteButton';
import {
  applyWatchStateToCard,
  isCardWatched,
  onWatchStateChanged,
} from '../../utils/watchStateEvents';
import {
  applyFavoriteToCard,
  onFavoriteChanged,
} from '../../utils/favoriteEvents';
import { formatDurationCompact } from '../../utils/layoutScope';

export { formatDurationCompact };

export interface DenseListWidgetProps {
  title?: string;
  items?: CardViewModel[];
  widgetId?: string;
  screenId?: string;
  targetDevice?: TargetDevice;
  totalCount?: number;
  nextCursor?: string;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (itemId: number) => void;
}

interface DenseListRowProps {
  item: CardViewModel;
  onSelect: () => void;
  onPlay?: () => void;
}

const DenseListRow: FC<DenseListRowProps> = ({ item, onSelect, onPlay }) => {
  const [imageFailed, setImageFailed] = useState(false);

  const artworkUrl =
    item.backdrop_url ||
    item.poster_url ||
    (item.id ? api.getArtworkUrl(item.id, 'backdrop') : undefined);

  const watched = isCardWatched(item);
  const hasProgress =
    typeof item.playback_progress === 'number' && item.playback_progress > 0;
  const progressPercent = hasProgress
    ? Math.min(100, Math.max(0, Math.round((item.playback_progress || 0) * 100)))
    : 0;

  const displayBadge =
    item.badge === 'RESUME' ? (!watched ? 'RESUME' : undefined) : item.badge;
  const runtimeLabel =
    item.media_type !== 'live_channel'
      ? formatDurationCompact(item.duration_seconds)
      : '';
  const canToggleWatched =
    item.media_type !== 'show' &&
    item.media_type !== 'season' &&
    item.media_type !== 'live_channel';
  const canPlayDirect =
    item.media_type !== 'show' && item.media_type !== 'season';

  return (
    <li className="bg-panel/80 hover:bg-panel-hover border border-border-subtle rounded-xl p-2.5 sm:p-3 flex items-center gap-3 sm:gap-4 transition-colors">
      {/* Main Info & Thumbnail Trigger */}
      <button
        type="button"
        onClick={onSelect}
        className="group flex-1 min-w-0 min-h-7 flex items-center gap-3 sm:gap-4 text-left rounded-lg cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
      >
        {/* 16:9 Artwork Thumbnail */}
        <div className="relative w-24 sm:w-28 aspect-video rounded-lg overflow-hidden bg-canvas border border-border-subtle shrink-0 flex items-center justify-center">
          {!imageFailed && artworkUrl ? (
            <img
              src={artworkUrl}
              alt=""
              onError={() => setImageFailed(true)}
              className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 transition-all duration-200"
            />
          ) : (
            <div className="w-full h-full bg-gradient-to-br from-canvas via-panel to-canvas flex items-center justify-center">
              <Film
                aria-hidden="true"
                className="w-6 h-6 text-muted group-hover:text-accent transition-colors"
              />
            </div>
          )}

          {/* Subtle Hover Overlay */}
          <div className="absolute inset-0 bg-canvas/35 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity duration-200 pointer-events-none">
            <div className="w-7 h-7 rounded-full bg-cta/95 text-white flex items-center justify-center shadow-md">
              <Play
                aria-hidden="true"
                className="w-3.5 h-3.5 fill-white text-white ml-0.5"
              />
            </div>
          </div>

          {/* Subtle Corner Watched Checkmark */}
          {watched && (
            <div
              aria-hidden="true"
              className="absolute top-1.5 right-1.5 w-5 h-5 rounded-full bg-emerald-500/95 text-white flex items-center justify-center shadow-sm"
            >
              <Check aria-hidden="true" className="w-3 h-3 stroke-[2.75]" />
            </div>
          )}
        </div>

        {/* Center Metadata & Progress */}
        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2 min-w-0">
            <span className="text-sm sm:text-base font-semibold text-text-main group-hover:text-white transition-colors truncate">
              {item.title}
            </span>

            {displayBadge && (
              <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wider uppercase bg-highlight/20 text-highlight border border-highlight/40 shrink-0">
                {displayBadge}
              </span>
            )}

            {watched && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-semibold tracking-wider uppercase bg-emerald-500/15 text-emerald-300 border border-emerald-500/35 shrink-0">
                <Check aria-hidden="true" className="w-3 h-3 stroke-[2.5]" />
                <span>Watched</span>
              </span>
            )}

            {typeof item.rating === 'number' && item.rating > 0 && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] font-semibold bg-panel text-highlight border border-border-subtle shrink-0">
                <Star
                  aria-hidden="true"
                  className="w-3 h-3 fill-highlight text-highlight"
                />
                <span>{item.rating.toFixed(1)}</span>
              </span>
            )}
          </div>

          {(item.subtitle || item.release_year) && (
            <div className="flex items-center gap-1.5 text-xs text-muted truncate">
              {!item.subtitle && item.release_year && (
                <span>{item.release_year}</span>
              )}
              {item.subtitle && <span className="truncate">{item.subtitle}</span>}
            </div>
          )}

          {hasProgress && !watched && (
            <div
              role="progressbar"
              aria-label={`${item.title} playback progress`}
              aria-valuenow={progressPercent}
              aria-valuemin={0}
              aria-valuemax={100}
              className="w-full max-w-xs h-1.5 rounded-full bg-muted/40 overflow-hidden"
            >
              <div
                className="h-full bg-accent transition-all"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
          )}
        </div>
      </button>

      {/* Right Cluster: Runtime + Inline Quick Actions */}
      <div className="flex items-center gap-2 shrink-0">
        {runtimeLabel && (
          <span className="px-2 py-0.5 rounded-md bg-canvas/70 border border-border-subtle font-mono text-xs text-muted">
            {runtimeLabel}
          </span>
        )}

        <FavoriteButton
          itemId={item.id}
          title={item.title}
          mediaType={item.media_type}
          isFavorite={Boolean(item.is_favorite)}
          variant="icon"
          alwaysVisible
        />

        {canToggleWatched && (
          <WatchStateButton
            itemId={item.id}
            title={item.title}
            mediaType={item.media_type}
            watched={watched}
            variant="icon"
            alwaysVisible
          />
        )}

        {canPlayDirect && (
          <button
            type="button"
            aria-label={`Play ${item.title}`}
            onClick={onPlay}
            className="min-w-7 min-h-7 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cta hover:bg-cta-hover text-white text-xs font-semibold shadow-sm transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <Play aria-hidden="true" className="w-3.5 h-3.5 fill-current" />
            <span>Play</span>
          </button>
        )}
      </div>
    </li>
  );
};

function dedupeCards(cards: CardViewModel[]): CardViewModel[] {
  const seen = new Set<number>();
  const deduped: CardViewModel[] = [];
  for (const card of cards) {
    if (!seen.has(card.id)) {
      seen.add(card.id);
      deduped.push(card);
    }
  }
  return deduped;
}

export const DenseListWidget: FC<DenseListWidgetProps> = ({
  title,
  items: initialItems,
  widgetId,
  screenId,
  targetDevice,
  totalCount,
  nextCursor,
  onSelectItem,
  onPlayItem,
}) => {
  const [items, setItems] = useState<CardViewModel[]>(() =>
    initialItems ? dedupeCards(initialItems) : []
  );
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState<number>(() =>
    initialItems && initialItems.length > 0 ? initialItems.length : 20
  );
  const [loading, setLoading] = useState(false);
  const [hasMore, setHasMore] = useState<boolean>(() =>
    initialItems !== undefined ? Boolean(nextCursor) : true
  );

  useEffect(() => {
    if (initialItems !== undefined) {
      const deduped = dedupeCards(initialItems);
      setItems(deduped);
      setPage(0);
      setPageSize(deduped.length > 0 ? deduped.length : 20);
      setHasMore(Boolean(nextCursor));
      return;
    }

    if (!widgetId) return;

    let isMounted = true;
    setLoading(true);

    const defaultLimit = 20;
    api
      .getWidgetData(
        widgetId,
        0,
        defaultLimit,
        undefined,
        screenId,
        ...(targetDevice ? [targetDevice] : [])
      )
      .then((data) => {
        if (isMounted) {
          const deduped = dedupeCards(data);
          setItems(deduped);
          setPage(0);
          setPageSize(defaultLimit);
          setHasMore(deduped.length >= defaultLimit);
          setLoading(false);
        }
      })
      .catch(() => {
        if (isMounted) {
          setHasMore(false);
          setLoading(false);
        }
      });

    return () => {
      isMounted = false;
    };
  }, [initialItems, nextCursor, widgetId, screenId, targetDevice]);

  useEffect(() => {
    return onWatchStateChanged(({ itemIds, watched }) => {
      setItems((prev) =>
        prev.map((it) =>
          itemIds.has(it.id) ? applyWatchStateToCard(it, watched) : it
        )
      );
    });
  }, []);

  useEffect(() => {
    return onFavoriteChanged(({ itemId, isFavorite }) => {
      setItems((prev) =>
        prev.map((it) =>
          it.id === itemId ? applyFavoriteToCard(it, isFavorite) : it
        )
      );
    });
  }, []);

  const loadMore = async () => {
    if (!widgetId || loading || !hasMore) return;
    const nextPage = page + 1;
    setLoading(true);
    try {
      const moreItems = await api.getWidgetData(
        widgetId,
        nextPage,
        pageSize,
        undefined,
        screenId,
        ...(targetDevice ? [targetDevice] : [])
      );
      if (moreItems.length < pageSize) {
        setHasMore(false);
      }
      setItems((prev) => {
        const seenIds = new Set(prev.map((i) => i.id));
        const unique = moreItems.filter((i) => !seenIds.has(i.id));
        const merged = [...prev, ...unique];
        if (
          unique.length === 0 ||
          (totalCount !== undefined && merged.length >= totalCount)
        ) {
          setHasMore(false);
        }
        return merged;
      });
      setPage(nextPage);
    } catch {
      setHasMore(false);
    } finally {
      setLoading(false);
    }
  };

  if (!loading && items.length === 0) {
    return null;
  }

  return (
    <section className="space-y-3.5 py-2">
      {title && (
        <div className="flex items-baseline gap-2.5">
          <h2 className="text-xl font-bold tracking-tight text-text-main">
            {title}
          </h2>
          {(totalCount !== undefined || items.length > 0) && (
            <span className="text-xs font-medium text-muted">
              ({totalCount !== undefined ? totalCount : items.length})
            </span>
          )}
        </div>
      )}

      {items.length > 0 && (
        <ul role="list" className="space-y-2.5">
          {items.map((item) => (
            <DenseListRow
              key={item.id}
              item={item}
              onSelect={() => onSelectItem(item)}
              onPlay={() => onPlayItem?.(item.id)}
            />
          ))}
        </ul>
      )}

      {widgetId && hasMore && items.length > 0 && (
        <div className="flex justify-center pt-4">
          <button
            type="button"
            onClick={loadMore}
            disabled={loading}
            className="min-w-7 min-h-7 flex items-center gap-2 px-6 py-2.5 rounded-xl bg-panel hover:bg-panel-hover text-sm font-semibold text-text-main border border-border-subtle transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            {loading ? (
              <>
                <Loader2
                  aria-hidden="true"
                  className="w-4 h-4 animate-spin text-cta"
                />
                <span>Loading...</span>
              </>
            ) : (
              'Load More'
            )}
          </button>
        </div>
      )}
    </section>
  );
};

export default DenseListWidget;
