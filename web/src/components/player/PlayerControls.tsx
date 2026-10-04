import { useState, type FC } from 'react';
import {
  ArrowLeft,
  Play,
  Pause,
  Volume2,
  VolumeX,
  Subtitles,
  Maximize,
  Minimize,
  Check,
  Search,
  Download,
  Loader2,
} from 'lucide-react';
import type { SubtitleTrack, SubtitleSearchResult } from '../../types';

export type { SubtitleSearchResult };

export interface PlayerControlsProps {
  title: string;
  subtitle?: string;
  releaseYear?: number;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
  isFullscreen: boolean;
  subtitles: SubtitleTrack[];
  activeSubtitleId: number | null;
  isVisible: boolean;
  onPlayPause: () => void;
  onSeek: (seconds: number) => void;
  onVolumeChange: (volume: number) => void;
  onToggleMute: () => void;
  onToggleFullscreen: () => void;
  onSelectSubtitle: (subtitleId: number | null) => void;
  onClose: () => void;
  onSearchSubtitles?: (language: string) => Promise<SubtitleSearchResult[]>;
  onDownloadSubtitle?: (result: SubtitleSearchResult) => Promise<void>;
  recentlyDownloadedId?: number | null;
  onSubtitlesMenuToggle?: (isOpen: boolean) => void;
}

export function formatPlaybackTime(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return '00:00';
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => n.toString().padStart(2, '0');

  if (h > 0) {
    return `${pad(h)}:${pad(m)}:${pad(s)}`;
  }
  return `${pad(m)}:${pad(s)}`;
}

