import React, { useCallback, useEffect, useRef, useState } from 'react';
import { AlertCircle, CheckCircle2, Puzzle } from 'lucide-react';
import { api, describeApiError } from '../../api/client';
import type { PluginPermission, PluginRecord, PluginSettings } from '../../types';
import { ApprovePluginDialog } from './ApprovePluginDialog';
import { ConnectionDialog } from './ConnectionDialog';
import { PluginList } from './PluginList';
import { RegisterPluginForm } from './RegisterPluginForm';
import { RemovePluginDialog } from './RemovePluginDialog';

const REFRESH_INTERVAL_MS = 15000;

type Banner = { type: 'success' | 'error' | 'warning'; text: string } | null;

export const PluginsPanel: React.FC = () => {
  const [settings, setSettings] = useState<PluginSettings | null>(null);
  const [timeoutInput, setTimeoutInput] = useState('5');
  const [allowPrivate, setAllowPrivate] = useState(false);
  const [plugins, setPlugins] = useState<PluginRecord[]>([]);
  const [banner, setBanner] = useState<Banner>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [approving, setApproving] = useState<PluginRecord | null>(null);
  const [removing, setRemoving] = useState<PluginRecord | null>(null);
  const [connecting, setConnecting] = useState<PluginRecord | null>(null);
  const [dialogError, setDialogError] = useState<string | null>(null);

  const isMountedRef = useRef(true);
  const refreshSeqRef = useRef(0);
  const enabledRef = useRef(false);

  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  const enabled = settings?.enabled ?? false;

  const applySettings = useCallback((s: PluginSettings) => {
    setSettings(s);
    enabledRef.current = s.enabled;
    setTimeoutInput(String(s.resolve_timeout_secs));
    setAllowPrivate(s.allow_private_proxy);
  }, []);

  const refresh = useCallback(async (silent = false): Promise<boolean> => {
    const seq = ++refreshSeqRef.current;
    try {
      const data = await api.listPlugins();
      if (!isMountedRef.current || seq !== refreshSeqRef.current || !enabledRef.current) {
        return false;
      }
      setPlugins(data);
      return true;
    } catch (err) {
      if (!isMountedRef.current || seq !== refreshSeqRef.current || !enabledRef.current) {
        return false;
      }
      setBanner(
        silent
          ? { type: 'warning', text: 'Could not refresh plugin status; showing the last known state.' }
          : { type: 'error', text: describeApiError(err) }
      );
      return false;
    }
  }, []);

  // Initial load: settings first, the plugin list only when plugins are enabled.
  useEffect(() => {
    (async () => {
      try {
        const s = await api.getPluginSettings();
        if (!isMountedRef.current) return;
        applySettings(s);
        if (s.enabled) await refresh();
      } catch (err) {
        if (isMountedRef.current) setBanner({ type: 'error', text: describeApiError(err) });
      } finally {
        if (isMountedRef.current) setLoading(false);
      }
    })();
  }, [applySettings, refresh]);

  // Keep statuses fresh (health changes) while plugins are enabled.
  useEffect(() => {
    if (!enabled) return;
    const id = setInterval(() => {
      void refresh(true);
    }, REFRESH_INTERVAL_MS);
    return () => clearInterval(id);
  }, [enabled, refresh]);

  const run = async (action: () => Promise<string | void>) => {
    setBusy(true);
    try {
      const message = await action();
      if (isMountedRef.current && message) {
        setBanner({ type: 'success', text: message });
      }
    } catch (err) {
      if (isMountedRef.current) {
        setBanner({ type: 'error', text: describeApiError(err) });
      }
    } finally {
      if (isMountedRef.current) {
        setBusy(false);
      }
    }
  };

  const toggleEnabled = () =>
    run(async () => {
      const nextEnabled = !enabled;
      if (!nextEnabled) {
        refreshSeqRef.current++;
        enabledRef.current = false;
        setPlugins([]);
      }
      const updated = await api.updatePluginSettings({ enabled: nextEnabled });
      if (!isMountedRef.current) return;
      applySettings(updated);
      if (updated.enabled) {
        await refresh();
      } else {
        refreshSeqRef.current++;
        enabledRef.current = false;
        setPlugins([]);
      }
      return updated.enabled ? 'Plugins enabled.' : 'Plugins disabled.';
    });

  const saveSettings = (e: React.FormEvent) => {
    e.preventDefault();
    const secs = Number(timeoutInput);
    if (!Number.isInteger(secs) || secs < 1 || secs > 60) {
      setBanner({
        type: 'error',
        text: 'Resolve timeout must be a whole number between 1 and 60 seconds.',
      });
      return;
    }
    void run(async () => {
      const updated = await api.updatePluginSettings({
        resolve_timeout_secs: secs,
        allow_private_proxy: allowPrivate,
      });
      if (isMountedRef.current) {
        applySettings(updated);
      }
      return 'Plugin settings saved.';
    });
  };

  const register = async (baseUrl: string): Promise<boolean> => {
    let ok = false;
    await run(async () => {
      const res = await api.registerPlugin(baseUrl);
      ok = true;
      if (isMountedRef.current) {
        setBanner({
          type: 'success',
          text: `Registered "${res.plugin.name}". Review and approve its permissions to enable it.`,
        });
      }
      await refresh(true);
    });
    return ok;
  };

  const confirmApprove = (permissions: PluginPermission[]) => {
    const target = approving;
    if (!target) return;
    setDialogError(null);
    setBusy(true);
    (async () => {
      try {
        const res = await api.approvePlugin(target.id, permissions);
        if (!isMountedRef.current) return;
        setApproving(null);
        setDialogError(null);
        await refresh();
        if (isMountedRef.current) {
          setBanner({ type: 'success', text: `"${res.plugin.name}" approved and enabled.` });
        }
      } catch (err) {
        if (!isMountedRef.current) return;
        setDialogError(describeApiError(err));
      } finally {
        if (isMountedRef.current) {
          setBusy(false);
        }
      }
    })();
  };

  const setPluginEnabled = (plugin: PluginRecord, value: boolean) =>
    run(async () => {
      const updated = await api.setPluginEnabled(plugin.id, value);
      await refresh();
      return `"${updated.name}" ${value ? 'enabled' : 'disabled'}.`;
    });

  const confirmRemove = (purge: boolean) => {
    const target = removing;
    if (!target) return;
    setDialogError(null);
    setBusy(true);
    (async () => {
      try {
        await api.removePlugin(target.id, purge);
        if (!isMountedRef.current) return;
        setRemoving(null);
        setDialogError(null);
        await refresh();
        if (isMountedRef.current) {
          setBanner({ type: 'success', text: `Removed "${target.name}".` });
        }
      } catch (err) {
        if (!isMountedRef.current) return;
        setDialogError(describeApiError(err));
      } finally {
        if (isMountedRef.current) {
          setBusy(false);
        }
      }
    })();
  };

  const openApprove = (p: PluginRecord) => {
    setDialogError(null);
    setApproving(p);
  };

  const closeApprove = () => {
    setDialogError(null);
    setApproving(null);
  };

  const openRemove = (p: PluginRecord) => {
    setDialogError(null);
    setRemoving(p);
  };

  const closeRemove = () => {
    setDialogError(null);
    setRemoving(null);
  };

  const openConnecting = (p: PluginRecord) => {
    setConnecting(p);
  };

  const closeConnecting = () => {
    setConnecting(null);
  };

  return (
    <div className="mt-8 max-w-3xl space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-text-main flex items-center gap-2">
          <Puzzle className="w-5 h-5" />
          Plugins
        </h2>
        <p className="text-muted text-sm mt-0.5">
          Plugins add virtual libraries (live TV, streaming sources) and resolve playback for them.
        </p>
      </div>

      {banner && (
        <div
          role={banner.type === 'success' ? 'status' : 'alert'}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium ${
            banner.type === 'success'
              ? 'bg-emerald-950/80 border border-emerald-500/50 text-emerald-300'
              : banner.type === 'warning'
              ? 'bg-amber-950/80 border border-amber-500/50 text-amber-300'
              : 'bg-cta/15 border border-cta/30 text-cta'
          }`}
        >
          {banner.type === 'success' ? (
            <CheckCircle2 className="w-4 h-4 shrink-0" />
          ) : (
            <AlertCircle className="w-4 h-4 shrink-0" />
          )}
          <span>{banner.text}</span>
        </div>
      )}

      {loading ? (
        <p className="text-sm text-muted">Loading plugin settings…</p>
      ) : (
        <>
          <div className="bg-panel border border-border-subtle rounded-xl p-6 flex items-center justify-between gap-4 shadow-sm">
            <div>
              <h3 className="text-sm font-semibold text-text-main">Enable plugins</h3>
              <p className="text-xs text-muted mt-0.5">
                Turning plugins off keeps their data; their items just cannot be played.
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={enabled}
              aria-label="Enable plugins"
              disabled={busy}
              onClick={toggleEnabled}
              className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors cursor-pointer disabled:opacity-50 ${
                enabled ? 'bg-accent' : 'bg-panel-hover border border-border-subtle'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 rounded-full bg-canvas transition-transform ${
                  enabled ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </button>
          </div>

          <form
            onSubmit={saveSettings}
            noValidate
            className="bg-panel border border-border-subtle rounded-xl p-6 space-y-4 shadow-sm"
          >
            <h3 className="text-sm font-semibold text-accent uppercase tracking-wider">Settings</h3>
            <div>
              <label htmlFor="plugin-timeout" className="block text-xs font-medium text-text-main mb-1">
                Resolve timeout (seconds)
              </label>
              <input
                id="plugin-timeout"
                type="number"
                min={1}
                max={60}
                value={timeoutInput}
                onChange={(e) => setTimeoutInput(e.target.value)}
                className="w-32 bg-canvas border border-border-subtle rounded-xl px-3 py-2 text-sm text-text-main focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
              />
              <p className="text-xs text-muted mt-1">
                How long Kadr waits for a plugin to answer (1 to 60). Applies immediately.
              </p>
            </div>
            <label className="flex items-start gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={allowPrivate}
                onChange={(e) => setAllowPrivate(e.target.checked)}
                aria-label="Allow plugins to stream from private/LAN addresses"
                className="mt-1 accent-accent"
              />
              <span>
                <span className="block text-sm text-text-main">
                  Allow plugins to stream from private/LAN addresses
                </span>
                <span className="block text-xs text-amber-300">
                  Only enable this for plugins you trust: it lets them make Kadr connect to devices
                  on your network.
                </span>
              </span>
            </label>
            <button
              type="submit"
              disabled={busy}
              className="px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
            >
              Save plugin settings
            </button>
          </form>

          {!enabled && (
            <p className="text-sm text-muted">
              Plugins are disabled. Turn them on to register and manage plugins.
            </p>
          )}

          <div
            aria-disabled={!enabled}
            className={`space-y-6 ${enabled ? '' : 'opacity-50 pointer-events-none'}`}
          >
            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider">
                Register a plugin
              </h3>
              <RegisterPluginForm onRegister={register} disabled={!enabled || busy} />
            </div>

            <div className="space-y-3">
              <h3 className="text-sm font-semibold text-accent uppercase tracking-wider">
                Installed plugins
              </h3>
              <PluginList
                plugins={plugins}
                busy={busy}
                onApprove={openApprove}
                onEnable={(p) => void setPluginEnabled(p, true)}
                onDisable={(p) => void setPluginEnabled(p, false)}
                onConnection={openConnecting}
                onRemove={openRemove}
              />
            </div>
          </div>
        </>
      )}

      {approving && (
        <ApprovePluginDialog
          plugin={approving}
          busy={busy}
          error={dialogError}
          onConfirm={confirmApprove}
          onCancel={closeApprove}
        />
      )}
      {removing && (
        <RemovePluginDialog
          plugin={removing}
          busy={busy}
          error={dialogError}
          onConfirm={confirmRemove}
          onCancel={closeRemove}
        />
      )}
      {connecting && (
        <ConnectionDialog plugin={connecting} onClose={closeConnecting} />
      )}
    </div>
  );
};
