import { useState, useEffect, type FC } from 'react';
import { Film, Loader2 } from 'lucide-react';
import type { CardViewModel } from '../../types';
import { api } from '../../api/client';

export interface GridWidgetProps {
  title?: string;
  columns?: number;
  items?: CardViewModel[];
  widgetId?: string;
  totalCount?: number;
  nextCursor?: string;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (itemId: number) => void;
}

interface GridCardProps {
  item: CardViewModel;
  onClick: () => void;
}

const GridCard: FC<GridCardProps> = ({ item, onClick }) => {
  const [imageFailed, setImageFailed] = useState(false);

  const posterUrl =
    item.poster_url || (item.id ? api.getArtworkUrl(item.id, 'poster') : undefined);

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
      className="group relative flex flex-col cursor-pointer focus:outline-none"
    >
      {/* Poster */}
      <div className="relative aspect-[2/3] w-full rounded-2xl overflow-hidden bg-zinc-900 border border-zinc-800/80 shadow-md group-hover:border-zinc-700/80 group-hover:scale-105 transition-transform duration-200">
        {!imageFailed && posterUrl ? (
          <img
            src={posterUrl}
            alt={item.title}
            onError={() => setImageFailed(true)}
            className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 transition-all duration-200"
          />
        ) : (
          <div className="w-full h-full bg-gradient-to-br from-zinc-850 via-zinc-900 to-zinc-950 flex flex-col items-center justify-center p-4 text-center">
            <Film className="w-10 h-10 text-zinc-600 mb-2 group-hover:text-rose-500/80 transition-colors" />
            <span className="text-xs font-semibold text-zinc-300 line-clamp-3">
              {item.title}
            </span>
          </div>
        )}

        {/* Badge Overlay */}
        {item.badge && (
          <div className="absolute top-2 left-2">
            <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wider uppercase bg-rose-600/90 text-white shadow-sm">
              {item.badge}
            </span>
          </div>
        )}
      </div>

      {/* Metadata */}
      <div className="mt-2.5 space-y-0.5">
        <h3 className="text-sm font-semibold text-zinc-100 truncate group-hover:text-rose-400 transition-colors">
          {item.title}
        </h3>
        <div className="flex items-center gap-1.5 text-xs text-zinc-400 truncate">
          {item.release_year && <span>{item.release_year}</span>}
          {item.release_year && item.subtitle && <span>•</span>}
          {item.subtitle && <span className="truncate">{item.subtitle}</span>}
        </div>
      </div>
    </div>
  );
};

export const GridWidget: FC<GridWidgetProps> = ({
  title,
  columns = 6,
  items: initialItems,
  widgetId,
  onSelectItem,
}) => {
  const [items, setItems] = useState<CardViewModel[]>(initialItems || []);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [hasMore, setHasMore] = useState(true);

  useEffect(() => {
    if (initialItems) {
      setItems(initialItems);
      return;
    }

    if (!widgetId) return;

    let isMounted = true;
    setLoading(true);

    api
      .getWidgetData(widgetId, 0, 24)
      .then((data) => {
        if (isMounted) {
          setItems(data);
          if (data.length < 24) setHasMore(false);
          setLoading(false);
        }
      })
      .catch(() => {
        if (isMounted) setLoading(false);
      });

    return () => {
      isMounted = false;
    };
  }, [initialItems, widgetId]);

  const loadMore = async () => {
    if (!widgetId || loading) return;
    const nextPage = page + 1;
    setLoading(true);
    try {
      const moreItems = await api.getWidgetData(widgetId, nextPage, 24);
      if (moreItems.length < 24) {
        setHasMore(false);
      }
      setItems((prev) => [...prev, ...moreItems]);
      setPage(nextPage);
    } catch {
      setHasMore(false);
    } finally {
      setLoading(false);
    }
  };

  // Determine grid column styles
  const colClass =
    columns <= 3
      ? 'grid-cols-2 sm:grid-cols-3'
      : columns <= 4
      ? 'grid-cols-2 sm:grid-cols-3 md:grid-cols-4'
      : 'grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6';

  return (
    <section className="space-y-4 py-2">
      {title && (
        <h2 className="text-xl font-bold tracking-tight text-white">{title}</h2>
      )}

      {/* Media Grid */}
      <div className={`grid ${colClass} gap-4 sm:gap-6`}>
        {items.map((item) => (
          <GridCard
            key={item.id}
            item={item}
            onClick={() => onSelectItem(item)}
          />
        ))}
      </div>

      {/* Load More Button for paginated widgets */}
      {widgetId && hasMore && items.length >= 24 && (
        <div className="flex justify-center pt-6">
          <button
            onClick={loadMore}
            disabled={loading}
            className="flex items-center gap-2 px-6 py-2.5 rounded-xl bg-zinc-900 hover:bg-zinc-800 text-sm font-semibold text-zinc-300 hover:text-white border border-zinc-800 transition-colors disabled:opacity-50 cursor-pointer"
          >
            {loading ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin text-rose-500" />
                Loading...
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
export default GridWidget;
