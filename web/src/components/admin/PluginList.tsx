import React from 'react';
import type { PluginRecord, PluginStatus } from '../../types';

export interface PluginListProps {
  plugins: PluginRecord[];
  busy?: boolean;
  onApprove: (plugin: PluginRecord) => void;
  onEnable: (plugin: PluginRecord) => void;
  onDisable: (plugin: PluginRecord) => void;
  onConnection: (plugin: PluginRecord) => void;
  onRemove: (plugin: PluginRecord) => void;
}

const STATUS_LABEL: Record<PluginStatus, string> = {
  pending: 'Pending',
  enabled: 'Enabled',
  disabled: 'Disabled',
  unhealthy: 'Unhealthy',
};

const STATUS_CLASS: Record<PluginStatus, string> = {
  pending: 'bg-amber-950/80 border-amber-500/50 text-amber-300',
  enabled: 'bg-emerald-950/80 border-emerald-500/50 text-emerald-300',
  disabled: 'bg-panel-hover border-border-subtle text-muted',
  unhealthy: 'bg-cta/15 border-cta/30 text-cta',
};

const buttonClass =
  'px-3 py-1.5 text-xs font-medium rounded-lg border border-border-subtle text-text-main hover:bg-panel-hover disabled:opacity-50 cursor-pointer';
const primaryClass =
  'px-3 py-1.5 text-xs font-semibold rounded-lg bg-accent text-canvas disabled:opacity-50 cursor-pointer';
const dangerClass =
  'px-3 py-1.5 text-xs font-medium rounded-lg border border-cta/30 text-cta hover:bg-cta/15 disabled:opacity-50 cursor-pointer';

export const PluginList: React.FC<PluginListProps> = ({
  plugins,
  busy = false,
  onApprove,
  onEnable,
  onDisable,
  onConnection,
  onRemove,
}) => {
  if (plugins.length === 0) {
    return <p className="text-sm text-muted">No plugins registered yet.</p>;
  }

  return (
    <ul className="space-y-3">
      {plugins.map((plugin) => {
        const pending = plugin.status === 'pending';
        const permissions = pending ? plugin.manifest.permissions : plugin.granted_permissions;
        return (
          <li
            key={plugin.id}
            data-testid={`plugin-row-${plugin.id}`}
            className="bg-panel border border-border-subtle rounded-xl p-4 flex flex-col md:flex-row md:items-center justify-between gap-4"
          >
            <div className="min-w-0 space-y-1">
              <div className="flex items-center gap-3 flex-wrap">
                <span className="text-sm font-semibold text-text-main">{plugin.name}</span>
                <span className="text-xs text-muted">v{plugin.manifest.version}</span>
                <span
                  className={`text-xs font-medium px-2 py-0.5 rounded-full border ${STATUS_CLASS[plugin.status]}`}
                >
                  {STATUS_LABEL[plugin.status]}
                </span>
              </div>
              <p className="text-xs text-muted break-all">{plugin.base_url}</p>
              <p className="text-xs text-muted">
                {pending ? 'Awaiting approval. Requests: ' : 'Granted: '}
                {permissions.length === 0 ? 'none' : permissions.join(', ')}
              </p>
            </div>

            <div className="flex items-center gap-2 flex-wrap">
              {pending && (
                <button type="button" className={primaryClass} disabled={busy} onClick={() => onApprove(plugin)}>
                  Review &amp; approve
                </button>
              )}
              {plugin.status === 'disabled' && (
                <button type="button" className={primaryClass} disabled={busy} onClick={() => onEnable(plugin)}>
                  Enable
                </button>
              )}
              {!pending && (
                <button type="button" className={buttonClass} disabled={busy} onClick={() => onConnection(plugin)}>
                  Connection details
                </button>
              )}
              {(plugin.status === 'enabled' || plugin.status === 'unhealthy') && (
                <button type="button" className={buttonClass} disabled={busy} onClick={() => onDisable(plugin)}>
                  Disable
                </button>
              )}
              <button type="button" className={dangerClass} disabled={busy} onClick={() => onRemove(plugin)}>
                Remove
              </button>
            </div>
          </li>
        );
      })}
    </ul>
  );
};
