import React, { useState } from 'react';
import { X } from 'lucide-react';
import type { PluginRecord } from '../../types';

export interface RemovePluginDialogProps {
  plugin: PluginRecord;
  busy?: boolean;
  onConfirm: (purge: boolean) => void;
  onCancel: () => void;
}

export const RemovePluginDialog: React.FC<RemovePluginDialogProps> = ({
  plugin,
  busy = false,
  onConfirm,
  onCancel,
}) => {
  const [purge, setPurge] = useState(false);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Remove ${plugin.name}`}
        className="w-full max-w-md bg-panel border border-border-subtle rounded-xl p-6 shadow-xl space-y-4"
      >
        <div className="flex items-start justify-between gap-4">
          <h3 className="text-lg font-semibold text-text-main">Remove {plugin.name}?</h3>
          <button
            type="button"
            onClick={onCancel}
            aria-label="Close"
            className="p-1.5 text-muted hover:text-text-main rounded-lg hover:bg-panel-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <p className="text-sm text-muted">
          Kadr will stop talking to this plugin. Items it published stay in your libraries but cannot
          be played unless you delete them too.
        </p>

        <label className="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            checked={purge}
            onChange={(e) => setPurge(e.target.checked)}
            className="mt-1 accent-accent"
          />
          <span className="text-sm text-text-main">
            Also delete this plugin&apos;s libraries and items
          </span>
        </label>

        <div className="flex justify-end gap-3 pt-2">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-sm text-muted hover:text-text-main rounded-xl hover:bg-panel-hover cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => onConfirm(purge)}
            disabled={busy}
            className="px-4 py-2 text-sm font-semibold bg-cta text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
          >
            Remove plugin
          </button>
        </div>
      </div>
    </div>
  );
};
