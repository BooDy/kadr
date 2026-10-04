import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
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
      paths: ['/media/movies', '/mnt/nas/movies'],
      media_type: 'Movie',
      is_private: false,
      created_at: 1700000000,
    },
    {
      id: 'lib-2',
      name: 'TV Series',
      path: '/media/shows',
      paths: ['/media/shows'],
      media_type: 'Episode',
      is_private: false,
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
    vi.spyOn(api, 'browseFilesystem').mockResolvedValue({
      current_path: '/media/docs',
      parent_path: '/media',
      directories: [],
      shortcuts: [{ name: 'Media', path: '/media' }],
    });
    vi.spyOn(api, 'addLibraryPath').mockResolvedValue();
    vi.spyOn(api, 'removeLibraryPath').mockResolvedValue();
    vi.spyOn(api, 'createLibrary').mockImplementation(async (payload) => ({
      id: 'lib-new',
      name: payload.name,
      path: payload.paths?.[0] || payload.path || '',
      paths: payload.paths || (payload.path ? [payload.path] : []),
      media_type: payload.media_type,
      is_private: payload.is_private ?? false,
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

  it('renders admin header and tabs with initial libraries and multiple paths', async () => {
    render(<AdminDashboard />);

    expect(screen.getByText(/Administration & Settings/i)).toBeDefined();

    await waitFor(() => {
      expect(screen.getByText(/Libraries \(2\)/i)).toBeDefined();
      expect(screen.getByText(/Server Configuration/i)).toBeDefined();
      expect(screen.getByText(/User Profiles \(2\)/i)).toBeDefined();
      expect(screen.getByText('Featured Movies')).toBeDefined();
      expect(screen.getByText('/media/movies')).toBeDefined();
      expect(screen.getByText('/mnt/nas/movies')).toBeDefined();
      expect(screen.getByText('TV Series')).toBeDefined();
      expect(screen.getByText('/media/shows')).toBeDefined();
    });
  });

  it('renders "+ Add Server Folder" button instead of text input in Add Library modal', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Featured Movies')).toBeDefined();
    });

    const addBtn = screen.getByRole('button', { name: /Add Library/i });
    fireEvent.click(addBtn);

    expect(screen.getByText(/Add Media Library/i)).toBeDefined();
    expect(screen.getByRole('button', { name: /\+ Add Server Folder/i })).toBeDefined();
    expect(screen.queryByPlaceholderText(/\/var\/lib\/kadr\/media\/movies/i)).toBeNull();
  });

  it('opens Add Library modal, adds folder path via picker, and creates library', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Featured Movies')).toBeDefined();
    });

    const addBtn = screen.getByRole('button', { name: /Add Library/i });
    fireEvent.click(addBtn);

    const nameInput = screen.getByPlaceholderText(/e\.g\. Movies, TV Shows, Anime/i);
    fireEvent.change(nameInput, { target: { value: 'Documentaries' } });

    // Click "+ Add Server Folder"
    const addFolderBtn = screen.getByRole('button', { name: /\+ Add Server Folder/i });
    fireEvent.click(addFolderBtn);

    // FolderPickerModal opens
    await waitFor(() => {
      expect(screen.getByText('Select Server Folder')).toBeDefined();
    });

    // Select current folder
    const selectFolderBtn = screen.getByRole('button', { name: /Select This Folder/i });
    fireEvent.click(selectFolderBtn);

    // Selected folder chip appears
    await waitFor(() => {
      expect(screen.getByText('/media/docs')).toBeDefined();
    });

    const submitBtn = screen.getByRole('button', { name: 'Create Library' });
    expect(submitBtn).not.toBeDisabled();
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(api.createLibrary).toHaveBeenCalledWith({
        name: 'Documentaries',
        paths: ['/media/docs'],
        media_type: 'Movie',
        is_private: false,
        pin: undefined,
      });
      expect(screen.getByText('Documentaries')).toBeDefined();
    });
  });

  it('allows adding and removing folder paths in Add Library modal and disables submit with 0 paths', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Featured Movies')).toBeDefined();
    });

    const addBtn = screen.getByRole('button', { name: /Add Library/i });
    fireEvent.click(addBtn);

    const nameInput = screen.getByPlaceholderText(/e\.g\. Movies, TV Shows, Anime/i);
    fireEvent.change(nameInput, { target: { value: 'Anime' } });

    // Submit should be disabled with 0 paths
    const submitBtn = screen.getByRole('button', { name: 'Create Library' });
    expect(submitBtn).toBeDisabled();

    // Add first folder
    fireEvent.click(screen.getByRole('button', { name: /\+ Add Server Folder/i }));
    await waitFor(() => expect(screen.getByText('Select Server Folder')).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /Select This Folder/i }));

    await waitFor(() => {
      expect(screen.getByText('/media/docs')).toBeDefined();
    });
    expect(submitBtn).not.toBeDisabled();

    // Remove folder
    const removeBtn = screen.getByRole('button', { name: /Remove folder \/media\/docs/i });
    fireEvent.click(removeBtn);

    expect(screen.queryByText('/media/docs')).toBeNull();
    expect(submitBtn).toBeDisabled();
  });

  it('toggles Private Library to show 4-digit PIN input and submits with private flag and PIN', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Featured Movies')).toBeDefined();
    });

    const addBtn = screen.getByRole('button', { name: /Add Library/i });
    fireEvent.click(addBtn);

    const nameInput = screen.getByPlaceholderText(/e\.g\. Movies, TV Shows, Anime/i);
    fireEvent.change(nameInput, { target: { value: 'Secret Vault' } });

    // Add folder
    fireEvent.click(screen.getByRole('button', { name: /\+ Add Server Folder/i }));
    await waitFor(() => expect(screen.getByText('Select Server Folder')).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /Select This Folder/i }));

    const privateToggle = screen.getByLabelText(/Private Library/i);
    expect(screen.queryByPlaceholderText(/4-digit PIN/i)).toBeNull();

    fireEvent.click(privateToggle);

    const pinInput = screen.getByPlaceholderText(/4-digit PIN/i);
    expect(pinInput).toBeDefined();

    fireEvent.change(pinInput, { target: { value: '5678' } });

    const submitBtn = screen.getByRole('button', { name: 'Create Library' });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(api.createLibrary).toHaveBeenCalledWith({
        name: 'Secret Vault',
        paths: ['/media/docs'],
        media_type: 'Movie',
        is_private: true,
        pin: '5678',
      });
    });
  });

  it('allows removing an extra path from an existing library card', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('/mnt/nas/movies')).toBeDefined();
    });

    // lib-1 has 2 paths: /media/movies and /mnt/nas/movies
    const removeNasBtn = screen.getByRole('button', { name: /Remove path \/mnt\/nas\/movies/i });
    expect(removeNasBtn).not.toBeDisabled();
    fireEvent.click(removeNasBtn);

    await waitFor(() => {
      expect(api.removeLibraryPath).toHaveBeenCalledWith('lib-1', '/mnt/nas/movies');
    });
  });

  it('disables delete path button with tooltip when library only has 1 path', async () => {
    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('/media/shows')).toBeDefined();
    });

    // lib-2 has only 1 path: /media/shows
    const removeShowsBtn = screen.getByRole('button', { name: /Remove path \/media\/shows/i });
    expect(removeShowsBtn).toBeDisabled();
    expect(removeShowsBtn).toHaveAttribute('title', 'At least one path is required');
  });

  it('allows adding an extra path to an existing library card via inline "+ Add Folder"', async () => {
    vi.spyOn(api, 'browseFilesystem').mockResolvedValueOnce({
      current_path: '/mnt/nas/shows',
      parent_path: '/mnt/nas',
      directories: [],
      shortcuts: [],
    });

    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('TV Series')).toBeDefined();
    });

    const addFolderBtns = screen.getAllByRole('button', { name: /\+ Add Folder/i });
    // Click Add Folder on TV Series card
    fireEvent.click(addFolderBtns[1]);

    await waitFor(() => {
      expect(screen.getByText('Select Server Folder')).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: /Select This Folder/i }));

    await waitFor(() => {
      expect(api.addLibraryPath).toHaveBeenCalledWith('lib-2', '/mnt/nas/shows');
    });
  });

  it('renders amber Private badge with Lock icon on private library cards', async () => {
    vi.spyOn(api, 'getLibraries').mockResolvedValueOnce([
      ...mockLibraries,
      {
        id: 'lib-priv',
        name: 'Private Collection',
        path: '/media/private',
        paths: ['/media/private'],
        media_type: 'Movie',
        is_private: true,
        created_at: 1700000000,
      },
    ]);

    render(<AdminDashboard />);

    await waitFor(() => {
      expect(screen.getByText('Private Collection')).toBeDefined();
      expect(screen.getByText('Private')).toBeDefined();
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
