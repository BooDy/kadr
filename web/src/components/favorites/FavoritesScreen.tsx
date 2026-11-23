import {
  useState,
  useEffect,
  useCallback,
  useRef,
  useId,
  type FC,
} from 'react';
import {
  Heart,
  Lock,
  Play,
  Shuffle,
  Film,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import type {
  CardViewModel,
  FavoriteItemView,
  PlaylistItemView,
  User,
} from '../../types';
import { api, describeApiError } from '../../api/client';
import { FavoriteButton } from './FavoriteButton';
import { WatchStateButton } from '../browse/WatchStateButton';
import { ItemDetailsModal } from '../browse/ItemDetailsModal';
import {
  applyWatchStateToCard,
  isCardWatched,
  onWatchStateChanged,
} from '../../utils/watchStateEvents';
import {
  applyFavoriteToCard,
  onFavoriteChanged,
} from '../../utils/favoriteEvents';

export interface FavoritesScreenProps {
  currentUser?: User | null;
  onPlayItem: (itemId: number) => void;
  onPlayQueue: (items: PlaylistItemView[], startIndex?: number) => void;
  onNavigateToFolder?: (libraryId: string, folderPath: string) => void;
}

type FavoriteFilterType =
  | 'all'
  | 'movie'
  | 'show'
  | 'episode'
  | 'home_videos'
  | 'live_channel';

const TYPE_FILTER_PILLS: { value: FavoriteFilterType; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'movie', label: 'Movies' },
  { value: 'show', label: 'TV Shows' },
  { value: 'episode', label: 'Episodes' },
  { value: 'home_videos', label: 'Home Videos' },
  { value: 'live_channel', label: 'Live Channels' },
];

const PAGE_SIZE = 60;

const FAVORITES_SORT_OPTIONS: { value: string; label: string }[] = [
  { value: 'favorited_at:desc', label: 'Recently Favorited' },
  { value: 'favorited_at:asc', label: 'Oldest Favorited' },
  { value: 'title:asc', label: 'Title (A–Z)' },
  { value: 'release_year:desc', label: 'Release Year' },
  { value: 'rating:desc', label: 'Rating' },
  { value: 'duration_seconds:asc', label: 'Duration (Shortest)' },
];

function dedupeFavoriteItems(items: FavoriteItemView[]): FavoriteItemView[] {
  const seen = new Set<number>();
  const out: FavoriteItemView[] = [];
  for (const item of items) {
    if (!seen.has(item.card.id)) {
      seen.add(item.card.id);
      out.push(item);
    }
  }
  return out;
}

function isPlayableFavorite(item: FavoriteItemView): boolean {
  return item.card.media_type !== 'show' && item.card.media_type !== 'season';
}

function toPlaylistQueue(items: FavoriteItemView[]): PlaylistItemView[] {
  return items.map((item, idx) => ({
    position: idx,
    added_at: item.favorited_at,
    duration_seconds: item.duration_seconds,
    ...(item.resume_position_seconds !== undefined
      ? { resume_position_seconds: item.resume_position_seconds }
      : {}),
    card: item.card,
  }));
}

interface FavoritePosterCardProps {
  card: CardViewModel;
  onSelect: () => void;
}

