import { useState, useEffect, useCallback, type FC } from 'react';
import { Plus, Globe, Lock, ListVideo, Loader2, AlertCircle } from 'lucide-react';
import type { PlaylistItemView, PlaylistSummary, User } from '../../types';
import { api, describeApiError } from '../../api/client';
import {
  PlaylistCoverMosaic,
  PlaylistDetailView,
  formatPlaylistRuntime,
} from './PlaylistDetailView';
import { PlaylistFormModal, type PlaylistFormValues } from './PlaylistFormModal';

export interface PlaylistsScreenProps {
  currentUser: User;
  activePlaylistId?: string | null;
  onSelectPlaylist?: (playlistId: string | null) => void;
  onPlayItem: (itemId: number) => void;
  onPlayQueue: (items: PlaylistItemView[], startIndex?: number) => void;
  onNavigateToFolder?: (libraryId: string, folderPath: string) => void;
}

const PlaylistCard: FC<{
  playlist: PlaylistSummary;
  showOwner?: boolean;
  onSelect: (id: string) => void;
}> = ({ playlist, showOwner = false, onSelect }) => {
  return (
    <button
      type="button"
      onClick={() => onSelect(playlist.id)}
      className="group flex flex-col text-left rounded-xl bg-panel hover:bg-panel-hover border border-border-subtle p-3 transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
    >
      <div className="relative w-full">
        <PlaylistCoverMosaic
          urls={playlist.cover_poster_urls}
          title={playlist.name}
          className="w-full aspect-square"
        />

        <div className="absolute top-2 right-2 flex items-center gap-1.5">
          {playlist.locked_item_count > 0 && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-lg bg-canvas/90 border border-highlight/40 text-[11px] font-mono text-highlight">
              <Lock className="w-3 h-3" />
              <span>+{playlist.locked_item_count} locked</span>
            </span>
          )}
          <span className="inline-flex items-center p-1.5 rounded-lg bg-canvas/90 border border-border-subtle text-text-main">
            {playlist.is_public ? (
              <Globe className="w-3.5 h-3.5 text-accent" />
            ) : (
              <Lock className="w-3.5 h-3.5 text-muted" />
            )}
          </span>
        </div>
      </div>

      <div className="mt-3 space-y-1 min-w-0 w-full">
        <h3 className="text-sm sm:text-base font-semibold text-text-main group-hover:text-accent transition-colors truncate">
          {playlist.name}
        </h3>

        {showOwner && (
          <p className="text-xs text-muted truncate">by @{playlist.owner_username}</p>
        )}

        <p className="font-mono text-xs text-muted truncate">
          {playlist.visible_item_count}{' '}
          {playlist.visible_item_count === 1 ? 'item' : 'items'} •{' '}
          {formatPlaylistRuntime(playlist.total_duration_seconds)}
        </p>
      </div>
    </button>
  );
};

export const PlaylistsScreen: FC<PlaylistsScreenProps> = ({
  currentUser,
  activePlaylistId,
  onSelectPlaylist,
  onPlayItem,
  onPlayQueue,
  onNavigateToFolder,
}) => {
  const [internalPlaylistId, setInternalPlaylistId] = useState<string | null>(null);
  const selectedId =
    activePlaylistId !== undefined ? activePlaylistId : internalPlaylistId;

  const selectPlaylist = (id: string | null) => {
    setInternalPlaylistId(id);
    onSelectPlaylist?.(id);
  };

  const [playlists, setPlaylists] = useState<PlaylistSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  const loadPlaylists = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.getPlaylists();
      setPlaylists(res.playlists || []);
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (selectedId === null) {
      loadPlaylists();
    }
  }, [selectedId, loadPlaylists]);

  const handleCreatePlaylist = async (values: PlaylistFormValues) => {
    const created = await api.createPlaylist({
      name: values.name,
      description: values.description,
      is_public: values.is_public,
    });
    setPlaylists((prev) => [
      created.summary,
      ...prev.filter((p) => p.id !== created.summary.id),
    ]);
  };

  if (selectedId !== null) {
    return (
      <PlaylistDetailView
        playlistId={selectedId}
        onBack={() => selectPlaylist(null)}
        onPlayItem={onPlayItem}
        onPlayQueue={onPlayQueue}
        onNavigateToFolder={onNavigateToFolder}
        onDeleted={() => {
          selectPlaylist(null);
        }}
      />
    );
  }

  const myPlaylists = playlists.filter((p) => p.owner_user_id === currentUser.id);
  const sharedPlaylists = playlists.filter(
    (p) => p.is_public && p.owner_user_id !== currentUser.id
  );

  return (
    <div className="max-w-7xl mx-auto p-4 sm:p-6 lg:p-8 space-y-8">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <ListVideo className="w-7 h-7 text-accent" />
          <h1 className="text-2xl sm:text-3xl font-semibold text-text-main">Playlists</h1>
        </div>

        <button
          type="button"
          onClick={() => setIsCreateOpen(true)}
          className="inline-flex items-center gap-2 px-5 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-sm font-semibold transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <Plus className="w-4 h-4" />
          <span>+ New Playlist</span>
        </button>
      </div>

      {loading ? (
        <div className="flex flex-col items-center justify-center py-20 text-muted space-y-3">
          <Loader2 className="w-8 h-8 animate-spin text-accent" />
          <p className="text-xs font-mono uppercase tracking-wider">Loading playlists...</p>
        </div>
      ) : error ? (
        <div className="p-8 rounded-xl bg-panel border border-border-subtle flex flex-col items-center text-center space-y-3">
          <AlertCircle className="w-8 h-8 text-highlight" />
          <p className="text-sm text-text-main">{error}</p>
          <button
            type="button"
            onClick={loadPlaylists}
            className="px-4 py-2 rounded-xl bg-cta hover:bg-cta-hover text-text-main text-xs font-semibold cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            Retry
          </button>
        </div>
      ) : (
        <div className="space-y-10">
          {/* My Playlists Section */}
          <section className="space-y-4">
            <h2 className="text-sm font-semibold uppercase tracking-wider text-muted">
              My Playlists
            </h2>

            {myPlaylists.length === 0 ? (
              <div className="p-10 rounded-xl bg-panel border border-border-subtle flex flex-col items-center justify-center text-center space-y-3">
                <ListVideo className="w-10 h-10 text-muted" />
                <h3 className="text-base font-medium text-text-main">No playlists yet</h3>
                <p className="text-sm text-muted max-w-md">
                  Create a playlist to curate movies, series seasons, or folders in any playback order.
                </p>
              </div>
            ) : (
              <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                {myPlaylists.map((pl) => (
                  <PlaylistCard
                    key={pl.id}
                    playlist={pl}
                    onSelect={(id) => selectPlaylist(id)}
                  />
                ))}
              </div>
            )}
          </section>

          {/* Shared on Server Section */}
          {sharedPlaylists.length > 0 && (
            <section className="space-y-4">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-muted">
                Shared on Server
              </h2>

              <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                {sharedPlaylists.map((pl) => (
                  <PlaylistCard
                    key={pl.id}
                    playlist={pl}
                    showOwner={true}
                    onSelect={(id) => selectPlaylist(id)}
                  />
                ))}
              </div>
            </section>
          )}
        </div>
      )}

      <PlaylistFormModal
        isOpen={isCreateOpen}
        mode="create"
        onClose={() => setIsCreateOpen(false)}
        onSubmit={handleCreatePlaylist}
      />
    </div>
  );
};
