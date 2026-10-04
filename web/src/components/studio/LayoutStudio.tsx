import { useState, useEffect, useCallback, useRef, type FC } from 'react';
import {
  Tv,
  Tablet,
  Smartphone,
  Layers,
  Loader2,
  AlertCircle,
  Eye,
  EyeOff,
  Plus,
  ArrowUp,
  ArrowDown,
  Pencil,
  Trash2,
  Save,
  RotateCcw,
  Check,
  X,
} from 'lucide-react';
import { api } from '../../api/client';
import type {
  ScreenLayout,
  ScreenSummary,
  WidgetNode,
  CardViewModel,
  Library,
} from '../../types';
import { SpotlightWidget } from '../browse/SpotlightWidget';
import { CarouselWidget } from '../browse/CarouselWidget';
import { GridWidget } from '../browse/GridWidget';
import { ItemDetailsModal } from '../browse/ItemDetailsModal';
import { WidgetConfigModal } from './WidgetConfigModal';

export type ViewportMode = 'tv' | 'tablet' | 'mobile';

const DEFAULT_SCREENS = ['home', 'movies', 'shows'];

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

  const previewItem: CardViewModel = item || {
    id: 9997,
    title: 'Hero Spotlight Sample',
    media_type: 'movie',
    release_year: 2024,
  };

  return (
    <SpotlightWidget
      item={previewItem}
      onPlay={(id) => onPlay?.(id)}
      onMoreInfo={onMoreInfo}
    />
  );
};

// Studio preview carousel wrapper providing fallback preview items
const StudioCarousel: FC<{
  widget: Extract<WidgetNode, { type: 'carousel' }>;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (id: number) => void;
}> = ({ widget, onSelectItem, onPlayItem }) => {
  const [items, setItems] = useState<CardViewModel[]>(widget.items || []);

  useEffect(() => {
    if (widget.items && widget.items.length > 0) {
      setItems(widget.items);
      return;
    }

    let isMounted = true;
    api
      .getWidgetData(widget.id, 0, 10)
      .then((data) => {
        if (isMounted && data && data.length > 0) {
          setItems(data);
        }
      })
      .catch(() => {});

    return () => {
      isMounted = false;
    };
  }, [widget.id, widget.items]);

  const previewItems: CardViewModel[] =
    items.length > 0
      ? items
      : [
          {
            id: 9999,
            title: `${widget.title || 'Widget'} Sample Item`,
            media_type: 'movie',
            release_year: 2024,
          },
        ];

  return (
    <CarouselWidget
      key={widget.id}
      widgetId={widget.id}
      title={widget.title}
      items={previewItems}
      onSelectItem={onSelectItem}
      onPlayItem={onPlayItem}
    />
  );
};

