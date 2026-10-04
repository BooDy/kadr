import { useState, useEffect, useCallback, type FC } from 'react';
import {
  Tv,
  Tablet,
  Smartphone,
  Layers,
  Loader2,
  AlertCircle,
  Eye,
  EyeOff,
} from 'lucide-react';
import { api } from '../../api/client';
import type {
  ScreenLayout,
  ScreenSummary,
  WidgetNode,
  CardViewModel,
} from '../../types';
import { SpotlightWidget } from '../browse/SpotlightWidget';
import { CarouselWidget } from '../browse/CarouselWidget';
import { GridWidget } from '../browse/GridWidget';
import { ItemDetailsModal } from '../browse/ItemDetailsModal';

export type ViewportMode = 'tv' | 'tablet' | 'mobile';

export interface LayoutStudioProps {
  onPlayItem?: (itemId: number) => void;
}

// Hydrated spotlight helper for LayoutStudio preview
const HydratedSpotlight: FC<{
  widget: Extract<WidgetNode, { type: 'hero_banner' }>;
  onPlay?: (itemId: number) => void;
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

  return (
    <SpotlightWidget
      item={item}
      onPlay={(id) => onPlay?.(id)}
      onMoreInfo={onMoreInfo}
    />
  );
};

export const LayoutStudio: FC<LayoutStudioProps> = ({ onPlayItem }) => {
  const [screens, setScreens] = useState<ScreenSummary[]>([
    { id: 'home', title: 'Home' },
    { id: 'movies', title: 'Movies' },
    { id: 'shows', title: 'Shows' },
  ]);
  const [selectedScreenId, setSelectedScreenId] = useState<string>('home');
  const [viewport, setViewport] = useState<ViewportMode>('tv');
  const [layout, setLayout] = useState<ScreenLayout | null>(null);
  const [disabledWidgetIds, setDisabledWidgetIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<number | null>(null);

  // Fetch available screens
  useEffect(() => {
    api
      .getScreens()
      .then((data) => {
        if (data && data.length > 0) {
          setScreens(data);
          if (!data.some((s) => s.id === selectedScreenId)) {
            setSelectedScreenId(data[0].id);
          }
        }
      })
      .catch(() => {});
  }, [selectedScreenId]);

  // Fetch screen AST layout whenever selectedScreenId changes
  const fetchLayout = useCallback(async (screenId: string) => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.getScreen(screenId);
      setLayout(data);
      setDisabledWidgetIds(new Set());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setError(`Failed to load screen layout: ${msg}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchLayout(selectedScreenId);
  }, [selectedScreenId, fetchLayout]);

  const toggleWidget = (widgetId: string) => {
    setDisabledWidgetIds((prev) => {
      const next = new Set(prev);
      if (next.has(widgetId)) {
        next.delete(widgetId);
      } else {
        next.add(widgetId);
      }
      return next;
    });
  };

  const getViewportWrapperStyle = () => {
    switch (viewport) {
      case 'tablet':
        return 'max-w-[768px] mx-auto bg-panel border border-border-subtle rounded-2xl p-4 shadow-2xl';
      case 'mobile':
        return 'max-w-[390px] mx-auto bg-panel border border-border-subtle rounded-2xl p-3 shadow-2xl';
      case 'tv':
      default:
        return 'w-full bg-panel border border-border-subtle rounded-2xl p-6 shadow-2xl';
    }
  };

  return (
    <div className="space-y-6 pb-16">
      {/* Top Header & Controls */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-border-subtle pb-6">
        <div>
          <h2 className="text-2xl font-bold tracking-tight text-text-main flex items-center gap-2.5">
            <Layers className="h-6 w-6 text-accent" />
            Layout Studio
          </h2>
          <p className="text-sm text-muted mt-1">
            Declarative AST screen inspector and multi-device viewport simulator.
          </p>
        </div>

        {/* Viewport Simulator Mode Selector */}
        <div className="flex items-center gap-1.5 p-1 rounded-xl bg-panel border border-border-subtle self-start md:self-auto">
          <button
            onClick={() => setViewport('tv')}
            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer ${
              viewport === 'tv'
                ? 'bg-accent text-canvas font-semibold shadow-sm'
                : 'text-muted hover:text-text-main hover:bg-panel-hover'
            }`}
          >
            <Tv className="w-3.5 h-3.5" />
            <span>TV (16:9)</span>
          </button>
          <button
            onClick={() => setViewport('tablet')}
            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer ${
              viewport === 'tablet'
                ? 'bg-accent text-canvas font-semibold shadow-sm'
                : 'text-muted hover:text-text-main hover:bg-panel-hover'
            }`}
          >
            <Tablet className="w-3.5 h-3.5" />
            <span>Tablet (4:3)</span>
          </button>
          <button
            onClick={() => setViewport('mobile')}
            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer ${
              viewport === 'mobile'
                ? 'bg-accent text-canvas font-semibold shadow-sm'
                : 'text-muted hover:text-text-main hover:bg-panel-hover'
            }`}
          >
            <Smartphone className="w-3.5 h-3.5" />
            <span>Mobile (9:16)</span>
          </button>
        </div>
      </div>

      {/* Screen Selector Tabs */}
      <div className="flex items-center gap-2 overflow-x-auto pb-1">
        <span className="text-xs font-semibold text-muted uppercase tracking-wider mr-2">
          Screens:
        </span>
        {screens.map((s) => (
          <button
            key={s.id}
            onClick={() => setSelectedScreenId(s.id)}
            className={`px-4 py-2 rounded-xl text-sm transition-all cursor-pointer ${
              selectedScreenId === s.id
                ? 'bg-panel text-accent font-semibold border border-border-subtle rounded-xl shadow-md'
                : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl border border-transparent'
            }`}
          >
            {s.title || s.id}
          </button>
        ))}
      </div>

      {/* Main Studio Body: Widget Tree Inspector (Sidebar) & Live Preview */}
      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6 items-start">
        {/* Widget Tree Inspector */}
        <div className="lg:col-span-1 rounded-2xl border border-border-subtle bg-panel/60 backdrop-blur-md p-4 space-y-4">
          <div className="flex items-center justify-between border-b border-border-subtle pb-3">
            <h3 className="text-sm font-bold text-text-main flex items-center gap-2">
              <Layers className="w-4 h-4 text-accent" />
              Widget Tree
            </h3>
            <span className="text-[11px] font-mono text-muted bg-canvas px-2 py-0.5 rounded-md border border-border-subtle">
              {layout?.widgets.length || 0} nodes
            </span>
          </div>

          {loading ? (
            <div className="py-8 flex flex-col items-center justify-center text-muted gap-2">
              <Loader2 className="w-5 h-5 animate-spin text-accent" />
              <span className="text-xs">Loading widgets...</span>
            </div>
          ) : !layout || layout.widgets.length === 0 ? (
            <div className="py-6 text-center text-xs text-muted">
              No widgets configured for this screen.
            </div>
          ) : (
            <div className="space-y-2">
              {layout.widgets.map((widget, index) => {
                const isEnabled = !disabledWidgetIds.has(widget.id);
                const displayType =
                  (widget as { display_type?: string }).display_type ||
                  widget.type;
                const widgetTitle =
                  'title' in widget ? (widget.title as string) : null;

                return (
                  <div
                    key={widget.id}
                    className={`font-mono text-xs text-text-main bg-canvas/80 p-3 rounded-xl border border-border-subtle transition-all ${
                      isEnabled
                        ? 'hover:border-border-subtle/80 hover:bg-canvas'
                        : 'opacity-50'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="space-y-1 min-w-0 flex-1">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="font-semibold text-text-main truncate">
                            {widget.id}
                          </span>
                          <span className="rounded bg-accent/15 px-1.5 py-0.5 text-[10px] font-mono font-medium text-accent border border-accent/20">
                            {displayType}
                          </span>
                        </div>
                        {widgetTitle && (
                          <p className="text-[11px] text-muted truncate">
                            {widgetTitle}
                          </p>
                        )}
                        <span className="text-[10px] text-muted font-mono">
                          #{index + 1}
                        </span>
                      </div>

                      {/* Enable/Disable Toggle */}
                      <label className="flex items-center cursor-pointer p-1 rounded-lg hover:bg-panel-hover transition-colors">
                        <input
                          type="checkbox"
                          aria-label={`Toggle ${widget.id}`}
                          checked={isEnabled}
                          onChange={() => toggleWidget(widget.id)}
                          className="sr-only"
                        />
                        {isEnabled ? (
                          <Eye className="w-4 h-4 text-accent" />
                        ) : (
                          <EyeOff className="w-4 h-4 text-muted" />
                        )}
                      </label>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Live Interactive Preview Pane */}
        <div className="lg:col-span-3 space-y-4">
          <div className="flex items-center justify-between text-xs text-muted px-1">
            <span>Viewport: <strong className="text-text-main capitalize">{viewport}</strong></span>
            <span>Screen AST: <code className="text-accent">{selectedScreenId}</code></span>
          </div>

          <div
            data-testid="viewport-frame"
            data-viewport={viewport}
            className={`transition-all duration-300 overflow-x-hidden ${getViewportWrapperStyle()}`}
          >
            {loading && (
              <div className="py-24 flex flex-col items-center justify-center gap-3">
                <Loader2 className="w-8 h-8 animate-spin text-accent" />
                <span className="text-sm text-muted">Loading screen preview...</span>
              </div>
            )}

            {!loading && error && (
              <div className="py-16 text-center space-y-3">
                <AlertCircle className="w-10 h-10 text-cta mx-auto" />
                <p className="text-sm text-muted">{error}</p>
              </div>
            )}

            {!loading && !error && layout && (
              <div className="space-y-8 min-h-[400px]">
                {layout.widgets.map((widget) => {
                  if (disabledWidgetIds.has(widget.id)) return null;

                  const displayType =
                    (widget as { display_type?: string }).display_type ||
                    widget.type;

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
                        onPlayItem={(id) => onPlayItem?.(id)}
                      />
                    );
                  }

                  if (displayType === 'grid' || widget.type === 'grid') {
                    const gridWidget = widget as Extract<
                      WidgetNode,
                      { type: 'grid' }
                    >;
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
                        onPlayItem={(id) => onPlayItem?.(id)}
                      />
                    );
                  }

                  return null;
                })}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Item Details Inspection Modal inside Studio */}
      <ItemDetailsModal
        itemId={selectedItemId}
        isOpen={selectedItemId !== null}
        onClose={() => setSelectedItemId(null)}
        onPlay={(id) => onPlayItem?.(id)}
      />
    </div>
  );
};

export default LayoutStudio;
