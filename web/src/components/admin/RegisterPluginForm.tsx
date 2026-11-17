import React, { useState } from 'react';
import { Download, Plus } from 'lucide-react';

export interface RegisterPluginFormProps {
  /** Resolves `true` when registration succeeded (the field is then cleared). */
  onRegister: (baseUrl: string) => Promise<boolean>;
  /** Optional Git repository installer; when supplied, shows the Git / External mode switch. */
  onInstall?: (repoUrl: string, tag?: string) => Promise<boolean>;
  disabled?: boolean;
}

export const RegisterPluginForm: React.FC<RegisterPluginFormProps> = ({
  onRegister,
  onInstall,
  disabled = false,
}) => {
  const [mode, setMode] = useState<'git' | 'external'>(onInstall ? 'git' : 'external');
  const [repoUrl, setRepoUrl] = useState('');
  const [tag, setTag] = useState('');
  const [url, setUrl] = useState('');
  const [installing, setInstalling] = useState(false);

  const trimmedUrl = url.trim();
  const trimmedRepo = repoUrl.trim();
  const trimmedTag = tag.trim();

  const submitExternal = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!trimmedUrl || disabled) return;
    try {
      const ok = await onRegister(trimmedUrl);
      if (ok) setUrl('');
    } catch {
      // Rejections count as failed registration; keep field as typed
    }
  };

  const submitGit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!onInstall || !trimmedRepo || disabled || installing) return;
    setInstalling(true);
    try {
      const ok = await onInstall(trimmedRepo, trimmedTag || undefined);
      if (ok) {
        setRepoUrl('');
        setTag('');
      }
    } catch {
      // Keep typed input on error
    } finally {
      setInstalling(false);
    }
  };

  if (!onInstall) {
    return (
      <form onSubmit={submitExternal} className="flex flex-col sm:flex-row gap-3">
        <input
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          disabled={disabled}
          aria-label="Plugin base URL"
          placeholder="http://127.0.0.1:9100"
          className="flex-1 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
        />
        <button
          type="submit"
          disabled={disabled || !trimmedUrl}
          className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
        >
          <Plus className="w-4 h-4" aria-hidden="true" />
          Register plugin
        </button>
      </form>
    );
  }

  return (
    <div className="bg-panel border border-border-subtle rounded-xl p-4 space-y-4">
      <div
        role="tablist"
        aria-label="Plugin installation method"
        className="inline-flex rounded-lg bg-canvas p-1 border border-border-subtle"
      >
        <button
          type="button"
          role="tab"
          aria-selected={mode === 'git'}
          disabled={disabled}
          onClick={() => setMode('git')}
          className={`px-3 py-1.5 text-xs font-semibold rounded-md transition-colors cursor-pointer disabled:opacity-50 ${
            mode === 'git' ? 'bg-accent text-canvas' : 'text-muted hover:text-text-main'
          }`}
        >
          Install from Git
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === 'external'}
          disabled={disabled}
          onClick={() => setMode('external')}
          className={`px-3 py-1.5 text-xs font-semibold rounded-md transition-colors cursor-pointer disabled:opacity-50 ${
            mode === 'external' ? 'bg-accent text-canvas' : 'text-muted hover:text-text-main'
          }`}
        >
          External URL
        </button>
      </div>

      {mode === 'git' ? (
        <form onSubmit={submitGit} className="space-y-3">
          <div className="flex flex-col sm:flex-row gap-3">
            <input
              type="text"
              value={repoUrl}
              onChange={(e) => setRepoUrl(e.target.value)}
              disabled={disabled}
              aria-label="Repository URL"
              placeholder="https://github.com/owner/kadr-plugin-example"
              className="flex-1 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
            />
            <input
              type="text"
              value={tag}
              onChange={(e) => setTag(e.target.value)}
              disabled={disabled}
              aria-label="Release tag (optional)"
              placeholder="Tag (optional, e.g. v0.1.0)"
              className="sm:w-48 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
            />
            <button
              type="submit"
              disabled={disabled || installing || !trimmedRepo}
              className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
            >
              <Download className="w-4 h-4" aria-hidden="true" />
              {installing ? 'Installing…' : 'Install plugin'}
            </button>
          </div>
          <p className="text-xs text-muted">
            Supports public GitHub, GitLab, and Gitea repositories with prebuilt release archives.
          </p>
        </form>
      ) : (
        <form onSubmit={submitExternal} className="flex flex-col sm:flex-row gap-3">
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            disabled={disabled}
            aria-label="Plugin base URL"
            placeholder="http://127.0.0.1:9100"
            className="flex-1 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main placeholder:text-muted focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50"
          />
          <button
            type="submit"
            disabled={disabled || !trimmedUrl}
            className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
          >
            <Plus className="w-4 h-4" aria-hidden="true" />
            Register plugin
          </button>
        </form>
      )}
    </div>
  );
};
