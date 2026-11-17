import React, { useEffect, useState } from 'react';
import {
  Sparkles,
  Key,
  Globe,
  CheckCircle2,
  AlertCircle,
  Loader2,
  Save,
  Trash2,
} from 'lucide-react';
import { api, describeApiError } from '../../api/client';
import type { MetadataSettings, UpdateMetadataSettingsPayload } from '../../types';

export interface MetadataSettingsPanelProps {
  initialSettings?: MetadataSettings | null;
  onSettingsChange?: (settings: MetadataSettings) => void;
}

const LANGUAGE_PRESETS = ['en-US', 'ar-EG', 'fr-FR', 'es-ES', 'de-DE', 'ja-JP'];

export const MetadataSettingsPanel: React.FC<MetadataSettingsPanelProps> = ({
  initialSettings,
  onSettingsChange,
}) => {
  const [settings, setSettings] = useState<MetadataSettings | null>(initialSettings ?? null);
  const [enabled, setEnabled] = useState<boolean>(initialSettings?.enabled ?? false);
  const [autoFetchOnScan, setAutoFetchOnScan] = useState<boolean>(
    initialSettings?.auto_fetch_on_scan ?? false
  );
  const [language, setLanguage] = useState<string>(initialSettings?.language ?? 'en-US');
  const [apiKeyInput, setApiKeyInput] = useState<string>('');
  const [loading, setLoading] = useState<boolean>(!initialSettings);
  const [saving, setSaving] = useState<boolean>(false);
  const [banner, setBanner] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    api
      .getMetadataSettings()
      .then((res) => {
        if (!mounted) return;
        setSettings(res);
        setEnabled(res.enabled);
        setAutoFetchOnScan(res.auto_fetch_on_scan);
        setLanguage(res.language || 'en-US');
        onSettingsChange?.(res);
      })
      .catch((err) => {
        if (mounted) {
          setBanner({ type: 'error', text: describeApiError(err) });
        }
      })
      .finally(() => {
        if (mounted) {
          setLoading(false);
        }
      });
    return () => {
      mounted = false;
    };
  }, [onSettingsChange]);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setBanner(null);
    try {
      const payload: UpdateMetadataSettingsPayload = {
        enabled,
        auto_fetch_on_scan: autoFetchOnScan,
        language: language.trim() || 'en-US',
      };
      if (apiKeyInput.trim() !== '') {
        payload.api_key = apiKeyInput.trim();
      }
      const updated = await api.updateMetadataSettings(payload);
      setSettings(updated);
      setEnabled(updated.enabled);
      setAutoFetchOnScan(updated.auto_fetch_on_scan);
      setLanguage(updated.language);
      setApiKeyInput('');
      onSettingsChange?.(updated);
      setBanner({ type: 'success', text: 'Metadata & TMDb settings saved.' });
    } catch (err) {
      setBanner({ type: 'error', text: describeApiError(err) });
    } finally {
      setSaving(false);
    }
  };

  const handleClearApiKey = async () => {
    setSaving(true);
    setBanner(null);
    try {
      const updated = await api.updateMetadataSettings({ api_key: '' });
      setSettings(updated);
      setApiKeyInput('');
      onSettingsChange?.(updated);
      setBanner({ type: 'success', text: 'TMDb API key cleared.' });
    } catch (err) {
      setBanner({ type: 'error', text: describeApiError(err) });
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="mt-8 py-12 flex items-center justify-center gap-2 text-muted">
        <Loader2 className="w-5 h-5 animate-spin text-accent" />
        <span>Loading metadata settings...</span>
      </div>
    );
  }

  return (
    <div className="mt-8 max-w-3xl space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-text-main flex items-center gap-2">
          <Sparkles className="w-5 h-5 text-accent" />
          Metadata & TMDb Scraper
        </h2>
        <p className="text-muted text-sm mt-0.5">
          Opt-in TMDb metadata and artwork enrichment. Downloaded posters and backdrops are cached
          locally on disk for 100% offline serving.
        </p>
      </div>

      {banner && (
        <div
          className={`flex items-center gap-2 px-4 py-3 rounded-xl text-sm font-medium ${
            banner.type === 'success'
              ? 'bg-emerald-950/80 border border-emerald-500/50 text-emerald-300'
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

      <form
        onSubmit={handleSave}
        className="bg-panel border border-border-subtle rounded-xl p-6 space-y-6 shadow-sm"
      >
        {/* Enable Toggle */}
        <label className="flex items-start justify-between gap-4 cursor-pointer">
          <div>
            <span className="text-sm font-semibold text-text-main block">
              Enable TMDb Integration
            </span>
            <span className="text-xs text-muted block mt-0.5">
              Allows manual Identify matching and library metadata enrichment via TMDb API.
            </span>
          </div>
          <input
            type="checkbox"
            aria-label="Enable TMDb Integration"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
            className="mt-1 h-4 w-4 rounded border-border-subtle text-accent focus:ring-accent"
          />
        </label>

        {/* API Key / v4 Read Access Token */}
        <div className="space-y-2 pt-4 border-t border-border-subtle">
          <div className="flex items-center justify-between">
            <label
              htmlFor="tmdb-api-key"
              className="text-xs font-semibold text-muted uppercase tracking-wider flex items-center gap-1.5"
            >
              <Key className="w-3.5 h-3.5 text-accent" />
              TMDb API Key or v4 Read Access Token
            </label>
            {settings?.api_key_configured ? (
              <div className="flex items-center gap-2">
                <span className="text-xs px-2 py-0.5 rounded-full bg-emerald-950/80 border border-emerald-500/40 text-emerald-300 font-mono">
                  Configured ({settings.api_key_hint || '••••'})
                </span>
                <button
                  type="button"
                  onClick={handleClearApiKey}
                  disabled={saving}
                  className="inline-flex items-center gap-1 text-xs text-muted hover:text-cta transition-colors cursor-pointer"
                >
                  <Trash2 className="w-3 h-3" />
                  Clear Key
                </button>
              </div>
            ) : (
              <span className="text-xs text-muted">Not configured</span>
            )}
          </div>
          <input
            id="tmdb-api-key"
            type="password"
            value={apiKeyInput}
            onChange={(e) => setApiKeyInput(e.target.value)}
            placeholder={
              settings?.api_key_configured
                ? 'Leave blank to keep existing key, or paste a new key...'
                : 'Paste 32-char v3 API key or v4 JWT Read Access Token...'
            }
            className="w-full bg-canvas border border-border-subtle rounded-xl px-3.5 py-2.5 text-sm text-text-main font-mono placeholder:text-muted focus:outline-none focus:border-accent"
          />
          <p className="text-xs text-muted">
            Supports both TMDb v3 API Keys and TMDb v4 JWT Bearer Read Access Tokens automatically.
          </p>
        </div>

        {/* Language */}
        <div className="space-y-2 pt-4 border-t border-border-subtle">
          <label
            htmlFor="tmdb-language"
            className="text-xs font-semibold text-muted uppercase tracking-wider flex items-center gap-1.5"
          >
            <Globe className="w-3.5 h-3.5 text-accent" />
            Metadata Language
          </label>
          <div className="flex flex-wrap items-center gap-2">
            <input
              id="tmdb-language"
              type="text"
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              placeholder="en-US"
              className="w-40 bg-canvas border border-border-subtle rounded-xl px-3.5 py-2 text-sm text-text-main font-mono focus:outline-none focus:border-accent"
            />
            <div className="flex flex-wrap gap-1.5">
              {LANGUAGE_PRESETS.map((preset) => (
                <button
                  key={preset}
                  type="button"
                  onClick={() => setLanguage(preset)}
                  className={`px-2.5 py-1 rounded-lg text-xs font-mono border transition-colors cursor-pointer ${
                    language === preset
                      ? 'bg-accent/20 text-accent border-accent/40 font-semibold'
                      : 'bg-canvas text-muted hover:text-text-main border-border-subtle'
                  }`}
                >
                  {preset}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Auto-fetch on Scan */}
        <div className="pt-4 border-t border-border-subtle">
          <label className="flex items-start justify-between gap-4 cursor-pointer">
            <div>
              <span className="text-sm font-semibold text-text-main block">
                Auto-fetch on Library Scan
              </span>
              <span className="text-xs text-muted block mt-0.5">
                Automatically match newly ingested items lacking TMDb metadata when TMDb is enabled.
              </span>
            </div>
            <input
              type="checkbox"
              aria-label="Auto-fetch on Library Scan"
              checked={autoFetchOnScan}
              onChange={(e) => setAutoFetchOnScan(e.target.checked)}
              className="mt-1 h-4 w-4 rounded border-border-subtle text-accent focus:ring-accent"
            />
          </label>
        </div>

        <div className="flex justify-end pt-4 border-t border-border-subtle">
          <button
            type="submit"
            disabled={saving}
            className="inline-flex items-center gap-2 px-5 py-2.5 bg-cta hover:bg-cta-hover text-white font-semibold rounded-xl text-sm shadow-md disabled:opacity-50 transition-colors cursor-pointer"
          >
            {saving ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                Saving...
              </>
            ) : (
              <>
                <Save className="w-4 h-4" />
                Save Metadata Settings
              </>
            )}
          </button>
        </div>
      </form>
    </div>
  );
};
