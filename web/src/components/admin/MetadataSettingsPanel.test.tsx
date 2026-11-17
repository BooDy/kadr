import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { MetadataSettingsPanel } from './MetadataSettingsPanel';
import { AdminDashboard } from './AdminDashboard';
import { api } from '../../api/client';

describe('MetadataSettingsPanel & AdminDashboard TMDb Integration', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('loads TMDb settings, selects language preset, and saves API key and toggles', async () => {
    vi.spyOn(api, 'getMetadataSettings').mockResolvedValue({
      enabled: false,
      auto_fetch_on_scan: false,
      language: 'en-US',
      api_key_configured: false,
      api_key_hint: null,
    });

    const updateSpy = vi.spyOn(api, 'updateMetadataSettings').mockResolvedValue({
      enabled: true,
      auto_fetch_on_scan: true,
      language: 'ar-EG',
      api_key_configured: true,
      api_key_hint: '••••cdef',
    });

    const onSettingsChange = vi.fn();
    render(<MetadataSettingsPanel onSettingsChange={onSettingsChange} />);

    await waitFor(() => {
      expect(screen.getByText(/Metadata & TMDb Scraper/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByLabelText('Enable TMDb Integration'));
    fireEvent.click(screen.getByLabelText('Auto-fetch on Library Scan'));
    fireEvent.click(screen.getByRole('button', { name: 'ar-EG' }));
    fireEvent.change(screen.getByLabelText(/TMDb API Key or v4 Read Access Token/i), {
      target: { value: '0123456789abcdef0123456789abcdef' },
    });

    fireEvent.click(screen.getByRole('button', { name: /Save Metadata Settings/i }));

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith({
        enabled: true,
        auto_fetch_on_scan: true,
        language: 'ar-EG',
        api_key: '0123456789abcdef0123456789abcdef',
      });
      expect(screen.getByText(/Configured \(••••cdef\)/i)).toBeInTheDocument();
      expect(onSettingsChange).toHaveBeenCalled();
    });
  });

  it('displays "Fetch Missing Metadata" button on libraries in AdminDashboard when TMDb is enabled', async () => {
    vi.spyOn(api, 'getLibraries').mockResolvedValue([
      {
        id: 'lib-1',
        name: 'Cinema Vault',
        path: '/media/movies',
        paths: ['/media/movies'],
        media_type: 'movie',
        is_private: false,
        created_at: 1700000000,
      },
    ]);
    vi.spyOn(api, 'getSystemConfig').mockResolvedValue({
      name: 'Kadr',
      host: '0.0.0.0',
      port: 8492,
      data_dir: '/var/lib/kadr',
      database_path: '/var/lib/kadr/kadr.db',
      max_readers: 4,
      debounce_millis: 500,
      use_ffprobe: true,
    });
    vi.spyOn(api, 'getProfiles').mockResolvedValue([]);
    vi.spyOn(api, 'getMetadataSettings').mockResolvedValue({
      enabled: true,
      auto_fetch_on_scan: false,
      language: 'en-US',
      api_key_configured: true,
      api_key_hint: '••••9999',
    });
    const enrichSpy = vi.spyOn(api, 'enrichLibraryTmdb').mockResolvedValue({
      scanned: 10,
      matched: 8,
      skipped: 2,
      errors: 0,
    });

    render(<AdminDashboard />);

    const enrichBtn = await screen.findByRole('button', {
      name: /Fetch Missing Metadata/i,
    });
    fireEvent.click(enrichBtn);

    await waitFor(() => {
      expect(enrichSpy).toHaveBeenCalledWith('lib-1');
      expect(screen.getByText(/matched 8 of 10 items/i)).toBeInTheDocument();
    });
  });
});
