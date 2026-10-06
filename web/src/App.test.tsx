import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import App from './App';
import { api } from './api/client';

describe('App Shell', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('renders KADR brand and streamlined navigation items without legacy tabs', async () => {
    api.setUser({ id: 'user-1', username: 'testuser', role: 'standard' });
    api.setToken('test-token');

    const mockLibraries = [
      {
        id: 'lib-1',
        name: '4K Cinema',
        path: '/media/cinema',
        media_type: 'movie',
        is_private: false,
        created_at: 1700000000,
      },
      {
        id: 'lib-2',
        name: 'Anime Hub',
        path: '/media/anime',
        media_type: 'anime',
        is_private: false,
        created_at: 1700000000,
      },
    ];
    vi.spyOn(api, 'getLibraries').mockResolvedValue(mockLibraries as any);

    render(<App />);
    expect(screen.getByText('KADR')).toBeDefined();
    expect(screen.getByRole('button', { name: /home/i })).toBeDefined();

    // Verify active libraries are rendered in navigation
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /4K Cinema/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Anime Hub/i })).toBeDefined();
    });

    // Verify legacy hardcoded tabs are absent from navigation
    expect(screen.queryByRole('button', { name: /^movies$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^shows$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^studio$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^telemetry$/i })).toBeNull();
  });

  it('shows ProfileSelect when no user is logged in', async () => {
    render(<App />);
    expect(screen.getByText(/Who's watching\?/i)).toBeDefined();
    await waitFor(() => expect(screen.getByText('admin')).toBeDefined());
  });

  it('navigates between views when user is authenticated', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    const mockLibraries = [
      {
        id: 'lib-movies',
        name: 'Action Movies',
        path: '/media/action',
        media_type: 'movie',
        is_private: false,
        created_at: 1700000000,
      },
    ];
    vi.spyOn(api, 'getLibraries').mockResolvedValue(mockLibraries as any);
    vi.spyOn(api, 'getSystemConfig').mockResolvedValue({
      name: 'Kadr Media Server',
      host: '0.0.0.0',
      port: 8080,
      data_dir: '/data',
      database_path: '/data/kadr.db',
      max_readers: 4,
      debounce_millis: 500,
      use_ffprobe: true,
    });
    vi.spyOn(api, 'getProfiles').mockResolvedValue([
      { id: 'admin-1', username: 'admin', role: 'admin' },
    ]);

    vi.spyOn(api, 'getScreen').mockImplementation(async (screenId: string) => {
      if (screenId === 'home') {
        return {
          id: 'home',
          title: 'Home',
          widgets: [
            {
              type: 'carousel',
              id: 'cw',
              title: 'Continue Watching',
              binding: { macro_type: 'continue_watching', limit: 10 },
              items: [{ id: 1, title: 'Movie 1', media_type: 'movie' }],
            },
          ],
        };
      }
      if (screenId === 'lib-movies') {
        return {
          id: 'lib-movies',
          title: 'Action Movies',
          widgets: [
            {
              type: 'grid',
              id: 'all_action',
              title: 'All Action Movies',
              columns: 6,
              binding: { macro_type: 'top_rated', limit: 20 },
              items: [{ id: 2, title: 'Movie 2', media_type: 'movie' }],
            },
          ],
        };
      }
      throw new Error('Not found: ' + screenId);
    });

    render(<App />);
    await waitFor(() => expect(screen.getByText('Continue Watching')).toBeDefined());

    // Dynamic library button is present and navigates to the library screen
    await waitFor(() => expect(screen.getByRole('button', { name: /Action Movies/i })).toBeDefined());
    fireEvent.click(screen.getByRole('button', { name: /Action Movies/i }));
    await waitFor(() => expect(screen.getByText('All Action Movies')).toBeDefined());

    // Admin button is present for admin users and navigates to Admin view
    const adminBtn = screen.getByRole('button', { name: /^admin$/i });
    expect(adminBtn).toBeDefined();
    fireEvent.click(adminBtn);
    await waitFor(() => expect(screen.getByText(/Administration & Settings/i)).toBeDefined());

    // Navigate back to Home
    fireEvent.click(screen.getByRole('button', { name: /home/i }));
    await waitFor(() => expect(screen.getByText('Continue Watching')).toBeDefined());

    // Confirm Movies, Shows, Studio, and Telemetry buttons do not exist in top nav
    expect(screen.queryByRole('button', { name: /^movies$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^shows$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^studio$/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /^telemetry$/i })).toBeNull();
  });

  it('renders dynamic media type icons for libraries and conditionally displays Admin tab', async () => {
    api.setUser({ id: 'user-standard', username: 'viewer', role: 'standard' });
    api.setToken('test-token');

    const diverseLibraries = [
      { id: 'lib-film', name: 'Cinema', path: '/p1', media_type: 'movie', is_private: false, created_at: 100 },
      { id: 'lib-tv', name: 'Series', path: '/p2', media_type: 'show', is_private: false, created_at: 100 },
      { id: 'lib-anime', name: 'Anime Zone', path: '/p3', media_type: 'anime', is_private: false, created_at: 100 },
      { id: 'lib-music', name: 'Tunes', path: '/p4', media_type: 'music', is_private: false, created_at: 100 },
      { id: 'lib-videos', name: 'Home Clips', path: '/p5', media_type: 'home_videos', is_private: false, created_at: 100 },
      { id: 'lib-audiobooks', name: 'Stories', path: '/p6', media_type: 'audiobook', is_private: false, created_at: 100 },
      { id: 'lib-other', name: 'Documents', path: '/p7', media_type: 'other', is_private: false, created_at: 100 },
    ];
    vi.spyOn(api, 'getLibraries').mockResolvedValue(diverseLibraries as any);
    vi.spyOn(api, 'getScreen').mockResolvedValue({ id: 'home', title: 'Home', widgets: [] });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Cinema/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Series/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Anime Zone/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Tunes/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Home Clips/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Stories/i })).toBeDefined();
      expect(screen.getByRole('button', { name: /Documents/i })).toBeDefined();
    });

    // For standard user, Admin button should not exist
    expect(screen.queryByRole('button', { name: /^admin$/i })).toBeNull();
  });

  it('navigates to player placeholder when onPlayItem is triggered', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    vi.spyOn(api, 'getScreen').mockResolvedValue({
      id: 'home',
      title: 'Home',
      widgets: [
        {
          type: 'hero_banner',
          id: 'spotlight',
          binding: { macro_type: 'spotlight_item', limit: 1 },
          data: {
            id: 999,
            title: 'Sample Film',
            media_type: 'movie',
          },
        },
      ],
    });

    render(<App />);
    await waitFor(() => expect(screen.getByText('Sample Film')).toBeInTheDocument());

    fireEvent.click(screen.getByRole('button', { name: /play now/i }));

    await waitFor(() => {
      expect(screen.getByText(/Cinema Player/i)).toBeInTheDocument();
      expect(screen.getByText(/Now playing item #999/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /back to browse/i }));
    await waitFor(() => {
      expect(screen.queryByText(/Cinema Player/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/Now playing item #999/i)).not.toBeInTheDocument();
      expect(screen.getByText('Sample Film')).toBeInTheDocument();
    });
  });

  it('shows active user avatar in header with Switch Profile menu and opens switch profile modal', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    render(<App />);
    expect(screen.getByText('admin')).toBeDefined();
    expect(screen.getByText('ADMIN')).toBeDefined();

    const switchBtn = screen.getByTitle('Switch Profile');
    expect(switchBtn).toBeDefined();

    fireEvent.click(switchBtn);

    // Profile selector should be opened in modal with active user indicator
    await waitFor(() => {
      expect(screen.getByText('Active')).toBeDefined();
    });

    // Close/Cancel switch profile modal
    const cancelBtn = screen.getByRole('button', { name: /cancel/i });
    fireEvent.click(cancelBtn);
    expect(screen.queryByText('Active')).toBeNull();
  });

  it('logs out when sign out is clicked and returns to ProfileSelect', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    render(<App />);
    expect(screen.getByText('admin')).toBeDefined();

    const signOutBtn = screen.getByTitle('Sign out');
    fireEvent.click(signOutBtn);

    expect(api.getToken()).toBeNull();
    expect(api.getUser()).toBeNull();

    // Now unauthenticated: ProfileSelect is shown
    await waitFor(() => expect(screen.getByText(/Who's watching\?/i)).toBeDefined());
  });

  it('triggers PIN keypad prompt when clicking a locked private library', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    const mockLibraries = [
      {
        id: 'lib-vault',
        name: 'Private Vault',
        path: '/media/vault',
        media_type: 'Movie' as const,
        is_private: true,
        created_at: 1700000000,
      },
    ];

    vi.spyOn(api, 'getLibraries').mockResolvedValue(mockLibraries);
    vi.spyOn(api, 'getScreen').mockResolvedValue({
      id: 'home',
      title: 'Home',
      widgets: [],
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Private Vault/i })).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: /Private Vault/i }));

    await waitFor(() => {
      expect(screen.getByText(/Unlock Private Vault/i)).toBeDefined();
    });
  });

  it('unlocks private library with PIN, saves token to session storage, and displays library contents', async () => {
    sessionStorage.clear();
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    const mockLibraries = [
      {
        id: 'lib-vault',
        name: 'Private Vault',
        path: '/media/vault',
        media_type: 'Movie' as const,
        is_private: true,
        created_at: 1700000000,
      },
    ];

    vi.spyOn(api, 'getLibraries').mockResolvedValue(mockLibraries);
    vi.spyOn(api, 'unlockLibrary').mockResolvedValue({
      library_id: 'lib-vault',
      token: 'vault-unlock-token-123',
      expires_at: 1800000000,
    });
    vi.spyOn(api, 'getScreen').mockImplementation(async (screenId) => {
      if (screenId === 'lib-vault') {
        return {
          id: 'lib-vault',
          title: 'Vault Movies',
          widgets: [
            {
              type: 'carousel',
              id: 'vault_cw',
              title: 'Vault Items',
              binding: { macro_type: 'recently_added', limit: 10 },
              items: [{ id: 501, title: 'Secret Film', media_type: 'movie' }],
            },
          ],
        };
      }
      return { id: 'home', title: 'Home', widgets: [] };
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Private Vault/i })).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: /Private Vault/i }));

    await waitFor(() => {
      expect(screen.getByText(/Unlock Private Vault/i)).toBeDefined();
    });

    // Enter 4 digits (1, 2, 3, 4)
    fireEvent.click(screen.getByRole('button', { name: '1' }));
    fireEvent.click(screen.getByRole('button', { name: '2' }));
    fireEvent.click(screen.getByRole('button', { name: '3' }));
    fireEvent.click(screen.getByRole('button', { name: '4' }));

    await waitFor(() => {
      expect(api.unlockLibrary).toHaveBeenCalledWith('lib-vault', '1234');
      expect(screen.getByText('Vault Items')).toBeDefined();
    });

    // Verify sessionStorage has saved token under kadr_unlocked_libraries
    const stored = sessionStorage.getItem('kadr_unlocked_libraries');
    expect(stored).toBeTruthy();
    expect(stored).toContain('vault-unlock-token-123');
  });
});

