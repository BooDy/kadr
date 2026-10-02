import { useState, useEffect, type FC } from 'react';
import {
  X,
  Play,
  Film,
  Download,
  Search,
  Check,
  Loader2,
} from 'lucide-react';
import type {
  ItemDetailsPayload,
  OnlineSubtitleMatch,
  SubtitleTrack,
} from '../../types';
import { api } from '../../api/client';

export interface ItemDetailsModalProps {
  itemId: number | null;
  isOpen: boolean;
  onClose: () => void;
  onPlay: (itemId: number) => void;
}

export const ItemDetailsModal: FC<ItemDetailsModalProps> = ({
  itemId,
  isOpen,
  onClose,
  onPlay,
}) => {
  const [details, setDetails] = useState<ItemDetailsPayload | null>(null);
  const [subtitles, setSubtitles] = useState<SubtitleTrack[]>([]);
  const [loading, setLoading] = useState(false);
  const [activeTab, setActiveTab] = useState<'details' | 'subtitles'>('details');

  // Subtitle search state
  const [searchLang, setSearchLang] = useState('');
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<OnlineSubtitleMatch[]>([]);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadSuccessMsg, setDownloadSuccessMsg] = useState<string | null>(null);

  // Artwork error state
  const [backdropFailed, setBackdropFailed] = useState(false);
  const [posterFailed, setPosterFailed] = useState(false);

  useEffect(() => {
    if (!isOpen || itemId === null) {
      setDetails(null);
      setSubtitles([]);
      setSearchResults([]);
      setSearchError(null);
      setDownloadSuccessMsg(null);
      setActiveTab('details');
      setBackdropFailed(false);
      setPosterFailed(false);
      return;
    }

    let isMounted = true;
    setLoading(true);

    Promise.all([api.getItemDetails(itemId), api.getSubtitles(itemId)])
      .then(([itemDetails, tracks]) => {
        if (isMounted) {
          setDetails(itemDetails);
          setSubtitles(tracks || []);
          setLoading(false);
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
  }, [isOpen, itemId]);

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen || itemId === null) return null;

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

  const card = details?.card;
  const duration =
    details?.duration_seconds || details?.technical?.duration_seconds;
  const backdropUrl =
    card?.backdrop_url || api.getArtworkUrl(itemId, 'backdrop');
  const posterUrl = card?.poster_url || api.getArtworkUrl(itemId, 'poster');

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="item-modal-title"
      className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-6 bg-black/85 backdrop-blur-md overflow-y-auto"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-4xl max-h-[90vh] overflow-y-auto rounded-3xl bg-zinc-950 border border-zinc-800 shadow-2xl flex flex-col my-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Close Button */}
        <button
          onClick={onClose}
          aria-label="Close modal"
          className="absolute top-4 right-4 z-30 p-2.5 rounded-full bg-zinc-950/80 hover:bg-zinc-800 text-zinc-400 hover:text-white border border-zinc-800/80 transition-colors cursor-pointer"
        >
          <X className="w-5 h-5" />
        </button>

        {loading ? (
          <div className="flex flex-col items-center justify-center min-h-[380px] p-12 text-zinc-400">
            <Loader2 className="w-10 h-10 animate-spin text-rose-500 mb-4" />
            <p className="text-sm">Loading media information...</p>
          </div>
        ) : !details ? (
          <div className="flex flex-col items-center justify-center min-h-[300px] p-8 text-zinc-400">
            <Film className="w-12 h-12 text-zinc-600 mb-3" />
            <p className="text-base text-zinc-300">Media information unavailable</p>
          </div>
        ) : (
          <>
            {/* Header Hero Area with Backdrop */}
            <div className="relative min-h-[220px] sm:min-h-[280px] w-full overflow-hidden bg-zinc-900 border-b border-zinc-800/80 flex items-end">
              {!backdropFailed && backdropUrl ? (
                <img
                  src={backdropUrl}
                  alt={card?.title}
                  onError={() => setBackdropFailed(true)}
                  className="absolute inset-0 w-full h-full object-cover filter brightness-[0.7]"
                />
              ) : (
                <div className="absolute inset-0 bg-gradient-to-tr from-zinc-950 via-zinc-900 to-rose-950/20" />
              )}

              {/* Gradient Vignette Overlays */}
              <div className="absolute inset-0 bg-gradient-to-t from-zinc-950 via-zinc-950/70 to-transparent" />
              <div className="absolute inset-0 bg-gradient-to-r from-zinc-950/80 to-transparent" />

              {/* Poster and Title Info */}
              <div className="relative z-10 flex flex-col sm:flex-row items-center sm:items-end gap-5 p-6 sm:p-8 w-full">
                {/* Poster Artwork with Placeholder */}
                <div className="flex-shrink-0 w-28 sm:w-36 aspect-[2/3] rounded-xl overflow-hidden bg-zinc-900 border border-zinc-700/80 shadow-xl hidden sm:block">
                  {!posterFailed && posterUrl ? (
                    <img
                      src={posterUrl}
                      alt={card?.title}
                      onError={() => setPosterFailed(true)}
                      className="w-full h-full object-cover"
                    />
                  ) : (
                    <div className="w-full h-full bg-zinc-800 flex flex-col items-center justify-center p-2 text-center">
                      <Film className="w-8 h-8 text-zinc-500 mb-1" />
                      <span className="text-[10px] text-zinc-400 font-medium">
                        {card?.title}
                      </span>
                    </div>
                  )}
                </div>

                {/* Details Header Text */}
                <div className="space-y-2 flex-1 text-center sm:text-left">
                  <div className="flex flex-wrap items-center justify-center sm:justify-start gap-2 text-xs font-semibold">
                    {card?.release_year && (
                      <span className="px-2 py-0.5 rounded bg-zinc-800 text-zinc-300 border border-zinc-700/60">
                        {card.release_year}
                      </span>
                    )}
                    {duration && (
                      <span className="px-2 py-0.5 rounded bg-zinc-800 text-zinc-300 border border-zinc-700/60">
                        {formatDuration(duration)}
                      </span>
                    )}
                    {details.technical?.resolution && (
                      <span className="px-2 py-0.5 rounded bg-rose-600/90 text-white font-bold">
                        {details.technical.resolution}
                      </span>
                    )}
                    {card?.badge && (
                      <span className="px-2 py-0.5 rounded bg-amber-500/20 text-amber-400 border border-amber-500/30">
                        {card.badge}
                      </span>
                    )}
                  </div>

                  <h1
                    id="item-modal-title"
                    className="text-2xl sm:text-3xl lg:text-4xl font-extrabold text-white tracking-tight"
                  >
                    {card?.title}
                  </h1>

                  {card?.subtitle && (
                    <p className="text-xs sm:text-sm text-zinc-400 italic">
                      {card.subtitle}
                    </p>
                  )}
                </div>

                {/* Quick Play Action Button */}
                <div className="self-center sm:self-end">
                  <button
                    onClick={() => {
                      onPlay(itemId);
                      onClose();
                    }}
                    className="flex items-center gap-2 px-6 py-3 rounded-xl bg-rose-600 hover:bg-rose-500 text-white font-bold text-sm shadow-lg shadow-rose-950/50 hover:scale-105 active:scale-95 transition-all cursor-pointer"
                  >
                    <Play className="w-4 h-4 fill-white text-white" />
                    Play
                  </button>
                </div>
              </div>
            </div>

            {/* Navigation Tabs */}
            <div className="flex items-center border-b border-zinc-800 px-6 sm:px-8 bg-zinc-950">
              <button
                onClick={() => setActiveTab('details')}
                className={`py-3.5 px-4 text-sm font-semibold border-b-2 transition-colors cursor-pointer ${
                  activeTab === 'details'
                    ? 'border-rose-500 text-rose-400'
                    : 'border-transparent text-zinc-400 hover:text-zinc-200'
                }`}
              >
                Overview
              </button>
              <button
                onClick={() => setActiveTab('subtitles')}
                className={`py-3.5 px-4 text-sm font-semibold border-b-2 transition-colors cursor-pointer ${
                  activeTab === 'subtitles'
                    ? 'border-rose-500 text-rose-400'
                    : 'border-transparent text-zinc-400 hover:text-zinc-200'
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
                    <h3 className="text-xs font-bold tracking-wider text-zinc-400 uppercase">
                      Synopsis
                    </h3>
                    <p className="text-sm sm:text-base text-zinc-300 leading-relaxed">
                      {details.overview || card?.subtitle || 'No overview available for this title.'}
                    </p>
                  </div>

                  {/* Genres */}
                  {details.genres && details.genres.length > 0 && (
                    <div className="space-y-2">
                      <h3 className="text-xs font-bold tracking-wider text-zinc-400 uppercase">
                        Genres
                      </h3>
                      <div className="flex flex-wrap gap-2">
                        {details.genres.map((genre) => (
                          <span
                            key={genre}
                            className="px-3 py-1 rounded-lg bg-zinc-900 border border-zinc-800 text-xs font-medium text-zinc-300"
                          >
                            {genre}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Technical Information Grid */}
                  <div className="space-y-3 pt-2 border-t border-zinc-800/80">
                    <h3 className="text-xs font-bold tracking-wider text-zinc-400 uppercase">
                      Technical Specifications
                    </h3>

                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
                      {details.technical?.resolution && (
                        <div className="p-3 rounded-xl bg-zinc-900/80 border border-zinc-800">
                          <span className="text-zinc-500 block mb-1">Resolution</span>
                          <span className="font-semibold text-zinc-200">
                            {details.technical.resolution}
                          </span>
                        </div>
                      )}

                      {details.technical?.video_codec && (
                        <div className="p-3 rounded-xl bg-zinc-900/80 border border-zinc-800">
                          <span className="text-zinc-500 block mb-1">Video Codec</span>
                          <span className="font-semibold text-zinc-200">
                            {details.technical.video_codec}
                          </span>
                        </div>
                      )}

                      {details.technical?.audio_codec && (
                        <div className="p-3 rounded-xl bg-zinc-900/80 border border-zinc-800">
                          <span className="text-zinc-500 block mb-1">Audio Codec</span>
                          <span className="font-semibold text-zinc-200">
                            {details.technical.audio_codec}
                            {details.technical.audio_channels
                              ? ` (${details.technical.audio_channels}ch)`
                              : ''}
                          </span>
                        </div>
                      )}

                      {details.technical?.container && (
                        <div className="p-3 rounded-xl bg-zinc-900/80 border border-zinc-800">
                          <span className="text-zinc-500 block mb-1">Container</span>
                          <span className="font-semibold text-zinc-200 uppercase">
                            {details.technical.container}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              ) : (
                /* Subtitles Management Tab */
                <div className="space-y-6">
                  {/* Current Tracks Section */}
                  <div className="space-y-3">
                    <h3 className="text-xs font-bold tracking-wider text-zinc-400 uppercase">
                      Current Subtitle Tracks
                    </h3>

                    {subtitles.length === 0 ? (
                      <p className="text-sm text-zinc-500 italic p-3 rounded-xl bg-zinc-900/40 border border-zinc-850">
                        No subtitle tracks currently installed.
                      </p>
                    ) : (
                      <div className="space-y-2">
                        {subtitles.map((track) => (
                          <div
                            key={track.id}
                            className="flex items-center justify-between p-3.5 rounded-xl bg-zinc-900/90 border border-zinc-800 hover:border-zinc-700/80 transition-colors"
                          >
                            <div className="flex items-center gap-3">
                              <span className="font-semibold text-sm text-zinc-200">
                                {track.title || track.language}
                              </span>
                              <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-zinc-800 text-zinc-400 border border-zinc-700/50 uppercase">
                                {track.format}
                              </span>
                              {track.source === 'downloaded' && (
                                <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">
                                  Downloaded
                                </span>
                              )}
                              {track.source === 'embedded' && (
                                <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-indigo-500/20 text-indigo-400 border border-indigo-500/30">
                                  Embedded
                                </span>
                              )}
                              {track.is_default && (
                                <span className="px-2 py-0.5 rounded text-[11px] font-medium bg-rose-500/20 text-rose-400 border border-rose-500/30">
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
                  <div className="space-y-4 pt-4 border-t border-zinc-800">
                    <h3 className="text-xs font-bold tracking-wider text-zinc-400 uppercase flex items-center gap-2">
                      <Search className="w-4 h-4 text-rose-500" />
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
                        className="flex-1 px-4 py-2.5 rounded-xl bg-zinc-900 border border-zinc-800 text-zinc-200 placeholder-zinc-500 text-sm focus:outline-none focus:border-rose-500/70"
                      />
                      <button
                        onClick={handleSearchSubtitles}
                        disabled={searching}
                        className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-rose-600 hover:bg-rose-500 text-white font-semibold text-sm transition-colors disabled:opacity-50 cursor-pointer"
                      >
                        {searching ? (
                          <>
                            <Loader2 className="w-4 h-4 animate-spin" />
                            Searching...
                          </>
                        ) : (
                          'Search Online'
                        )}
                      </button>
                    </div>

                    {downloadSuccessMsg && (
                      <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 text-sm flex items-center gap-2">
                        <Check className="w-4 h-4" />
                        {downloadSuccessMsg}
                      </div>
                    )}

                    {searchError && (
                      <div className="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-400 text-sm">
                        {searchError}
                      </div>
                    )}

                    {/* Search Results List */}
                    {searchResults.length > 0 && (
                      <div className="space-y-2 pt-2">
                        <h4 className="text-xs font-semibold text-zinc-400">
                          Found {searchResults.length} online match{searchResults.length === 1 ? '' : 'es'}:
                        </h4>
                        <div className="space-y-2 max-h-60 overflow-y-auto pr-1">
                          {searchResults.map((match) => (
                            <div
                              key={match.id}
                              className="flex items-center justify-between p-3.5 rounded-xl bg-zinc-900 border border-zinc-800 hover:border-zinc-700/80 transition-colors"
                            >
                              <div className="space-y-1 flex-1 pr-4">
                                <div className="text-sm font-semibold text-zinc-200">
                                  {match.release_name || `${match.language} Subtitle`}
                                </div>
                                <div className="flex items-center gap-2 text-xs text-zinc-400">
                                  <span>{match.language}</span>
                                  <span>•</span>
                                  <span className="uppercase">{match.format}</span>
                                  <span>•</span>
                                  <span>{match.download_count} downloads</span>
                                  {match.rating && (
                                    <>
                                      <span>•</span>
                                      <span>★ {match.rating}</span>
                                    </>
                                  )}
                                </div>
                              </div>

                              <button
                                onClick={() => handleDownloadSubtitle(match)}
                                disabled={downloadingId === match.id}
                                className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-zinc-800 hover:bg-rose-600 text-zinc-200 hover:text-white text-xs font-semibold border border-zinc-700/60 transition-colors disabled:opacity-50 cursor-pointer"
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
    </div>
  );
};
export default ItemDetailsModal;
