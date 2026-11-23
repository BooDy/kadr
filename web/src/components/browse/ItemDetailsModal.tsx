import { useState, useEffect, useMemo, type FC } from 'react';
import {
  X,
  Play,
  Film,
  Download,
  Search,
  Check,
  CheckCircle2,
  Loader2,
  Folder,
  ListPlus,
  Pencil,
  Sparkles,
  Lock,
} from 'lucide-react';
import type {
  AudioStreamInfo,
  CardViewModel,
  ItemDetailsPayload,
  OnlineSubtitleMatch,
  PlaylistAddTarget,
  SubtitleTrack,
  User,
} from '../../types';
import { api } from '../../api/client';
import { AddToPlaylistModal } from '../playlists/AddToPlaylistModal';
import { MetadataEditorModal } from './MetadataEditorModal';
import { IdentifyTmdbModal } from './IdentifyTmdbModal';
import { WatchStateButton } from './WatchStateButton';
import { FavoriteButton } from '../favorites/FavoriteButton';
import {
  applyWatchStateToCard,
  emitWatchStateChanged,
  isCardWatched,
} from '../../utils/watchStateEvents';
import { applyFavoriteToCard } from '../../utils/favoriteEvents';
import {
  formatChannelLayout,
  formatLanguageCode,
} from '../../player/compatibility';

function formatAudioStreamBadge(track: AudioStreamInfo): string {
  const lang =
    formatLanguageCode(track.language) ||
    track.title?.trim() ||
    `Track ${track.audio_index + 1}`;
  const codecUpper = track.codec ? track.codec.toUpperCase() : '';
  const layout = formatChannelLayout(track.channels);
  const techPart = [codecUpper, layout].filter(Boolean).join(' ');
  const base = [lang, techPart].filter(Boolean).join(' • ');
  return track.is_commentary ? `${base} (Commentary)` : base;
}

export interface ItemDetailsModalProps {
  itemId: number | null;
  isOpen?: boolean;
  onClose: () => void;
  onPlay?: (itemId: number) => void;
  onPlayItem?: (itemId: number) => void;
  initialDetails?: ItemDetailsPayload;
  onNavigateToFolder?: (libraryId: string, folderPath: string) => void;
  currentUser?: User | null;
}

interface EpisodeCardProps {
  episode: CardViewModel;
  onPlay: (id: number) => void;
  onAddToPlaylist: (episode: CardViewModel) => void;
  onToggleWatched: (episodeId: number, watched: boolean) => void;
  onToggleFavorite: (episodeId: number, isFavorite: boolean) => void;
  formatDuration: (seconds?: number) => string | null;
}

