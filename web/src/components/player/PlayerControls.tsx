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
} from 'lucide-react';
import type { SubtitleTrack } from '../../types';

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
}) => {
  const [isSubtitlesMenuOpen, setIsSubtitlesMenuOpen] = useState(false);

  const subtitleMeta = [subtitle, releaseYear ? `${releaseYear}` : null]
    .filter(Boolean)
    .join(' • ');

  const progressPercent = duration > 0 ? Math.min(100, Math.max(0, (currentTime / duration) * 100)) : 0;
  const currentVolume = isMuted ? 0 : volume;
  const volumePercent = Math.min(100, Math.max(0, currentVolume * 100));

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
                onClick={() => setIsSubtitlesMenuOpen(!isSubtitlesMenuOpen)}
                aria-label="Subtitles"
                className={`p-2 rounded-lg border transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                  activeSubtitleId !== null
                    ? 'text-accent bg-accent/15 border-accent/40'
                    : 'text-muted hover:text-accent hover:bg-panel-hover/60 border-transparent'
                }`}
              >
                <Subtitles className="w-5 h-5" />
              </button>

              {/* Subtitles Dropdown Menu */}
              {isSubtitlesMenuOpen && (
                <div className="absolute right-0 bottom-full mb-2 w-56 bg-panel border border-border-subtle rounded-xl p-2 shadow-2xl text-text-main backdrop-blur-md z-50">
                  <div className="px-3 py-1.5 text-xs font-semibold text-muted uppercase tracking-wider border-b border-border-subtle mb-1">
                    Subtitles
                  </div>
                  <button
                    onClick={() => {
                      onSelectSubtitle(null);
                      setIsSubtitlesMenuOpen(false);
                    }}
                    className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                      activeSubtitleId === null
                        ? 'bg-accent/20 text-accent font-semibold'
                        : 'text-text-main hover:bg-panel-hover hover:text-white'
                    }`}
                  >
                    <span>Off</span>
                    {activeSubtitleId === null && <Check className="w-4 h-4 text-accent" />}
                  </button>

                  {subtitles.map((track) => {
                    const isSelected = activeSubtitleId === track.id;
                    const trackLabel = track.title || track.language;
                    return (
                      <button
                        key={track.id}
                        onClick={() => {
                          onSelectSubtitle(track.id);
                          setIsSubtitlesMenuOpen(false);
                        }}
                        className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
                          isSelected
                            ? 'bg-accent/20 text-accent font-semibold'
                            : 'text-text-main hover:bg-panel-hover hover:text-white'
                        }`}
                      >
                        <span className="truncate">{trackLabel}</span>
                        {isSelected && <Check className="w-4 h-4 text-accent" />}
                      </button>
                    );
                  })}
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
