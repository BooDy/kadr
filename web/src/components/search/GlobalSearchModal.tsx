import {
  useState,
  useEffect,
  useRef,
  useCallback,
  type FC,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import {
  Search,
  X,
  Film,
  Tv,
  Video,
  Radio,
  Star,
  Loader2,
  CornerDownLeft,
  ArrowUp,
  ArrowDown,
} from 'lucide-react';
import { api, describeApiError } from '../../api/client';
import type { SearchResultItem } from '../../types';

export type SearchFilterType = 'all' | 'movie' | 'show' | 'episode' | 'home_videos';

export interface GlobalSearchModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectItem: (item: SearchResultItem) => void;
  libraryId?: string;
}

const FILTER_PILLS: ReadonlyArray<{ id: SearchFilterType; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'movie', label: 'Movies' },
  { id: 'show', label: 'Shows' },
  { id: 'episode', label: 'Episodes' },
  { id: 'home_videos', label: 'Home Videos' },
];

function formatEpisodeCode(season?: number | null, episode?: number | null): string | null {
  if (typeof season === 'number' && typeof episode === 'number') {
    return `S${String(season).padStart(2, '0')}E${String(episode).padStart(2, '0')}`;
  }
  if (typeof season === 'number') {
    return `S${String(season).padStart(2, '0')}`;
  }
  if (typeof episode === 'number') {
    return `E${String(episode).padStart(2, '0')}`;
  }
  return null;
}

function formatDuration(seconds: number): string | null {
  if (!seconds || seconds <= 0) return null;
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (hrs > 0) {
    return mins > 0 ? `${hrs}h ${mins}m` : `${hrs}h`;
  }
  return `${Math.max(1, mins)}m`;
}

function buildSubtitle(item: SearchResultItem): string | null {
  const parts: string[] = [];
  if (item.director) {
    parts.push(`Dir. ${item.director}`);
  }
  if (item.genres && item.genres.length > 0) {
    parts.push(item.genres.slice(0, 3).join(', '));
  }
  const dur = formatDuration(item.duration_seconds);
  if (dur) {
    parts.push(dur);
  }
  return parts.length > 0 ? parts.join(' • ') : null;
}

const SearchResultPoster: FC<{ item: SearchResultItem }> = ({ item }) => {
  const [imgFailed, setImgFailed] = useState(false);

  const fallbackIcon = () => {
    switch (item.item_type) {
      case 'show':
      case 'episode':
      case 'season':
        return <Tv className="w-5 h-5 text-muted" />;
      case 'home_videos':
        return <Video className="w-5 h-5 text-muted" />;
      case 'live_channel':
        return <Radio className="w-5 h-5 text-muted" />;
      default:
        return <Film className="w-5 h-5 text-muted" />;
    }
  };

  return (
    <div className="relative w-12 h-18 sm:w-14 sm:h-20 rounded-lg overflow-hidden bg-canvas border border-border-subtle flex-shrink-0 flex items-center justify-center">
      {!imgFailed && item.poster_url ? (
        <img
          src={item.poster_url}
          alt={item.title}
          onError={() => setImgFailed(true)}
          className="w-full h-full object-cover"
        />
      ) : (
        fallbackIcon()
      )}
    </div>
  );
};

