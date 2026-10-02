import { useState, useEffect, useRef, type FC } from 'react';
import { ChevronLeft, ChevronRight, Film } from 'lucide-react';
import type { CardViewModel } from '../../types';
import { api } from '../../api/client';

export interface CarouselWidgetProps {
  title: string;
  items?: CardViewModel[];
  widgetId?: string;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (itemId: number) => void;
}

interface MediaCardProps {
  item: CardViewModel;
  onClick: () => void;
}

const MediaCard: FC<MediaCardProps> = ({ item, onClick }) => {
  const [imageFailed, setImageFailed] = useState(false);

  const posterUrl =
    item.poster_url || (item.id ? api.getArtworkUrl(item.id, 'poster') : undefined);

  const hasProgress =
    typeof item.playback_progress === 'number' && item.playback_progress > 0;
  const progressPercent = hasProgress
    ? Math.min(100, Math.max(0, Math.round((item.playback_progress || 0) * 100)))
    : 0;

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
      className="group relative flex-shrink-0 w-36 sm:w-44 md:w-48 cursor-pointer focus:outline-none"
    >
      {/* Poster Container */}
      <div className="relative aspect-[2/3] w-full rounded-2xl overflow-hidden bg-zinc-900 border border-zinc-800/80 shadow-md group-hover:shadow-rose-950/20 group-hover:border-zinc-700/80 group-hover:scale-[1.03] transition-all duration-200">
        {!imageFailed && posterUrl ? (
          <img
            src={posterUrl}
            alt={item.title}
            onError={() => setImageFailed(true)}
            className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 transition-all duration-200"
          />
        ) : (
          <div className="w-full h-full bg-gradient-to-br from-zinc-800/80 via-zinc-900 to-zinc-950 flex flex-col items-center justify-center p-4 text-center">
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

        {/* Resume Progress Bar */}
        {hasProgress && (
          <div className="absolute bottom-0 inset-x-0 h-1.5 bg-zinc-950/80">
            <div
              className="h-full bg-rose-500"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        )}
      </div>

      {/* Card Metadata */}
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

export const CarouselWidget: FC<CarouselWidgetProps> = ({
  title,
  items: initialItems,
  widgetId,
  onSelectItem,
}) => {
  const [items, setItems] = useState<CardViewModel[]>(initialItems || []);
  const [loading, setLoading] = useState(false);
  const railRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (initialItems) {
      setItems(initialItems);
      return;
    }

    if (!widgetId) return;

    let isMounted = true;
    setLoading(true);

    api
      .getWidgetData(widgetId, 0, 20)
      .then((data) => {
        if (isMounted) {
          setItems(data);
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

  const scroll = (direction: 'left' | 'right') => {
    if (!railRef.current) return;
    const scrollAmount = direction === 'left' ? -480 : 480;
    railRef.current.scrollBy({ left: scrollAmount, behavior: 'smooth' });
  };

  if (!loading && items.length === 0) {
    return null;
  }

  return (
    <section className="space-y-3.5 py-2">
      {/* Rail Header with Title and Scroll Controls */}
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold tracking-tight text-white flex items-center gap-2">
          {title}
        </h2>

        <div className="flex items-center gap-1.5">
          <button
            onClick={() => scroll('left')}
            aria-label={`Scroll ${title} left`}
            className="p-1.5 rounded-lg bg-zinc-900/80 hover:bg-zinc-800 text-zinc-400 hover:text-white border border-zinc-800 transition-colors cursor-pointer"
          >
            <ChevronLeft className="w-5 h-5" />
          </button>
          <button
            onClick={() => scroll('right')}
            aria-label={`Scroll ${title} right`}
            className="p-1.5 rounded-lg bg-zinc-900/80 hover:bg-zinc-800 text-zinc-400 hover:text-white border border-zinc-800 transition-colors cursor-pointer"
          >
            <ChevronRight className="w-5 h-5" />
          </button>
        </div>
      </div>

      {/* Horizontal Scroll Rail */}
      <div
        ref={railRef}
        className="flex items-start gap-4 overflow-x-auto scrollbar-none pb-2 scroll-smooth"
        style={{ scrollbarWidth: 'none', msOverflowStyle: 'none' }}
      >
        {items.map((item) => (
          <MediaCard
            key={item.id}
            item={item}
            onClick={() => onSelectItem(item)}
          />
        ))}
      </div>
    </section>
  );
};
export default CarouselWidget;
