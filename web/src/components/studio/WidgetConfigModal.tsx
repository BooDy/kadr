import React, { useState, useEffect } from 'react';
import { X, Sliders, Check, ChevronDown, ChevronRight, Filter } from 'lucide-react';
import type { Library, WidgetNode, QueryMacro, WidgetQueryBinding, WidgetFilterConfig } from '../../types';

export interface WidgetConfigModalProps {
  isOpen: boolean;
  initialWidget?: WidgetNode | null;
  libraries: Library[];
  onSave: (widget: WidgetNode) => void;
  onClose: () => void;
}

type MacroTypeOption =
  | 'recently_added'
  | 'top_rated'
  | 'continue_watching'
  | 'library_items'
  | 'genre_shelf'
  | 'spotlight_item';

const SORT_OPTIONS = [
  { value: '', label: 'Default' },
  { value: 'title:asc', label: 'Title (A-Z)' },
  { value: 'title:desc', label: 'Title (Z-A)' },
  { value: 'rating:desc', label: 'Rating (Highest)' },
  { value: 'rating:asc', label: 'Rating (Lowest)' },
  { value: 'added_at:desc', label: 'Recently Added' },
  { value: 'added_at:asc', label: 'Oldest Added' },
  { value: 'release_year:desc', label: 'Release Year (Newest)' },
  { value: 'release_year:asc', label: 'Release Year (Oldest)' },
];

const POPULAR_GENRES = ['Horror', 'Romance', 'Kids', 'Animation', 'Documentary', 'Drama'];

const AGE_PRESETS = [
  { label: '7 Days', days: 7 },
  { label: '30 Days', days: 30 },
  { label: '90 Days', days: 90 },
  { label: '1 Year', days: 365 },
  { label: 'All Time', days: 0 },
];

