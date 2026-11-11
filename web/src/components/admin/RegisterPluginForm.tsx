import React, { useState } from 'react';
import { Plus } from 'lucide-react';

export interface RegisterPluginFormProps {
  /** Resolves `true` when registration succeeded (the field is then cleared). */
  onRegister: (baseUrl: string) => Promise<boolean>;
  disabled?: boolean;
}

export const RegisterPluginForm: React.FC<RegisterPluginFormProps> = ({
  onRegister,
  disabled = false,
}) => {
  const [url, setUrl] = useState('');
  const trimmed = url.trim();

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!trimmed || disabled) return;
    try {
      const ok = await onRegister(trimmed);
      if (ok) setUrl('');
    } catch {
      // Rejections count as failed registration; keep field as typed
    }
  };

  return (
    <form onSubmit={submit} className="flex flex-col sm:flex-row gap-3">
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
        disabled={disabled || !trimmed}
        className="flex items-center justify-center gap-2 px-4 py-2 text-sm font-semibold bg-accent text-canvas rounded-xl shadow-md disabled:opacity-50 cursor-pointer"
      >
        <Plus className="w-4 h-4" aria-hidden="true" />
        Register plugin
      </button>
    </form>
  );
};
