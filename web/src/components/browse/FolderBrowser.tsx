import { useState, useEffect, useCallback, type FC } from 'react';
import {
  Folder,
  FolderOpen,
  Film,
  Play,
  ArrowLeft,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import type { CardViewModel, LibraryFolderResponse } from '../../types';
import { api } from '../../api/client';
import { ItemDetailsModal } from './ItemDetailsModal';

export interface FolderBrowserProps {
  libraryId: string;
  onPlayItem: (itemId: number) => void;
}

interface MediaCardProps {
  item: CardViewModel;
  onSelect: (id: number) => void;
  onPlay: (id: number) => void;
}

const MediaCard: FC<MediaCardProps> = ({ item, onSelect, onPlay }) => {
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
      onClick={() => onSelect(item.id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect(item.id);
        }
      }}
      className="group relative flex flex-col cursor-pointer rounded-xl overflow-hidden border border-border-subtle bg-panel hover:bg-panel-hover transition-all duration-200 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none p-2"
    >
      {/* Poster Container */}
      <div className="relative aspect-[2/3] w-full rounded-lg overflow-hidden bg-canvas">
        {!imageFailed && posterUrl ? (
          <img
            src={posterUrl}
            alt={item.title}
            onError={() => setImageFailed(true)}
            className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 transition-all duration-200"
          />
        ) : (
          <div className="w-full h-full bg-gradient-to-br from-canvas via-panel to-canvas flex flex-col items-center justify-center p-4 text-center">
            <Film className="w-10 h-10 text-muted mb-2 group-hover:text-accent transition-colors" />
            <span className="text-xs font-semibold text-text-main line-clamp-3">
              {item.title}
            </span>
          </div>
        )}

        {/* Hover Action Overlay: Play CTA Button */}
        <div className="absolute inset-0 bg-canvas/40 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity duration-200">
          <button
            type="button"
            aria-label={`Play ${item.title}`}
            onClick={(e) => {
              e.stopPropagation();
              onPlay(item.id);
            }}
            className="w-11 h-11 rounded-full bg-cta hover:bg-cta-hover text-white flex items-center justify-center shadow-lg shadow-cta/30 transform scale-90 group-hover:scale-100 transition-transform duration-200 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none cursor-pointer"
          >
            <Play className="w-5 h-5 fill-white text-white ml-0.5" />
          </button>
        </div>

        {/* Badge Overlay */}
        {item.badge && (
          <div className="absolute top-2 left-2 pointer-events-none">
            <span className="px-2 py-0.5 rounded text-[10px] font-bold tracking-wider uppercase bg-highlight/20 text-highlight border border-highlight/40 shadow-xs">
              {item.badge}
            </span>
          </div>
        )}

        {/* Resume Progress Bar */}
        {hasProgress && (
          <div className="absolute bottom-0 inset-x-0 h-1.5 bg-muted pointer-events-none">
            <div
              className="h-full bg-accent"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        )}
      </div>

      {/* Card Metadata */}
      <div className="mt-2.5 px-0.5 space-y-0.5 min-w-0">
        <h3 className="text-sm font-medium text-text-main truncate group-hover:text-white transition-colors">
          {item.title}
        </h3>
        <div className="flex items-center gap-1.5 text-xs text-muted truncate">
          {item.release_year && <span>{item.release_year}</span>}
          {item.release_year && (item.rating || item.subtitle) && <span>•</span>}
          {item.rating ? (
            <span className="text-highlight">★ {item.rating.toFixed(1)}</span>
          ) : item.subtitle ? (
            <span className="truncate">{item.subtitle}</span>
          ) : null}
        </div>
      </div>
    </div>
  );
};