export const PlayerControls: FC<PlayerControlsProps> = ({
  title,
  subtitle,
  releaseYear,
  isPlaying,
  currentTime,
  duration,
  volume,
  isMuted,
  isFullscreen,
  subtitles,
  activeSubtitleId,
  isVisible,
  onPlayPause,
  onSeek,
  onVolumeChange,
  onToggleMute,
  onToggleFullscreen,
  onSelectSubtitle,
  onClose,
  onSearchSubtitles,
  onDownloadSubtitle,
  recentlyDownloadedId,
  onSubtitlesMenuToggle,
}) => {
  const [isSubtitlesMenuOpen, setIsSubtitlesMenuOpen] = useState(false);
  const [popoverView, setPopoverView] = useState<'list' | 'search'>('list');
  const [searchLang, setSearchLang] = useState('en');
  const [searchResults, setSearchResults] = useState<SubtitleSearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [hasSearched, setHasSearched] = useState(false);

  const subtitleMeta = [subtitle, releaseYear ? `${releaseYear}` : null]
    .filter(Boolean)
    .join(' • ');

  const progressPercent = duration > 0 ? Math.min(100, Math.max(0, (currentTime / duration) * 100)) : 0;
  const currentVolume = isMuted ? 0 : volume;
  const volumePercent = Math.min(100, Math.max(0, currentVolume * 100));

  const handleToggleSubtitlesMenu = () => {
    const nextState = !isSubtitlesMenuOpen;
    setIsSubtitlesMenuOpen(nextState);
    if (nextState) {
      setPopoverView('list');
      setSearchError(null);
      setDownloadError(null);
    }
    onSubtitlesMenuToggle?.(nextState);
  };

  const handleSelectSubtitle = (subtitleId: number | null) => {
    onSelectSubtitle(subtitleId);
    setIsSubtitlesMenuOpen(false);
    onSubtitlesMenuToggle?.(false);
  };

  const handleSearch = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!onSearchSubtitles) return;
    setIsSearching(true);
    setSearchError(null);
    setDownloadError(null);
    try {
      const lang = searchLang.trim() || 'en';
      const results = await onSearchSubtitles(lang);
      setSearchResults(results || []);
      setHasSearched(true);
    } catch {
      setSearchError('Failed to search online subtitles.');
    } finally {
      setIsSearching(false);
    }
  };

  const handleDownload = async (match: SubtitleSearchResult) => {
    if (!onDownloadSubtitle) return;
    setDownloadingId(match.id);
    setDownloadError(null);
    try {
      await onDownloadSubtitle(match);
      setPopoverView('list');
    } catch {
      setDownloadError('Failed to download subtitle.');
    } finally {
      setDownloadingId(null);
    }
  };

  return (
    <div
      className={`absolute inset-0 z-30 flex flex-col justify-between p-6 bg-gradient-to-t from-canvas via-canvas/80 to-transparent transition-opacity duration-300 pointer-events-auto ${
        isVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
      }`}
    >
      {/* Top Bar: Back to Browse and Title */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            aria-label="Back to Browse"
            className="group flex items-center gap-2 px-3.5 py-2 rounded-xl bg-canvas/90 hover:bg-panel text-sm font-medium text-text-main hover:text-white border border-border-subtle backdrop-blur-md transition-all cursor-pointer shadow-lg focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <ArrowLeft className="w-4 h-4 text-muted group-hover:text-accent transition-colors" />
            <span>Back to Browse</span>
          </button>
          <div className="flex flex-col">
            <h1 className="text-base sm:text-lg font-bold text-text-main tracking-tight drop-shadow-md">
              {title}
            </h1>
            {subtitleMeta && (
              <span className="text-xs text-muted drop-shadow-sm">
                {subtitleMeta}
              </span>
            )}
          </div>
        </div>
      </div>

      {/* Bottom Bar: Timeline slider & player controls */}
      <div className="flex flex-col gap-3">
        {/* Seek Progress Slider & Timestamps */}
        <div className="flex items-center gap-3 w-full">
          <span className="font-mono text-xs text-text-main min-w-12 text-right select-none drop-shadow">
            {formatPlaybackTime(currentTime)}
          </span>
          <div className="relative flex-1 flex items-center group/seek focus-within:ring-3 focus-within:ring-highlight focus-within:ring-offset-2 focus-within:ring-offset-canvas rounded-full">
            <div className="relative w-full bg-muted/40 h-1.5 group-hover/seek:h-2.5 hover:h-2.5 rounded-full transition-all cursor-pointer flex items-center">
              <div
                className="bg-accent rounded-full relative h-full flex items-center justify-end"
                style={{ width: `${progressPercent}%` }}
              >
                <div className="bg-highlight w-3.5 h-3.5 rounded-full shadow-md absolute right-0 translate-x-1/2" />
              </div>
            </div>
            <input
              type="range"
              aria-label="Seek"
              min={0}
              max={duration || 100}
              step={1}
              value={currentTime}
              onChange={(e) => onSeek(Number(e.target.value))}
              className="absolute inset-0 w-full h-full opacity-0 cursor-pointer focus:outline-none rounded-full"
            />
          </div>
          <span className="font-mono text-xs text-text-main min-w-12 select-none drop-shadow">
            {formatPlaybackTime(duration)}
          </span>
        </div>

        {/* Buttons Row */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {/* Play/Pause Button */}
            <button
              onClick={onPlayPause}
              aria-label={isPlaying ? 'Pause' : 'Play'}
              className="p-2.5 rounded-full bg-cta hover:bg-cta-hover text-white transition-all transform hover:scale-105 active:scale-95 cursor-pointer shadow-lg shadow-cta/30 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              {isPlaying ? (
                <Pause className="w-5 h-5 fill-white" />
              ) : (
                <Play className="w-5 h-5 fill-white ml-0.5" />
              )}
            </button>

            {/* Volume Controls */}
            <div className="flex items-center gap-2 group/vol">
              <button
                onClick={onToggleMute}
                aria-label={isMuted ? 'Unmute' : 'Mute'}
                className="p-2 rounded-lg text-muted hover:text-accent hover:bg-panel-hover/60 transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
              >
                {isMuted || volume === 0 ? (
                  <VolumeX className="w-5 h-5 text-accent" />
                ) : (
                  <Volume2 className="w-5 h-5" />
                )}
              </button>
              <div className="relative w-16 sm:w-24 flex items-center group/vol focus-within:ring-3 focus-within:ring-highlight focus-within:ring-offset-2 focus-within:ring-offset-canvas rounded-full">
                <div className="relative w-full bg-muted/40 h-1.5 group-hover/vol:h-2 rounded-full transition-all cursor-pointer flex items-center">
                  <div
                    className="bg-highlight rounded-full relative h-full flex items-center justify-end"
                    style={{ width: `${volumePercent}%` }}
                  >
                    <div className="bg-highlight w-3 h-3 rounded-full shadow-md absolute right-0 translate-x-1/2" />
                  </div>
                </div>
                <input
                  type="range"
                  aria-label="Volume"
                  min={0}
                  max={1}
                  step={0.05}
                  value={isMuted ? 0 : volume}
                  onChange={(e) => onVolumeChange(Number(e.target.value))}
                  className="absolute inset-0 w-full h-full opacity-0 cursor-pointer focus:outline-none rounded-full accent-highlight"
                />
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2 relative">
            {/* Subtitles Popover Toggle */}
            <div className="relative">
              <button
                onClick={handleToggleSubtitlesMenu}
                aria-label="Subtitles"
                className={`p-2 rounded-lg border transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  activeSubtitleId !== null
                    ? 'text-accent bg-accent/15 border-accent/40'
                    : 'text-muted hover:text-accent hover:bg-panel-hover/60 border-transparent'
                }`}
              >
                <Subtitles className="w-5 h-5" />
              </button>

              {/* Subtitles Multi-View Popover Menu */}
              {isSubtitlesMenuOpen && (
                <div className="absolute right-0 bottom-full mb-2 w-80 sm:w-96 max-w-[calc(100vw-3rem)] bg-panel border border-border-subtle rounded-xl p-3 shadow-2xl text-text-main backdrop-blur-md z-50">
                  {popoverView === 'list' ? (
                    /* Track List View */
                    <div>
                      <div className="flex items-center justify-between px-1 py-1 text-xs font-semibold text-muted uppercase tracking-wider border-b border-border-subtle mb-2">
                        <span>Subtitles</span>
                      </div>

                      {/* Downloaded Track Alert Prompt */}
                      {recentlyDownloadedId !== null && recentlyDownloadedId !== undefined && (
                        <div className="p-2.5 mb-2 rounded-xl bg-highlight/15 border border-highlight/40 text-highlight text-xs flex items-center gap-2">
                          <Check className="w-4 h-4 shrink-0 text-highlight" />
                          <span>Subtitle downloaded! Click below to display on screen.</span>
                        </div>
                      )}

                      {/* Off Option */}
                      <button
                        onClick={() => handleSelectSubtitle(null)}
                        className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                          activeSubtitleId === null
                            ? 'bg-accent/20 text-accent font-semibold'
                            : 'text-text-main hover:bg-panel-hover hover:text-white'
                        }`}
                      >
                        <span>Off</span>
                        {activeSubtitleId === null && <Check className="w-4 h-4 text-accent shrink-0" />}
                      </button>

                      {/* Track List */}
                      {subtitles.length > 0 && (
                        <div className="space-y-1 max-h-48 overflow-y-auto mt-1 pr-0.5">
                          {subtitles.map((track) => {
                            const isSelected = activeSubtitleId === track.id;
                            const isRecentlyDownloaded = track.id === recentlyDownloadedId;
                            const trackLabel = track.title || track.language;
                            return (
                              <button
                                key={track.id}
                                onClick={() => handleSelectSubtitle(track.id)}
                                className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                                  isSelected
                                    ? 'bg-accent/20 text-accent font-semibold'
                                    : isRecentlyDownloaded
                                    ? 'bg-highlight/10 text-highlight border border-highlight/30'
                                    : 'text-text-main hover:bg-panel-hover hover:text-white'
                                }`}
                              >
                                <div className="flex items-center gap-2 truncate">
                                  <span className="truncate">{trackLabel}</span>
                                  <span className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-canvas/80 text-muted border border-border-subtle uppercase">
                                    {track.format}
                                  </span>
                                  {track.source === 'downloaded' && (
                                    <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-highlight/20 text-highlight border border-highlight/30">
                                      Downloaded
                                    </span>
                                  )}
                                  {track.source === 'embedded' && (
                                    <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-accent/20 text-accent border border-accent/30">
                                      Embedded
                                    </span>
                                  )}
                                </div>
                                {isSelected && <Check className="w-4 h-4 text-accent shrink-0 ml-2" />}
                              </button>
                            );
                          })}
                        </div>
                      )}

                      {/* Empty State when no subtitles */}
                      {subtitles.length === 0 && (
                        <div className="py-2.5 px-2 text-center space-y-2.5">
                          <p className="text-xs text-muted">
                            No subtitle tracks found on server
                          </p>
                          <button
                            onClick={() => {
                              setPopoverView('search');
                              setSearchError(null);
                              setDownloadError(null);
                            }}
                            className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-xl bg-cta hover:bg-cta-hover text-white text-xs font-semibold shadow-md transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                          >
                            <Search className="w-3.5 h-3.5" />
                            <span>Search OpenSubtitles Online</span>
                          </button>
                        </div>
                      )}

                      {/* Bottom Search Online Button when subtitles are present */}
                      {subtitles.length > 0 && (
                        <button
                          onClick={() => {
                            setPopoverView('search');
                            setSearchError(null);
                            setDownloadError(null);
                          }}
                          className="w-full flex items-center justify-center gap-2 mt-2 pt-2.5 border-t border-border-subtle text-xs font-semibold text-accent hover:text-white hover:bg-panel-hover px-3 py-2 rounded-xl transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
                        >
                          <Search className="w-3.5 h-3.5" />
                          <span>+ Search & Download Online</span>
                        </button>
                      )}
                    </div>
                  ) : (
                    /* Search Drawer View */
                    <div>
                      {/* Back Button */}
                      <div className="flex items-center gap-2 pb-2 mb-2.5 border-b border-border-subtle">
                        <button
                          onClick={() => {
                            setPopoverView('list');
                            setSearchError(null);
                            setDownloadError(null);
                          }}
                          aria-label="Back to Subtitles"
                          className="flex items-center gap-1.5 text-xs font-medium text-muted hover:text-accent transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none px-2 py-1 rounded-lg"
                        >
                          <ArrowLeft className="w-3.5 h-3.5" />
                          <span>Back to Subtitles</span>
                        </button>
                      </div>

                      {/* Language Search Form */}
                      <form onSubmit={handleSearch} className="flex items-center gap-2 mb-2.5">
                        <input
                          type="text"
                          placeholder="Language (e.g. en, fr, es, ar)..."
                          value={searchLang}
                          onChange={(e) => setSearchLang(e.target.value)}
                          className="flex-1 px-3 py-1.5 rounded-xl bg-canvas/60 border border-border-subtle text-text-main placeholder-muted text-xs focus:outline-none focus:border-accent focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-highlight"
                        />
                        <button
                          type="submit"
                          disabled={isSearching}
                          className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cta hover:bg-cta-hover text-white font-semibold text-xs transition-colors disabled:opacity-50 cursor-pointer shadow-md focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shrink-0"
                        >
                          {isSearching ? (
                            <>
                              <Loader2 className="w-3.5 h-3.5 animate-spin" />
                              <span>Searching...</span>
                            </>
                          ) : (
                            <>
                              <Search className="w-3.5 h-3.5" />
                              <span>Search</span>
                            </>
                          )}
                        </button>
                      </form>

                      {/* Error Messages */}
                      {searchError && (
                        <div className="p-2.5 mb-2 rounded-xl bg-cta/15 border border-cta/40 text-cta text-xs">
                          {searchError}
                        </div>
                      )}
                      {downloadError && (
                        <div className="p-2.5 mb-2 rounded-xl bg-cta/15 border border-cta/40 text-cta text-xs">
                          {downloadError}
                        </div>
                      )}

                      {/* Search Results */}
                      {searchResults.length > 0 && (
                        <div className="space-y-1.5">
                          <div className="text-[11px] font-semibold text-muted px-1">
                            Found {searchResults.length} online match{searchResults.length === 1 ? '' : 'es'}:
                          </div>
                          <div className="space-y-1.5 max-h-56 overflow-y-auto pr-0.5">
                            {searchResults.map((match) => (
                              <div
                                key={match.id}
                                className="flex items-center justify-between p-2.5 rounded-xl bg-canvas/50 border border-border-subtle hover:bg-canvas transition-colors gap-2"
                              >
                                <div className="space-y-0.5 flex-1 min-w-0 pr-1">
                                  <div className="text-xs font-semibold text-text-main font-mono truncate">
                                    {match.release_name || `${match.language} Subtitle`}
                                  </div>
                                  <div className="flex items-center gap-1.5 text-[10px] text-muted">
                                    <span className="uppercase font-semibold">{match.language}</span>
                                    <span>•</span>
                                    <span className="uppercase">{match.format}</span>
                                    {match.download_count !== undefined && (
                                      <>
                                        <span>•</span>
                                        <span>{match.download_count} dl</span>
                                      </>
                                    )}
                                    {match.rating !== undefined && (
                                      <>
                                        <span>•</span>
                                        <span className="text-highlight">★ {match.rating}</span>
                                      </>
                                    )}
                                  </div>
                                </div>
                                <button
                                  onClick={() => handleDownload(match)}
                                  disabled={downloadingId !== null}
                                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cta hover:bg-cta-hover text-white text-xs font-semibold shadow-md transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shrink-0"
                                >
                                  {downloadingId === match.id ? (
                                    <>
                                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                                      <span>Downloading...</span>
                                    </>
                                  ) : (
                                    <>
                                      <Download className="w-3.5 h-3.5" />
                                      <span>Download</span>
                                    </>
                                  )}
                                </button>
                              </div>
                            ))}
                          </div>
                        </div>
                      )}

                      {/* Zero Results Empty Notice */}
                      {hasSearched && searchResults.length === 0 && !searchError && (
                        <div className="py-4 text-center">
                          <p className="text-xs text-muted">
                            No subtitles found for this language. Try another language code.
                          </p>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* Fullscreen Toggle */}
            <button
              onClick={onToggleFullscreen}
              aria-label={isFullscreen ? 'Exit Fullscreen' : 'Fullscreen'}
              className={`p-2 rounded-lg transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                isFullscreen
                  ? 'text-accent hover:bg-panel-hover/60'
                  : 'text-muted hover:text-accent hover:bg-panel-hover/60'
              }`}
            >
              {isFullscreen ? (
                <Minimize className="w-5 h-5" />
              ) : (
                <Maximize className="w-5 h-5" />
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
