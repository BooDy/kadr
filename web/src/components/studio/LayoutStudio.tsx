import { useState, useEffect, useCallback, useRef, useId, type FC } from 'react';
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
  Clock,
  User as UserIcon,
} from 'lucide-react';
import { api } from '../../api/client';
import {
  countFilterTreeRules,
  type ScreenLayout,
  type ScreenSummary,
  type WidgetNode,
  type WidgetQueryBinding,
  type CardViewModel,
  type Library,
  type TargetDevice,
  type User,
} from '../../types';
import { formatLocalTimeHHMM, isTimeWindowActive } from '../../utils/layoutScope';
import { SpotlightWidget } from '../browse/SpotlightWidget';
import { CarouselWidget } from '../browse/CarouselWidget';
import { GridWidget } from '../browse/GridWidget';
import { DenseListWidget } from '../browse/DenseListWidget';
import { ItemDetailsModal } from '../browse/ItemDetailsModal';
import { WidgetConfigModal } from './WidgetConfigModal';

export type ViewportMode = 'tv' | 'tablet' | 'mobile';

const DEFAULT_SCREENS = ['home', 'movies', 'shows'];

export interface LayoutStudioProps {
  currentUser?: User | null;
  defaultScope?: 'global' | 'personal';
  onPlayItem?: (itemId: number) => void;
}

function formatWidgetBindingSummary(binding?: WidgetQueryBinding): string | null {
  if (!binding) return null;
  const macro = binding.macro_type;
  const ruleCount = countFilterTreeRules(binding.filters?.rule_tree);
  const ruleSuffix =
    ruleCount > 0 ? ` · ${ruleCount} ${ruleCount === 1 ? 'rule' : 'rules'}` : '';

  if (macro === 'custom_query') {
    return `Custom Filter Query${ruleSuffix}`;
  }
  if (macro === 'favorites') {
    return `My Favorites${ruleSuffix}`;
  }
  if (ruleCount > 0) {
    return `${ruleCount} filter ${ruleCount === 1 ? 'rule' : 'rules'}`;
  }
  return null;
}

