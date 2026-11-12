import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PlaylistsScreen } from './PlaylistsScreen';
import { api } from '../../api/client';
import type { PlaylistDetail, PlaylistSummary, User } from '../../types';

const currentUser: User = { id: 'u-1', username: 'alex', role: 'standard' };

const mySummary: PlaylistSummary = {
  id: 'pl-mine',
  name: 'Sci-Fi Marathon',
  description: 'Director cuts',
  owner_user_id: 'u-1',
  owner_username: 'alex',
  is_public: false,
  is_editable: true,
  visible_item_count: 2,
  locked_item_count: 1,
  total_duration_seconds: 12600,
  cover_poster_urls: [
    '/api/v1/artwork/1/poster',
    '/api/v1/artwork/2/poster',
    '/api/v1/artwork/3/poster',
    '/api/v1/artwork/4/poster',
  ],
  created_at: 1000,
  updated_at: 2000,
};

const sharedSummary: PlaylistSummary = {
  id: 'pl-shared',
  name: 'Family Favorites',
  owner_user_id: 'u-2',
  owner_username: 'sam',
  is_public: true,
  is_editable: false,
  visible_item_count: 1,
  locked_item_count: 0,
  total_duration_seconds: 5400,
  cover_poster_urls: ['/api/v1/artwork/9/poster'],
  created_at: 1000,
  updated_at: 1900,
};

const detailPayload: PlaylistDetail = {
  summary: mySummary,
  items: [
    {
      position: 0,
      added_at: 1100,
      duration_seconds: 7000,
      playback_progress: undefined,
      card: {
        id: 10,
        title: 'Arrival',
        subtitle: '2016 • 4K',
        media_type: 'movie',
        playback_progress: 0.25,
      },
    } as PlaylistDetail['items'][number],
    {
      position: 1,
      added_at: 1200,
      duration_seconds: 5600,
      card: { id: 14, title: 'Blade Runner 2049', subtitle: '2017 • 4K', media_type: 'movie' },
    },
  ],
};

describe('PlaylistsScreen & PlaylistDetailView', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders My Playlists and Shared on Server sections, 2x2 mosaic, locked badge, and creates playlist', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [mySummary, sharedSummary] });
    const createSpy = vi.spyOn(api, 'createPlaylist').mockResolvedValue(detailPayload);

    render(
      <PlaylistsScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('My Playlists')).toBeInTheDocument();
      expect(screen.getByText('Shared on Server')).toBeInTheDocument();
      expect(screen.getByText('Sci-Fi Marathon')).toBeInTheDocument();
      expect(screen.getByText('Family Favorites')).toBeInTheDocument();
      expect(screen.getByText(/\+1 locked/i)).toBeInTheDocument();
      expect(screen.getByText(/by @sam/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /\+ New Playlist/i }));
    fireEvent.change(screen.getByLabelText(/Playlist Name/i), { target: { value: 'Late Night' } });
    fireEvent.click(screen.getByRole('button', { name: /^Create Playlist$/i }));

    await waitFor(() => {
      expect(createSpy).toHaveBeenCalledWith(
        expect.objectContaining({ name: 'Late Night', is_public: false })
      );
    });
  });

  it('opens detail view and supports Play All, Shuffle, Move Down/Up reorder, drag-and-drop, edit, delete, and item removal', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [mySummary] });
    vi.spyOn(api, 'getPlaylist').mockResolvedValue(detailPayload);
    const reorderSpy = vi.spyOn(api, 'reorderPlaylistItems').mockResolvedValue(mySummary);
    const updateSpy = vi.spyOn(api, 'updatePlaylist').mockResolvedValue({
      ...mySummary,
      name: 'Sci-Fi Updated',
      is_public: true,
    });
    const deleteSpy = vi.spyOn(api, 'deletePlaylist').mockResolvedValue();
    const removeSpy = vi.spyOn(api, 'removePlaylistItem').mockResolvedValue({
      ...mySummary,
      visible_item_count: 1,
    });
    const onPlayQueue = vi.fn();

    render(
      <PlaylistsScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={onPlayQueue}
      />
    );

    await waitFor(() => expect(screen.getByText('Sci-Fi Marathon')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Sci-Fi Marathon'));

    await waitFor(() => {
      expect(screen.getByText('Arrival')).toBeInTheDocument();
      expect(screen.getByText('Blade Runner 2049')).toBeInTheDocument();
      expect(screen.getByText(/1 item\(s\) from locked private libraries are hidden/i)).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /Play All/i }));
    expect(onPlayQueue).toHaveBeenCalledWith(detailPayload.items, 0);

    fireEvent.click(screen.getByRole('button', { name: /Shuffle/i }));
    expect(onPlayQueue).toHaveBeenCalledTimes(2);
    expect(onPlayQueue.mock.calls[1][0]).toHaveLength(2);
    expect(onPlayQueue.mock.calls[1][1]).toBe(0);

    // Move Arrival down -> [14, 10]
    fireEvent.click(screen.getByRole('button', { name: /Move Arrival down/i }));
    await waitFor(() => {
      expect(reorderSpy).toHaveBeenCalledWith('pl-mine', [14, 10]);
    });

    // Move Arrival back up -> [10, 14]
    fireEvent.click(screen.getByRole('button', { name: /Move Arrival up/i }));
    await waitFor(() => {
      expect(reorderSpy).toHaveBeenCalledWith('pl-mine', [10, 14]);
    });

    // Edit playlist metadata
    fireEvent.click(screen.getByRole('button', { name: /^Edit$/i }));
    fireEvent.change(screen.getByLabelText(/Playlist Name/i), {
      target: { value: 'Sci-Fi Updated' },
    });
    fireEvent.click(screen.getByLabelText(/Share with everyone on server/i));
    fireEvent.click(screen.getByRole('button', { name: /Save Changes/i }));

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith(
        'pl-mine',
        expect.objectContaining({ name: 'Sci-Fi Updated', is_public: true })
      );
      expect(screen.getByText('Sci-Fi Updated')).toBeInTheDocument();
    });

    // Remove item
    fireEvent.click(screen.getByRole('button', { name: /Remove Arrival from playlist/i }));
    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith('pl-mine', 10);
    });

    // Delete playlist with confirmation
    fireEvent.click(screen.getByRole('button', { name: /^Delete$/i }));
    fireEvent.click(screen.getByRole('button', { name: /Confirm Delete/i }));
    await waitFor(() => {
      expect(deleteSpy).toHaveBeenCalledWith('pl-mine');
    });
  });
});
