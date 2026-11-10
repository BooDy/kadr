import React, { useEffect, useState } from 'react';
import { X } from 'lucide-react';
import type { PluginPermission, PluginRecord } from '../../types';

const PERMISSION_INFO: Record<PluginPermission, { title: string; description: string }> = {
  'items:publish': {
    title: 'Publish items',
    description: 'Create its own libraries and add or remove items in them.',
  },
  'playback:resolve': {
    title: 'Resolve playback',
    description: 'Be asked how to play the items it published (stream URL, proxy or transcode).',
  },
};

export interface ApprovePluginDialogProps {
  plugin: PluginRecord;
  busy?: boolean;
  onConfirm: (permissions: PluginPermission[]) => void;
  onCancel: () => void;
}

export const ApprovePluginDialog: React.FC<ApprovePluginDialogProps> = ({
  plugin,
  busy = false,
  onConfirm,
  onCancel,
}) => {
  const requested = plugin.manifest.permissions;
  const [selected, setSelected] = useState<PluginPermission[]>(requested);

  useEffect(() => {
    setSelected(plugin.manifest.permissions);
  }, [plugin.id, plugin.manifest.permissions]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !busy) {
        onCancel();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onCancel, busy]);

  const toggle = (permission: PluginPermission) => {
    setSelected((current) =>
      current.includes(permission)
        ? current.filter((p) => p !== permission)
        : [...current, permission]
    );
  };

  const confirm = () => onConfirm(requested.filter((p) => selected.includes(p)));

  const handleBackdropClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget && !busy) {
      onCancel();
    }
  };

  return (
    <div
      onClick={handleBackdropClick}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Approve ${plugin.name}`}
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-lg bg-panel border border-border-subtle rounded-xl p-6 shadow-xl space-y-4"
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h3 className="text-lg font-semibold text-text-main">Approve {plugin.name}</h3>
            <p className="text-xs text-muted mt-0.5">
              v{plugin.manifest.version} · {plugin.base_url}
            </p>
          </div>
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
          This plugin asks for the permissions below. Nothing is granted until you confirm.
        </p>

        {requested.length === 0 ? (
          <p className="text-sm text-text-main">This plugin does not request any permissions.</p>
        ) : (
          <ul className="space-y-3">
            {requested.map((permission) => (
              <li key={permission}>
                <label className="flex items-start gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={selected.includes(permission)}
                    onChange={() => toggle(permission)}
                    className="mt-1 accent-accent"
                  />
                  <span>
                    <span className="block text-sm font-medium text-text-main">
                      {PERMISSION_INFO[permission].title}
                    </span>
                    <span className="block text-xs text-muted">
                      {PERMISSION_INFO[permission].description}
                    </span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}

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
            onClick={confirm}
            disabled={busy}
            className="px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
          >
            Approve and enable
          </button>
        </div>
      </div>
    </div>
  );
};
