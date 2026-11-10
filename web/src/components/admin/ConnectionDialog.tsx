import React, { useEffect, useRef, useState } from 'react';
import { Copy, X } from 'lucide-react';
import { api, describeApiError } from '../../api/client';
import type { PluginConnection, PluginRecord } from '../../types';

export interface ConnectionDialogProps {
  plugin: PluginRecord;
  onClose: () => void;
}

const inputClass =
  'flex-1 min-w-0 bg-canvas border border-border-subtle rounded-lg px-3 py-2 text-sm text-text-main font-mono';
const copyClass =
  'shrink-0 flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-text-main bg-panel-hover rounded-lg hover:bg-accent hover:text-canvas cursor-pointer';

export const ConnectionDialog: React.FC<ConnectionDialogProps> = ({ plugin, onClose }) => {
  const [connection, setConnection] = useState<PluginConnection | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  const pluginIdRef = useRef(plugin.id);
  pluginIdRef.current = plugin.id;
  const isMountedRef = useRef(true);

  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  useEffect(() => {
    let cancelled = false;
    setConnection(null);
    setToken(null);
    setError(null);
    setCopied(null);
    setGenerating(false);

    api
      .getPluginConnection(plugin.id)
      .then((c) => {
        if (!cancelled && isMountedRef.current && pluginIdRef.current === plugin.id) {
          setConnection(c);
        }
      })
      .catch((err) => {
        if (!cancelled && isMountedRef.current && pluginIdRef.current === plugin.id) {
          setError(describeApiError(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [plugin.id]);

  const generateToken = async () => {
    const currentId = plugin.id;
    setGenerating(true);
    setError(null);
    try {
      const res = await api.issuePluginToken(currentId);
      if (isMountedRef.current && pluginIdRef.current === currentId) {
        setToken(res.token);
      }
    } catch (err) {
      if (isMountedRef.current && pluginIdRef.current === currentId) {
        setError(describeApiError(err));
      }
    } finally {
      if (isMountedRef.current && pluginIdRef.current === currentId) {
        setGenerating(false);
      }
    }
  };

  const copy = async (label: string, text: string) => {
    const currentId = plugin.id;
    try {
      await navigator.clipboard.writeText(text);
      if (isMountedRef.current && pluginIdRef.current === currentId) {
        setCopied(label);
        setError(null);
      }
    } catch {
      if (isMountedRef.current && pluginIdRef.current === currentId) {
        setCopied(null);
        setError('Copy failed. Select the text and copy it manually.');
      }
    }
  };

  const handleBackdropClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  const envBlock = connection
    ? [
        `KADR_URL=${window.location.origin}`,
        `KADR_PLUGIN_SECRET=${connection.secret}`,
        `KADR_PLUGIN_TOKEN=${token ?? '<click "Generate token">'}`,
      ].join('\n')
    : '';

  return (
    <div
      onClick={handleBackdropClick}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Connection details for ${plugin.name}`}
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-2xl bg-panel border border-border-subtle rounded-xl p-6 shadow-xl space-y-4"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h3 className="text-lg font-semibold text-text-main">
              Connection details for {plugin.name}
            </h3>
            <p className="text-xs text-muted mt-0.5">
              Give these values to the plugin process. Treat the secret and token like passwords.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close dialog"
            className="p-1.5 text-muted hover:text-text-main rounded-lg hover:bg-panel-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {error && (
          <div role="alert" className="text-sm text-cta bg-cta/15 border border-cta/30 rounded-lg px-3 py-2">
            {error}
          </div>
        )}

        {connection && (
          <div className="space-y-4">
            <div>
              <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-1">
                Plugin base URL
              </label>
              <input readOnly value={connection.base_url} aria-label="Plugin base URL" className={inputClass} />
            </div>

            <div>
              <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-1">
                Secret (Kadr authenticates to the plugin with this)
              </label>
              <div className="flex gap-2">
                <input readOnly value={connection.secret} aria-label="Secret" className={inputClass} />
                <button type="button" className={copyClass} onClick={() => copy('secret', connection.secret)}>
                  <Copy className="w-3.5 h-3.5" /> Copy secret
                </button>
              </div>
            </div>

            <div>
              <label className="block text-xs font-semibold text-muted uppercase tracking-wider mb-1">
                Token (the plugin authenticates to Kadr with this)
              </label>
              {token ? (
                <div className="flex gap-2">
                  <input readOnly value={token} aria-label="Token" className={inputClass} />
                  <button type="button" className={copyClass} onClick={() => copy('token', token)}>
                    <Copy className="w-3.5 h-3.5" /> Copy token
                  </button>
                </div>
              ) : (
                <p className="text-xs text-muted mb-2">No token generated yet.</p>
              )}
              <button
                type="button"
                onClick={generateToken}
                disabled={generating || plugin.status === 'pending'}
                title={plugin.status === 'pending' ? 'Approve the plugin first' : undefined}
                className="mt-2 px-3 py-2 text-xs font-semibold bg-accent text-canvas rounded-lg disabled:opacity-50 cursor-pointer"
              >
                Generate token
              </button>
            </div>

            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-semibold text-muted uppercase tracking-wider">
                  Environment variables
                </span>
                <button type="button" className={copyClass} onClick={() => copy('env', envBlock)}>
                  <Copy className="w-3.5 h-3.5" /> Copy environment variables
                </button>
              </div>
              <pre
                data-testid="connection-env"
                className="bg-canvas border border-border-subtle rounded-lg p-3 text-xs text-text-main font-mono overflow-x-auto whitespace-pre-wrap break-all"
              >
                {envBlock}
              </pre>
            </div>
            {copied && <p className="text-xs text-emerald-300">Copied {copied}.</p>}
          </div>
        )}

        <div className="flex justify-end pt-2">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 text-sm text-muted hover:text-text-main rounded-xl hover:bg-panel-hover cursor-pointer"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
