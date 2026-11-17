import { useState, useEffect, type FC, type FormEvent } from 'react';
import { X, Lock, RotateCcw, Save, Loader2, AlertCircle } from 'lucide-react';
import type { ItemDetailsPayload, UpdateItemMetadataPayload } from '../../types';
import { api, describeApiError } from '../../api/client';

export interface MetadataEditorModalProps {
  isOpen: boolean;
  itemId: number;
  details: ItemDetailsPayload;
  onClose: () => void;
  onSaved: (updated: ItemDetailsPayload) => void;
}

const parseCsvList = (value: string): string[] =>
  value
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);

const sameList = (a: string[], b: string[]): boolean =>
  a.length === b.length && a.every((v, idx) => v === b[idx]);

export const MetadataEditorModal: FC<MetadataEditorModalProps> = ({
  isOpen,
  itemId,
  details,
  onClose,
  onSaved,
}) => {
  const [title, setTitle] = useState('');
  const [originalTitle, setOriginalTitle] = useState('');
  const [releaseYear, setReleaseYear] = useState('');
  const [rating, setRating] = useState('');
  const [genres, setGenres] = useState('');
  const [director, setDirector] = useState('');
  const [cast, setCast] = useState('');
  const [studio, setStudio] = useState('');
  const [country, setCountry] = useState('');
  const [overview, setOverview] = useState('');
  const [posterUrl, setPosterUrl] = useState('');
  const [backdropUrl, setBackdropUrl] = useState('');

  const [saving, setSaving] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setTitle(details.card.title || '');
    setOriginalTitle(details.original_title || '');
    setReleaseYear(
      details.card.release_year !== undefined ? String(details.card.release_year) : ''
    );
    setRating(details.card.rating !== undefined ? String(details.card.rating) : '');
    setGenres((details.genres || []).join(', '));
    setDirector(details.director || '');
    setCast((details.actors || []).join(', '));
    setStudio(details.studio || '');
    setCountry(details.country || '');
    setOverview(details.overview || '');
    setPosterUrl('');
    setBackdropUrl('');
    setError(null);
  }, [isOpen, details]);

  if (!isOpen) return null;

  const lockedSet = new Set(details.locked_fields || []);
  const isLocked = (field: string) => lockedSet.has(field);

  const renderLockBadge = (field: string) =>
    isLocked(field) ? (
      <span
        data-testid={`locked-badge-${field}`}
        className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-highlight/15 text-highlight border border-highlight/30"
      >
        <Lock className="w-2.5 h-2.5" />
        Locked
      </span>
    ) : null;

  const handleResetLocks = async () => {
    setResetting(true);
    setError(null);
    try {
      const updated = await api.updateItemMetadata(itemId, { reset_locks: true });
      onSaved(updated);
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setResetting(false);
    }
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setError('Title cannot be empty.');
      return;
    }

    const payload: UpdateItemMetadataPayload = {};

    if (trimmedTitle !== (details.card.title || '')) {
      payload.title = trimmedTitle;
    }

    const origTrimmed = originalTitle.trim();
    if (origTrimmed !== (details.original_title || '')) {
      payload.original_title = origTrimmed ? origTrimmed : null;
    }

    const yearTrimmed = releaseYear.trim();
    const origYearStr =
      details.card.release_year !== undefined ? String(details.card.release_year) : '';
    if (yearTrimmed !== origYearStr) {
      if (!yearTrimmed) {
        payload.release_year = null;
      } else {
        const parsedYear = Number.parseInt(yearTrimmed, 10);
        if (!Number.isFinite(parsedYear) || parsedYear < 1800 || parsedYear > 2200) {
          setError('Release year must be between 1800 and 2200.');
          return;
        }
        payload.release_year = parsedYear;
      }
    }

    const ratingTrimmed = rating.trim();
    const origRatingStr =
      details.card.rating !== undefined ? String(details.card.rating) : '';
    if (ratingTrimmed !== origRatingStr) {
      if (!ratingTrimmed) {
        payload.rating = null;
      } else {
        const parsedRating = Number.parseFloat(ratingTrimmed);
        if (!Number.isFinite(parsedRating) || parsedRating < 0 || parsedRating > 10) {
          setError('Rating must be between 0 and 10.');
          return;
        }
        payload.rating = parsedRating;
      }
    }

    const nextGenres = parseCsvList(genres);
    if (!sameList(nextGenres, details.genres || [])) {
      payload.genres = nextGenres;
    }

    const dirTrimmed = director.trim();
    if (dirTrimmed !== (details.director || '')) {
      payload.director = dirTrimmed ? dirTrimmed : null;
    }

    const nextActors = parseCsvList(cast);
    if (!sameList(nextActors, details.actors || [])) {
      payload.actors = nextActors;
    }

    const studioTrimmed = studio.trim();
    if (studioTrimmed !== (details.studio || '')) {
      payload.studio = studioTrimmed ? studioTrimmed : null;
    }

    const countryTrimmed = country.trim();
    if (countryTrimmed !== (details.country || '')) {
      payload.country = countryTrimmed ? countryTrimmed : null;
    }

    const overviewTrimmed = overview.trim();
    if (overviewTrimmed !== (details.overview || '')) {
      payload.overview = overviewTrimmed ? overviewTrimmed : null;
    }

    if (posterUrl.trim()) {
      payload.poster_url = posterUrl.trim();
    }
    if (backdropUrl.trim()) {
      payload.backdrop_url = backdropUrl.trim();
    }

    setSaving(true);
    setError(null);
    try {
      const updated = await api.updateItemMetadata(itemId, payload);
      onSaved(updated);
      onClose();
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="metadata-editor-title"
      className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/85 backdrop-blur-md overflow-y-auto"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-2xl max-h-[90vh] overflow-y-auto rounded-2xl bg-panel border border-border-subtle shadow-2xl p-6 text-text-main my-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between pb-4 border-b border-border-subtle">
          <div>
            <h2 id="metadata-editor-title" className="text-xl font-bold text-text-main">
              Edit Metadata
            </h2>
            <p className="text-xs text-muted mt-0.5">
              Edited fields are locked in SQLite so library rescans never overwrite them.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close metadata editor"
            className="p-2 rounded-xl text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Lock status & Reset Locks bar */}
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3 p-3 rounded-xl bg-canvas/60 border border-border-subtle">
          <div className="flex items-center gap-2 text-xs text-muted">
            <Lock className="w-4 h-4 text-highlight shrink-0" />
            <span>
              {lockedSet.size === 0
                ? 'No fields are currently locked.'
                : `${lockedSet.size} field${lockedSet.size === 1 ? '' : 's'} locked against rescan overwrites.`}
            </span>
          </div>
          <button
            type="button"
            disabled={lockedSet.size === 0 || resetting || saving}
            onClick={handleResetLocks}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-panel hover:bg-panel-hover text-text-main border border-border-subtle disabled:opacity-40 disabled:cursor-not-allowed transition-colors cursor-pointer"
          >
            {resetting ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <RotateCcw className="w-3.5 h-3.5 text-highlight" />
            )}
            <span>Reset Locks</span>
          </button>
        </div>

        {error && (
          <div className="mt-4 p-3 rounded-xl bg-cta/15 border border-cta/40 text-cta text-sm flex items-center gap-2">
            <AlertCircle className="w-4 h-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        <form onSubmit={handleSubmit} className="mt-5 space-y-4">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <div className="flex items-center justify-between mb-1">
                <label htmlFor="meta-title" className="text-xs font-semibold text-muted uppercase">
                  Title
                </label>
                {renderLockBadge('title')}
              </div>
              <input
                id="meta-title"
                type="text"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                required
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label
                  htmlFor="meta-original-title"
                  className="text-xs font-semibold text-muted uppercase"
                >
                  Original Title
                </label>
                {renderLockBadge('original_title')}
              </div>
              <input
                id="meta-original-title"
                type="text"
                value={originalTitle}
                onChange={(e) => setOriginalTitle(e.target.value)}
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label htmlFor="meta-year" className="text-xs font-semibold text-muted uppercase">
                  Year
                </label>
                {renderLockBadge('release_year')}
              </div>
              <input
                id="meta-year"
                type="number"
                value={releaseYear}
                onChange={(e) => setReleaseYear(e.target.value)}
                placeholder="e.g. 2024"
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label htmlFor="meta-rating" className="text-xs font-semibold text-muted uppercase">
                  Rating
                </label>
                {renderLockBadge('rating')}
              </div>
              <input
                id="meta-rating"
                type="number"
                step="0.1"
                min="0"
                max="10"
                value={rating}
                onChange={(e) => setRating(e.target.value)}
                placeholder="0.0 - 10.0"
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label htmlFor="meta-genres" className="text-xs font-semibold text-muted uppercase">
                  Genres
                </label>
                {renderLockBadge('genres')}
              </div>
              <input
                id="meta-genres"
                type="text"
                value={genres}
                onChange={(e) => setGenres(e.target.value)}
                placeholder="Sci-Fi, Drama, Thriller"
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label
                  htmlFor="meta-director"
                  className="text-xs font-semibold text-muted uppercase"
                >
                  Director
                </label>
                {renderLockBadge('director')}
              </div>
              <input
                id="meta-director"
                type="text"
                value={director}
                onChange={(e) => setDirector(e.target.value)}
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label htmlFor="meta-studio" className="text-xs font-semibold text-muted uppercase">
                  Studio
                </label>
                {renderLockBadge('studio')}
              </div>
              <input
                id="meta-studio"
                type="text"
                value={studio}
                onChange={(e) => setStudio(e.target.value)}
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label
                  htmlFor="meta-country"
                  className="text-xs font-semibold text-muted uppercase"
                >
                  Country
                </label>
                {renderLockBadge('country')}
              </div>
              <input
                id="meta-country"
                type="text"
                value={country}
                onChange={(e) => setCountry(e.target.value)}
                placeholder="US, EG, FR..."
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>
          </div>

          <div>
            <div className="flex items-center justify-between mb-1">
              <label htmlFor="meta-cast" className="text-xs font-semibold text-muted uppercase">
                Cast
              </label>
              {renderLockBadge('actors')}
            </div>
            <input
              id="meta-cast"
              type="text"
              value={cast}
              onChange={(e) => setCast(e.target.value)}
              placeholder="Comma-separated cast list"
              className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
            />
          </div>

          <div>
            <div className="flex items-center justify-between mb-1">
              <label htmlFor="meta-overview" className="text-xs font-semibold text-muted uppercase">
                Overview
              </label>
              {renderLockBadge('overview')}
            </div>
            <textarea
              id="meta-overview"
              rows={3}
              value={overview}
              onChange={(e) => setOverview(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
            />
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2 border-t border-border-subtle">
            <div>
              <div className="flex items-center justify-between mb-1">
                <label
                  htmlFor="meta-poster-url"
                  className="text-xs font-semibold text-muted uppercase"
                >
                  Poster URL
                </label>
                {renderLockBadge('poster_path')}
              </div>
              <input
                id="meta-poster-url"
                type="url"
                value={posterUrl}
                onChange={(e) => setPosterUrl(e.target.value)}
                placeholder="https://... (cached offline)"
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <label
                  htmlFor="meta-backdrop-url"
                  className="text-xs font-semibold text-muted uppercase"
                >
                  Backdrop URL
                </label>
                {renderLockBadge('backdrop_path')}
              </div>
              <input
                id="meta-backdrop-url"
                type="url"
                value={backdropUrl}
                onChange={(e) => setBackdropUrl(e.target.value)}
                placeholder="https://... (cached offline)"
                className="w-full px-3 py-2 rounded-xl bg-canvas border border-border-subtle text-sm text-text-main focus:outline-none focus:border-accent"
              />
            </div>
          </div>

          <div className="flex items-center justify-end gap-3 pt-4 border-t border-border-subtle">
            <button
              type="button"
              onClick={onClose}
              disabled={saving}
              className="px-4 py-2 rounded-xl text-sm font-medium bg-canvas hover:bg-panel-hover text-text-main border border-border-subtle transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={saving}
              className="inline-flex items-center gap-2 px-5 py-2 rounded-xl text-sm font-bold bg-cta hover:bg-cta-hover text-white shadow-md disabled:opacity-50 transition-colors cursor-pointer"
            >
              {saving ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Saving...
                </>
              ) : (
                <>
                  <Save className="w-4 h-4" />
                  Save Changes
                </>
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