export const GlobalSearchModal: FC<GlobalSearchModalProps> = ({
  isOpen,
  onClose,
  onSelectItem,
  libraryId,
}) => {
  const [query, setQuery] = useState('');
  const [activeFilter, setActiveFilter] = useState<SearchFilterType>('all');
  const [items, setItems] = useState<SearchResultItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [hasSearched, setHasSearched] = useState(false);

  const inputRef = useRef<HTMLInputElement | null>(null);
  const rowRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const latestRequestId = useRef(0);

  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setActiveFilter('all');
      setItems([]);
      setTotal(0);
      setLoading(false);
      setError(null);
      setSelectedIndex(0);
      setHasSearched(false);
      // Focus search input immediately when modal opens
      const focusTimer = setTimeout(() => {
        inputRef.current?.focus();
      }, 0);
      return () => clearTimeout(focusTimer);
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;

    const trimmed = query.trim();
    if (!trimmed) {
      latestRequestId.current += 1;
      setItems([]);
      setTotal(0);
      setLoading(false);
      setError(null);
      setSelectedIndex(0);
      setHasSearched(false);
      return;
    }

    setLoading(true);
    setError(null);
    const reqId = ++latestRequestId.current;

    const timer = setTimeout(() => {
      api
        .search({
          q: trimmed,
          item_type: activeFilter === 'all' ? undefined : activeFilter,
          library_id: libraryId,
          limit: 30,
        })
        .then((res) => {
          if (reqId !== latestRequestId.current) return;
          setItems(res.items || []);
          setTotal(res.total ?? (res.items ? res.items.length : 0));
          setSelectedIndex(0);
          setHasSearched(true);
          setLoading(false);
        })
        .catch((err: unknown) => {
          if (reqId !== latestRequestId.current) return;
          setError(describeApiError(err));
          setItems([]);
          setTotal(0);
          setHasSearched(true);
          setLoading(false);
        });
    }, 150);

    return () => clearTimeout(timer);
  }, [isOpen, query, activeFilter, libraryId]);

  useEffect(() => {
    const activeRow = rowRefs.current[selectedIndex];
    activeRow?.scrollIntoView?.({ block: 'nearest' });
  }, [selectedIndex]);

  const handleKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLElement>) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onClose();
        return;
      }

      if (e.key === 'ArrowDown') {
        if (items.length > 0) {
          e.preventDefault();
          setSelectedIndex((prev) => (prev + 1) % items.length);
        }
        return;
      }

      if (e.key === 'ArrowUp') {
        if (items.length > 0) {
          e.preventDefault();
          setSelectedIndex((prev) => (prev - 1 + items.length) % items.length);
        }
        return;
      }

      if (e.key === 'Enter') {
        const target = e.target as HTMLElement | null;
        // Allow filter buttons or close button to handle Enter naturally if focused
        if (target && target.tagName === 'BUTTON' && target !== rowRefs.current[selectedIndex]) {
          return;
        }
        if (items.length > 0 && items[selectedIndex]) {
          e.preventDefault();
          onSelectItem(items[selectedIndex]);
        }
      }
    },
    [items, selectedIndex, onClose, onSelectItem]
  );

  if (!isOpen) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Global Search"
      onKeyDown={handleKeyDown}
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/80 backdrop-blur-sm px-4 pt-12 sm:pt-20 pb-8"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-3xl rounded-2xl border border-border-subtle bg-panel text-text-main overflow-hidden flex flex-col max-h-[82vh]"
      >
        {/* Top Search Input Row */}
        <div className="flex items-center gap-3 px-4 py-3.5 border-b border-border-subtle bg-canvas/50">
          <Search className="w-5 h-5 text-accent flex-shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search movies, shows, episodes, cast, directors, genres..."
            aria-label="Global media search"
            dir="auto"
            className="flex-1 bg-transparent text-sm sm:text-base text-text-main placeholder:text-muted focus:outline-none"
          />
          {loading && (
            <Loader2
              aria-label="Searching"
              className="w-4 h-4 text-accent animate-spin flex-shrink-0"
            />
          )}
          {query && (
            <button
              type="button"
              onClick={() => {
                setQuery('');
                inputRef.current?.focus();
              }}
              aria-label="Clear search query"
              className="px-2 py-1 rounded-lg text-xs text-muted hover:text-text-main hover:bg-panel-hover transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              Clear
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close search"
            className="p-1.5 rounded-xl text-muted hover:text-text-main hover:bg-panel-hover border border-transparent hover:border-border-subtle transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Media Type Filter Pills */}
        <div
          role="group"
          aria-label="Search media type filters"
          className="flex items-center gap-2 px-4 py-2.5 border-b border-border-subtle bg-panel overflow-x-auto"
        >
          {FILTER_PILLS.map((pill) => {
            const isSelected = activeFilter === pill.id;
            return (
              <button
                key={pill.id}
                type="button"
                aria-pressed={isSelected}
                onClick={() => {
                  setActiveFilter(pill.id);
                  inputRef.current?.focus();
                }}
                className={`px-3 py-1.5 rounded-xl text-xs font-medium whitespace-nowrap transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  isSelected
                    ? 'bg-accent text-canvas font-semibold border border-accent'
                    : 'bg-canvas/60 text-muted hover:text-text-main hover:bg-panel-hover border border-border-subtle'
                }`}
              >
                {pill.label}
              </button>
            );
          })}
          {hasSearched && !loading && total > 0 && (
            <span className="ml-auto text-xs font-mono text-muted whitespace-nowrap pl-2">
              {total} {total === 1 ? 'match' : 'matches'}
            </span>
          )}
        </div>

        {/* Results / Empty / Error Area */}
        <div className="flex-1 overflow-y-auto p-3 space-y-1.5">
          {error ? (
            <div className="py-10 px-4 text-center text-sm text-cta">{error}</div>
          ) : !query.trim() ? (
            <div className="py-12 px-4 text-center space-y-2">
              <p className="text-sm font-medium text-text-main">
                Instant search across your libraries
              </p>
              <p className="text-xs text-muted">
                Find titles, series episodes (e.g. S01E02), directors, cast, studios, genres, or release years.
              </p>
            </div>
          ) : hasSearched && items.length === 0 && !loading ? (
            <div className="py-12 px-4 text-center space-y-1">
              <p className="text-sm font-medium text-text-main">
                No results found for &ldquo;{query.trim()}&rdquo;
              </p>
              <p className="text-xs text-muted">
                Try another title, actor, director, genre, or switch filter tabs.
              </p>
            </div>
          ) : (
            <div role="listbox" aria-label="Search results" className="space-y-1.5">
              {items.map((item, idx) => {
                const isSelected = idx === selectedIndex;
                const epCode = formatEpisodeCode(item.season, item.episode);
                const subtitle = buildSubtitle(item);
                const hasProgress =
                  typeof item.playback_progress === 'number' && item.playback_progress > 0;
                const progressPercent = hasProgress
                  ? Math.min(100, Math.max(0, Math.round((item.playback_progress || 0) * 100)))
                  : 0;

                return (
                  <button
                    key={item.id}
                    ref={(el) => {
                      rowRefs.current[idx] = el;
                    }}
                    type="button"
                    role="option"
                    aria-selected={isSelected}
                    onMouseEnter={() => setSelectedIndex(idx)}
                    onClick={() => onSelectItem(item)}
                    className={`w-full text-left flex items-center gap-3.5 p-2.5 rounded-xl border transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                      isSelected
                        ? 'bg-panel-hover border-highlight ring-2 ring-highlight/60'
                        : 'bg-canvas/40 border-border-subtle hover:bg-panel-hover'
                    }`}
                  >
                    <SearchResultPoster item={item} />

                    <div className="flex-1 min-w-0 space-y-1">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span
                          dir="auto"
                          className="text-sm sm:text-base font-semibold text-text-main truncate max-w-full"
                        >
                          {item.title}
                        </span>
                        {item.original_title && item.original_title !== item.title && (
                          <span
                            dir="auto"
                            className="text-xs text-muted truncate max-w-[14rem]"
                          >
                            ({item.original_title})
                          </span>
                        )}
                        {item.badge && (
                          <span className="px-1.5 py-0.5 rounded text-[10px] font-bold tracking-wider uppercase bg-highlight/20 text-highlight border border-highlight/40">
                            {item.badge}
                          </span>
                        )}
                      </div>

                      {/* Context Row: Episode S01E02 + Series, Year, Rating, Library */}
                      <div className="flex items-center gap-2 flex-wrap text-xs text-muted">
                        {epCode && (
                          <span className="px-1.5 py-0.5 rounded font-mono text-[11px] font-semibold bg-accent/15 text-accent border border-accent/30">
                            {epCode}
                          </span>
                        )}
                        {item.series_title && (
                          <span className="font-medium text-text-main/90 truncate max-w-[14rem]">
                            {item.series_title}
                          </span>
                        )}
                        {typeof item.release_year === 'number' && item.release_year > 0 && (
                          <span className="font-mono">{item.release_year}</span>
                        )}
                        {typeof item.rating === 'number' && item.rating > 0 && (
                          <span className="inline-flex items-center gap-0.5 text-highlight font-medium">
                            <Star className="w-3 h-3 fill-highlight text-highlight" />
                            {item.rating.toFixed(1)}
                          </span>
                        )}
                        <span className="px-1.5 py-0.5 rounded bg-canvas/80 border border-border-subtle text-[11px] text-muted">
                          {item.library_name}
                        </span>
                      </div>

                      {subtitle && (
                        <p className="text-xs text-muted truncate">{subtitle}</p>
                      )}

                      {hasProgress && (
                        <div className="pt-1 flex items-center gap-2">
                          <div
                            role="progressbar"
                            aria-label={`Playback progress for ${item.title}`}
                            aria-valuemin={0}
                            aria-valuemax={100}
                            aria-valuenow={progressPercent}
                            className="h-1.5 flex-1 max-w-xs rounded-full bg-muted/60 overflow-hidden"
                          >
                            <div
                              className="h-full bg-accent rounded-full"
                              style={{ width: `${progressPercent}%` }}
                            />
                          </div>
                          <span className="text-[10px] font-mono text-accent">
                            {progressPercent}%
                          </span>
                        </div>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer Keyboard Navigation Legend */}
        <div className="flex items-center justify-between px-4 py-2.5 border-t border-border-subtle bg-canvas/60 text-[11px] text-muted">
          <div className="flex items-center gap-4">
            <span className="inline-flex items-center gap-1">
              <ArrowUp className="w-3 h-3" />
              <ArrowDown className="w-3 h-3" />
              Navigate
            </span>
            <span className="inline-flex items-center gap-1">
              <CornerDownLeft className="w-3 h-3" />
              Open Details
            </span>
          </div>
          <span className="font-mono">ESC to close</span>
        </div>
      </div>
    </div>
  );
};

export default GlobalSearchModal;
