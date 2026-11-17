import { useState, useEffect, useCallback, type FC, type FormEvent } from 'react';
import { X, Search, Film, Check, Loader2, AlertCircle } from 'lucide-react';
import type { ItemDetailsPayload, TmdbSearchMatch } from '../../types';
import { api, describeApiError } from '../../api/client';

export interface IdentifyTmdbModalProps {
  isOpen: boolean;
  itemId: number;
  details: ItemDetailsPayload;
  onClose: () => void;
  onApplied: (updated: ItemDetailsPayload) => void;
}

const inferTmdbMediaType = (mediaType?: string): 'movie' | 'tv' => {
  if (mediaType === 'show' || mediaType === 'episode' || mediaType === 'season') {
    return 'tv';
  }
  return 'movie';
};

export const IdentifyTmdbModal: FC<IdentifyTmdbModalProps> = ({
  isOpen,
  itemId,
  details,
  onClose,
  onApplied,
}) => {
  const [query, setQuery] = useState('');
  const [year, setYear] = useState('');
  const [mediaType, setMediaType] = useState<'movie' | 'tv'>('movie');
  const [results, setResults] = useState<TmdbSearchMatch[]>([]);
  const [searching, setSearching] = useState(false);
  const [applyingId, setApplyingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runSearch = useCallback(
    async (q: string, yr: string, mt: 'movie' | 'tv') => {
      setSearching(true);
      setError(null);
      try {
        const parsedYear = yr.trim() ? Number.parseInt(yr.trim(), 10) : undefined;
        const res = await api.searchItemTmdb(itemId, {
          q: q.trim() || undefined,
          year: parsedYear !== undefined && Number.isFinite(parsedYear) ? parsedYear : undefined,
          media_type: mt,
        });
        setResults(res.results || []);
      } catch (err) {
        setError(describeApiError(err));
        setResults([]);
      } finally {
        setSearching(false);
      }
    },
    [itemId]
  );

  useEffect(() => {
    if (!isOpen) return;
    const initialTitle = details.card.title || '';
    const initialYear =
      details.card.release_year !== undefined ? String(details.card.release_year) : '';
    const initialType = inferTmdbMediaType(details.card.media_type);
    setQuery(initialTitle);
    setYear(initialYear);
    setMediaType(initialType);
    setResults([]);
    setError(null);
    runSearch(initialTitle, initialYear, initialType);
  }, [isOpen, details.card.title, details.card.release_year, details.card.media_type, runSearch]);

  if (!isOpen) return null;

  const handleSubmitSearch = (e: FormEvent) => {
    e.preventDefault();
    runSearch(query, year, mediaType);
  };

  const handleApply = async (match: TmdbSearchMatch) => {
    setApplyingId(match.tmdb_id);
    setError(null);
    try {
      const updated = await api.applyItemTmdb(itemId, {
        tmdb_id: match.tmdb_id,
        media_type: match.media_type,
        download_artwork: true,
      });
      onApplied(updated);
      onClose();
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setApplyingId(null);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="identify-tmdb-title"
      className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/85 backdrop-blur-md overflow-y-auto"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-3xl max-h-[90vh] overflow-y-auto rounded-2xl bg-panel border border-border-subtle shadow-2xl p-6 text-text-main my-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between pb-4 border-b border-border-subtle">
          <div>
            <h2 id="identify-tmdb-title" className="text-xl font-bold text-text-main">
              Identify on TMDb
            </h2>
            <p className="text-xs text-muted mt-0.5">
              Search TMDb to enrich metadata and download offline artwork into local storage.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close identify modal"
            className="p-2 rounded-xl text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <form onSubmit={handleSubmitSearch} className="mt-4 flex flex-col sm:flex-row gap-2">
          <input
            type="text"
            aria-label="TMDb search query"
            placeholder="Title..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1 px-3.5 py-2.5 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
          />
          <input
            type="number"
            aria-label="Release year"
            placeholder="Year"
            value={year}
            onChange={(e) => setYear(e.target.value)}
            className="w-full sm:w-28 px-3 py-2.5 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
          />
          <select
            aria-label="Media type"
            value={mediaType}
            onChange={(e) => setMediaType(e.target.value === 'tv' ? 'tv' : 'movie')}
            className="w-full sm:w-32 px-3 py-2.5 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
          >
            <option value="movie">Movie</option>
            <option value="tv">TV Show</option>
          </select>
          <button
            type="submit"
            disabled={searching}
            className="inline-flex items-center justify-center gap-2 px-5 py-2.5 rounded-xl bg-cta hover:bg-cta-hover text-white text-sm font-semibold disabled:opacity-50 transition-colors cursor-pointer"
          >
            {searching ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Search className="w-4 h-4" />
            )}
            <span>Search</span>
          </button>
        </form>

        {error && (
          <div className="mt-4 p-3 rounded-xl bg-cta/15 border border-cta/40 text-cta text-sm flex items-center gap-2">
            <AlertCircle className="w-4 h-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        <div className="mt-5 space-y-3 max-h-[55vh] overflow-y-auto pr-1">
          {searching ? (
            <div className="py-12 flex flex-col items-center justify-center text-muted gap-2">
              <Loader2 className="w-7 h-7 animate-spin text-accent" />
              <span className="text-sm">Searching TMDb...</span>
            </div>
          ) : results.length === 0 ? (
            <div className="py-10 text-center text-sm text-muted border border-dashed border-border-subtle rounded-xl">
              No TMDb candidates found. Try adjusting the title or year.
            </div>
          ) : (
            results.map((match) => (
              <div
                key={`${match.media_type}-${match.tmdb_id}`}
                className="flex items-start gap-4 p-3.5 rounded-xl bg-canvas/60 border border-border-subtle hover:bg-canvas transition-colors"
              >
                <div className="w-16 sm:w-20 aspect-[2/3] rounded-lg overflow-hidden bg-panel border border-border-subtle shrink-0 flex items-center justify-center">
                  {match.poster_url ? (
                    <img
                      src={match.poster_url}
                      alt={match.title}
                      className="w-full h-full object-cover"
                    />
                  ) : (
                    <Film className="w-6 h-6 text-muted" />
                  )}
                </div>

                <div className="flex-1 min-w-0 space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-base font-bold text-text-main">{match.title}</h3>
                    {match.release_year && (
                      <span className="px-2 py-0.5 rounded text-xs font-mono bg-panel text-muted border border-border-subtle">
                        {match.release_year}
                      </span>
                    )}
                    {match.rating !== undefined && (
                      <span className="text-xs font-semibold text-highlight">
                        ★ {match.rating.toFixed(1)}
                      </span>
                    )}
                  </div>
                  {match.original_title && match.original_title !== match.title && (
                    <p className="text-xs text-muted italic">{match.original_title}</p>
                  )}
                  <p className="text-xs text-muted line-clamp-3">
                    {match.overview || 'No synopsis available.'}
                  </p>
                </div>

                <button
                  type="button"
                  disabled={applyingId !== null}
                  aria-label={`Apply ${match.title}`}
                  onClick={() => handleApply(match)}
                  className="self-center inline-flex items-center gap-1.5 px-4 py-2 rounded-xl bg-accent hover:bg-accent/90 text-white text-xs font-bold shadow-sm disabled:opacity-50 transition-colors cursor-pointer shrink-0"
                >
                  {applyingId === match.tmdb_id ? (
                    <>
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      <span>Applying...</span>
                    </>
                  ) : (
                    <>
                      <Check className="w-3.5 h-3.5" />
                      <span>Apply</span>
                    </>
                  )}
                </button>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