// Hydrated spotlight helper for LayoutStudio preview
const HydratedSpotlight: FC<{
  widget: Extract<WidgetNode, { type: 'hero_banner' }>;
  screenId: string;
  isDirty: boolean;
  onPlay?: (itemId: number) => void;
  onMoreInfo: (itemId: number) => void;
}> = ({ widget, screenId, isDirty, onPlay, onMoreInfo }) => {
  const [item, setItem] = useState<CardViewModel | undefined>(widget.data);
  const bindingKey = JSON.stringify(widget.binding);

  useEffect(() => {
    if (widget.data) {
      setItem(widget.data);
      return;
    }

    let isMounted = true;
    const controller = new AbortController();

    const fetchFallback = () => {
      if (typeof api.getWidgetData !== 'function') return;
      api
        .getWidgetData(widget.id, 0, 1)
        .then((items) => {
          if (isMounted && items && items.length > 0) {
            setItem(items[0]);
          }
        })
        .catch(() => {});
    };

    if (typeof api.previewWidget === 'function') {
      api
        .previewWidget(
          {
            binding: JSON.parse(bindingKey) as WidgetQueryBinding,
            screen_id: screenId,
            limit: 1,
          },
          controller.signal
        )
        .then((res) => {
          if (!isMounted || controller.signal.aborted) return;
          if (res && Array.isArray(res.items)) {
            setItem(res.items[0]);
          }
        })
        .catch(() => {
          if (!isMounted || controller.signal.aborted) return;
          fetchFallback();
        });
    } else {
      fetchFallback();
    }

    return () => {
      isMounted = false;
      controller.abort();
    };
  }, [widget.id, widget.data, bindingKey, screenId, isDirty]);

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
  screenId: string;
  isDirty: boolean;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (id: number) => void;
}> = ({ widget, screenId, isDirty, onSelectItem, onPlayItem }) => {
  const [items, setItems] = useState<CardViewModel[]>(widget.items || []);
  const bindingKey = JSON.stringify(widget.binding);

  useEffect(() => {
    if (widget.items && widget.items.length > 0) {
      setItems(widget.items);
      return;
    }

    let isMounted = true;
    const controller = new AbortController();

    const fetchFallback = () => {
      if (typeof api.getWidgetData !== 'function') return;
      api
        .getWidgetData(widget.id, 0, 10)
        .then((data) => {
          if (isMounted && data && data.length > 0) {
            setItems(data);
          }
        })
        .catch(() => {});
    };

    if (typeof api.previewWidget === 'function') {
      const parsedBinding = JSON.parse(bindingKey) as WidgetQueryBinding;
      api
        .previewWidget(
          {
            binding: parsedBinding,
            screen_id: screenId,
            limit: Math.min(Math.max(1, parsedBinding.limit || 10), 20),
          },
          controller.signal
        )
        .then((res) => {
          if (!isMounted || controller.signal.aborted) return;
          if (res && Array.isArray(res.items)) {
            setItems(res.items);
          }
        })
        .catch(() => {
          if (!isMounted || controller.signal.aborted) return;
          fetchFallback();
        });
    } else {
      fetchFallback();
    }

    return () => {
      isMounted = false;
      controller.abort();
    };
  }, [widget.id, widget.items, bindingKey, screenId, isDirty]);

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
  screenId: string;
  isDirty: boolean;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (id: number) => void;
}> = ({ widget, screenId, isDirty, onSelectItem, onPlayItem }) => {
  const [items, setItems] = useState<CardViewModel[]>(widget.items || []);
  const [previewTotalCount, setPreviewTotalCount] = useState<number | undefined>(
    widget.total_count
  );
  const bindingKey = JSON.stringify(widget.binding);

  useEffect(() => {
    if (widget.items && widget.items.length > 0) {
      setItems(widget.items);
      setPreviewTotalCount(widget.total_count);
      return;
    }

    let isMounted = true;
    const controller = new AbortController();

    const fetchFallback = () => {
      if (typeof api.getWidgetData !== 'function') return;
      api
        .getWidgetData(widget.id, 0, 24)
        .then((data) => {
          if (isMounted && data && data.length > 0) {
            setItems(data);
          }
        })
        .catch(() => {});
    };

    if (typeof api.previewWidget === 'function') {
      const parsedBinding = JSON.parse(bindingKey) as WidgetQueryBinding;
      api
        .previewWidget(
          {
            binding: parsedBinding,
            screen_id: screenId,
            limit: Math.min(Math.max(1, parsedBinding.limit || 24), 24),
          },
          controller.signal
        )
        .then((res) => {
          if (!isMounted || controller.signal.aborted) return;
          if (res && Array.isArray(res.items)) {
            setItems(res.items);
            setPreviewTotalCount(res.total_count);
          }
        })
        .catch(() => {
          if (!isMounted || controller.signal.aborted) return;
          fetchFallback();
        });
    } else {
      fetchFallback();
    }

    return () => {
      isMounted = false;
      controller.abort();
    };
  }, [widget.id, widget.items, widget.total_count, bindingKey, screenId, isDirty]);

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
      totalCount={previewTotalCount ?? widget.total_count}
      nextCursor={widget.next_cursor}
      onSelectItem={onSelectItem}
      onPlayItem={onPlayItem}
    />
  );
};

