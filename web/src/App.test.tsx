import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import App from './App';
import { api } from './api/client';

describe('App Shell', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('renders KADR brand and navigation items', async () => {
    render(<App />);
    expect(screen.getByText('KADR')).toBeDefined();
    expect(screen.getByRole('button', { name: /home/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /movies/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /shows/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /studio/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /telemetry/i })).toBeDefined();
    await waitFor(() => expect(screen.getByText('admin')).toBeDefined());
  });

  it('shows ProfileSelect when no user is logged in', async () => {
    render(<App />);
    expect(screen.getByText(/Who's watching\?/i)).toBeDefined();
    await waitFor(() => expect(screen.getByText('admin')).toBeDefined());
  });

  it('navigates between views when user is authenticated', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

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
      if (screenId === 'movies') {
        return {
          id: 'movies',
          title: 'Movies',
          widgets: [
            {
              type: 'grid',
              id: 'all_movies',
              title: 'All Movies',
              columns: 6,
              binding: { macro_type: 'top_rated', limit: 20 },
              items: [{ id: 2, title: 'Movie 2', media_type: 'movie' }],
            },
          ],
        };
      }
      if (screenId === 'shows') {
        return {
          id: 'shows',
          title: 'TV Shows',
          widgets: [
            {
              type: 'grid',
              id: 'all_shows',
              title: 'All TV Shows',
              columns: 6,
              binding: { macro_type: 'top_rated', limit: 20 },
              items: [{ id: 3, title: 'Show 1', media_type: 'show' }],
            },
          ],
        };
      }
      throw new Error('Not found');
    });

    render(<App />);
    await waitFor(() => expect(screen.getByText('Continue Watching')).toBeDefined());

    fireEvent.click(screen.getByRole('button', { name: /movies/i }));
    await waitFor(() => expect(screen.getByText('All Movies')).toBeDefined());

    fireEvent.click(screen.getByRole('button', { name: /shows/i }));
    await waitFor(() => expect(screen.getByText('All TV Shows')).toBeDefined());

    fireEvent.click(screen.getByRole('button', { name: /studio/i }));
    expect(screen.getByText('Layout Studio')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /telemetry/i }));
    expect(screen.getByText('System Telemetry')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /home/i }));
    await waitFor(() => expect(screen.getByText('Continue Watching')).toBeDefined());
  });

  it('navigates to player placeholder when onPlayItem is triggered', async () => {
    api.setUser({ id: 'admin-1', username: 'admin', role: 'admin' });
    api.setToken('test-jwt');

    vi.spyOn(api, 'getScreen').mockResolvedValueOnce({
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
    await waitFor(() => expect(screen.getByText('Sample Film')).toBeDefined());

    fireEvent.click(screen.getByRole('button', { name: /play now/i }));

    await waitFor(() => {
      expect(screen.getByText(/Now playing item #999/i)).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: /back to browse/i }));
    expect(screen.queryByText(/Now playing item #999/i)).toBeNull();
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
});