const EpisodeCard: FC<EpisodeCardProps> = ({
  episode,
  onPlay,
  onAddToPlaylist,
  onToggleWatched,
  onToggleFavorite,
  formatDuration,
}) => {
  const [thumbFailed, setThumbFailed] = useState(false);
  const thumbUrl =
    episode.backdrop_url ||
    episode.poster_url ||
    (episode.id ? api.getArtworkUrl(episode.id, 'backdrop') : undefined);
  const watched = isCardWatched(episode);
  const hasProgress =
    typeof episode.playback_progress === 'number' && episode.playback_progress > 0;
  const progressPercent = hasProgress
    ? Math.min(100, Math.max(0, Math.round((episode.playback_progress || 0) * 100)))
    : 0;
  const epBadge =
    episode.episode !== undefined
      ? `E${String(episode.episode).padStart(2, '0')}`
      : episode.badge;
  const durationText = episode.duration_seconds
    ? formatDuration(episode.duration_seconds)
    : null;

  const handlePlay = () => {
    onPlay(episode.id);
  };

  return (
    <div className="group relative flex flex-col sm:flex-row items-start sm:items-center gap-4 p-3 rounded-2xl bg-canvas/40 border border-border-subtle hover:bg-canvas/70 transition-all duration-200">
      {/* Thumbnail Container */}
      <div
        role="button"
        tabIndex={0}
        aria-label={`Play episode ${episode.title}`}
        onClick={handlePlay}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            handlePlay();
          }
        }}
        className="relative w-full sm:w-44 aspect-video rounded-xl overflow-hidden bg-canvas border border-border-subtle flex-shrink-0 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
      >
        {!thumbFailed && thumbUrl ? (
          <img
            src={thumbUrl}
            alt={episode.title}
            onError={() => setThumbFailed(true)}
            className="w-full h-full object-cover filter brightness-95 group-hover:brightness-105 transition-all duration-200"
          />
        ) : (
          <div className="w-full h-full flex flex-col items-center justify-center p-3 text-center bg-gradient-to-tr from-canvas via-panel to-canvas">
            <Film className="w-8 h-8 text-muted mb-1 group-hover:text-accent transition-colors" />
            <span className="text-[11px] text-muted font-medium line-clamp-1">{episode.title}</span>
          </div>
        )}

        {/* Play hover overlay */}
        <div className="absolute inset-0 bg-canvas/40 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity duration-200 pointer-events-none">
          <div className="w-10 h-10 rounded-full bg-cta text-white flex items-center justify-center shadow-lg shadow-cta/30 transform scale-90 group-hover:scale-100 transition-transform duration-200">
            <Play className="w-4 h-4 fill-white text-white ml-0.5" />
          </div>
        </div>

        {/* Duration Badge on Thumbnail */}
        {durationText && (
          <div className="absolute bottom-1.5 right-1.5 px-1.5 py-0.5 rounded text-[10px] font-mono font-medium bg-canvas/80 text-text-main border border-border-subtle">
            {durationText}
          </div>
        )}

        {/* Progress bar */}
        {hasProgress && !watched && (
          <div className="absolute bottom-0 inset-x-0 h-1 bg-muted">
            <div
              className="h-full bg-accent"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        )}
      </div>

      {/* Episode Details */}
      <div className="flex-1 min-w-0 space-y-1.5 w-full">
        <div className="flex items-center gap-2 flex-wrap">
          {epBadge && (
            <span className="px-2 py-0.5 rounded text-[11px] font-bold font-mono bg-highlight/20 text-highlight border border-highlight/40">
              {epBadge}
            </span>
          )}
          {durationText && (
            <span className="text-xs text-muted font-medium">
              {durationText}
            </span>
          )}
        </div>

        <h4 className="text-sm sm:text-base font-bold text-text-main group-hover:text-accent transition-colors line-clamp-1">
          {episode.title}
        </h4>

        {episode.subtitle && (
          <p className="text-xs text-muted line-clamp-2">
            {episode.subtitle}
          </p>
        )}
      </div>

      {/* 1-Click Play, Favorite Toggle, Watch Toggle & Add to Playlist Buttons */}
      <div className="self-end sm:self-center flex items-center gap-2 flex-shrink-0">
        <FavoriteButton
          itemId={episode.id}
          title={episode.title}
          mediaType={episode.media_type}
          isFavorite={Boolean(episode.is_favorite)}
          variant="icon"
          alwaysVisible
          onToggled={(fav) => onToggleFavorite(episode.id, fav)}
        />
        <WatchStateButton
          itemId={episode.id}
          title={episode.title}
          mediaType={episode.media_type}
          watched={watched}
          variant="icon"
          alwaysVisible
          onToggled={(w) => onToggleWatched(episode.id, w)}
        />
        <button
          type="button"
          aria-label={`Add ${episode.title} to playlist`}
          onClick={() => onAddToPlaylist(episode)}
          className="p-2 rounded-xl bg-panel hover:bg-panel-hover text-muted hover:text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <ListPlus className="w-4 h-4 text-accent" />
        </button>
        <button
          type="button"
          aria-label={`Play ${episode.title}`}
          onClick={handlePlay}
          className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-cta hover:bg-cta-hover text-white text-xs font-bold shadow-md shadow-cta/20 transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <Play className="w-3.5 h-3.5 fill-white text-white" />
          <span>Play</span>
        </button>
      </div>
    </div>
  );
};