// Studio preview dense_list wrapper providing fallback preview items
const StudioDenseList: FC<{
  widget: Extract<WidgetNode, { type: 'dense_list' }>;
  screenId: string;
  targetDevice: TargetDevice;
  isDirty: boolean;
  onSelectItem: (item: CardViewModel) => void;
  onPlayItem?: (id: number) => void;
}> = ({ widget, screenId, targetDevice, isDirty, onSelectItem, onPlayItem }) => {
  const [items, setItems] = useState<CardViewModel[]>(widget.items || []);
  const [previewTotalCount, setPreviewTotalCount] = useState<number | undefined>(
    widget.total_count
  );
  const bindingKey = JSON.stringify(widget.binding);

  useEffect(() => {
    if (widget.items && widget.items.length > 0) {
      setItems(widget.items);
      setPreviewTotalCount(widget.total_count);
      return;
    }

    let isMounted = true;
    const controller = new AbortController();

    const fetchFallback = () => {
      if (typeof api.getWidgetData !== 'function') return;
      api
        .getWidgetData(widget.id, 0, 20)
        .then((data) => {
          if (isMounted && data && data.length > 0) {
            setItems(data);
          }
        })
        .catch(() => {});
    };

    if (typeof api.previewWidget === 'function') {
      const parsedBinding = JSON.parse(bindingKey) as WidgetQueryBinding;
      api
        .previewWidget(
          {
            binding: parsedBinding,
            screen_id: screenId,
            limit: Math.min(Math.max(1, parsedBinding.limit || 20), 24),
          },
          controller.signal
        )
        .then((res) => {
          if (!isMounted || controller.signal.aborted) return;
          if (res && Array.isArray(res.items)) {
            setItems(res.items);
            setPreviewTotalCount(res.total_count);
          }
        })
        .catch(() => {
          if (!isMounted || controller.signal.aborted) return;
          fetchFallback();
        });
    } else {
      fetchFallback();
    }

    return () => {
      isMounted = false;
      controller.abort();
    };
  }, [widget.id, widget.items, widget.total_count, bindingKey, screenId, isDirty]);

  const previewItems: CardViewModel[] =
    items.length > 0
      ? items
      : [
          {
            id: 9996,
            title: `${widget.title || 'Dense List'} Sample Item`,
            media_type: 'movie',
            release_year: 2024,
            duration_seconds: 5400,
          },
        ];

  return (
    <DenseListWidget
      key={widget.id}
      widgetId={widget.id}
      screenId={screenId}
      targetDevice={targetDevice !== 'all' ? targetDevice : undefined}
      title={widget.title}
      items={previewItems}
      totalCount={previewTotalCount ?? widget.total_count}
      nextCursor={widget.next_cursor}
      onSelectItem={onSelectItem}
      onPlayItem={onPlayItem}
    />
  );
};