export const WidgetConfigModal: React.FC<WidgetConfigModalProps> = ({
  isOpen,
  initialWidget,
  libraries,
  onSave,
  onClose,
}) => {
  const [widgetType, setWidgetType] = useState<'hero_banner' | 'carousel' | 'grid'>('carousel');
  const [title, setTitle] = useState<string>('');
  const [columns, setColumns] = useState<number>(4);
  const [macroType, setMacroType] = useState<MacroTypeOption>('recently_added');
  const [selectedLibraryId, setSelectedLibraryId] = useState<string>('');
  const [genre, setGenre] = useState<string>('Action');
  const [itemId, setItemId] = useState<string>('');
  const [limit, setLimit] = useState<number>(20);
  const [sort, setSort] = useState<string>('');

  // Advanced Filters State
  const [isFiltersOpen, setIsFiltersOpen] = useState<boolean>(false);
  const [excludePrivate, setExcludePrivate] = useState<boolean>(false);
  const [excludeLibraryIds, setExcludeLibraryIds] = useState<string[]>([]);
  const [excludeGenres, setExcludeGenres] = useState<string[]>([]);
  const [genreInputText, setGenreInputText] = useState<string>('');
  const [maxAgeDays, setMaxAgeDays] = useState<number | null>(null);

  useEffect(() => {
    if (!isOpen) return;

    if (initialWidget) {
      if (initialWidget.type === 'hero_banner' || initialWidget.type === 'carousel' || initialWidget.type === 'grid') {
        setWidgetType(initialWidget.type);
      } else {
        setWidgetType('carousel');
      }

      setTitle('title' in initialWidget ? initialWidget.title : '');
      setColumns('columns' in initialWidget ? initialWidget.columns : 4);

      const binding = 'binding' in initialWidget ? initialWidget.binding : undefined;
      if (binding) {
        setLimit(binding.limit ?? 20);
        setSort(typeof binding.sort === 'string' ? binding.sort : '');

        const macro = binding.macro_type;
        if (typeof macro === 'string') {
          if (
            macro === 'recently_added' ||
            macro === 'top_rated' ||
            macro === 'continue_watching' ||
            macro === 'library_items' ||
            macro === 'genre_shelf' ||
            macro === 'spotlight_item'
          ) {
            setMacroType(macro as MacroTypeOption);
          } else {
            setMacroType('recently_added');
          }
        } else if (typeof macro === 'object' && macro !== null) {
          const m = macro as Record<string, any>;
          if ('library_items' in m && m.library_items) {
            setMacroType('library_items');
            setSelectedLibraryId(m.library_items.library_id || libraries[0]?.id || '');
          } else if ('genre_shelf' in m && m.genre_shelf) {
            setMacroType('genre_shelf');
            setGenre(m.genre_shelf.genre || 'Action');
          } else if ('spotlight_item' in m && m.spotlight_item) {
            setMacroType('spotlight_item');
            setItemId(
              m.spotlight_item.item_id !== undefined ? String(m.spotlight_item.item_id) : ''
            );
          } else {
            setMacroType('recently_added');
          }
        }

        const filters = binding.filters;
        const hasActiveFilters = Boolean(
          filters?.exclude_private ||
          (filters?.exclude_library_ids && filters.exclude_library_ids.length > 0) ||
          (filters?.exclude_genres && filters.exclude_genres.length > 0) ||
          (filters?.max_age_days && filters.max_age_days > 0)
        );

        setIsFiltersOpen(hasActiveFilters);
        setExcludePrivate(Boolean(filters?.exclude_private));
        setExcludeLibraryIds(filters?.exclude_library_ids ?? []);
        setExcludeGenres(filters?.exclude_genres ?? []);
        setGenreInputText((filters?.exclude_genres ?? []).join(', '));
        setMaxAgeDays(filters?.max_age_days ?? null);
      } else {
        setIsFiltersOpen(false);
        setExcludePrivate(false);
        setExcludeLibraryIds([]);
        setExcludeGenres([]);
        setGenreInputText('');
        setMaxAgeDays(null);
      }
    } else {
      setWidgetType('carousel');
      setTitle('');
      setColumns(4);
      setMacroType('recently_added');
      setSelectedLibraryId(libraries[0]?.id || '');
      setGenre('Action');
      setItemId('');
      setLimit(20);
      setSort('');
      setIsFiltersOpen(false);
      setExcludePrivate(false);
      setExcludeLibraryIds([]);
      setExcludeGenres([]);
      setGenreInputText('');
      setMaxAgeDays(null);
    }
  }, [isOpen, initialWidget, libraries]);

  const toggleGenreChip = (genreName: string) => {
    const exists = excludeGenres.some((g) => g.toLowerCase() === genreName.toLowerCase());
    let nextGenres: string[];
    if (exists) {
      nextGenres = excludeGenres.filter((g) => g.toLowerCase() !== genreName.toLowerCase());
    } else {
      nextGenres = [...excludeGenres, genreName];
    }
    setExcludeGenres(nextGenres);
    setGenreInputText(nextGenres.join(', '));
  };

  const handleGenreInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setGenreInputText(val);
    const parsed = val
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    setExcludeGenres(parsed);
  };

  const handleAgePresetClick = (days: number) => {
    if (days === 0) {
      setMaxAgeDays(null);
    } else {
      setMaxAgeDays(days);
    }
  };

  const handleMaxAgeInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value.trim();
    if (val === '') {
      setMaxAgeDays(null);
    } else {
      const num = Number(val);
      setMaxAgeDays(isNaN(num) || num <= 0 ? null : num);
    }
  };

  const activeFilterCount =
    (excludePrivate ? 1 : 0) +
    (excludeLibraryIds.length > 0 ? 1 : 0) +
    (excludeGenres.length > 0 ? 1 : 0) +
    (maxAgeDays !== null && maxAgeDays > 0 ? 1 : 0);

  // Handle escape key
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    let resolvedMacro: QueryMacro;
    switch (macroType) {
      case 'continue_watching':
        resolvedMacro = 'continue_watching';
        break;
      case 'top_rated':
        resolvedMacro = 'top_rated';
        break;
      case 'library_items':
        resolvedMacro = {
          library_items: {
            library_id: selectedLibraryId || libraries[0]?.id || '',
          },
        };
        break;
      case 'genre_shelf':
        resolvedMacro = {
          genre_shelf: {
            genre: genre.trim() || 'Action',
          },
        };
        break;
      case 'spotlight_item':
        resolvedMacro = itemId.trim()
          ? { spotlight_item: { item_id: Number(itemId) } }
          : { spotlight_item: {} };
        break;
      case 'recently_added':
      default:
        resolvedMacro = 'recently_added';
        break;
    }

    const filterConfig: WidgetFilterConfig = {};
    let hasFilters = false;

    if (excludePrivate) {
      filterConfig.exclude_private = true;
      hasFilters = true;
    }

    if (excludeLibraryIds.length > 0) {
      filterConfig.exclude_library_ids = excludeLibraryIds;
      hasFilters = true;
    }

    if (excludeGenres.length > 0) {
      filterConfig.exclude_genres = excludeGenres;
      hasFilters = true;
    }

    if (maxAgeDays !== null && maxAgeDays > 0) {
      filterConfig.max_age_days = maxAgeDays;
      hasFilters = true;
    }

    const binding: WidgetQueryBinding = {
      macro_type: resolvedMacro,
      limit: Math.max(1, Number(limit) || 20),
      ...(sort.trim() ? { sort: sort.trim() } : {}),
      ...(hasFilters ? { filters: filterConfig } : {}),
    };

    const id = initialWidget?.id || `${widgetType}_${Date.now()}`;

    if (widgetType === 'hero_banner') {
      const heroNode: Extract<WidgetNode, { type: 'hero_banner' }> = {
        type: 'hero_banner',
        id,
        binding,
        ...(initialWidget && 'data' in initialWidget && initialWidget.data ? { data: initialWidget.data } : {}),
        ...(initialWidget?.display_type ? { display_type: initialWidget.display_type } : {}),
      };
      onSave(heroNode);
    } else if (widgetType === 'grid') {
      const gridNode: Extract<WidgetNode, { type: 'grid' }> = {
        type: 'grid',
        id,
        title: title.trim(),
        columns: Math.max(1, Number(columns) || 4),
        binding,
        ...(initialWidget && 'items' in initialWidget && initialWidget.items ? { items: initialWidget.items } : {}),
        ...(initialWidget?.display_type ? { display_type: initialWidget.display_type } : {}),
      };
      onSave(gridNode);
    } else {
      const carouselNode: Extract<WidgetNode, { type: 'carousel' }> = {
        type: 'carousel',
        id,
        title: title.trim(),
        binding,
        ...(initialWidget && 'items' in initialWidget && initialWidget.items ? { items: initialWidget.items } : {}),
        ...(initialWidget?.display_type ? { display_type: initialWidget.display_type } : {}),
      };
      onSave(carouselNode);
    }

    onClose();
  };

  const isCustomSort = sort && !SORT_OPTIONS.some((opt) => opt.value === sort);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={initialWidget ? 'Edit Widget' : 'Add Widget'}
      className="fixed inset-0 z-50 bg-canvas/80 backdrop-blur-sm flex items-center justify-center p-4"
    >
      <div className="bg-panel border border-border-subtle rounded-2xl max-w-xl w-full p-6 shadow-2xl flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="flex items-center justify-between pb-4 border-b border-border-subtle">
          <div className="flex items-center gap-2.5">
            <Sliders className="w-5 h-5 text-accent" />
            <h3 className="text-lg font-bold text-text-main">
              {initialWidget ? 'Edit Widget' : 'Add Widget'}
            </h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close modal"
            className="p-1.5 text-muted hover:text-text-main hover:bg-panel-hover rounded-lg transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="mt-4 space-y-4 overflow-y-auto pr-1 flex-1">
          {/* Widget Type Selector */}
          <div>
            <label htmlFor="widget-type" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
              Widget Type
            </label>
            <select
              id="widget-type"
              aria-label="Widget Type"
              value={widgetType}
              onChange={(e) => setWidgetType(e.target.value as 'hero_banner' | 'carousel' | 'grid')}
              className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <option value="carousel">Carousel Rail</option>
              <option value="grid">Grid Layout</option>
              <option value="hero_banner">Hero Banner</option>
            </select>
          </div>

          {/* Title (for Carousel and Grid) */}
          {widgetType !== 'hero_banner' && (
            <div>
              <label htmlFor="widget-title" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Title
              </label>
              <input
                id="widget-title"
                aria-label="Title"
                type="text"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="e.g. Continue Watching, Recommended"
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              />
            </div>
          )}

          {/* Columns (only for Grid) */}
          {widgetType === 'grid' && (
            <div>
              <label htmlFor="widget-columns" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Columns
              </label>
              <input
                id="widget-columns"
                aria-label="Columns"
                type="number"
                min="1"
                max="12"
                value={columns}
                onChange={(e) => setColumns(Number(e.target.value))}
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              />
            </div>
          )}

          {/* Query Macro Selector */}
          <div>
            <label htmlFor="widget-macro" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
              Query Macro
            </label>
            <select
              id="widget-macro"
              aria-label="Query Macro"
              value={macroType}
              onChange={(e) => setMacroType(e.target.value as MacroTypeOption)}
              className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <option value="recently_added">Recently Added</option>
              <option value="top_rated">Top Rated</option>
              <option value="continue_watching">Continue Watching</option>
              <option value="library_items">Library Items</option>
              <option value="genre_shelf">Genre Shelf</option>
              <option value="spotlight_item">Spotlight Item</option>
            </select>
          </div>

          {/* Target Library (when library_items) */}
          {macroType === 'library_items' && (
            <div>
              <label htmlFor="widget-library" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Target Library
              </label>
              <select
                id="widget-library"
                aria-label="Target Library"
                value={selectedLibraryId}
                onChange={(e) => setSelectedLibraryId(e.target.value)}
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              >
                {libraries.length === 0 ? (
                  <option value="">No libraries available</option>
                ) : (
                  libraries.map((lib) => (
                    <option key={lib.id} value={lib.id}>
                      {lib.name}
                    </option>
                  ))
                )}
              </select>
            </div>
          )}

          {/* Genre Input (when genre_shelf) */}
          {macroType === 'genre_shelf' && (
            <div>
              <label htmlFor="widget-genre" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Genre
              </label>
              <input
                id="widget-genre"
                aria-label="Genre"
                type="text"
                value={genre}
                onChange={(e) => setGenre(e.target.value)}
                placeholder="e.g. Action, Sci-Fi, Comedy"
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              />
            </div>
          )}

          {/* Item ID Input (when spotlight_item) */}
          {macroType === 'spotlight_item' && (
            <div>
              <label htmlFor="widget-item-id" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Item ID (optional)
              </label>
              <input
                id="widget-item-id"
                aria-label="Item ID"
                type="number"
                value={itemId}
                onChange={(e) => setItemId(e.target.value)}
                placeholder="Leave blank for automatic top candidate"
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              />
            </div>
          )}

          {/* Limit and Sort Grid */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <label htmlFor="widget-limit" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Limit
              </label>
              <input
                id="widget-limit"
                aria-label="Limit"
                type="number"
                min="1"
                max="100"
                value={limit}
                onChange={(e) => setLimit(Number(e.target.value))}
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              />
            </div>

            <div>
              <label htmlFor="widget-sort" className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                Sort
              </label>
              <select
                id="widget-sort"
                aria-label="Sort"
                value={sort}
                onChange={(e) => setSort(e.target.value)}
                className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              >
                {SORT_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
                {isCustomSort && <option value={sort}>{sort}</option>}
              </select>
            </div>
          </div>

          {/* Advanced Filters Collapsible Section */}
          <div className="pt-2 border-t border-border-subtle/50">
            <button
              type="button"
              onClick={() => setIsFiltersOpen((prev) => !prev)}
              aria-expanded={isFiltersOpen}
              aria-controls="advanced-filters-panel"
              className="w-full flex items-center justify-between py-2.5 px-3 bg-canvas/60 hover:bg-panel-hover border border-border-subtle rounded-xl text-sm font-medium text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <div className="flex items-center gap-2">
                <Filter className="w-4 h-4 text-accent" />
                <span>Advanced Filters</span>
                {activeFilterCount > 0 && (
                  <span className="bg-accent/20 text-accent font-semibold px-2 py-0.5 rounded text-xs">
                    {activeFilterCount} {activeFilterCount === 1 ? 'filter active' : 'filters active'}
                  </span>
                )}
              </div>
              {isFiltersOpen ? (
                <ChevronDown className="w-4 h-4 text-muted" />
              ) : (
                <ChevronRight className="w-4 h-4 text-muted" />
              )}
            </button>

            {isFiltersOpen && (
              <div id="advanced-filters-panel" className="mt-4 space-y-4 pl-1">
                {/* Exclude Private Libraries */}
                <div>
                  <div className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                    Exclude Private Libraries
                  </div>
                  <div className="flex items-start gap-3 bg-canvas/40 border border-border-subtle rounded-xl p-3">
                    <input
                      id="widget-exclude-private"
                      type="checkbox"
                      checked={excludePrivate}
                      onChange={(e) => setExcludePrivate(e.target.checked)}
                      className="mt-0.5 h-4 w-4 rounded border-border-subtle bg-canvas text-accent focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none accent-accent cursor-pointer"
                    />
                    <div>
                      <label
                        htmlFor="widget-exclude-private"
                        className="text-sm font-medium text-text-main cursor-pointer"
                      >
                        Always exclude private libraries from this widget
                      </label>
                      <p className="text-xs text-muted mt-0.5">
                        Items from private libraries will never appear, even when unlocked in your active session.
                      </p>
                    </div>
                  </div>
                </div>

                {/* Exclude Specific Libraries */}
                <div>
                  <div className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5">
                    Exclude Libraries
                  </div>
                  {libraries.length === 0 ? (
                    <p className="text-xs text-muted">No libraries available</p>
                  ) : (
                    <div className="space-y-2 max-h-36 overflow-y-auto pr-1 bg-canvas/40 border border-border-subtle rounded-xl p-3">
                      {libraries.map((lib) => (
                        <label
                          key={lib.id}
                          htmlFor={`widget-exclude-lib-${lib.id}`}
                          className="flex items-center gap-2.5 text-sm text-text-main cursor-pointer hover:text-accent transition-colors"
                        >
                          <input
                            id={`widget-exclude-lib-${lib.id}`}
                            type="checkbox"
                            checked={excludeLibraryIds.includes(lib.id)}
                            onChange={(e) => {
                              if (e.target.checked) {
                                setExcludeLibraryIds((prev) => [...prev, lib.id]);
                              } else {
                                setExcludeLibraryIds((prev) => prev.filter((id) => id !== lib.id));
                              }
                            }}
                            className="h-4 w-4 rounded border-border-subtle bg-canvas text-accent focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none accent-accent cursor-pointer"
                          />
                          <span>{lib.name}</span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>

                {/* Exclude Genres */}
                <div>
                  <label
                    htmlFor="widget-exclude-genres"
                    className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5"
                  >
                    Exclude Genres
                  </label>
                  <input
                    id="widget-exclude-genres"
                    aria-label="Exclude Genres"
                    type="text"
                    value={genreInputText}
                    onChange={handleGenreInputChange}
                    placeholder="e.g. Horror, Thriller"
                    className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none mb-2"
                  />
                  <div className="flex flex-wrap gap-1.5">
                    {POPULAR_GENRES.map((g) => {
                      const isSelected = excludeGenres.some((item) => item.toLowerCase() === g.toLowerCase());
                      return (
                        <button
                          key={g}
                          type="button"
                          onClick={() => toggleGenreChip(g)}
                          aria-pressed={isSelected}
                          className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                            isSelected
                              ? 'bg-accent text-canvas font-semibold'
                              : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                          }`}
                        >
                          {g}
                        </button>
                      );
                    })}
                  </div>
                </div>

                {/* Exclude by Date Added (Max Age) */}
                <div>
                  <label
                    htmlFor="widget-max-age"
                    className="block text-xs font-semibold uppercase tracking-wider text-muted mb-1.5"
                  >
                    Date Added (Max Age)
                  </label>
                  <input
                    id="widget-max-age"
                    aria-label="Only include items added within the last N days"
                    type="number"
                    min="0"
                    placeholder="Only include items added within the last N days (leave empty or 0 for all time)"
                    value={maxAgeDays !== null && maxAgeDays > 0 ? maxAgeDays : ''}
                    onChange={handleMaxAgeInputChange}
                    className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted/60 transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none mb-2"
                  />
                  <div className="flex flex-wrap gap-1.5">
                    {AGE_PRESETS.map((preset) => {
                      const isSelected =
                        preset.days === 0
                          ? maxAgeDays === null || maxAgeDays === 0
                          : maxAgeDays === preset.days;
                      return (
                        <button
                          key={preset.label}
                          type="button"
                          onClick={() => handleAgePresetClick(preset.days)}
                          aria-pressed={isSelected}
                          className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                            isSelected
                              ? 'bg-accent text-canvas font-semibold'
                              : 'bg-canvas border border-border-subtle text-muted hover:text-text-main hover:bg-panel-hover'
                          }`}
                        >
                          {preset.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Actions */}
          <div className="pt-4 border-t border-border-subtle flex items-center justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2.5 rounded-xl text-sm font-semibold text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-5 py-2.5 rounded-xl text-sm font-bold bg-cta hover:bg-cta-hover text-white shadow-md flex items-center gap-2 transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <Check className="w-4 h-4" />
              Save Widget
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