const FavoritePosterCard: FC<FavoritePosterCardProps> = ({ card, onSelect }) => {
  const [imageFailed, setImageFailed] = useState(false);

  const posterUrl =
    card.poster_url || (card.id ? api.getArtworkUrl(card.id, 'poster') : undefined);

  const watched = isCardWatched(card);
  const hasProgress =
    typeof card.playback_progress === 'number' && card.playback_progress > 0;
  const progressPercent = hasProgress
    ? Math.min(100, Math.max(0, Math.round((card.playback_progress || 0) * 100)))
    : 0;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.target !== e.currentTarget) return;
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      className="group relative flex flex-col cursor-pointer rounded-xl overflow-hidden border border-border-subtle bg-panel hover:bg-panel-hover transition-all duration-200 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none p-2"
    >
      <div className="relative aspect-[2/3] w-full rounded-lg overflow-hidden bg-canvas">
        {!imageFailed && posterUrl ? (
          <img
            src={posterUrl}
            alt={card.title}
            onError={() => setImageFailed(true)}
            className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 transition-all duration-200"
          />
        ) : (
          <div className="w-full h-full bg-gradient-to-br from-canvas via-panel to-canvas flex flex-col items-center justify-center p-4 text-center">
            <Film
              aria-hidden="true"
              className="w-10 h-10 text-muted mb-2 group-hover:text-accent transition-colors"
            />
            <span className="text-xs font-semibold text-text-main line-clamp-3">
              {card.title}
            </span>
          </div>
        )}

        <div className="absolute inset-0 bg-canvas/40 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity duration-200 pointer-events-none">
          <div className="w-11 h-11 rounded-full bg-cta text-white flex items-center justify-center shadow-lg shadow-cta/30 transform scale-90 group-hover:scale-100 transition-transform duration-200">
            <Play aria-hidden="true" className="w-5 h-5 fill-white text-white ml-0.5" />
          </div>
        </div>

        {card.badge && (
          <div className="absolute top-2 left-2">
            <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wider uppercase bg-highlight/20 text-highlight border border-highlight/40 shadow-xs">
              {card.badge}
            </span>
          </div>
        )}

        <div className="absolute top-2 right-2 z-20 flex items-center gap-1.5">
          <FavoriteButton
            itemId={card.id}
            title={card.title}
            mediaType={card.media_type}
            isFavorite={card.is_favorite ?? true}
            variant="icon"
            alwaysVisible
          />
          {card.media_type !== 'show' &&
            card.media_type !== 'season' &&
            card.media_type !== 'live_channel' && (
              <WatchStateButton
                itemId={card.id}
                title={card.title}
                mediaType={card.media_type}
                watched={watched}
                variant="icon"
              />
            )}
        </div>

        {hasProgress && !watched && (
          <div className="absolute bottom-0 inset-x-0 h-1.5 bg-muted">
            <div
              className="h-full bg-accent"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        )}
      </div>

      <div className="mt-2.5 px-0.5 space-y-0.5 min-w-0">
        <h3 className="text-sm font-medium text-text-main truncate group-hover:text-white transition-colors">
          {card.title}
        </h3>
        <div className="flex items-center gap-1.5 text-xs text-muted truncate">
          {card.release_year && <span>{card.release_year}</span>}
          {card.release_year && card.subtitle && <span>•</span>}
          {card.subtitle && <span className="truncate">{card.subtitle}</span>}
        </div>
      </div>
    </div>
  );
};

interface FavoritesState {
  items: FavoriteItemView[];
  totalCount: number;
  lockedCount: number;
  hasMorePages: boolean;
}