export const LayoutStudio: FC<LayoutStudioProps> = ({
  currentUser,
  defaultScope = 'global',
  onPlayItem,
}) => {
  const isAdmin = !currentUser || currentUser.role === 'admin';

  const [scopeUserId, setScopeUserId] = useState<string>(() => {
    if (!currentUser) return 'global';
    if (currentUser.role !== 'admin') return currentUser.id;
    return defaultScope === 'personal' ? currentUser.id : 'global';
  });
  const effectiveScopeUserId =
    currentUser && currentUser.role !== 'admin' ? currentUser.id : scopeUserId;

  const [profiles, setProfiles] = useState<User[]>([]);
  const [targetDevice, setTargetDevice] = useState<TargetDevice>('all');
  const [simulatedTime, setSimulatedTime] = useState<string>('');
  const effectiveSimulatedTime = simulatedTime || formatLocalTimeHHMM();

  const scopeOwnerSelectId = useId();
  const targetDeviceSelectId = useId();
  const simulateTimeInputId = useId();

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

  // Sync scopeUserId when currentUser or defaultScope changes
  useEffect(() => {
    if (!currentUser) return;
    if (currentUser.role !== 'admin') {
      setScopeUserId(currentUser.id);
    } else {
      setScopeUserId(defaultScope === 'personal' ? currentUser.id : 'global');
    }
  }, [currentUser, defaultScope]);

  // Load household profiles for Admin scope switcher
  useEffect(() => {
    if (!currentUser || !isAdmin || typeof api.getProfiles !== 'function') return;
    let mounted = true;
    api
      .getProfiles()
      .then((list) => {
        if (mounted && Array.isArray(list)) {
          setProfiles(list);
        }
      })
      .catch(() => {});
    return () => {
      mounted = false;
    };
  }, [currentUser, isAdmin]);

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
          const seen = new Set<string>();
          const uniqueScreens: ScreenSummary[] = [];
          for (const s of data) {
            const titleKey = (s.title || s.id).trim().toLowerCase();
            const idKey = s.id.trim().toLowerCase();
            if (!seen.has(titleKey) && !seen.has(idKey)) {
              seen.add(titleKey);
              seen.add(idKey);
              uniqueScreens.push(s);
            }
          }
          setScreens(uniqueScreens);
        }
      })
      .catch(() => {});
  }, []);

  const isScopedSlot = effectiveScopeUserId !== 'global' || targetDevice !== 'all';
  const useScopedApi = Boolean(currentUser) || isScopedSlot;

  // Fetch screen AST layout whenever selectedScreenId, scope, or targetDevice changes
  const fetchLayout = useCallback(
    async (screenId: string) => {
      setLoading(true);
      setError(null);
      try {
        const data = useScopedApi
          ? await api.getScreen(screenId, true, {
              scopeUserId: effectiveScopeUserId,
              targetDevice,
            })
          : await api.getScreen(screenId);
        setLayout(data);
        setDisabledWidgetIds(new Set());
        setIsDirty(false);
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : 'Unknown error';
        setError(`Failed to load screen layout: ${msg}`);
      } finally {
        setLoading(false);
      }
    },
    [useScopedApi, effectiveScopeUserId, targetDevice]
  );

  useEffect(() => {
    fetchLayout(selectedScreenId);
  }, [selectedScreenId, fetchLayout]);

  const handleTargetDeviceChange = (nextDevice: TargetDevice) => {
    setTargetDevice(nextDevice);
    if (nextDevice === 'mobile') {
      setViewport('mobile');
    } else if (nextDevice === 'tv' || nextDevice === 'web') {
      setViewport('tv');
    }
  };

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
      const saved = useScopedApi
        ? await api.saveScreen(layout.id, layout, {
            scopeUserId: effectiveScopeUserId,
            targetDevice,
          })
        : await api.saveScreen(layout.id, layout);
      if (saved && saved.resolved_scope) {
        setLayout((prev) => (prev ? { ...prev, resolved_scope: saved.resolved_scope } : prev));
      }
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

  // Reset scoped override back to parent fallback tier
  const handleResetOverride = async () => {
    const screenId = layout?.id || selectedScreenId;
    if (!screenId) return;
    setLoading(true);
    setError(null);
    try {
      await api.deleteScreen(screenId, {
        scopeUserId: effectiveScopeUserId,
        targetDevice,
      });
      const refreshed = await api.getScreen(screenId, true, {
        scopeUserId: effectiveScopeUserId,
        targetDevice,
      });
      setLayout(refreshed);
      setIsDirty(false);
      setDisabledWidgetIds(new Set());
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unknown error';
      setError(`Failed to reset layout override: ${msg}`);
    } finally {
      setLoading(false);
    }
  };

  // Reset default screen layout (Global + All)
  const handleResetLayout = async () => {
    const screenId = layout?.id || selectedScreenId;
    if (!screenId) return;
    setLoading(true);
    setError(null);
    try {
      await api.resetScreen(screenId);
      const refreshed = useScopedApi
        ? await api.getScreen(screenId, true, {
            scopeUserId: effectiveScopeUserId,
            targetDevice,
          })
        : await api.getScreen(screenId);
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

  // Delete custom screen (Global + All, Admin only)
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
      setScreens((prev) => {
        const filtered = prev.filter(
          (s) =>
            s.id.toLowerCase() !== created.id.toLowerCase() &&
            (s.title || '').trim().toLowerCase() !== created.title.trim().toLowerCase()
        );
        return [...filtered, { id: created.id, title: created.title }];
      });
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

  const isExactOverride = Boolean(layout?.resolved_scope?.is_exact_override);
  const showScopeBadge = Boolean(layout?.resolved_scope) || Boolean(currentUser) || isScopedSlot;

  return (
    <div className="space-y-6 pb-16">
      {/* Top Header & Controls */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-border-subtle pb-6">
        <div>
          <h2 className="text-2xl font-bold tracking-tight text-text-main flex items-center gap-2.5">
            <Layers className="h-6 w-6 text-accent" aria-hidden="true" />
            Layout Studio
          </h2>
          <p className="text-sm text-muted mt-1">
            Declarative AST screen inspector and multi-device viewport simulator.
          </p>
        </div>

        {/* Viewport Simulator Mode Selector */}
        <div className="flex items-center gap-1.5 p-1 rounded-xl bg-panel border border-border-subtle self-start md:self-auto">
          <button
            type="button"
            aria-pressed={viewport === 'tv'}
            onClick={() => setViewport('tv')}
            className={`min-w-7 min-h-7 flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
              viewport === 'tv'
                ? 'bg-accent text-canvas font-semibold shadow-sm'
                : 'text-muted hover:text-text-main hover:bg-panel-hover'
            }`}
          >
            <Tv className="w-3.5 h-3.5" aria-hidden="true" />
            <span>TV (16:9)</span>
          </button>
          <button
            type="button"
            aria-pressed={viewport === 'tablet'}
            onClick={() => setViewport('tablet')}
            className={`min-w-7 min-h-7 flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
              viewport === 'tablet'
                ? 'bg-accent text-canvas font-semibold shadow-sm'
                : 'text-muted hover:text-text-main hover:bg-panel-hover'
            }`}
          >
            <Tablet className="w-3.5 h-3.5" aria-hidden="true" />
            <span>Tablet (4:3)</span>
          </button>
          <button
            type="button"
            aria-pressed={viewport === 'mobile'}
            onClick={() => setViewport('mobile')}
            className={`min-w-7 min-h-7 flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
              viewport === 'mobile'
                ? 'bg-accent text-canvas font-semibold shadow-sm'
                : 'text-muted hover:text-text-main hover:bg-panel-hover'
            }`}
          >
            <Smartphone className="w-3.5 h-3.5" aria-hidden="true" />
            <span>Mobile (9:16)</span>
          </button>
        </div>
      </div>

      {/* Scope, Target Device & Simulate Time Toolbar */}
      <div className="flex flex-wrap items-end justify-between gap-4 rounded-2xl border border-border-subtle bg-panel/70 p-4">
        <div className="flex flex-wrap items-end gap-4">
          {/* Scope Owner Selector (Admin) vs Locked Personal Badge (Standard) */}
          {isAdmin ? (
            <div>
              <label
                htmlFor={scopeOwnerSelectId}
                className="block text-[11px] font-semibold uppercase tracking-wider text-muted mb-1"
              >
                Scope Owner
              </label>
              <select
                id={scopeOwnerSelectId}
                value={effectiveScopeUserId}
                onChange={(e) => setScopeUserId(e.target.value)}
                className="min-h-7 bg-canvas border border-border-subtle rounded-xl px-3 py-1.5 text-xs font-medium text-text-main transition-colors focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <option value="global">Global (All Users)</option>
                {currentUser && (
                  <option value={currentUser.id}>My Profile ({currentUser.username})</option>
                )}
                {profiles
                  .filter((p) => p.id !== currentUser?.id)
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.username} ({p.role})
                    </option>
                  ))}
              </select>
            </div>
          ) : (
            currentUser && (
              <div>
                <span className="block text-[11px] font-semibold uppercase tracking-wider text-muted mb-1">
                  Layout Scope
                </span>
                <div className="min-h-7 inline-flex items-center gap-2 px-3 py-1.5 rounded-xl bg-accent/15 border border-accent/30 text-xs font-semibold text-accent">
                  <UserIcon className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>Personal Layout · {currentUser.username}</span>
                </div>
              </div>
            )
          )}

          {/* Target Device Selector */}
          <div>
            <label
              htmlFor={targetDeviceSelectId}
              className="block text-[11px] font-semibold uppercase tracking-wider text-muted mb-1"
            >
              Target Device
            </label>
            <select
              id={targetDeviceSelectId}
              value={targetDevice}
              onChange={(e) => handleTargetDeviceChange(e.target.value as TargetDevice)}
              className="min-h-7 bg-canvas border border-border-subtle rounded-xl px-3 py-1.5 text-xs font-medium text-text-main transition-colors focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              <option value="all">All Devices</option>
              <option value="web">Web</option>
              <option value="tv">TV</option>
              <option value="mobile">Mobile</option>
            </select>
          </div>

          {/* Scope Override Status Badge + Reset Override */}
          {showScopeBadge && (
            <div className="flex items-center gap-2 pb-0.5">
              {isExactOverride ? (
                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold bg-emerald-500/15 text-emerald-300 border border-emerald-500/35">
                  <Check className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>Custom Override Active</span>
                </span>
              ) : (
                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium bg-canvas text-muted border border-border-subtle">
                  <span>Inherited Default</span>
                </span>
              )}

              {isScopedSlot && isExactOverride && (
                <button
                  type="button"
                  onClick={handleResetOverride}
                  disabled={loading}
                  className="min-w-7 min-h-7 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-muted hover:text-text-main hover:bg-panel-hover border border-border-subtle transition-colors cursor-pointer disabled:opacity-50 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  <RotateCcw className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>Reset Override</span>
                </button>
              )}
            </div>
          )}
        </div>

        {/* Simulate Time Control */}
        <div className="flex items-end gap-2">
          <div>
            <label
              htmlFor={simulateTimeInputId}
              className="block text-[11px] font-semibold uppercase tracking-wider text-muted mb-1"
            >
              Simulate Time
            </label>
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                aria-pressed={simulatedTime === ''}
                onClick={() => setSimulatedTime('')}
                className={`min-w-7 min-h-7 inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl text-xs font-semibold border transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
                  simulatedTime === ''
                    ? 'bg-accent text-canvas border-accent'
                    : 'bg-canvas text-muted hover:text-text-main border-border-subtle'
                }`}
              >
                <Clock className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Live</span>
              </button>
              <input
                id={simulateTimeInputId}
                name="simulate_time"
                type="time"
                value={simulatedTime}
                onChange={(e) => setSimulatedTime(e.target.value)}
                className="min-h-7 bg-canvas border border-border-subtle rounded-xl px-2.5 py-1 text-xs text-text-main transition-colors focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              />
            </div>
          </div>
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
              type="button"
              onClick={() => setSelectedScreenId(s.id)}
              className={`min-w-7 min-h-7 px-4 py-2 rounded-xl text-sm transition-all cursor-pointer whitespace-nowrap focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight ${
                selectedScreenId === s.id
                  ? 'bg-panel text-accent font-semibold border border-border-subtle rounded-xl shadow-md'
                  : 'text-muted hover:text-text-main hover:bg-panel-hover rounded-xl border border-transparent'
              }`}
            >
              {s.title || s.id}
            </button>
          ))}
          {isAdmin && (
            <button
              type="button"
              onClick={() => {
                setNewScreenId('');
                setNewScreenTitle('');
                setCreateScreenError(null);
                setIsCreateScreenOpen(true);
              }}
              className="min-w-7 min-h-7 flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-accent hover:bg-accent/10 border border-accent/30 transition-colors cursor-pointer flex-shrink-0 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
            >
              <Plus className="w-3.5 h-3.5" aria-hidden="true" />
              <span>+ New Screen</span>
            </button>
          )}
        </div>

        {/* Top actions: Dirty indicator, Reset/Delete (Global+All Admin), Save Layout */}
        <div className="flex items-center gap-2.5 flex-shrink-0 flex-wrap">
          {isDirty && (
            <span className="flex items-center gap-1.5 text-xs font-medium text-cta bg-cta/10 px-2.5 py-1 rounded-lg border border-cta/20">
              <span className="w-1.5 h-1.5 rounded-full bg-cta animate-pulse" aria-hidden="true" />
              Unsaved Changes
            </span>
          )}
          {saveSuccess && (
            <span className="flex items-center gap-1 text-xs font-medium text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-lg border border-emerald-500/20">
              <Check className="w-3.5 h-3.5" aria-hidden="true" />
              {saveSuccess}
            </span>
          )}

          {isAdmin && !isScopedSlot && (
            DEFAULT_SCREENS.includes(selectedScreenId) ? (
              <button
                type="button"
                onClick={handleResetLayout}
                disabled={loading}
                className="min-w-7 min-h-7 flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-muted hover:text-text-main hover:bg-panel-hover border border-border-subtle transition-colors cursor-pointer disabled:opacity-50 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <RotateCcw className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Reset to Default</span>
              </button>
            ) : (
              <button
                type="button"
                onClick={handleDeleteScreen}
                disabled={loading}
                className="min-w-7 min-h-7 flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold text-red-400 hover:text-red-300 hover:bg-red-500/10 border border-red-500/30 transition-colors cursor-pointer disabled:opacity-50 focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Delete Screen</span>
              </button>
            )
          )}

          <button
            type="button"
            onClick={handleSaveLayout}
            disabled={!isDirty || isSaving || !layout}
            className="min-w-7 min-h-7 flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-xs font-bold bg-cta hover:bg-cta-hover text-white shadow-md transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
          >
            {isSaving ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
            ) : (
              <Save className="w-3.5 h-3.5" aria-hidden="true" />
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
              <Layers className="w-4 h-4 text-accent" aria-hidden="true" />
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
                className="min-w-7 min-h-7 flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-semibold text-accent hover:bg-accent/10 border border-accent/30 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <Plus className="w-3 h-3" aria-hidden="true" />
                <span>+ Add Widget</span>
              </button>
            </div>
          </div>

          {loading ? (
            <div className="py-8 flex flex-col items-center justify-center text-muted gap-2">
              <Loader2 className="w-5 h-5 animate-spin text-accent" aria-hidden="true" />
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
                const bindingSummary = formatWidgetBindingSummary(
                  'binding' in widget ? widget.binding : undefined
                );
                const timeWindow =
                  'time_window' in widget ? widget.time_window : undefined;
                const isTwActive = timeWindow
                  ? isTimeWindowActive(timeWindow, effectiveSimulatedTime)
                  : true;

                return (
                  <div
                    key={widget.id}
                    data-testid="widget-tree-item"
                    className={`font-mono text-xs text-text-main bg-canvas/80 p-3 rounded-xl border border-border-subtle transition-all ${
                      isEnabled && isTwActive
                        ? 'hover:border-border-subtle/80 hover:bg-canvas'
                        : 'opacity-60'
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
                        {bindingSummary && (
                          <p className="text-[10px] text-accent/90 font-sans font-medium truncate">
                            {bindingSummary}
                          </p>
                        )}
                        {timeWindow && (
                          <div className="flex flex-wrap items-center gap-1.5 pt-0.5 font-sans">
                            <span className="inline-flex items-center gap-1 rounded bg-panel px-1.5 py-0.5 text-[10px] font-medium text-text-main border border-border-subtle">
                              <Clock className="w-3 h-3 text-accent" aria-hidden="true" />
                              <span>
                                {timeWindow.start}–{timeWindow.end}
                              </span>
                            </span>
                            {!isTwActive && (
                              <span className="inline-flex items-center rounded bg-highlight/15 px-1.5 py-0.5 text-[10px] font-semibold text-highlight border border-highlight/35">
                                Inactive at {effectiveSimulatedTime}
                              </span>
                            )}
                          </div>
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
                          className="min-w-7 min-h-7 inline-flex items-center justify-center p-1 text-muted hover:text-text-main disabled:opacity-30 disabled:hover:text-muted rounded-md transition-colors cursor-pointer disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <ArrowUp className="w-3.5 h-3.5" aria-hidden="true" />
                        </button>
                        <button
                          type="button"
                          aria-label={`Move ${widget.id} down`}
                          disabled={index === layout.widgets.length - 1}
                          onClick={() => moveWidget(index, 'down')}
                          className="min-w-7 min-h-7 inline-flex items-center justify-center p-1 text-muted hover:text-text-main disabled:opacity-30 disabled:hover:text-muted rounded-md transition-colors cursor-pointer disabled:cursor-not-allowed focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <ArrowDown className="w-3.5 h-3.5" aria-hidden="true" />
                        </button>
                        <button
                          type="button"
                          aria-label={`Edit ${widget.id}`}
                          onClick={() => {
                            setEditingWidgetIndex(index);
                            setIsConfigModalOpen(true);
                          }}
                          className="min-w-7 min-h-7 inline-flex items-center justify-center p-1 text-muted hover:text-accent rounded-md transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <Pencil className="w-3.5 h-3.5" aria-hidden="true" />
                        </button>
                        <button
                          type="button"
                          aria-label={`Delete ${widget.id}`}
                          onClick={() => deleteWidget(index)}
                          className="min-w-7 min-h-7 inline-flex items-center justify-center p-1 text-muted hover:text-red-400 rounded-md transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        >
                          <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                        </button>
                        <label className="min-w-7 min-h-7 inline-flex items-center justify-center cursor-pointer p-1 rounded-lg hover:bg-panel-hover transition-colors has-[:focus-visible]:ring-3 has-[:focus-visible]:ring-highlight has-[:focus-visible]:outline-none">
                          <input
                            type="checkbox"
                            aria-label={`Toggle ${widget.id}`}
                            checked={isEnabled}
                            onChange={() => toggleWidget(widget.id)}
                            className="sr-only"
                          />
                          {isEnabled ? (
                            <Eye className="w-4 h-4 text-accent" aria-hidden="true" />
                          ) : (
                            <EyeOff className="w-4 h-4 text-muted" aria-hidden="true" />
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
                <Loader2 className="w-8 h-8 animate-spin text-accent" aria-hidden="true" />
                <span className="text-sm text-muted">Loading screen preview...</span>
              </div>
            )}

            {!loading && error && (
              <div className="py-16 text-center space-y-3">
                <AlertCircle className="w-10 h-10 text-cta mx-auto" aria-hidden="true" />
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
                  const timeWindow =
                    'time_window' in widget ? widget.time_window : undefined;
                  const isTwActive = timeWindow
                    ? isTimeWindowActive(timeWindow, effectiveSimulatedTime)
                    : true;

                  let renderedNode: React.ReactNode = null;

                  if (displayType === 'spotlight' || widget.type === 'hero_banner') {
                    renderedNode = (
                      <HydratedSpotlight
                        key={widget.id}
                        widget={widget as Extract<WidgetNode, { type: 'hero_banner' }>}
                        screenId={selectedScreenId}
                        isDirty={isDirty}
                        onPlay={onPlayItem}
                        onMoreInfo={(id) => setSelectedItemId(id)}
                      />
                    );
                  } else if (displayType === 'dense_list' || widget.type === 'dense_list') {
                    renderedNode = (
                      <StudioDenseList
                        key={widget.id}
                        widget={widget as Extract<WidgetNode, { type: 'dense_list' }>}
                        screenId={selectedScreenId}
                        targetDevice={targetDevice}
                        isDirty={isDirty}
                        onSelectItem={(item) => setSelectedItemId(item.id)}
                        onPlayItem={(id) => onPlayItem?.(id)}
                      />
                    );
                  } else if (displayType === 'carousel' || widget.type === 'carousel') {
                    renderedNode = (
                      <StudioCarousel
                        key={widget.id}
                        widget={widget as Extract<WidgetNode, { type: 'carousel' }>}
                        screenId={selectedScreenId}
                        isDirty={isDirty}
                        onSelectItem={(item) => setSelectedItemId(item.id)}
                        onPlayItem={(id) => onPlayItem?.(id)}
                      />
                    );
                  } else if (displayType === 'grid' || widget.type === 'grid') {
                    renderedNode = (
                      <StudioGrid
                        key={widget.id}
                        widget={widget as Extract<WidgetNode, { type: 'grid' }>}
                        screenId={selectedScreenId}
                        isDirty={isDirty}
                        onSelectItem={(item) => setSelectedItemId(item.id)}
                        onPlayItem={(id) => onPlayItem?.(id)}
                      />
                    );
                  }

                  if (!renderedNode) return null;

                  if (timeWindow && !isTwActive) {
                    return (
                      <div
                        key={widget.id}
                        className="rounded-2xl border border-dashed border-highlight/40 bg-canvas/40 p-3 space-y-2"
                      >
                        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-highlight">
                          <span className="inline-flex items-center gap-1.5 font-semibold">
                            <Clock className="w-3.5 h-3.5" aria-hidden="true" />
                            <span>
                              {timeWindow.start}–{timeWindow.end}
                            </span>
                            <span aria-hidden="true">·</span>
                            <span>Inactive at {effectiveSimulatedTime}</span>
                          </span>
                          <span className="text-[11px] text-muted">
                            Hidden on live screens outside schedule
                          </span>
                        </div>
                        <div className="opacity-50">{renderedNode}</div>
                      </div>
                    );
                  }

                  return renderedNode;
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
        screenId={selectedScreenId}
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
                <Plus className="w-5 h-5 text-accent" aria-hidden="true" />
                Create New Screen
              </h3>
              <button
                type="button"
                onClick={() => setIsCreateScreenOpen(false)}
                aria-label="Close modal"
                className="min-w-7 min-h-7 p-1.5 text-muted hover:text-text-main hover:bg-panel-hover rounded-lg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
              >
                <X className="w-5 h-5" aria-hidden="true" />
              </button>
            </div>

            <form onSubmit={handleCreateScreen} className="space-y-4">
              {createScreenError && (
                <p role="alert" className="text-xs text-cta">
                  {createScreenError}
                </p>
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
                  className="min-w-7 min-h-7 px-4 py-2 rounded-xl text-sm font-semibold text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="min-w-7 min-h-7 px-5 py-2 rounded-xl text-sm font-bold bg-cta hover:bg-cta-hover text-white shadow-md flex items-center gap-2 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                >
                  <Plus className="w-4 h-4" aria-hidden="true" />
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
