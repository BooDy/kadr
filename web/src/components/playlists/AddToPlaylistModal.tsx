import { useState, useEffect, type FC, type FormEvent } from 'react';
import { X, Check, Plus, ListPlus, Globe, Lock, Loader2 } from 'lucide-react';
import type {
  AddPlaylistItemsPayload,
  PlaylistAddTarget,
  PlaylistSummary,
} from '../../types';
import { api, describeApiError } from '../../api/client';

export interface AddToPlaylistModalProps {
  isOpen: boolean;
  target: PlaylistAddTarget | null;
  onClose: () => void;
  onUpdated?: () => void;
}

function buildAddPayload(
  target: PlaylistAddTarget,
  includeSubfolders: boolean
): AddPlaylistItemsPayload {
  switch (target.kind) {
    case 'item':
      return { item_ids: [target.itemId] };
    case 'season':
      return {
        show: {
          show_id: target.showId,
          season_number: target.seasonNumber,
        },
      };
    case 'folder':
      return {
        folder: {
          library_id: target.libraryId,
          path: target.path,
          recursive: includeSubfolders,
        },
      };
  }
}

export const AddToPlaylistModal: FC<AddToPlaylistModalProps> = ({
  isOpen,
  target,
  onClose,
  onUpdated,
}) => {
  const [playlists, setPlaylists] = useState<PlaylistSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [includeSubfolders, setIncludeSubfolders] = useState(true);
  const [newName, setNewName] = useState('');
  const [newIsPublic, setNewIsPublic] = useState(false);
  const [creating, setCreating] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen || !target) return;

    setFeedback(null);
    setError(null);
    setNewName('');
    setNewIsPublic(false);
    setIncludeSubfolders(target.kind === 'folder' ? target.recursive : true);

    let mounted = true;
    setLoading(true);
    const containsId = target.kind === 'item' ? target.itemId : undefined;
    api
      .getPlaylists(containsId)
      .then((res) => {
        if (mounted) {
          setPlaylists((res.playlists || []).filter((p) => p.is_editable));
          setLoading(false);
        }
      })
      .catch((err) => {
        if (mounted) {
          setError(describeApiError(err));
          setLoading(false);
        }
      });

    return () => {
      mounted = false;
    };
  }, [isOpen, target]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen || !target) return null;

  const handleTogglePlaylist = async (playlist: PlaylistSummary) => {
    setBusyId(playlist.id);
    setError(null);
    setFeedback(null);

    try {
      if (target.kind === 'item' && playlist.contains_queried_item === true) {
        const updatedSummary = await api.removePlaylistItem(playlist.id, target.itemId);
        setPlaylists((prev) =>
          prev.map((p) =>
            p.id === playlist.id
              ? { ...updatedSummary, contains_queried_item: false }
              : p
          )
        );
        setFeedback(`Removed from "${playlist.name}"`);
      } else {
        const payload = buildAddPayload(target, includeSubfolders);
        const res = await api.addPlaylistItems(playlist.id, payload);
        setPlaylists((prev) =>
          prev.map((p) =>
            p.id === playlist.id
              ? {
                  ...res.summary,
                  contains_queried_item:
                    target.kind === 'item' ? true : p.contains_queried_item,
                }
              : p
          )
        );
        setFeedback(`Added (${res.added_count}) to "${playlist.name}"`);
      }
      onUpdated?.();
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setBusyId(null);
    }
  };

  const handleCreateAndAdd = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = newName.trim();
    if (!trimmed) return;

    setCreating(true);
    setError(null);
    setFeedback(null);

    try {
      const payload = buildAddPayload(target, includeSubfolders);
      const created = await api.createPlaylist({
        name: trimmed,
        is_public: newIsPublic,
        items: payload,
      });
      setPlaylists((prev) => [
        {
          ...created.summary,
          contains_queried_item: target.kind === 'item' ? true : undefined,
        },
        ...prev.filter((p) => p.id !== created.summary.id),
      ]);
      setNewName('');
      setNewIsPublic(false);
      setFeedback(`Created "${created.summary.name}" & added items`);
      onUpdated?.();
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`Add ${target.title} to playlist`}
      onClick={(e) => {
        e.stopPropagation();
        onClose();
      }}
      className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-canvas/85 backdrop-blur-sm"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-md rounded-xl bg-panel border border-border-subtle p-5 text-text-main space-y-4"
      >
        {/* Header */}
        <div className="flex items-start justify-between gap-3 border-b border-border-subtle pb-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <ListPlus className="w-5 h-5 text-accent shrink-0" />
              <h2 className="text-base font-semibold text-text-main">Add to Playlist</h2>
            </div>
            <p className="text-xs text-muted truncate mt-0.5">{target.title}</p>
          </div>
          <button
            type="button"
            aria-label="Close add to playlist modal"
            onClick={onClose}
            className="p-1.5 rounded-lg text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Recursive Toggle for Folder Target */}
        {target.kind === 'folder' && (
          <label className="flex items-center gap-2.5 p-3 rounded-xl bg-canvas/60 border border-border-subtle text-sm text-text-main cursor-pointer">
            <input
              type="checkbox"
              checked={includeSubfolders}
              onChange={(e) => setIncludeSubfolders(e.target.checked)}
              className="h-4 w-4 rounded border-border-subtle bg-canvas text-accent accent-accent cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <span>Include subfolders</span>
          </label>
        )}

        {/* Existing Editable Playlists */}
        <div className="space-y-2 max-h-60 overflow-y-auto pr-1">
          {loading ? (
            <div className="flex items-center justify-center py-8 text-muted gap-2">
              <Loader2 className="w-4 h-4 animate-spin text-accent" />
              <span className="text-xs font-mono">Loading playlists...</span>
            </div>
          ) : playlists.length === 0 ? (
            <p className="text-xs text-muted text-center py-4">
              No editable playlists yet. Create one below.
            </p>
          ) : (
            playlists.map((pl) => {
              const isChecked = pl.contains_queried_item === true;
              const isBusy = busyId === pl.id;
              return (
                <button
                  key={pl.id}
                  type="button"
                  disabled={isBusy}
                  onClick={() => handleTogglePlaylist(pl)}
                  className="w-full flex items-center justify-between gap-3 p-3 rounded-xl bg-canvas/60 hover:bg-panel-hover border border-border-subtle text-left transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                >
                  <div className="flex items-center gap-3 min-w-0">
                    <span
                      className={`w-5 h-5 rounded-md flex items-center justify-center border shrink-0 ${
                        isChecked
                          ? 'bg-accent border-accent text-canvas'
                          : 'bg-canvas border-border-subtle text-transparent'
                      }`}
                    >
                      <Check className="w-3.5 h-3.5" />
                    </span>
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-text-main truncate">
                        {pl.name}
                      </div>
                      <div className="font-mono text-[11px] text-muted">
                        {pl.visible_item_count}{' '}
                        {pl.visible_item_count === 1 ? 'item' : 'items'}
                      </div>
                    </div>
                  </div>

                  {pl.is_public ? (
                    <Globe className="w-3.5 h-3.5 text-accent shrink-0" />
                  ) : (
                    <Lock className="w-3.5 h-3.5 text-muted shrink-0" />
                  )}
                </button>
              );
            })
          )}
        </div>

        {feedback && (
          <div className="p-2.5 rounded-xl bg-highlight/15 border border-highlight/40 text-xs text-highlight">
            {feedback}
          </div>
        )}

        {error && (
          <div className="p-2.5 rounded-xl bg-cta/15 border border-cta/40 text-xs text-highlight">
            {error}
          </div>
        )}

        {/* 1-Step Inline Create & Add */}
        <form
          onSubmit={handleCreateAndAdd}
          className="pt-3 border-t border-border-subtle space-y-2.5"
        >
          <div className="text-xs font-medium uppercase tracking-wider text-muted">
            Create new playlist & add
          </div>
          <div className="flex items-center gap-2">
            <input
              type="text"
              maxLength={120}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="New playlist name..."
              className="flex-1 rounded-xl bg-canvas border border-border-subtle px-3 py-2 text-sm text-text-main placeholder:text-muted focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <button
              type="submit"
              disabled={creating || !newName.trim()}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-xs font-semibold transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shrink-0"
            >
              {creating ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Plus className="w-3.5 h-3.5" />
              )}
              <span>Create & Add</span>
            </button>
          </div>
          <label className="inline-flex items-center gap-2 text-xs text-muted cursor-pointer">
            <input
              type="checkbox"
              checked={newIsPublic}
              onChange={(e) => setNewIsPublic(e.target.checked)}
              className="h-3.5 w-3.5 rounded border-border-subtle bg-canvas text-accent accent-accent cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <span>Share on server</span>
          </label>
        </form>
      </div>
    </div>
  );
};
