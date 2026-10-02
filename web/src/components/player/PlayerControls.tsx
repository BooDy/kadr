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
  if (isNaN(seconds) || seconds < 0) return '00:00';
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

  return (
    <div
      className={`absolute inset-0 z-30 flex flex-col justify-between p-4 sm:p-6 bg-gradient-to-t from-black/90 via-black/20 to-black/80 transition-opacity duration-300 pointer-events-auto ${
        isVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
      }`}
    >
      {/* Top Bar: Back to Browse and Title */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            onClick={onClose}
            aria-label="Back to Browse"
            className="flex items-center gap-2 px-3.5 py-2 rounded-xl bg-zinc-900/80 hover:bg-zinc-800 text-sm font-medium text-zinc-200 hover:text-white border border-zinc-700/60 backdrop-blur-md transition-all cursor-pointer shadow-lg"
          >
            <ArrowLeft className="w-4 h-4 text-zinc-400 group-hover:text-white" />
            <span>Back to Browse</span>
          </button>
          <div className="flex flex-col">
            <h1 className="text-base sm:text-lg font-bold text-white tracking-tight drop-shadow-md">
              {title}
            </h1>
            {subtitleMeta && (
              <span className="text-xs text-zinc-400 drop-shadow-sm">
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
          <span className="text-xs font-mono text-zinc-300 min-w-12 text-right select-none drop-shadow">
            {formatPlaybackTime(currentTime)}
          </span>
          <div className="relative flex-1 flex items-center">
            <input
              type="range"
              aria-label="Seek"
              min={0}
              max={duration || 100}
              step={1}
              value={currentTime}
              onChange={(e) => onSeek(Number(e.target.value))}
              className="w-full h-1.5 bg-zinc-700/80 hover:h-2 rounded-lg appearance-none cursor-pointer accent-rose-500 transition-all focus:outline-none"
            />
          </div>
          <span className="text-xs font-mono text-zinc-400 min-w-12 select-none drop-shadow">
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
              className="p-2.5 rounded-full bg-rose-600 hover:bg-rose-500 text-white transition-all transform hover:scale-105 active:scale-95 cursor-pointer shadow-lg shadow-rose-950/40"
            >
              {isPlaying ? (
                <Pause className="w-5 h-5 fill-white" />
              ) : (
                <Play className="w-5 h-5 fill-white ml-0.5" />
              )}
            </button>

            {/* Volume Controls */}
            <div className="flex items-center gap-2 group">
              <button
                onClick={onToggleMute}
                aria-label={isMuted ? 'Unmute' : 'Mute'}
                className="p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors cursor-pointer"
              >
                {isMuted || volume === 0 ? (
                  <VolumeX className="w-5 h-5 text-rose-400" />
                ) : (
                  <Volume2 className="w-5 h-5" />
                )}
              </button>
              <input
                type="range"
                aria-label="Volume"
                min={0}
                max={1}
                step={0.05}
                value={isMuted ? 0 : volume}
                onChange={(e) => onVolumeChange(Number(e.target.value))}
                className="w-16 sm:w-24 h-1 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-rose-500 transition-all"
              />
            </div>
          </div>

          <div className="flex items-center gap-2 relative">
            {/* Subtitles Popover Toggle */}
            <div className="relative">
              <button
                onClick={() => setIsSubtitlesMenuOpen(!isSubtitlesMenuOpen)}
                aria-label="Subtitles"
                className={`p-2 rounded-lg border transition-colors cursor-pointer ${
                  activeSubtitleId !== null
                    ? 'text-rose-400 bg-rose-500/10 border-rose-500/30'
                    : 'text-zinc-300 hover:text-white hover:bg-zinc-800/60 border-transparent'
                }`}
              >
                <Subtitles className="w-5 h-5" />
              </button>

              {/* Subtitles Dropdown Menu */}
              {isSubtitlesMenuOpen && (
                <div className="absolute right-0 bottom-full mb-2 w-56 rounded-2xl border border-zinc-800 bg-zinc-950/95 backdrop-blur-md p-2 shadow-2xl z-50">
                  <div className="px-3 py-1.5 text-xs font-semibold text-zinc-400 uppercase tracking-wider border-b border-zinc-800/80 mb-1">
                    Subtitles
                  </div>
                  <button
                    onClick={() => {
                      onSelectSubtitle(null);
                      setIsSubtitlesMenuOpen(false);
                    }}
                    className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-colors cursor-pointer ${
                      activeSubtitleId === null
                        ? 'bg-rose-500/20 text-rose-300 font-semibold'
                        : 'text-zinc-300 hover:bg-zinc-900 hover:text-white'
                    }`}
                  >
                    <span>Off</span>
                    {activeSubtitleId === null && <Check className="w-4 h-4 text-rose-400" />}
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
                        className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-colors cursor-pointer ${
                          isSelected
                            ? 'bg-rose-500/20 text-rose-300 font-semibold'
                            : 'text-zinc-300 hover:bg-zinc-900 hover:text-white'
                        }`}
                      >
                        <span className="truncate">{trackLabel}</span>
                        {isSelected && <Check className="w-4 h-4 text-rose-400" />}
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
              className="p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors cursor-pointer"
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
