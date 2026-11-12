import { useState, useEffect, useCallback, type FC } from 'react';
import {
  ArrowLeft,
  Play,
  Shuffle,
  Pencil,
  Trash2,
  ChevronUp,
  ChevronDown,
  Globe,
  Lock,
  ListVideo,
  Info,
  Film,
  GripVertical,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import type { PlaylistDetail, PlaylistItemView } from '../../types';
import { api, describeApiError } from '../../api/client';
import { PlaylistFormModal, type PlaylistFormValues } from './PlaylistFormModal';
import { ItemDetailsModal } from '../browse/ItemDetailsModal';

export interface PlaylistDetailViewProps {
  playlistId: string;
  onBack: () => void;
  onPlayItem: (itemId: number) => void;
  onPlayQueue: (items: PlaylistItemView[], startIndex?: number) => void;
  onNavigateToFolder?: (libraryId: string, folderPath: string) => void;
  onDeleted: () => void;
}

export function formatPlaylistRuntime(totalSeconds: number): string {
  if (!totalSeconds || totalSeconds <= 0) return '0m';
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${Math.max(1, minutes)}m`;
}

function formatTrackDuration(seconds: number): string {
  if (!seconds || seconds <= 0) return '0:00';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) {
    return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  }
  return `${m}:${String(s).padStart(2, '0')}`;
}

export const PlaylistCoverMosaic: FC<{
  urls: string[];
  title: string;
  className?: string;
}> = ({ urls, title, className = 'w-full aspect-square' }) => {
  const validUrls = urls.filter(Boolean);

  if (validUrls.length >= 4) {
    return (
      <div
        className={`grid grid-cols-2 grid-rows-2 overflow-hidden bg-canvas border border-border-subtle rounded-xl ${className}`}
      >
        {validUrls.slice(0, 4).map((url, idx) => (
          <img
            key={`${url}-${idx}`}
            src={url}
            alt={`${title} cover ${idx + 1}`}
            className="w-full h-full object-cover"
          />
        ))}
      </div>
    );
  }

  if (validUrls.length >= 1) {
    return (
      <div
        className={`overflow-hidden bg-canvas border border-border-subtle rounded-xl ${className}`}
      >
        <img
          src={validUrls[0]}
          alt={title}
          className="w-full h-full object-cover"
        />
      </div>
    );
  }

  return (
    <div
      className={`flex items-center justify-center bg-canvas border border-border-subtle rounded-xl ${className}`}
    >
      <ListVideo className="w-12 h-12 text-muted" />
    </div>
  );
};

export const PlaylistDetailView: FC<PlaylistDetailViewProps> = ({
  playlistId,
  onBack,
  onPlayItem,
  onPlayQueue,
  onNavigateToFolder,
  onDeleted,
}) => {
  const [detail, setDetail] = useState<PlaylistDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [inspectItemId, setInspectItemId] = useState<number | null>(null);
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);

  const loadDetail = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.getPlaylist(playlistId);
      setDetail(res);
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setLoading(false);
    }
  }, [playlistId]);

  useEffect(() => {
    loadDetail();
  }, [loadDetail]);

  const handleShufflePlay = () => {
    if (!detail || detail.items.length === 0) return;
    const shuffled = [...detail.items];
    for (let i = shuffled.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
    }
    onPlayQueue(shuffled, 0);
  };

  const applyReorder = async (fromIndex: number, toIndex: number) => {
    if (!detail) return;
    if (
      fromIndex === toIndex ||
      fromIndex < 0 ||
      toIndex < 0 ||
      fromIndex >= detail.items.length ||
      toIndex >= detail.items.length
    ) {
      return;
    }

    const previousItems = detail.items;
    const nextItems = [...detail.items];
    const [moved] = nextItems.splice(fromIndex, 1);
    nextItems.splice(toIndex, 0, moved);
    const normalized = nextItems.map((item, idx) => ({ ...item, position: idx }));

    setDetail({ ...detail, items: normalized });
    setActionError(null);

    try {
      const updatedSummary = await api.reorderPlaylistItems(
        playlistId,
        normalized.map((i) => i.card.id)
      );
      setDetail((prev) =>
        prev ? { ...prev, summary: updatedSummary, items: normalized } : prev
      );
    } catch (err) {
      setDetail((prev) => (prev ? { ...prev, items: previousItems } : prev));
      setActionError(describeApiError(err));
    }
  };

  const handleRemoveItem = async (mediaItemId: number) => {
    if (!detail) return;
    setActionError(null);
    try {
      const updatedSummary = await api.removePlaylistItem(playlistId, mediaItemId);
      setDetail((prev) => {
        if (!prev) return prev;
        const remaining = prev.items
          .filter((item) => item.card.id !== mediaItemId)
          .map((item, idx) => ({ ...item, position: idx }));
        return {
          summary: updatedSummary,
          items: remaining,
        };
      });
    } catch (err) {
      setActionError(describeApiError(err));
    }
  };

  const handleUpdateMetadata = async (values: PlaylistFormValues) => {
    const updatedSummary = await api.updatePlaylist(playlistId, {
      name: values.name,
      description: values.description ?? '',
      is_public: values.is_public,
    });
    setDetail((prev) => (prev ? { ...prev, summary: updatedSummary } : prev));
  };

  const handleDeletePlaylist = async () => {
    setDeleting(true);
    setActionError(null);
    try {
      await api.deletePlaylist(playlistId);
      onDeleted();
    } catch (err) {
      setActionError(describeApiError(err));
      setDeleting(false);
    }
  };

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-24 text-muted space-y-3">
        <Loader2 className="w-8 h-8 animate-spin text-accent" />
        <p className="text-xs font-mono uppercase tracking-wider">Loading playlist...</p>
      </div>
    );
  }

  if (error || !detail) {
    return (
      <div className="max-w-4xl mx-auto p-6 sm:p-8 space-y-4">
        <button
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded-xl bg-panel hover:bg-panel-hover border border-border-subtle text-sm font-medium text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <ArrowLeft className="w-4 h-4 text-accent" />
          <span>All Playlists</span>
        </button>

        <div className="p-8 rounded-xl bg-panel border border-border-subtle flex flex-col items-center text-center space-y-3">
          <AlertCircle className="w-10 h-10 text-highlight" />
          <h2 className="text-base font-semibold text-text-main">Playlist Unavailable</h2>
          <p className="text-sm text-muted">{error || 'Unable to load playlist.'}</p>
        </div>
      </div>
    );
  }

  const { summary, items } = detail;

  return (
    <div className="max-w-6xl mx-auto p-4 sm:p-6 lg:p-8 space-y-6">
      {/* Back Button */}
      <div>
        <button
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded-xl bg-panel hover:bg-panel-hover border border-border-subtle text-sm font-medium text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <ArrowLeft className="w-4 h-4 text-accent" />
          <span>All Playlists</span>
        </button>
      </div>

      {/* Playlist Header Panel */}
      <div className="p-5 sm:p-6 rounded-xl bg-panel border border-border-subtle flex flex-col md:flex-row items-start md:items-center gap-6">
        <PlaylistCoverMosaic
          urls={summary.cover_poster_urls}
          title={summary.name}
          className="w-36 h-36 sm:w-44 sm:h-44 shrink-0"
        />

        <div className="flex-1 min-w-0 space-y-3 w-full">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-canvas border border-border-subtle text-text-main font-medium">
              {summary.is_public ? (
                <>
                  <Globe className="w-3.5 h-3.5 text-accent" />
                  <span>Public</span>
                </>
              ) : (
                <>
                  <Lock className="w-3.5 h-3.5 text-muted" />
                  <span>Private</span>
                </>
              )}
            </span>

            <span className="px-2.5 py-1 rounded-lg bg-canvas border border-border-subtle text-muted font-mono">
              @{summary.owner_username}
            </span>

            <span className="px-2.5 py-1 rounded-lg bg-canvas border border-border-subtle text-text-main font-mono">
              {summary.visible_item_count}{' '}
              {summary.visible_item_count === 1 ? 'item' : 'items'} •{' '}
              {formatPlaylistRuntime(summary.total_duration_seconds)}
            </span>
          </div>

          <h1 className="text-2xl sm:text-3xl font-semibold text-text-main truncate">
            {summary.name}
          </h1>

          {summary.description && (
            <p className="text-sm text-muted leading-relaxed max-w-3xl">
              {summary.description}
            </p>
          )}

          {/* Primary Controls */}
          <div className="flex flex-wrap items-center gap-3 pt-2">
            <button
              type="button"
              disabled={items.length === 0}
              onClick={() => onPlayQueue(items, 0)}
              className="inline-flex items-center gap-2 px-5 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-sm font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <Play className="w-4 h-4 fill-current" />
              <span>Play All</span>
            </button>

            <button
              type="button"
              disabled={items.length === 0}
              onClick={handleShufflePlay}
              className="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-canvas hover:bg-panel-hover border border-border-subtle text-text-main text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              <Shuffle className="w-4 h-4 text-accent" />
              <span>Shuffle</span>
            </button>

            {summary.is_editable && (
              <>
                <button
                  type="button"
                  onClick={() => setIsEditOpen(true)}
                  className="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-canvas hover:bg-panel-hover border border-border-subtle text-text-main text-sm font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                >
                  <Pencil className="w-4 h-4 text-accent" />
                  <span>Edit</span>
                </button>

                {!confirmingDelete ? (
                  <button
                    type="button"
                    onClick={() => setConfirmingDelete(true)}
                    className="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-canvas hover:bg-panel-hover border border-border-subtle text-text-main text-sm font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                  >
                    <Trash2 className="w-4 h-4 text-cta" />
                    <span>Delete</span>
                  </button>
                ) : (
                  <div className="inline-flex items-center gap-2">
                    <button
                      type="button"
                      disabled={deleting}
                      onClick={handleDeletePlaylist}
                      className="inline-flex items-center gap-1.5 px-4 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-sm font-semibold transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                    >
                      <Trash2 className="w-4 h-4" />
                      <span>Confirm Delete</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => setConfirmingDelete(false)}
                      className="px-3 py-2.5 rounded-xl bg-canvas hover:bg-panel-hover border border-border-subtle text-sm text-muted hover:text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                    >
                      Cancel
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      </div>

      {/* Locked Private Library Notice Banner */}
      {summary.locked_item_count > 0 && (
        <div className="flex items-center gap-3 p-4 rounded-xl bg-panel border border-highlight/40 text-sm text-highlight">
          <Lock className="w-4 h-4 shrink-0 text-highlight" />
          <span>
            {summary.locked_item_count} item(s) from locked private libraries are hidden.
            Unlock private libraries in the top bar to view and play them.
          </span>
        </div>
      )}

      {actionError && (
        <div className="p-3.5 rounded-xl bg-cta/15 border border-cta/40 text-sm text-highlight">
          {actionError}
        </div>
      )}

      {/* Ordered Track / Item List */}
      {items.length === 0 ? (
        <div className="p-12 rounded-xl bg-panel border border-border-subtle flex flex-col items-center justify-center text-center space-y-2">
          <ListVideo className="w-10 h-10 text-muted" />
          <h3 className="text-base font-medium text-text-main">No visible items in this playlist</h3>
          <p className="text-sm text-muted max-w-md">
            Browse your libraries, shows, or folders and click "+ Playlist" to queue items here.
          </p>
        </div>
      ) : (
        <div className="space-y-2.5">
          {items.map((item, index) => {
            const thumbUrl =
              item.card.poster_url ||
              item.card.backdrop_url ||
              api.getArtworkUrl(item.card.id, 'poster');
            const progress =
              typeof item.card.playback_progress === 'number' &&
              item.card.playback_progress > 0
                ? Math.min(100, Math.max(0, Math.round(item.card.playback_progress * 100)))
                : 0;

            return (
              <div
                key={item.card.id}
                draggable={summary.is_editable}
                onDragStart={() => setDraggedIndex(index)}
                onDragOver={(e) => {
                  if (summary.is_editable) {
                    e.preventDefault();
                  }
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  if (summary.is_editable && draggedIndex !== null) {
                    applyReorder(draggedIndex, index);
                    setDraggedIndex(null);
                  }
                }}
                onDragEnd={() => setDraggedIndex(null)}
                className="group flex items-center gap-3 sm:gap-4 p-3 rounded-xl bg-panel hover:bg-panel-hover border border-border-subtle transition-colors"
              >
                {summary.is_editable && (
                  <GripVertical
                    aria-hidden="true"
                    className="w-4 h-4 text-muted shrink-0 hidden sm:block cursor-grab"
                  />
                )}

                <span className="w-7 text-center font-mono text-xs text-muted shrink-0">
                  {index + 1}
                </span>

                {/* Main Track Play Trigger */}
                <button
                  type="button"
                  onClick={() => onPlayQueue(items, index)}
                  className="flex-1 min-w-0 flex items-center gap-3.5 text-left rounded-lg cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                >
                  <div className="relative w-12 h-16 sm:w-14 sm:h-20 rounded-lg overflow-hidden bg-canvas border border-border-subtle shrink-0 flex items-center justify-center">
                    {thumbUrl ? (
                      <img
                        src={thumbUrl}
                        alt={item.card.title}
                        onError={(e) => {
                          (e.currentTarget as HTMLImageElement).style.display = 'none';
                        }}
                        className="w-full h-full object-cover"
                      />
                    ) : (
                      <Film className="w-5 h-5 text-muted" />
                    )}
                    {progress > 0 && (
                      <div className="absolute bottom-0 inset-x-0 h-1 bg-muted">
                        <div
                          className="h-full bg-accent"
                          style={{ width: `${progress}%` }}
                        />
                      </div>
                    )}
                  </div>

                  <div className="flex-1 min-w-0 space-y-1">
                    <div className="text-sm sm:text-base font-medium text-text-main group-hover:text-accent transition-colors truncate">
                      {item.card.title}
                    </div>
                    {item.card.subtitle && (
                      <div className="text-xs text-muted truncate">
                        {item.card.subtitle}
                      </div>
                    )}
                  </div>
                </button>

                <span className="font-mono text-xs text-muted shrink-0 hidden sm:inline">
                  {formatTrackDuration(item.duration_seconds)}
                </span>

                {/* Row Actions */}
                <div className="flex items-center gap-1.5 shrink-0">
                  <button
                    type="button"
                    aria-label={`Details for ${item.card.title}`}
                    onClick={() => setInspectItemId(item.card.id)}
                    className="p-2 rounded-lg bg-canvas/60 hover:bg-canvas text-muted hover:text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                  >
                    <Info className="w-4 h-4" />
                  </button>

                  {summary.is_editable && (
                    <>
                      <button
                        type="button"
                        aria-label={`Move ${item.card.title} up`}
                        disabled={index === 0}
                        onClick={() => applyReorder(index, index - 1)}
                        className="p-2 rounded-lg bg-canvas/60 hover:bg-canvas text-muted hover:text-text-main border border-border-subtle transition-colors disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      >
                        <ChevronUp className="w-4 h-4" />
                      </button>

                      <button
                        type="button"
                        aria-label={`Move ${item.card.title} down`}
                        disabled={index === items.length - 1}
                        onClick={() => applyReorder(index, index + 1)}
                        className="p-2 rounded-lg bg-canvas/60 hover:bg-canvas text-muted hover:text-text-main border border-border-subtle transition-colors disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      >
                        <ChevronDown className="w-4 h-4" />
                      </button>

                      <button
                        type="button"
                        aria-label={`Remove ${item.card.title} from playlist`}
                        onClick={() => handleRemoveItem(item.card.id)}
                        className="p-2 rounded-lg bg-canvas/60 hover:bg-cta/20 text-muted hover:text-cta border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <PlaylistFormModal
        isOpen={isEditOpen}
        mode="edit"
        initialName={summary.name}
        initialDescription={summary.description ?? ''}
        initialIsPublic={summary.is_public}
        onClose={() => setIsEditOpen(false)}
        onSubmit={handleUpdateMetadata}
      />

      <ItemDetailsModal
        itemId={inspectItemId}
        isOpen={inspectItemId !== null}
        onClose={() => setInspectItemId(null)}
        onPlayItem={onPlayItem}
        onNavigateToFolder={onNavigateToFolder}
      />
    </div>
  );
};
