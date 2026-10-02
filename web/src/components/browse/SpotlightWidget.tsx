import { useState, type FC } from 'react';
import { Play, Info, Clapperboard, Sparkles } from 'lucide-react';
import type { CardViewModel } from '../../types';
import { api } from '../../api/client';

export interface SpotlightWidgetProps {
  item: CardViewModel;
  onPlay: (itemId: number) => void;
  onMoreInfo: (itemId: number) => void;
}

export const SpotlightWidget: FC<SpotlightWidgetProps> = ({
  item,
  onPlay,
  onMoreInfo,
}) => {
  const [imageFailed, setImageFailed] = useState(false);

  const backdropUrl =
    item.backdrop_url || item.poster_url || api.getArtworkUrl(item.id, 'backdrop');

  // Determine badges
  const isResume =
    typeof item.playback_progress === 'number' && item.playback_progress > 0;
  const isNewRelease =
    !isResume && item.release_year && item.release_year >= 2024;
  const qualityBadge = item.badge || '4K ULTRA HD';

  return (
    <section
      aria-label={`Spotlight: ${item.title}`}
      className="relative w-full rounded-3xl overflow-hidden min-h-[460px] sm:min-h-[520px] flex items-end border border-zinc-800/80 shadow-2xl bg-zinc-950"
    >
      {/* Background Artwork or Fallback Cinema Gradient */}
      <div className="absolute inset-0 z-0">
        {!imageFailed && backdropUrl ? (
          <img
            src={backdropUrl}
            alt={item.title}
            onError={() => setImageFailed(true)}
            className="w-full h-full object-cover object-center filter brightness-[0.85]"
          />
        ) : (
          <div className="w-full h-full bg-gradient-to-tr from-zinc-950 via-zinc-900 to-rose-950/20 flex items-center justify-end pr-16 opacity-40">
            <Clapperboard className="w-96 h-96 text-zinc-800/40 select-none stroke-[1]" />
          </div>
        )}

        {/* Cinematic Vignette & Gradient Overlays */}
        <div className="absolute inset-0 bg-gradient-to-t from-zinc-950 via-zinc-950/80 to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-r from-zinc-950 via-zinc-950/60 to-transparent w-full md:w-3/4" />
      </div>

      {/* Hero Content Details */}
      <div className="relative z-10 w-full p-6 sm:p-10 lg:p-14 max-w-3xl space-y-4">
        {/* Badges Bar */}
        <div className="flex flex-wrap items-center gap-2 sm:gap-2.5 text-xs font-semibold">
          {/* Status Badge (RESUME or NEW) */}
          {isResume && (
            <span className="inline-flex items-center gap-1 px-2.5 py-1 rounded-md bg-amber-500/20 text-amber-400 border border-amber-500/30 uppercase tracking-wider text-[11px] font-bold">
              RESUME
            </span>
          )}
          {isNewRelease && (
            <span className="inline-flex items-center gap-1 px-2.5 py-1 rounded-md bg-rose-500/20 text-rose-400 border border-rose-500/30 uppercase tracking-wider text-[11px] font-bold">
              <Sparkles className="w-3 h-3 text-rose-400" />
              NEW
            </span>
          )}

          {/* Quality Badge */}
          <span className="px-2.5 py-1 rounded-md bg-zinc-800/90 text-zinc-200 border border-zinc-700/60 tracking-wider text-[11px] font-bold">
            {qualityBadge}
          </span>

          {/* Release Year */}
          {item.release_year && (
            <span className="px-2.5 py-1 rounded-md bg-zinc-900/80 text-zinc-300 border border-zinc-800 tracking-wider text-[11px]">
              {item.release_year}
            </span>
          )}

          {/* Media Type */}
          <span className="px-2.5 py-1 rounded-md bg-zinc-900/80 text-zinc-400 border border-zinc-800 uppercase tracking-wider text-[10px]">
            {item.media_type}
          </span>
        </div>

        {/* Title */}
        <h1 className="text-3xl sm:text-4xl lg:text-5xl font-extrabold tracking-tight text-white drop-shadow-md">
          {item.title}
        </h1>

        {/* Synopsis / Subtitle */}
        {item.subtitle && (
          <p className="text-sm sm:text-base text-zinc-300 line-clamp-3 leading-relaxed max-w-2xl drop-shadow">
            {item.subtitle}
          </p>
        )}

        {/* Call to Actions */}
        <div className="flex flex-wrap items-center gap-3 pt-3">
          <button
            onClick={() => onPlay(item.id)}
            className="flex items-center gap-2.5 px-6 py-3 rounded-xl bg-rose-600 hover:bg-rose-500 text-white font-semibold text-sm shadow-lg shadow-rose-950/50 hover:scale-[1.02] active:scale-[0.98] transition-all cursor-pointer"
          >
            <Play className="h-4 w-4 fill-white text-white" />
            Play Now
          </button>

          <button
            onClick={() => onMoreInfo(item.id)}
            className="flex items-center gap-2 px-5 py-3 rounded-xl bg-zinc-900/90 hover:bg-zinc-800 text-zinc-200 hover:text-white font-semibold text-sm border border-zinc-700/70 backdrop-blur-md hover:scale-[1.02] active:scale-[0.98] transition-all cursor-pointer"
          >
            <Info className="h-4 w-4 text-zinc-400 group-hover:text-white" />
            More Info
          </button>
        </div>
      </div>
    </section>
  );
};
export default SpotlightWidget;