export const FolderBrowser: FC<FolderBrowserProps> = ({
  libraryId,
  onPlayItem,
}) => {
  const [currentPath, setCurrentPath] = useState<string>('');
  const [data, setData] = useState<LibraryFolderResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<number | null>(null);

  const navigateTo = useCallback(
    async (path: string) => {
      setLoading(true);
      setError(null);
      setCurrentPath(path);
      try {
        const res = await api.getLibraryFolders(libraryId, path);
        setData(res);
        setCurrentPath(res.current_path);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load folder contents');
      } finally {
        setLoading(false);
      }
    },
    [libraryId]
  );

  useEffect(() => {
    navigateTo(currentPath);
  }, [libraryId]);

  const hasParent = data?.parent_path !== null && data?.parent_path !== undefined;

  return (
    <div className="flex flex-col w-full h-full p-4 sm:p-6 lg:p-8 space-y-6">
      {/* Breadcrumb Navigation Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 p-3.5 rounded-xl bg-panel border border-border-subtle shadow-xs">
        <nav aria-label="Folder breadcrumbs" className="flex flex-wrap items-center gap-1.5 text-sm">
          {data?.breadcrumbs && data.breadcrumbs.length > 0 ? (
            data.breadcrumbs.map((crumb, idx) => {
              const isLast = idx === data.breadcrumbs.length - 1;
              return (
                <div key={crumb.path || `crumb-${idx}`} className="flex items-center gap-1.5">
                  {idx > 0 && <span className="text-muted select-none">/</span>}
                  <button
                    type="button"
                    onClick={() => navigateTo(crumb.path)}
                    disabled={isLast}
                    className={`px-2.5 py-1 rounded-md text-xs sm:text-sm transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                      isLast
                        ? 'font-semibold text-text-main bg-canvas/40 cursor-default'
                        : 'text-muted hover:text-text-main hover:bg-canvas/30 cursor-pointer'
                    }`}
                  >
                    {crumb.name}
                  </button>
                </div>
              );
            })
          ) : (
            <span className="text-xs text-muted font-mono">{currentPath || 'Root'}</span>
          )}
        </nav>

        {/* Up one level navigation button */}
        {hasParent && (
          <button
            type="button"
            onClick={() => navigateTo(data?.parent_path ?? '')}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-text-main bg-canvas/60 hover:bg-canvas border border-border-subtle transition-all duration-150 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none cursor-pointer"
          >
            <ArrowLeft className="w-3.5 h-3.5 text-accent" />
            <span>← Up one level</span>
          </button>
        )}
      </div>

      {/* Loading state indicator */}
      {loading && !data && (
        <div className="flex flex-col items-center justify-center py-20 text-muted space-y-3">
          <Loader2 className="w-8 h-8 animate-spin text-accent" />
          <p className="text-xs uppercase tracking-wider font-mono">Loading folder contents...</p>
        </div>
      )}

      {/* Error State Banner */}
      {error && (
        <div className="p-8 rounded-xl bg-panel border border-border-subtle flex flex-col items-center justify-center text-center space-y-4">
          <div className="p-3 rounded-full bg-cta/15 text-cta">
            <AlertCircle className="w-8 h-8" />
          </div>
          <div className="space-y-1">
            <h3 className="text-base font-semibold text-text-main">Unable to Browse Folder</h3>
            <p className="text-sm text-muted max-w-md">{error}</p>
          </div>
          <button
            type="button"
            onClick={() => navigateTo('')}
            className="px-4 py-2 rounded-lg bg-cta hover:bg-cta-hover text-white text-xs font-semibold uppercase tracking-wider transition-colors focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none cursor-pointer"
          >
            Back to Root
          </button>
        </div>
      )}

      {/* Main Content Area */}
      {!loading && !error && data && (
        <>
          {/* Empty State Banner */}
          {data.directories.length === 0 && data.items.length === 0 && (
            <div className="p-16 rounded-xl bg-panel border border-border-subtle flex flex-col items-center justify-center text-center space-y-3">
              <FolderOpen className="w-12 h-12 text-muted" />
              <h3 className="text-base font-medium text-text-main">This folder is empty</h3>
              <p className="text-sm text-muted max-w-sm">
                No subdirectories or media items were found in this directory.
              </p>
            </div>
          )}

          {/* Directories Grid */}
          {data.directories.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-xs font-semibold tracking-wider uppercase text-muted">
                Folders ({data.directories.length})
              </h2>
              <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4">
                {data.directories.map((dir) => (
                  <button
                    key={dir.path}
                    type="button"
                    onClick={() => navigateTo(dir.path)}
                    className="group relative flex items-center justify-between p-3.5 rounded-xl border border-border-subtle bg-panel hover:bg-panel-hover transition-all duration-200 cursor-pointer text-left focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                  >
                    <div className="flex items-center gap-3 min-w-0 pr-2">
                      <Folder className="w-5 h-5 text-accent shrink-0" />
                      <span className="text-sm font-medium text-text-main truncate group-hover:text-white transition-colors">
                        {dir.name}
                      </span>
                    </div>
                    <span className="font-mono text-xs text-muted bg-canvas/60 px-2 py-0.5 rounded-md shrink-0">
                      {dir.item_count} {dir.item_count === 1 ? 'item' : 'items'}
                    </span>
                  </button>
                ))}
              </div>
            </section>
          )}

          {/* Media Cards Grid */}
          {data.items.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-xs font-semibold tracking-wider uppercase text-muted">
                Media ({data.items.length})
              </h2>
              <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
                {data.items.map((item) => (
                  <MediaCard
                    key={item.id}
                    item={item}
                    onSelect={(id) => setSelectedItemId(id)}
                    onPlay={onPlayItem}
                  />
                ))}
              </div>
            </section>
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