export const ItemDetailsModal: FC<ItemDetailsModalProps> = ({
  itemId,
  isOpen = true,
  onClose,
  onPlay,
  onPlayItem,
  initialDetails,
  onNavigateToFolder,
  currentUser,
}) => {
  const [details, setDetails] = useState<ItemDetailsPayload | null>(initialDetails ?? null);
  const [subtitles, setSubtitles] = useState<SubtitleTrack[]>([]);
  const [fetchedAudioTracks, setFetchedAudioTracks] = useState<AudioStreamInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [activeTab, setActiveTab] = useState<'details' | 'subtitles'>('details');
  const [selectedSeason, setSelectedSeason] = useState<number | null>(null);
  const [seriesBatchBusy, setSeriesBatchBusy] = useState(false);
  const [seasonBatchBusy, setSeasonBatchBusy] = useState(false);

  // Subtitle search state
  const [searchLang, setSearchLang] = useState('');
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<OnlineSubtitleMatch[]>([]);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadSuccessMsg, setDownloadSuccessMsg] = useState<string | null>(null);

  // Artwork & child modal state
  const [backdropFailed, setBackdropFailed] = useState(false);
  const [posterFailed, setPosterFailed] = useState(false);
  const [artworkRev, setArtworkRev] = useState(0);
  const [playlistTarget, setPlaylistTarget] = useState<PlaylistAddTarget | null>(null);
  const [isMetadataEditorOpen, setIsMetadataEditorOpen] = useState(false);
  const [isIdentifyOpen, setIsIdentifyOpen] = useState(false);

  const effectiveUser = currentUser !== undefined ? currentUser : api.getUser();
  const isAdmin = effectiveUser?.role === 'admin';

  useEffect(() => {
    if (!isOpen || itemId === null) {
      setDetails(null);
      setSubtitles([]);
      setFetchedAudioTracks([]);
      setSearchResults([]);
      setSearchError(null);
      setDownloadSuccessMsg(null);
      setActiveTab('details');
      setBackdropFailed(false);
      setPosterFailed(false);
      setSelectedSeason(null);
      setSeriesBatchBusy(false);
      setSeasonBatchBusy(false);
      setPlaylistTarget(null);
      setIsMetadataEditorOpen(false);
      setIsIdentifyOpen(false);
      return;
    }

    let isMounted = true;
    setFetchedAudioTracks([]);

    const maybeFetchAudioTracks = (target: ItemDetailsPayload | null | undefined) => {
      if (
        target &&
        !(target.technical?.audio_tracks && target.technical.audio_tracks.length > 0) &&
        target.card?.media_type !== 'show' &&
        target.card?.media_type !== 'live_channel' &&
        typeof api.getAudioTracks === 'function'
      ) {
        Promise.resolve(api.getAudioTracks(itemId))
          .then((tracks) => {
            if (isMounted && Array.isArray(tracks) && tracks.length > 0) {
              setFetchedAudioTracks(tracks);
            }
          })
          .catch(() => {});
      }
    };

    if (initialDetails) {
      setDetails(initialDetails);
      setLoading(false);
      maybeFetchAudioTracks(initialDetails);
      return () => {
        isMounted = false;
      };
    }

    setLoading(true);

    Promise.all([api.getItemDetails(itemId), api.getSubtitles(itemId)])
      .then(([itemDetails, tracks]) => {
        if (isMounted) {
          setDetails(itemDetails);
          setSubtitles(tracks || []);
          setLoading(false);
          maybeFetchAudioTracks(itemDetails);
        }
      })
      .catch(() => {
        if (isMounted) {
          setLoading(false);
        }
      });

    return () => {
      isMounted = false;
    };
  }, [isOpen, itemId, initialDetails]);

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        if (isMetadataEditorOpen) {
          setIsMetadataEditorOpen(false);
        } else if (isIdentifyOpen) {
          setIsIdentifyOpen(false);
        } else if (playlistTarget === null) {
          onClose();
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose, playlistTarget, isMetadataEditorOpen, isIdentifyOpen]);

  const episodesBySeason = useMemo(() => {
    if (!details?.episodes || details.episodes.length === 0) {
      return new Map<number, CardViewModel[]>();
    }
    const map = new Map<number, CardViewModel[]>();
    for (const ep of details.episodes) {
      const s = ep.season ?? 1;
      const list = map.get(s) || [];
      list.push(ep);
      map.set(s, list);
    }
    return map;
  }, [details?.episodes]);

  const seasons = useMemo(() => {
    return Array.from(episodesBySeason.keys()).sort((a, b) => a - b);
  }, [episodesBySeason]);

  const currentSeason = (selectedSeason !== null && episodesBySeason.has(selectedSeason))
    ? selectedSeason
    : (seasons[0] ?? 1);

  const episodesForCurrentSeason = episodesBySeason.get(currentSeason) || [];

  const hasEpisodes = Boolean(details?.episodes && details.episodes.length > 0);
  const allSeriesWatched = Boolean(
    hasEpisodes && details!.episodes!.every((ep) => isCardWatched(ep))
  );
  const allSeasonWatched =
    episodesForCurrentSeason.length > 0 &&
    episodesForCurrentSeason.every((ep) => isCardWatched(ep));

  const audioTracks =
    details?.technical?.audio_tracks && details.technical.audio_tracks.length > 0
      ? details.technical.audio_tracks
      : fetchedAudioTracks;

  if (!isOpen || itemId === null) return null;

  const handleToggleEpisodeWatched = (episodeId: number, watched: boolean) => {
    setDetails((prev) =>
      prev && prev.episodes
        ? {
            ...prev,
            episodes: prev.episodes.map((ep) =>
              ep.id === episodeId ? applyWatchStateToCard(ep, watched) : ep
            ),
          }
        : prev
    );
  };

  const handleToggleEpisodeFavorite = (episodeId: number, isFavorite: boolean) => {
    setDetails((prev) =>
      prev && prev.episodes
        ? {
            ...prev,
            episodes: prev.episodes.map((ep) =>
              ep.id === episodeId ? applyFavoriteToCard(ep, isFavorite) : ep
            ),
          }
        : prev
    );
  };

  const handleToggleSeriesWatched = async () => {
    if (
      !details?.episodes ||
      details.episodes.length === 0 ||
      seriesBatchBusy ||
      seasonBatchBusy
    ) {
      return;
    }
    const targetIds = details.episodes.map((e) => e.id);
    const nextWatched = !allSeriesWatched;
    const previousEpisodes = details.episodes;
    const idSet = new Set(targetIds);

    setDetails((prev) =>
      prev && prev.episodes
        ? {
            ...prev,
            episodes: prev.episodes.map((ep) =>
              idSet.has(ep.id) ? applyWatchStateToCard(ep, nextWatched) : ep
            ),
          }
        : prev
    );
    setSeriesBatchBusy(true);
    try {
      await api.setPlaybackStatesBatch(targetIds, nextWatched);
      emitWatchStateChanged(targetIds, nextWatched);
    } catch {
      setDetails((prev) => (prev ? { ...prev, episodes: previousEpisodes } : prev));
    } finally {
      setSeriesBatchBusy(false);
    }
  };

  const handleToggleSeasonWatched = async () => {
    if (
      episodesForCurrentSeason.length === 0 ||
      seriesBatchBusy ||
      seasonBatchBusy ||
      !details?.episodes
    ) {
      return;
    }
    const targetIds = episodesForCurrentSeason.map((e) => e.id);
    const nextWatched = !allSeasonWatched;
    const previousEpisodes = details.episodes;
    const idSet = new Set(targetIds);

    setDetails((prev) =>
      prev && prev.episodes
        ? {
            ...prev,
            episodes: prev.episodes.map((ep) =>
              idSet.has(ep.id) ? applyWatchStateToCard(ep, nextWatched) : ep
            ),
          }
        : prev
    );
    setSeasonBatchBusy(true);
    try {
      await api.setPlaybackStatesBatch(targetIds, nextWatched);
      emitWatchStateChanged(targetIds, nextWatched);
    } catch {
      setDetails((prev) => (prev ? { ...prev, episodes: previousEpisodes } : prev));
    } finally {
      setSeasonBatchBusy(false);
    }
  };

  const handleSearchSubtitles = async () => {
    if (itemId === null) return;
    setSearching(true);
    setSearchError(null);
    setDownloadSuccessMsg(null);
    try {
      const res = await api.searchSubtitles(itemId, searchLang.trim() || undefined);
      setSearchResults(res.matches || []);
    } catch {
      setSearchError('Failed to search online subtitles.');
    } finally {
      setSearching(false);
    }
  };

  const handleDownloadSubtitle = async (match: OnlineSubtitleMatch) => {
    if (itemId === null) return;
    setDownloadingId(match.id);
    setDownloadSuccessMsg(null);
    try {
      await api.downloadSubtitle(itemId, {
        file_id: match.id,
        language: match.language,
        title: match.release_name,
        is_forced: false,
      });
      // Refresh current subtitle tracks
      const updatedTracks = await api.getSubtitles(itemId);
      setSubtitles(updatedTracks);
      setDownloadSuccessMsg(`Downloaded "${match.language}" subtitle successfully.`);
    } catch {
      setSearchError('Failed to download subtitle.');
    } finally {
      setDownloadingId(null);
    }
  };

  const formatDuration = (seconds?: number) => {
    if (!seconds) return null;
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (hours > 0) {
      return `${hours}h ${minutes}m`;
    }
    return `${minutes}m`;
  };

  const handlePlayItem = (targetId: number | null) => {
    if (targetId === null) return;
    if (onPlayItem) {
      onPlayItem(targetId);
    } else if (onPlay) {
      onPlay(targetId);
    }
    onClose();
  };

  const card = details?.card;
  const duration =
    details?.duration_seconds || details?.technical?.duration_seconds;
  const baseBackdropUrl =
    card?.backdrop_url || api.getArtworkUrl(itemId, 'backdrop');
  const basePosterUrl = card?.poster_url || api.getArtworkUrl(itemId, 'poster');
  const backdropUrl =
    artworkRev > 0
      ? `${baseBackdropUrl}${baseBackdropUrl.includes('?') ? '&' : '?'}v=${artworkRev}`
      : baseBackdropUrl;
  const posterUrl =
    artworkRev > 0
      ? `${basePosterUrl}${basePosterUrl.includes('?') ? '&' : '?'}v=${artworkRev}`
      : basePosterUrl;

  const handleMetadataUpdated = (updated: ItemDetailsPayload) => {
    setDetails(updated);
    setPosterFailed(false);
    setBackdropFailed(false);
    setArtworkRev((r) => r + 1);
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="item-modal-title"
      className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-6 bg-black/85 backdrop-blur-md overflow-y-auto"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-4xl max-h-[90vh] overflow-y-auto rounded-2xl bg-panel border border-border-subtle shadow-2xl flex flex-col my-auto text-text-main"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Close Button */}
        <button
          onClick={onClose}
          aria-label="Close modal"
          className="absolute top-4 right-4 z-30 p-2.5 rounded-full bg-canvas/60 hover:bg-panel text-muted hover:text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
        >
          <X className="w-5 h-5" />
        </button>

        {loading ? (
          <div className="flex flex-col items-center justify-center min-h-[380px] p-12 text-muted">
            <Loader2 className="w-10 h-10 animate-spin text-cta mb-4" />
            <p className="text-sm">Loading media information...</p>
          </div>
        ) : !details ? (
          <div className="flex flex-col items-center justify-center min-h-[300px] p-8 text-muted">
            <Film className="w-12 h-12 text-muted mb-3" />
            <p className="text-base text-text-main">Media information unavailable</p>
          </div>
        ) : (
          <>
            {/* Header Hero Area with Backdrop */}
            <div className="relative min-h-[220px] sm:min-h-[280px] w-full overflow-hidden bg-canvas border-b border-border-subtle flex items-end">
              {!backdropFailed && backdropUrl ? (
                <img
                  src={backdropUrl}
                  alt={card?.title}
                  onError={() => setBackdropFailed(true)}
                  className="absolute inset-0 w-full h-full object-cover filter brightness-[0.7]"
                />
              ) : (
                <div className="absolute inset-0 bg-gradient-to-tr from-canvas via-panel to-canvas" />
              )}

              {/* Gradient Vignette Overlays */}
              <div className="absolute inset-0 bg-gradient-to-t from-panel via-panel/80 to-transparent" />
              <div className="absolute inset-0 bg-gradient-to-r from-panel/90 to-transparent" />

              {/* Poster and Title Info */}
              <div className="relative z-10 flex flex-col sm:flex-row items-center sm:items-end gap-5 p-6 sm:p-8 w-full">
                {/* Poster Artwork with Placeholder */}
                <div className="flex-shrink-0 w-28 sm:w-36 aspect-[2/3] rounded-xl overflow-hidden bg-canvas border border-border-subtle shadow-xl hidden sm:block">
                  {!posterFailed && posterUrl ? (
                    <img
                      src={posterUrl}
                      alt={card?.title}
                      onError={() => setPosterFailed(true)}
                      className="w-full h-full object-cover"
                    />
                  ) : (
                    <div className="w-full h-full bg-canvas flex flex-col items-center justify-center p-2 text-center">
                      <Film className="w-8 h-8 text-muted mb-1" />
                      <span className="text-[10px] text-muted font-medium">
                        {card?.title}
                      </span>
                    </div>
                  )}
                </div>

                {/* Details Header Text */}
                <div className="space-y-2 flex-1 text-center sm:text-left">
                  <div className="flex flex-wrap items-center justify-center sm:justify-start gap-2 text-xs font-semibold">
                    {card?.release_year && (
                      <span className="px-2 py-0.5 rounded bg-canvas/60 text-text-main border border-border-subtle">
                        {card.release_year}
                      </span>
                    )}
                    {duration && (
                      <span className="px-2 py-0.5 rounded bg-canvas/60 text-text-main border border-border-subtle">
                        {formatDuration(duration)}
                      </span>
                    )}
                    {details.technical?.resolution && (
                      <span className="px-2 py-0.5 rounded bg-highlight/20 text-highlight border border-highlight/40 font-bold">
                        {details.technical.resolution}
                      </span>
                    )}
                    {card?.badge && (
                      <span className="px-2 py-0.5 rounded bg-accent/20 text-accent border border-accent/40">
                        {card.badge}
                      </span>
                    )}
                  </div>

                  <h1
                    id="item-modal-title"
                    className="text-2xl sm:text-3xl lg:text-4xl font-extrabold text-text-main tracking-tight"
                  >
                    {card?.title}
                  </h1>

                  {card?.subtitle && (
                    <p className="text-xs sm:text-sm text-muted italic">
                      {card.subtitle}
                    </p>
                  )}
                </div>

                {/* Quick Play & Actions */}
                <div className="self-center sm:self-end flex flex-wrap items-center gap-3">
                  {isAdmin && (
                    <>
                      <button
                        type="button"
                        onClick={() => setIsMetadataEditorOpen(true)}
                        className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium bg-panel hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      >
                        <Pencil className="w-4 h-4 text-accent" />
                        <span>Edit Metadata</span>
                      </button>
                      <button
                        type="button"
                        onClick={() => setIsIdentifyOpen(true)}
                        className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium bg-panel hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      >
                        <Sparkles className="w-4 h-4 text-highlight" />
                        <span>Identify</span>
                      </button>
                    </>
                  )}
                  {details.library_id && onNavigateToFolder && (
                    <button
                      type="button"
                      onClick={() => {
                        onClose();
                        onNavigateToFolder(details.library_id!, details.folder_path ?? '');
                      }}
                      className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium bg-panel hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                    >
                      <Folder className="w-4 h-4 text-accent" />
                      <span>Browse Folder</span>
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() =>
                      setPlaylistTarget({
                        kind: 'item',
                        itemId,
                        title: card?.title || `Item #${itemId}`,
                      })
                    }
                    className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium bg-panel hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                  >
                    <ListPlus className="w-4 h-4 text-accent" />
                    <span>+ Playlist</span>
                  </button>
                  <FavoriteButton
                    itemId={itemId}
                    title={card?.title || 'Item'}
                    mediaType={card?.media_type}
                    isFavorite={Boolean(card?.is_favorite)}
                    variant="pill"
                    onToggled={(fav) =>
                      setDetails((prev) =>
                        prev
                          ? {
                              ...prev,
                              card: applyFavoriteToCard(prev.card, fav),
                            }
                          : prev
                      )
                    }
                  />
                  {!hasEpisodes &&
                    card?.media_type !== 'show' &&
                    card?.media_type !== 'live_channel' && (
                      <WatchStateButton
                        itemId={itemId}
                        title={card?.title || 'Item'}
                        mediaType={card?.media_type}
                        watched={isCardWatched(card)}
                        variant="pill"
                        onToggled={(w) =>
                          setDetails((prev) =>
                            prev
                              ? {
                                  ...prev,
                                  resume_position_seconds: undefined,
                                  card: applyWatchStateToCard(prev.card, w),
                                }
                              : prev
                          )
                        }
                      />
                    )}
                  {hasEpisodes && card?.media_type !== 'live_channel' && (
                    <button
                      type="button"
                      aria-pressed={allSeriesWatched}
                      disabled={seriesBatchBusy || seasonBatchBusy}
                      onClick={handleToggleSeriesWatched}
                      className={`flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-medium border transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                        allSeriesWatched
                          ? 'bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border-emerald-500/40'
                          : 'bg-panel hover:bg-panel-hover text-text-main border-border-subtle'
                      }`}
                    >
                      {seriesBatchBusy ? (
                        <Loader2 aria-hidden="true" className="w-4 h-4 animate-spin" />
                      ) : (
                        <CheckCircle2
                          aria-hidden="true"
                          className={`w-4 h-4 ${
                            allSeriesWatched ? 'text-emerald-400' : 'text-muted'
                          }`}
                        />
                      )}
                      <span>
                        {allSeriesWatched ? 'Series Watched' : 'Mark Series Watched'}
                      </span>
                    </button>
                  )}
                  <button
                    onClick={() => handlePlayItem(itemId)}
                    className="flex items-center gap-2 px-6 py-3 rounded-xl bg-cta hover:bg-cta-hover text-white font-bold text-sm shadow-lg shadow-cta/25 hover:scale-105 active:scale-95 transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                  >
                    <Play className="w-4 h-4 fill-white text-white" />
                    Play
                  </button>
                </div>
              </div>
            </div>

            {/* Navigation Tabs */}
            <div className="flex items-center border-b border-border-subtle px-6 sm:px-8 bg-panel">
              <button
                onClick={() => setActiveTab('details')}
                className={`py-3.5 px-4 text-sm font-semibold border-b-2 transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  activeTab === 'details'
                    ? 'border-accent text-accent'
                    : 'border-transparent text-muted hover:text-text-main'
                }`}
              >
                Overview
              </button>
              <button
                onClick={() => setActiveTab('subtitles')}
                className={`py-3.5 px-4 text-sm font-semibold border-b-2 transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  activeTab === 'subtitles'
                    ? 'border-accent text-accent'
                    : 'border-transparent text-muted hover:text-text-main'
                }`}
              >
                Subtitles ({subtitles.length})
              </button>
            </div>

            {/* Modal Body */}
            <div className="p-6 sm:p-8 space-y-6">
              {activeTab === 'details' ? (
                <div className="space-y-6">
                  {/* Synopsis / Overview */}
                  <div className="space-y-2">
                    <h3 className="text-xs font-bold tracking-wider text-muted uppercase">
                      Synopsis
                    </h3>
                    <p className="text-sm sm:text-base text-text-main leading-relaxed">
                      {details.overview || card?.subtitle || 'No overview available for this title.'}
                    </p>
                  </div>

                  {/* Genres */}
                  {details.genres && details.genres.length > 0 && (
                    <div className="space-y-2">
                      <h3 className="text-xs font-bold tracking-wider text-muted uppercase">
                        Genres
                      </h3>
                      <div className="flex flex-wrap gap-2">
                        {details.genres.map((genre) => (
                          <span
                            key={genre}
                            className="px-3 py-1 rounded-lg bg-canvas/60 border border-border-subtle text-xs font-medium text-text-main"
                          >
                            {genre}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Credits & Production Metadata */}
                  {(details.director ||
                    (details.actors && details.actors.length > 0) ||
                    details.studio ||
                    details.country ||
                    (details.locked_fields && details.locked_fields.length > 0)) && (
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm pt-2 border-t border-border-subtle">
                      {details.director && (
                        <div>
                          <span className="text-xs font-bold uppercase tracking-wider text-muted block">
                            Director
                          </span>
                          <span className="text-text-main">{details.director}</span>
                        </div>
                      )}
                      {details.studio && (
                        <div>
                          <span className="text-xs font-bold uppercase tracking-wider text-muted block">
                            Studio
                          </span>
                          <span className="text-text-main">{details.studio}</span>
                        </div>
                      )}
                      {details.country && (
                        <div>
                          <span className="text-xs font-bold uppercase tracking-wider text-muted block">
                            Country
                          </span>
                          <span className="text-text-main">{details.country}</span>
                        </div>
                      )}
                      {details.actors && details.actors.length > 0 && (
                        <div className="sm:col-span-2">
                          <span className="text-xs font-bold uppercase tracking-wider text-muted block">
                            Cast
                          </span>
                          <span className="text-text-main">{details.actors.join(', ')}</span>
                        </div>
                      )}
                      {details.locked_fields && details.locked_fields.length > 0 && (
                        <div className="sm:col-span-2 flex items-center gap-1.5 text-xs text-highlight">
                          <Lock className="w-3.5 h-3.5" />
                          <span>
                            {details.locked_fields.length} locked metadata field
                            {details.locked_fields.length === 1 ? '' : 's'}
                          </span>
                        </div>
                      )}
                    </div>
                  )}

                  {/* Seasons & Episodes Section */}
                  {details.episodes && details.episodes.length > 0 && (
                    <div className="space-y-4 pt-4 border-t border-border-subtle">
                      <div className="flex items-center justify-between">
                        <h3 className="text-xs font-bold tracking-wider text-muted uppercase">
                          Episodes
                        </h3>
                        <span className="text-xs text-muted">
                          {details.episodes.length} {details.episodes.length === 1 ? 'Episode' : 'Episodes'}
                        </span>
                      </div>

                      {/* Season Pill Selector Tabs */}
                      {seasons.length > 0 && (
                        <div className="flex flex-wrap items-center justify-between gap-2 pb-1">
                          <div className="flex items-center gap-2 overflow-x-auto" aria-label="Season tabs">
                            {seasons.map((seasonNum) => {
                              const isSelected = seasonNum === currentSeason;
                              return (
                                <button
                                  key={seasonNum}
                                  type="button"
                                  onClick={() => setSelectedSeason(seasonNum)}
                                  className={`px-4 py-2 rounded-xl text-xs font-bold transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                                    isSelected
                                      ? 'bg-accent text-white shadow-md shadow-accent/25'
                                      : 'bg-canvas/60 text-muted hover:text-text-main hover:bg-panel-hover border border-border-subtle'
                                  }`}
                                >
                                  Season {seasonNum}
                                </button>
                              );
                            })}
                          </div>

                          <div className="flex flex-wrap items-center gap-2">
                            {card?.media_type !== 'live_channel' && (
                              <button
                                type="button"
                                aria-pressed={allSeasonWatched}
                                disabled={seriesBatchBusy || seasonBatchBusy}
                                onClick={handleToggleSeasonWatched}
                                className={`inline-flex items-center gap-1.5 px-3 py-2 rounded-xl border text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                                  allSeasonWatched
                                    ? 'bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border-emerald-500/40'
                                    : 'bg-canvas/60 hover:bg-panel-hover border-border-subtle text-text-main'
                                }`}
                              >
                                {seasonBatchBusy ? (
                                  <Loader2
                                    aria-hidden="true"
                                    className="w-3.5 h-3.5 animate-spin"
                                  />
                                ) : (
                                  <CheckCircle2
                                    aria-hidden="true"
                                    className={`w-3.5 h-3.5 ${
                                      allSeasonWatched ? 'text-emerald-400' : 'text-muted'
                                    }`}
                                  />
                                )}
                                <span>
                                  {allSeasonWatched ? 'Season Watched' : 'Mark Season Watched'}
                                </span>
                              </button>
                            )}

                            <button
                              type="button"
                              onClick={() =>
                                setPlaylistTarget({
                                  kind: 'season',
                                  showId: itemId,
                                  seasonNumber: currentSeason,
                                  title: `${card?.title || 'Show'} — Season ${currentSeason}`,
                                })
                              }
                              className="inline-flex items-center gap-1.5 px-3 py-2 rounded-xl bg-canvas/60 hover:bg-panel-hover border border-border-subtle text-xs font-medium text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                            >
                              <ListPlus className="w-3.5 h-3.5 text-accent" />
                              <span>Add Season</span>
                            </button>
                          </div>
                        </div>
                      )}

                      {/* Episode Cards List */}
                      <div className="space-y-3">
                        {episodesForCurrentSeason.map((ep) => (
                          <EpisodeCard
                            key={ep.id}
                            episode={ep}
                            onPlay={handlePlayItem}
                            onAddToPlaylist={(episode) =>
                              setPlaylistTarget({
                                kind: 'item',
                                itemId: episode.id,
                                title: episode.title,
                              })
                            }
                            onToggleWatched={handleToggleEpisodeWatched}
                            onToggleFavorite={handleToggleEpisodeFavorite}
                            formatDuration={formatDuration}
                          />
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Technical Information Grid */}
                  <div className="space-y-3 pt-2 border-t border-border-subtle">
                    <h3 className="text-xs font-bold tracking-wider text-muted uppercase">
                      Technical Specifications
                    </h3>

                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
                      {details.technical?.resolution && (
                        <div className="p-3 rounded-xl bg-canvas/40 border border-border-subtle flex flex-col gap-1.5">
                          <span className="text-muted text-[11px] block">Resolution</span>
                          <span className="font-mono text-xs text-text-main bg-canvas/60 px-2.5 py-1 rounded-lg border border-border-subtle w-fit">
                            {details.technical.resolution}
                          </span>
                        </div>
                      )}

                      {details.technical?.video_codec && (
                        <div className="p-3 rounded-xl bg-canvas/40 border border-border-subtle flex flex-col gap-1.5">
                          <span className="text-muted text-[11px] block">Video Codec</span>
                          <span className="font-mono text-xs text-text-main bg-canvas/60 px-2.5 py-1 rounded-lg border border-border-subtle w-fit">
                            {details.technical.video_codec}
                          </span>
                        </div>
                      )}

                      {details.technical?.audio_codec && (
                        <div className="p-3 rounded-xl bg-canvas/40 border border-border-subtle flex flex-col gap-1.5">
                          <span className="text-muted text-[11px] block">Audio Codec</span>
                          <span className="font-mono text-xs text-text-main bg-canvas/60 px-2.5 py-1 rounded-lg border border-border-subtle w-fit">
                            {details.technical.audio_codec}
                            {details.technical.audio_channels
                              ? ` (${details.technical.audio_channels}ch)`
                              : ''}
                          </span>
                        </div>
                      )}

                      {details.technical?.container && (
                        <div className="p-3 rounded-xl bg-canvas/40 border border-border-subtle flex flex-col gap-1.5">
                          <span className="text-muted text-[11px] block">Container</span>
                          <span className="font-mono text-xs text-text-main bg-canvas/60 px-2.5 py-1 rounded-lg border border-border-subtle w-fit uppercase">
                            {details.technical.container}
                          </span>
                        </div>
                      )}
                    </div>

                    {audioTracks.length > 0 && (
                      <div className="p-3 rounded-xl bg-canvas/40 border border-border-subtle space-y-2">
                        <span className="text-muted text-[11px] block">Audio Streams</span>
                        <div className="flex flex-wrap gap-2">
                          {audioTracks.map((track) => (
                            <span
                              key={`${track.audio_index}-${track.stream_index}`}
                              className="font-mono text-xs text-text-main bg-canvas/60 px-2.5 py-1 rounded-lg border border-border-subtle"
                            >
                              {formatAudioStreamBadge(track)}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              ) : (
                /* Subtitles Management Tab */
                <div className="space-y-6">
                  {/* Current Tracks Section */}
                  <div className="space-y-3">
                    <h3 className="text-xs font-bold tracking-wider text-muted uppercase">
                      Current Subtitle Tracks
                    </h3>

                    {subtitles.length === 0 ? (
                      <p className="text-sm text-muted italic p-3 rounded-xl bg-canvas/50 border border-border-subtle">
                        No subtitle tracks currently installed.
                      </p>
                    ) : (
                      <div className="space-y-2">
                        {subtitles.map((track) => (
                          <div
                            key={track.id}
                            className="flex items-center justify-between p-3.5 rounded-xl bg-canvas/50 border border-border-subtle hover:bg-canvas transition-colors"
                          >
                            <div className="flex items-center gap-3">
                              <span className="font-semibold text-sm text-text-main">
                                {track.title || track.language}
                              </span>
                              <span className="px-2 py-0.5 rounded text-[11px] font-mono bg-canvas text-muted border border-border-subtle uppercase">
                                {track.format}
                              </span>
                              {track.source === 'downloaded' && (
                                <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-highlight/20 text-highlight border border-highlight/40">
                                  Downloaded
                                </span>
                              )}
                              {track.source === 'embedded' && (
                                <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-accent/20 text-accent border border-accent/40">
                                  Embedded
                                </span>
                              )}
                              {track.is_default && (
                                <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-cta/20 text-cta border border-cta/40">
                                  Default
                                </span>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  {/* Online Search Section */}
                  <div className="space-y-4 pt-4 border-t border-border-subtle">
                    <h3 className="text-xs font-bold tracking-wider text-muted uppercase flex items-center gap-2">
                      <Search className="w-4 h-4 text-accent" />
                      Search OpenSubtitles
                    </h3>

                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        placeholder="Language filter (e.g. French, Spanish, en, fr)..."
                        value={searchLang}
                        onChange={(e) => setSearchLang(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            handleSearchSubtitles();
                          }
                        }}
                        className="flex-1 px-4 py-2.5 rounded-xl bg-canvas/60 border border-border-subtle text-text-main placeholder-muted text-sm focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      />
                      <button
                        onClick={handleSearchSubtitles}
                        disabled={searching}
                        className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-white font-semibold text-sm transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                      >
                        {searching ? (
                          <>
                            <Loader2 className="w-4 h-4 animate-spin text-white" />
                            Searching...
                          </>
                        ) : (
                          'Search Online'
                        )}
                      </button>
                    </div>

                    {downloadSuccessMsg && (
                      <div className="p-3 rounded-xl bg-highlight/15 border border-highlight/40 text-highlight text-sm flex items-center gap-2">
                        <Check className="w-4 h-4 text-highlight" />
                        {downloadSuccessMsg}
                      </div>
                    )}

                    {searchError && (
                      <div className="p-3 rounded-xl bg-cta/15 border border-cta/40 text-cta text-sm">
                        {searchError}
                      </div>
                    )}

                    {/* Search Results List */}
                    {searchResults.length > 0 && (
                      <div className="space-y-2 pt-2">
                        <h4 className="text-xs font-semibold text-muted">
                          Found {searchResults.length} online match{searchResults.length === 1 ? '' : 'es'}:
                        </h4>
                        <div className="space-y-2 max-h-60 overflow-y-auto pr-1">
                          {searchResults.map((match) => (
                            <div
                              key={match.id}
                              className="flex items-center justify-between p-3.5 rounded-xl bg-canvas/50 border border-border-subtle hover:bg-canvas transition-colors"
                            >
                              <div className="space-y-1 flex-1 pr-4">
                                <div className="text-sm font-semibold text-text-main font-mono">
                                  {match.release_name || `${match.language} Subtitle`}
                                </div>
                                <div className="flex items-center gap-2 text-xs text-muted">
                                  <span>{match.language}</span>
                                  <span>•</span>
                                  <span className="uppercase">{match.format}</span>
                                  <span>•</span>
                                  <span>{match.download_count} downloads</span>
                                  {match.rating && (
                                    <>
                                      <span>•</span>
                                      <span className="text-highlight">★ {match.rating}</span>
                                    </>
                                  )}
                                </div>
                              </div>

                              <button
                                onClick={() => handleDownloadSubtitle(match)}
                                disabled={downloadingId === match.id}
                                className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-cta hover:bg-cta-hover text-white text-xs font-semibold shadow-md transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                              >
                                {downloadingId === match.id ? (
                                  <>
                                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                                    Downloading...
                                  </>
                                ) : (
                                  <>
                                    <Download className="w-3.5 h-3.5" />
                                    Download
                                  </>
                                )}
                              </button>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </>
        )}
      </div>

      <AddToPlaylistModal
        isOpen={playlistTarget !== null}
        target={playlistTarget}
        onClose={() => setPlaylistTarget(null)}
      />

      {details && (
        <>
          <MetadataEditorModal
            isOpen={isMetadataEditorOpen}
            itemId={itemId}
            details={details}
            onClose={() => setIsMetadataEditorOpen(false)}
            onSaved={handleMetadataUpdated}
          />
          <IdentifyTmdbModal
            isOpen={isIdentifyOpen}
            itemId={itemId}
            details={details}
            onClose={() => setIsIdentifyOpen(false)}
            onApplied={handleMetadataUpdated}
          />
        </>
      )}
    </div>
  );
};
export default ItemDetailsModal;