// Studio preview grid wrapper providing fallback preview items
const StudioGrid: FC<{
  widget: Extract<WidgetNode, { type: 'grid' }>;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (id: number) => void;
}> = ({ widget, onSelectItem, onPlayItem }) => {
  const [items, setItems] = useState<CardViewModel[]>(widget.items || []);

  useEffect(() => {
    if (widget.items && widget.items.length > 0) {
      setItems(widget.items);
      return;
    }

    let isMounted = true;
    api
      .getWidgetData(widget.id, 0, 24)
      .then((data) => {
        if (isMounted && data && data.length > 0) {
          setItems(data);
        }
      })
      .catch(() => {});

    return () => {
      isMounted = false;
    };
  }, [widget.id, widget.items]);

  const previewItems: CardViewModel[] =
    items.length > 0
      ? items
      : [
          {
            id: 9998,
            title: `${widget.title || 'Grid'} Sample Item`,
            media_type: 'movie',
            release_year: 2024,
          },
        ];

  return (
    <GridWidget
      key={widget.id}
      widgetId={widget.id}
      title={widget.title}
      columns={widget.columns}
      items={previewItems}
      totalCount={widget.total_count}
      nextCursor={widget.next_cursor}
      onSelectItem={onSelectItem}
      onPlayItem={onPlayItem}
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

  // Studio mutation state
  const [libraries, setLibraries] = useState<Library[]>([]);
  const [isDirty, setIsDirty] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [saveSuccess, setSaveSuccess] = useState<string | null>(null);
  const saveTimerRef = useRef<NodeJS.Timeout | null>(null);

  // Clear save timer on unmount
  useEffect(() => {
    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
    };
  }, []);

  // Widget config modal state
  const [isConfigModalOpen, setIsConfigModalOpen] = useState<boolean>(false);
  const [editingWidgetIndex, setEditingWidgetIndex] = useState<number | null>(null);

  // New screen creation modal state
  const [isCreateScreenOpen, setIsCreateScreenOpen] = useState<boolean>(false);
  const [newScreenId, setNewScreenId] = useState<string>('');
  const [newScreenTitle, setNewScreenTitle] = useState<string>('');
  const [createScreenError, setCreateScreenError] = useState<string | null>(null);

  // Fetch libraries for widget config bindings
  useEffect(() => {
    api
      .getLibraries()
      .then((libs) => {
        if (libs) setLibraries(libs);
      })
      .catch(() => {});
  }, []);

  // Fetch available screens on mount
  useEffect(() => {
    api
      .getScreens()
      .then((data) => {
        if (data && data.length > 0) {
          setScreens(data);
        }
      })
      .catch(() => {});
  }, []);

  // Fetch screen AST layout whenever selectedScreenId changes
  const fetchLayout = useCallback(async (screenId: string) => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.getScreen(screenId);
      setLayout(data);
      setDisabledWidgetIds(new Set());
      setIsDirty(false);
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

  // Reorder widget in layout
  const moveWidget = (index: number, direction: 'up' | 'down') => {
    if (!layout) return;
    const targetIndex = direction === 'up' ? index - 1 : index + 1;
    if (targetIndex < 0 || targetIndex >= layout.widgets.length) return;

    const newWidgets = [...layout.widgets];
    const [moved] = newWidgets.splice(index, 1);
    newWidgets.splice(targetIndex, 0, moved);

    setLayout({
      ...layout,
      widgets: newWidgets,
    });
    setIsDirty(true);
  };

  // Delete widget from layout
  const deleteWidget = (index: number) => {
    if (!layout) return;
    const newWidgets = layout.widgets.filter((_, i) => i !== index);
    setLayout({
      ...layout,
      widgets: newWidgets,
    });
    setIsDirty(true);
  };

  // Save or update widget from modal
  const handleSaveWidget = (savedWidget: WidgetNode) => {
    if (!layout) return;

    if (
      editingWidgetIndex !== null &&
      editingWidgetIndex >= 0 &&
      editingWidgetIndex < layout.widgets.length
    ) {
      const updatedWidgets = [...layout.widgets];
      updatedWidgets[editingWidgetIndex] = savedWidget;
      setLayout({
        ...layout,
        widgets: updatedWidgets,
      });
    } else {
      setLayout({
        ...layout,
        widgets: [...layout.widgets, savedWidget],
      });
    }
    setIsDirty(true);
    setIsConfigModalOpen(false);
    setEditingWidgetIndex(null);
  };

  // Save layout persistence
  const handleSaveLayout = async () => {
    if (!layout) return;
    setIsSaving(true);
    setError(null);
    try {
      await api.saveScreen(layout.id, layout);
      setIsDirty(false);
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
      setSaveSuccess('Layout saved successfully');
      saveTimerRef.current = setTimeout(() => {
        setSaveSuccess(null);
        saveTimerRef.current = null;
      }, 3000);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setError(`Failed to save screen layout: ${msg}`);
    } finally {
      setIsSaving(false);
    }
  };

  // Reset default screen layout
  const handleResetLayout = async () => {
    const screenId = layout?.id || selectedScreenId;
    if (!screenId) return;
    setLoading(true);
    setError(null);
    try {
      await api.resetScreen(screenId);
      const refreshed = await api.getScreen(screenId);
      setLayout(refreshed);
      setIsDirty(false);
      setDisabledWidgetIds(new Set());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setError(`Failed to reset screen layout: ${msg}`);
    } finally {
      setLoading(false);
    }
  };

  // Delete custom screen
  const handleDeleteScreen = async () => {
    const screenId = layout?.id || selectedScreenId;
    if (!screenId) return;
    setLoading(true);
    setError(null);
    try {
      await api.deleteScreen(screenId);
      setScreens((prev) => prev.filter((s) => s.id !== screenId));
      setSelectedScreenId('home');
      setIsDirty(false);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setError(`Failed to delete screen: ${msg}`);
    } finally {
      setLoading(false);
    }
  };

  // Create new custom screen
  const handleCreateScreen = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmedId = newScreenId.trim();
    const trimmedTitle = newScreenTitle.trim();
    if (!trimmedId || !trimmedTitle) return;

    try {
      const created = await api.createScreen({
        id: trimmedId,
        title: trimmedTitle,
      });
      setScreens((prev) => [...prev, { id: created.id, title: created.title }]);
      setSelectedScreenId(created.id);
      setLayout(created);
      setIsDirty(false);
      setIsCreateScreenOpen(false);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setCreateScreenError(`Failed to create screen: ${msg}`);
    }
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
            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
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
            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
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
            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
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

      {/* Screen Selector Tabs & Actions Toolbar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-border-subtle pb-4">
        <div className="flex items-center gap-2 overflow-x-auto pb-1 flex-1 min-w-0">
          <span className="text-xs font-semibold text-muted uppercase tracking-wider mr-1 flex-shrink-0">
            Screens:
          </span>
          {screens.map((s) => (
            <button
              key={s.id}
              onClick={() => setSelectedScreenId(s.id)}
              className={`px-4 py-2 rounded-xl text-sm transition-all cursor-pointer whitespace-nowrap focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
                selectedScreenId === s.id
                  ? 'bg-panel text-accent font-semibold border border-border-subtle rounded-xl shadow-md'
                  : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl border border-transparent'
              }`}
            >
              {s.title || s.id}
            </button>
          ))}
          <button
            type="button"
            onClick={() => {
              setNewScreenId('');
              setNewScreenTitle('');
              setCreateScreenError(null);
              setIsCreateScreenOpen(true);
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-accent hover:bg-accent/10 border border-accent/30 transition-colors cursor-pointer flex-shrink-0 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>+ New Screen</span>
          </button>
        </div>

        {/* Top actions: Dirty indicator, Reset/Delete, Save Layout */}
        <div className="flex items-center gap-2.5 flex-shrink-0 flex-wrap">
          {isDirty && (
            <span className="flex items-center gap-1.5 text-xs font-medium text-cta bg-cta/10 px-2.5 py-1 rounded-lg border border-cta/20">
              <span className="w-1.5 h-1.5 rounded-full bg-cta animate-pulse" />
              Unsaved Changes
            </span>
          )}
          {saveSuccess && (
            <span className="flex items-center gap-1 text-xs font-medium text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-lg border border-emerald-500/20">
              <Check className="w-3.5 h-3.5" />
              {saveSuccess}
            </span>
          )}

          {DEFAULT_SCREENS.includes(selectedScreenId) ? (
            <button
              type="button"
              onClick={handleResetLayout}
              disabled={loading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-muted hover:text-text-main hover:bg-panel-hover border border-border-subtle transition-colors cursor-pointer disabled:opacity-50 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              <RotateCcw className="w-3.5 h-3.5" />
              <span>Reset to Default</span>
            </button>
          ) : (
            <button
              type="button"
              onClick={handleDeleteScreen}
              disabled={loading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-red-400 hover:text-red-300 hover:bg-red-500/10 border border-red-500/30 transition-colors cursor-pointer disabled:opacity-50 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              <Trash2 className="w-3.5 h-3.5" />
              <span>Delete Screen</span>
            </button>
          )}

          <button
            type="button"
            onClick={handleSaveLayout}
            disabled={!isDirty || isSaving || !layout}
            className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-xs font-bold bg-cta hover:bg-cta-hover text-white shadow-md transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
          >
            {isSaving ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Save className="w-3.5 h-3.5" />
            )}
            <span>Save Layout</span>
          </button>
        </div>
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
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-mono text-muted bg-canvas px-2 py-0.5 rounded-md border border-border-subtle">
                {layout?.widgets.length || 0} nodes
              </span>
              <button
                type="button"
                onClick={() => {
                  setEditingWidgetIndex(null);
                  setIsConfigModalOpen(true);
                }}
                className="flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-semibold text-accent hover:bg-accent/10 border border-accent/30 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <Plus className="w-3 h-3" />
                <span>+ Add Widget</span>
              </button>
            </div>
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
                    data-testid="widget-tree-item"
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

                      {/* Action buttons: Move Up, Move Down, Edit, Delete, Toggle */}
                      <div className="flex items-center gap-1 flex-shrink-0">
                        <button
                          type="button"
                          aria-label={`Move ${widget.id} up`}
                          disabled={index === 0}
                          onClick={() => moveWidget(index, 'up')}
                          className="p-1 text-muted hover:text-text-main disabled:opacity-30 disabled:hover:text-muted rounded-md transition-colors cursor-pointer disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <ArrowUp className="w-3.5 h-3.5" />
                        </button>
                        <button
                          type="button"
                          aria-label={`Move ${widget.id} down`}
                          disabled={index === layout.widgets.length - 1}
                          onClick={() => moveWidget(index, 'down')}
                          className="p-1 text-muted hover:text-text-main disabled:opacity-30 disabled:hover:text-muted rounded-md transition-colors cursor-pointer disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <ArrowDown className="w-3.5 h-3.5" />
                        </button>
                        <button
                          type="button"
                          aria-label={`Edit ${widget.id}`}
                          onClick={() => {
                            setEditingWidgetIndex(index);
                            setIsConfigModalOpen(true);
                          }}
                          className="p-1 text-muted hover:text-accent rounded-md transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <Pencil className="w-3.5 h-3.5" />
                        </button>
                        <button
                          type="button"
                          aria-label={`Delete ${widget.id}`}
                          onClick={() => deleteWidget(index)}
                          className="p-1 text-muted hover:text-red-400 rounded-md transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                        <label className="flex items-center cursor-pointer p-1 rounded-lg hover:bg-panel-hover transition-colors has-[:focus-visible]:ring-3 has-[:focus-visible]:ring-highlight has-[:focus-visible]:outline-none">
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
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Live Interactive Preview Pane */}
        <div className="lg:col-span-3 space-y-4">
          <div className="flex items-center justify-between text-xs text-muted px-1">
            <span>
              Viewport: <strong className="text-text-main capitalize">{viewport}</strong>
            </span>
            <span>
              Screen AST: <code className="text-accent">{selectedScreenId}</code>
            </span>
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
                    return (
                      <StudioCarousel
                        key={widget.id}
                        widget={widget as Extract<WidgetNode, { type: 'carousel' }>}
                        onSelectItem={(item) => setSelectedItemId(item.id)}
                        onPlayItem={(id) => onPlayItem?.(id)}
                      />
                    );
                  }

                  if (displayType === 'grid' || widget.type === 'grid') {
                    return (
                      <StudioGrid
                        key={widget.id}
                        widget={widget as Extract<WidgetNode, { type: 'grid' }>}
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

      {/* Widget Configuration Modal */}
      <WidgetConfigModal
        isOpen={isConfigModalOpen}
        initialWidget={
          editingWidgetIndex !== null && layout && layout.widgets[editingWidgetIndex]
            ? layout.widgets[editingWidgetIndex]
            : null
        }
        libraries={libraries}
        onSave={handleSaveWidget}
        onClose={() => {
          setIsConfigModalOpen(false);
          setEditingWidgetIndex(null);
        }}
      />

      {/* New Screen Dialog */}
      {isCreateScreenOpen && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label="New Screen"
          className="fixed inset-0 z-50 bg-canvas/80 backdrop-blur-sm flex items-center justify-center p-4"
        >
          <div className="bg-panel border border-border-subtle rounded-2xl max-w-md w-full p-6 shadow-2xl space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-border-subtle">
              <h3 className="text-lg font-bold text-text-main flex items-center gap-2">
                <Plus className="w-5 h-5 text-accent" />
                Create New Screen
              </h3>
              <button
                type="button"
                onClick={() => setIsCreateScreenOpen(false)}
                aria-label="Close modal"
                className="p-1.5 text-muted hover:text-text-main hover:bg-panel-hover rounded-lg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <form onSubmit={handleCreateScreen} className="space-y-4">
              {createScreenError && (
                <p className="text-xs text-cta">{createScreenError}</p>
              )}

              <div>
                <label
                  htmlFor="screen-id-input"
                  className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5"
                >
                  Screen ID
                </label>
                <input
                  id="screen-id-input"
                  aria-label="Screen ID"
                  type="text"
                  required
                  value={newScreenId}
                  onChange={(e) => setNewScreenId(e.target.value)}
                  placeholder="e.g. anime, kids, live_tv"
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                />
              </div>

              <div>
                <label
                  htmlFor="screen-title-input"
                  className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5"
                >
                  Screen Title
                </label>
                <input
                  id="screen-title-input"
                  aria-label="Screen Title"
                  type="text"
                  required
                  value={newScreenTitle}
                  onChange={(e) => setNewScreenTitle(e.target.value)}
                  placeholder="e.g. Anime Hub, Kids Corner"
                  className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                />
              </div>

              <div className="pt-3 border-t border-border-subtle flex items-center justify-end gap-3">
                <button
                  type="button"
                  onClick={() => setIsCreateScreenOpen(false)}
                  className="px-4 py-2 rounded-xl text-sm font-semibold text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-5 py-2 rounded-xl text-sm font-bold bg-cta hover:bg-cta-hover text-white shadow-md flex items-center gap-2 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  <Plus className="w-4 h-4" />
                  Create Screen
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

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