export const FavoritesScreen: FC<FavoritesScreenProps> = ({
  currentUser,
  onPlayItem,
  onPlayQueue,
  onNavigateToFolder,
}) => {
  const sortSelectId = useId();
  const [itemType, setItemType] = useState<FavoriteFilterType>('all');
  const [sort, setSort] = useState<string>('favorited_at:desc');
  const [data, setData] = useState<FavoritesState>({
    items: [],
    totalCount: 0,
    lockedCount: 0,
    hasMorePages: true,
  });
  const [loading, setLoading] = useState<boolean>(true);
  const [loadingMore, setLoadingMore] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [loadMoreError, setLoadMoreError] = useState<string | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<number | null>(null);
  const requestIdRef = useRef(0);
  const mountedRef = useRef(true);
  const itemsRef = useRef<FavoriteItemView[]>([]);
  itemsRef.current = data.items;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const loadFavorites = useCallback(async () => {
    if (typeof api.getFavorites !== 'function') {
      setLoading(false);
      return;
    }
    const reqId = ++requestIdRef.current;
    setLoading(true);
    setLoadingMore(false);
    setError(null);
    setLoadMoreError(null);
    try {
      const res = await api.getFavorites({
        ...(itemType !== 'all' ? { item_type: itemType } : {}),
        sort,
        limit: PAGE_SIZE,
        offset: 0,
      });
      if (!mountedRef.current || reqId !== requestIdRef.current) return;
      const incoming = res.items || [];
      setData({
        items: dedupeFavoriteItems(incoming),
        totalCount: res.total_count ?? 0,
        lockedCount: res.locked_item_count ?? 0,
        hasMorePages: incoming.length > 0,
      });
    } catch (err) {
      if (!mountedRef.current || reqId !== requestIdRef.current) return;
      setError(describeApiError(err));
    } finally {
      if (mountedRef.current && reqId === requestIdRef.current) {
        setLoading(false);
      }
    }
  }, [itemType, sort]);

  const handleLoadMore = useCallback(async () => {
    if (
      typeof api.getFavorites !== 'function' ||
      loading ||
      loadingMore
    ) {
      return;
    }
    const activeReqId = requestIdRef.current;
    const offset = itemsRef.current.length;
    setLoadingMore(true);
    setLoadMoreError(null);
    try {
      const res = await api.getFavorites({
        ...(itemType !== 'all' ? { item_type: itemType } : {}),
        sort,
        limit: PAGE_SIZE,
        offset,
      });
      if (!mountedRef.current || activeReqId !== requestIdRef.current) return;
      const incoming = res.items || [];
      setData((prev) => {
        const merged = dedupeFavoriteItems([...prev.items, ...incoming]);
        const grew = incoming.length > 0 && merged.length > prev.items.length;
        return {
          items: merged,
          totalCount: res.total_count ?? prev.totalCount,
          lockedCount: res.locked_item_count ?? prev.lockedCount,
          hasMorePages: grew,
        };
      });
    } catch (err) {
      if (!mountedRef.current || activeReqId !== requestIdRef.current) return;
      setLoadMoreError(describeApiError(err));
    } finally {
      if (mountedRef.current && activeReqId === requestIdRef.current) {
        setLoadingMore(false);
      }
    }
  }, [itemType, sort, loading, loadingMore]);

  useEffect(() => {
    loadFavorites();
  }, [loadFavorites]);

  useEffect(() => {
    return onFavoriteChanged(({ itemId, isFavorite }) => {
      if (!isFavorite) {
        setData((prev) => {
          const exists = prev.items.some((i) => i.card.id === itemId);
          if (!exists) return prev;
          return {
            ...prev,
            items: prev.items.filter((i) => i.card.id !== itemId),
            totalCount: Math.max(0, prev.totalCount - 1),
          };
        });
        return;
      }

      if (!itemsRef.current.some((i) => i.card.id === itemId)) {
        loadFavorites();
        return;
      }

      setData((prev) => ({
        ...prev,
        items: prev.items.map((i) =>
          i.card.id === itemId
            ? { ...i, card: applyFavoriteToCard(i.card, true) }
            : i
        ),
      }));
    });
  }, [loadFavorites]);

  useEffect(() => {
    return onWatchStateChanged(({ itemIds, watched }) => {
      setData((prev) => ({
        ...prev,
        items: prev.items.map((item) =>
          itemIds.has(item.card.id)
            ? {
                ...item,
                resume_position_seconds: undefined,
                card: applyWatchStateToCard(item.card, watched),
              }
            : item
        ),
      }));
    });
  }, []);

  const playableItems = data.items.filter(isPlayableFavorite);

  const handlePlayAll = () => {
    if (playableItems.length === 0) return;
    onPlayQueue(toPlaylistQueue(playableItems), 0);
  };

  const handleShuffle = () => {
    if (playableItems.length === 0) return;
    const shuffled = [...playableItems];
    for (let i = shuffled.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
    }
    onPlayQueue(toPlaylistQueue(shuffled), 0);
  };

  return (
    <div className="max-w-7xl mx-auto p-4 sm:p-6 lg:p-8 space-y-6">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <Heart aria-hidden="true" className="w-7 h-7 text-cta fill-cta/20" />
          <h1 className="text-2xl sm:text-3xl font-semibold text-text-main">
            My Favorites
          </h1>
          <span
            data-testid="favorites-total-count"
            className="px-2.5 py-0.5 rounded-lg bg-panel border border-border-subtle text-xs font-mono font-semibold text-muted"
          >
            {data.totalCount}
          </span>
          {data.lockedCount > 0 && (
            <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-panel border border-highlight/40 text-xs font-medium text-highlight">
              <Lock aria-hidden="true" className="w-3.5 h-3.5 shrink-0" />
              <span>{data.lockedCount} in locked libraries</span>
            </span>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <button
            type="button"
            disabled={playableItems.length === 0}
            onClick={handlePlayAll}
            className="min-w-7 min-h-7 inline-flex items-center gap-2 px-5 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-sm font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <Play aria-hidden="true" className="w-4 h-4 fill-current" />
            <span>Play All</span>
          </button>

          <button
            type="button"
            disabled={playableItems.length === 0}
            onClick={handleShuffle}
            className="min-w-7 min-h-7 inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-panel hover:bg-panel-hover border border-border-subtle text-text-main text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <Shuffle aria-hidden="true" className="w-4 h-4 text-accent" />
            <span>Shuffle</span>
          </button>
        </div>
      </div>

      {/* Filter Pills & Sort Controls */}
      <div className="flex flex-wrap items-center justify-between gap-4 pb-2 border-b border-border-subtle">
        <div className="flex flex-wrap items-center gap-2">
          {TYPE_FILTER_PILLS.map((pill) => {
            const active = itemType === pill.value;
            return (
              <button
                key={pill.value}
                type="button"
                aria-pressed={active}
                onClick={() => setItemType(pill.value)}
                className={`min-w-7 min-h-7 px-3.5 py-1.5 rounded-xl text-xs font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  active
                    ? 'bg-accent text-canvas shadow-xs'
                    : 'bg-panel hover:bg-panel-hover border border-border-subtle text-muted hover:text-text-main'
                }`}
              >
                {pill.label}
              </button>
            );
          })}
        </div>

        <div className="flex items-center gap-2.5">
          <label
            htmlFor={sortSelectId}
            className="text-xs font-semibold uppercase tracking-wider text-muted"
          >
            Sort
          </label>
          <select
            id={sortSelectId}
            value={sort}
            onChange={(e) => setSort(e.target.value)}
            className="min-h-7 bg-panel border border-border-subtle rounded-xl px-3 py-1.5 text-xs font-medium text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            {FAVORITES_SORT_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Content Area */}
      {loading ? (
        <div className="flex flex-col items-center justify-center py-20 text-muted space-y-3">
          <Loader2 aria-hidden="true" className="w-8 h-8 animate-spin text-accent" />
          <p className="text-xs font-mono uppercase tracking-wider">
            Loading favorites...
          </p>
        </div>
      ) : error ? (
        <div className="p-8 rounded-xl bg-panel border border-border-subtle flex flex-col items-center text-center space-y-3">
          <AlertCircle aria-hidden="true" className="w-8 h-8 text-highlight" />
          <p className="text-sm text-text-main">{error}</p>
          <button
            type="button"
            onClick={loadFavorites}
            className="min-w-7 min-h-7 px-4 py-2 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-xs font-semibold cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            Retry
          </button>
        </div>
      ) : data.items.length === 0 ? (
        <div className="p-12 rounded-xl bg-panel border border-border-subtle flex flex-col items-center justify-center text-center space-y-3">
          <Heart aria-hidden="true" className="w-10 h-10 text-muted" />
          <h2 className="text-base font-medium text-text-main">
            {itemType === 'all' ? 'No favorites yet' : 'No matching favorites'}
          </h2>
          <p className="text-sm text-muted max-w-md">
            Click the heart icon on any movie, TV show, episode, home video, or live
            channel across your libraries to build your personal watchlist.
          </p>
        </div>
      ) : (
        <>
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 sm:gap-6">
            {data.items.map((item) => (
              <FavoritePosterCard
                key={item.card.id}
                card={item.card}
                onSelect={() => setSelectedItemId(item.card.id)}
              />
            ))}
          </div>

          {data.hasMorePages && data.items.length < data.totalCount && (
            <div className="flex flex-col items-center gap-2 pt-4">
              {loadMoreError && (
                <p role="alert" className="text-xs text-highlight">
                  {loadMoreError}
                </p>
              )}
              <button
                type="button"
                onClick={handleLoadMore}
                disabled={loadingMore}
                className="min-w-7 min-h-7 inline-flex items-center gap-2 px-6 py-2.5 rounded-xl bg-panel hover:bg-panel-hover text-sm font-semibold text-text-main border border-border-subtle transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              >
                {loadingMore ? (
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
        </>
      )}

      <ItemDetailsModal
        itemId={selectedItemId}
        isOpen={selectedItemId !== null}
        currentUser={currentUser}
        onClose={() => setSelectedItemId(null)}
        onPlayItem={(id) => {
          setSelectedItemId(null);
          onPlayItem(id);
        }}
        onNavigateToFolder={
          onNavigateToFolder
            ? (libId, folderPath) => {
                setSelectedItemId(null);
                onNavigateToFolder(libId, folderPath);
              }
            : undefined
        }
      />
    </div>
  );
};

export default FavoritesScreen;
