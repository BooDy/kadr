import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AdminDashboard } from './AdminDashboard';
import { api } from '../../api/client';
import type { Library, SystemConfig, User } from '../../types';

describe('AdminDashboard Component', () => {
  const mockLibraries: Library[] = [
    {
      id: 'lib-1',
      name: 'Featured Movies',
      path: '/media/movies',
      media_type: 'Movie',
      created_at: 1700000000,
    },
    {
      id: 'lib-2',
      name: 'TV Series',
      path: '/media/shows',
      media_type: 'Episode',
      created_at: 1700000000,
    },
  ];

  const mockConfig: SystemConfig = {
    host: '0.0.0.0',
    port: 8492,
    data_dir: '/var/lib/kadr',
    web_dir: '/usr/share/kadr/web',
    database_path: '/var/lib/kadr/kadr.db',
    max_readers: 4,
    debounce_millis: 500,
    use_ffprobe: true,
  };

  const mockUsers: User[] = [
    { id: 'u1', username: 'admin', role: 'admin' },
    { id: 'u2', username: 'viewer', role: 'standard' },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(api, 'getLibraries').mockResolvedValue(mockLibraries);
    vi.spyOn(api, 'getSystemConfig').mockResolvedValue(mockConfig);
    vi.spyOn(api, 'getProfiles').mockResolvedValue(mockUsers);
    vi.spyOn(api, 'createLibrary').mockImplementation(async (payload) => ({
      id: 'lib-new',
      name: payload.name,
      path: payload.path,
      media_type: payload.media_type,
      created_at: 1700000100,
    }));
    vi.spyOn(api, 'deleteLibrary').mockResolvedValue();
    vi.spyOn(api, 'scanLibrary').mockResolvedValue({
      library_id: 'lib-1',
      files_scanned: 42,
      queued: true,
    });
    vi.spyOn(api, 'updateSystemConfig').mockImplementation(async (payload) => ({
      ...mockConfig,
      ...payload,
      port: payload.port ?? mockConfig.port,
      debounce_millis: payload.debounce_millis ?? mockConfig.debounce_millis,
      use_ffprobe: payload.use_ffprobe ?? mockConfig.use_ffprobe,
    }));
  });

  it('renders admin header and tabs with initial libraries', async () => {
    render(<AdminDashboard />);

    expect(screen.getByText(/Administration & Settings/i)).toBeDefined();

    await waitFor(() => {
      expect(screen.getByText(/Libraries \(2\)/i)).toBeDefined();
      expect(screen.getByText(/Server Configuration/i)).toBeDefined();
      expect(screen.getByText(/User Profiles \(2\)/i)).toBeDefined();
      expect(screen.getByText('Featured Movies')).toBeDefined();
      expect(screen.getByText('/media/movies')).toBeDefined();
      expect(screen.getByText('TV Series')).toBeDefined();
    });
  });

  it('opens Add Library modal and creates a new library', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Featured Movies')).toBeDefined();
    });

    const addBtn = screen.getByRole('button', { name: /Add Library/i });
    fireEvent.click(addBtn);

    expect(screen.getByText(/Add Media Library/i)).toBeDefined();

    const nameInput = screen.getByPlaceholderText(/e\.g\. Movies, TV Shows, Anime/i);
    const pathInput = screen.getByPlaceholderText(/\/var\/lib\/kadr\/media\/movies/i);

    fireEvent.change(nameInput, { target: { value: 'Documentaries' } });
    fireEvent.change(pathInput, { target: { value: '/media/docs' } });

    const submitBtn = screen.getByRole('button', { name: 'Create Library' });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(api.createLibrary).toHaveBeenCalledWith({
        name: 'Documentaries',
        path: '/media/docs',
        media_type: 'Movie',
      });
      expect(screen.getByText('Documentaries')).toBeDefined();
    });
  });

  it('triggers on-demand library scan', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Featured Movies')).toBeDefined();
    });

    const scanButtons = screen.getAllByRole('button', { name: /Scan Now/i });
    fireEvent.click(scanButtons[0]);

    await waitFor(() => {
      expect(api.scanLibrary).toHaveBeenCalledWith('lib-1');
      expect(screen.getByText(/Detected 42 media files/i)).toBeDefined();
    });
  });

  it('switches to Server Configuration tab and displays default port 8492', async () => {
    render(<AdminDashboard />);

    const configTab = screen.getByRole('button', { name: /Server Configuration/i });
    fireEvent.click(configTab);

    await waitFor(() => {
      expect(screen.getByText(/Server & Pipeline Configuration/i)).toBeDefined();
      expect(screen.getByDisplayValue('8492')).toBeDefined();
    });

    const saveBtn = screen.getByRole('button', { name: /Save Configuration/i });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(api.updateSystemConfig).toHaveBeenCalled();
      expect(screen.getByText(/Server configuration updated successfully/i)).toBeDefined();
    });
  });

  it('switches to User Profiles tab and displays existing profiles', async () => {
    render(<AdminDashboard />);

    const usersTab = screen.getByRole('button', { name: /User Profiles/i });
    fireEvent.click(usersTab);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'admin' })).toBeDefined();
      expect(screen.getByRole('heading', { name: 'viewer' })).toBeDefined();
    });
  });
});
