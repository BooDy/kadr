import { useState, useEffect, useCallback, type FC } from 'react';
import { Loader2, AlertCircle, RotateCcw, Folder, LayoutGrid } from 'lucide-react';
import type { ScreenLayout, CardViewModel, WidgetNode } from '../../types';
import { api } from '../../api/client';
import { SpotlightWidget } from './SpotlightWidget';
import { CarouselWidget } from './CarouselWidget';
import { GridWidget } from './GridWidget';
import { ItemDetailsModal } from './ItemDetailsModal';
import { FolderBrowser } from './FolderBrowser';

export interface BrowseScreenProps {
  screenId: string;
  onPlayItem: (itemId: number) => void;
}

// Wrapper to handle unhydrated spotlight hero widget
const HydratedSpotlight: FC<{
  widget: Extract<WidgetNode, { type: 'hero_banner' }>;
  onPlay: (itemId: number) => void;
  onMoreInfo: (itemId: number) => void;
}> = ({ widget, onPlay, onMoreInfo }) => {
  const [item, setItem] = useState<CardViewModel | undefined>(widget.data);

  useEffect(() => {
    if (widget.data) {
      setItem(widget.data);
      return;
    }

    let isMounted = true;
    api
      .getWidgetData(widget.id, 0, 1)
      .then((items) => {
        if (isMounted && items && items.length > 0) {
          setItem(items[0]);
        }
      })
      .catch(() => {});

    return () => {
      isMounted = false;
    };
  }, [widget.id, widget.data]);

  if (!item) return null;

  return <SpotlightWidget item={item} onPlay={onPlay} onMoreInfo={onMoreInfo} />;
};

export const BrowseScreen: FC<BrowseScreenProps> = ({
  screenId,
  onPlayItem,
}) => {
  const [layout, setLayout] = useState<ScreenLayout | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<number | null>(null);
  const [viewMode, setViewMode] = useState<'catalog' | 'folders'>('catalog');

  useEffect(() => {
    setViewMode('catalog');
  }, [screenId]);

  const fetchScreen = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.getScreen(screenId);
      setLayout(data);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setError(`Failed to load screen layout: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, [screenId]);

  useEffect(() => {
    fetchScreen();
  }, [fetchScreen]);

  return (
    <div className="min-h-screen bg-canvas text-text-main space-y-8 pb-12">
      {/* Top Header / View Mode Switcher for Library Screens */}
      {screenId !== 'home' && (
        <div className="flex items-center justify-end">
          <div
            role="group"
            aria-label="View mode"
            className="inline-flex items-center p-1 rounded-xl bg-panel border border-border-subtle shadow-xs"
          >
            <button
              type="button"
              onClick={() => setViewMode('catalog')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                viewMode === 'catalog'
                  ? 'bg-accent text-canvas font-semibold shadow-xs'
                  : 'text-muted hover:text-text-main hover:bg-panel-hover'
              }`}
            >
              <LayoutGrid className="w-4 h-4" />
              <span>Catalog</span>
            </button>
            <button
              type="button"
              onClick={() => setViewMode('folders')}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                viewMode === 'folders'
                  ? 'bg-accent text-canvas font-semibold shadow-xs'
                  : 'text-muted hover:text-text-main hover:bg-panel-hover'
              }`}
            >
              <Folder className="w-4 h-4" />
              <span>Folders</span>
            </button>
          </div>
        </div>
      )}

      {/* Main View: Folders Browser or Declarative Catalog */}
      {screenId !== 'home' && viewMode === 'folders' ? (
        <FolderBrowser libraryId={screenId} onPlayItem={onPlayItem} />
      ) : (
        <>
          {/* Loading State Skeleton */}
          {loading && (
            <div className="space-y-8 animate-pulse">
              <div className="w-full h-96 rounded-3xl bg-panel border border-border-subtle flex items-center justify-center">
                <Loader2 className="w-8 h-8 animate-spin text-cta" />
              </div>
              <div className="space-y-3">
                <div className="h-6 w-48 bg-panel border border-border-subtle rounded-md" />
                <div className="flex gap-4 overflow-hidden">
                  {[1, 2, 3, 4, 5].map((i) => (
                    <div
                      key={i}
                      className="w-48 aspect-[2/3] rounded-2xl bg-panel border border-border-subtle flex-shrink-0"
                    />
                  ))}
                </div>
              </div>
            </div>
          )}

          {/* Error State with Retry */}
          {!loading && error && (
            <div className="p-8 rounded-3xl bg-panel border border-border-subtle text-center space-y-4 max-w-lg mx-auto">
              <AlertCircle className="w-12 h-12 text-cta mx-auto" />
              <h3 className="text-lg font-bold text-text-main">Error Loading Content</h3>
              <p className="text-sm text-muted">{error}</p>
              <button
                onClick={() => fetchScreen()}
                className="inline-flex items-center gap-2 px-5 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-white text-sm font-semibold shadow-md transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <RotateCcw className="w-4 h-4" />
                Retry
              </button>
            </div>
          )}

          {/* Hydrated Screen Widgets */}
          {!loading && !error && layout && (
            <div className="space-y-8">
              {layout.widgets.map((widget) => {
                const displayType =
                  (widget as { display_type?: string }).display_type || widget.type;

                if (displayType === 'spotlight' || widget.type === 'hero_banner') {
                  return (
                    <HydratedSpotlight
                      key={widget.id}
                      widget={widget as Extract<WidgetNode, { type: 'hero_banner' }>}
                      onPlay={onPlayItem}
                      onMoreInfo={(id) => setSelectedItemId(id)}
                    />
                  );
                }

                if (displayType === 'carousel' || widget.type === 'carousel') {
                  const carouselWidget = widget as Extract<
                    WidgetNode,
                    { type: 'carousel' }
                  >;
                  return (
                    <CarouselWidget
                      key={carouselWidget.id}
                      widgetId={carouselWidget.id}
                      title={carouselWidget.title}
                      items={carouselWidget.items}
                      onSelectItem={(item) => setSelectedItemId(item.id)}
                      onPlayItem={onPlayItem}
                    />
                  );
                }

                if (displayType === 'grid' || widget.type === 'grid') {
                  const gridWidget = widget as Extract<WidgetNode, { type: 'grid' }>;
                  return (
                    <GridWidget
                      key={gridWidget.id}
                      widgetId={gridWidget.id}
                      title={gridWidget.title}
                      columns={gridWidget.columns}
                      items={gridWidget.items}
                      totalCount={gridWidget.total_count}
                      nextCursor={gridWidget.next_cursor}
                      onSelectItem={(item) => setSelectedItemId(item.id)}
                      onPlayItem={onPlayItem}
                    />
                  );
                }

                return null;
              })}
            </div>
          )}
        </>
      )}

      {/* Item Details Inspection Modal */}
      <ItemDetailsModal
        itemId={selectedItemId}
        isOpen={selectedItemId !== null}
        onClose={() => setSelectedItemId(null)}
        onPlay={onPlayItem}
      />
    </div>
  );
};
export default BrowseScreen;
