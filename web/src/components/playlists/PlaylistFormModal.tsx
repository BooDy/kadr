import { useState, useEffect, type FC, type FormEvent } from 'react';
import { X, Globe, Lock, Loader2 } from 'lucide-react';
import { describeApiError } from '../../api/client';

export interface PlaylistFormValues {
  name: string;
  description?: string;
  is_public: boolean;
}

export interface PlaylistFormModalProps {
  isOpen: boolean;
  mode: 'create' | 'edit';
  initialName?: string;
  initialDescription?: string;
  initialIsPublic?: boolean;
  onClose: () => void;
  onSubmit: (values: PlaylistFormValues) => Promise<void>;
}

export const PlaylistFormModal: FC<PlaylistFormModalProps> = ({
  isOpen,
  mode,
  initialName = '',
  initialDescription = '',
  initialIsPublic = false,
  onClose,
  onSubmit,
}) => {
  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState(initialDescription);
  const [isPublic, setIsPublic] = useState(initialIsPublic);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setName(initialName);
    setDescription(initialDescription);
    setIsPublic(initialIsPublic);
    setSubmitting(false);
    setError(null);
  }, [isOpen, initialName, initialDescription, initialIsPublic]);

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

  if (!isOpen) return null;

  const handleFormSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmedName = name.trim();
    if (!trimmedName || trimmedName.length > 120) {
      setError('Playlist name must be between 1 and 120 characters.');
      return;
    }
    const trimmedDesc = description.trim();
    if (trimmedDesc.length > 500) {
      setError('Description must be 500 characters or fewer.');
      return;
    }

    setSubmitting(true);
    setError(null);
    try {
      await onSubmit({
        name: trimmedName,
        description: mode === 'edit' ? trimmedDesc : trimmedDesc || undefined,
        is_public: isPublic,
      });
      onClose();
    } catch (err) {
      setError(describeApiError(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={mode === 'create' ? 'New Playlist' : 'Edit Playlist'}
      onClick={(e) => {
        e.stopPropagation();
        onClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-canvas/85 backdrop-blur-sm"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-md rounded-xl bg-panel border border-border-subtle p-6 text-text-main space-y-5"
      >
        <div className="flex items-center justify-between border-b border-border-subtle pb-3.5">
          <h2 className="text-lg font-semibold text-text-main">
            {mode === 'create' ? 'Create Playlist' : 'Edit Playlist'}
          </h2>
          <button
            type="button"
            aria-label="Close modal"
            onClick={onClose}
            className="p-1.5 rounded-lg text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <form onSubmit={handleFormSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label
              htmlFor="playlist-name-input"
              className="block text-xs font-medium uppercase tracking-wider text-muted"
            >
              Playlist Name
            </label>
            <input
              id="playlist-name-input"
              aria-label="Playlist Name"
              type="text"
              required
              maxLength={120}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Weekend Sci-Fi Marathon"
              className="w-full rounded-xl bg-canvas border border-border-subtle px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
          </div>

          <div className="space-y-1.5">
            <label
              htmlFor="playlist-description-input"
              className="block text-xs font-medium uppercase tracking-wider text-muted"
            >
              Description
            </label>
            <textarea
              id="playlist-description-input"
              aria-label="Description"
              rows={3}
              maxLength={500}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional notes about this playlist..."
              className="w-full rounded-xl bg-canvas border border-border-subtle px-3.5 py-2.5 text-sm text-text-main placeholder:text-muted resize-none focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
          </div>

          <label className="flex items-start gap-3 p-3 rounded-xl bg-canvas/60 border border-border-subtle cursor-pointer hover:bg-panel-hover transition-colors">
            <input
              type="checkbox"
              checked={isPublic}
              onChange={(e) => setIsPublic(e.target.checked)}
              className="mt-0.5 h-4 w-4 rounded border-border-subtle bg-canvas text-accent accent-accent cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            />
            <div className="space-y-0.5">
              <div className="flex items-center gap-1.5 text-sm font-medium text-text-main">
                {isPublic ? (
                  <Globe className="w-4 h-4 text-accent" />
                ) : (
                  <Lock className="w-4 h-4 text-muted" />
                )}
                <span>Share with everyone on server</span>
              </div>
              <p className="text-xs text-muted">
                Other profiles can view and play this playlist, while only you can edit it.
              </p>
            </div>
          </label>

          {error && (
            <div className="p-3 rounded-xl bg-cta/15 border border-cta/40 text-sm text-highlight">
              {error}
            </div>
          )}

          <div className="flex items-center justify-end gap-3 pt-2 border-t border-border-subtle">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-xl text-sm font-medium text-muted hover:text-text-main hover:bg-panel-hover transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="flex items-center gap-2 px-5 py-2 rounded-xl text-sm font-semibold bg-cta hover:bg-cta-hover text-text-main transition-colors disabled:opacity-50 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
            >
              {submitting && <Loader2 className="w-4 h-4 animate-spin" />}
              <span>{mode === 'create' ? 'Create Playlist' : 'Save Changes'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
