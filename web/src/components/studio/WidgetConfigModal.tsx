import React, { useState, useEffect } from 'react';
import { X, Sliders, Check } from 'lucide-react';
import type { Library, WidgetNode, QueryMacro, WidgetQueryBinding } from '../../types';

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
    }
  }, [isOpen, initialWidget, libraries]);

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

    const binding: WidgetQueryBinding = {
      macro_type: resolvedMacro,
      limit: Math.max(1, Number(limit) || 20),
      ...(sort.trim() ? { sort: sort.trim() } : {}),
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
      className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4"
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
