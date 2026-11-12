import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AddToPlaylistModal } from './AddToPlaylistModal';
import { api } from '../../api/client';
import type { PlaylistSummary } from '../../types';

const existingPlaylist: PlaylistSummary = {
  id: 'pl-1',
  name: 'Watchlist',
  owner_user_id: 'u-1',
  owner_username: 'alex',
  is_public: false,
  is_editable: true,
  visible_item_count: 3,
  locked_item_count: 0,
  total_duration_seconds: 9000,
  cover_poster_urls: [],
  created_at: 100,
  updated_at: 200,
  contains_queried_item: true,
};

const secondPlaylist: PlaylistSummary = {
  ...existingPlaylist,
  id: 'pl-2',
  name: 'Weekend Queue',
  contains_queried_item: false,
};

describe('AddToPlaylistModal', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('displays membership checkmarks, toggles single item membership, and supports 1-step Create & Add', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({
      playlists: [existingPlaylist, secondPlaylist],
    });
    const addSpy = vi.spyOn(api, 'addPlaylistItems').mockResolvedValue({
      added_count: 1,
      skipped_duplicate_count: 0,
      summary: secondPlaylist,
    });
    const removeSpy = vi.spyOn(api, 'removePlaylistItem').mockResolvedValue(existingPlaylist);
    const createSpy = vi.spyOn(api, 'createPlaylist').mockResolvedValue({
      summary: secondPlaylist,
      items: [],
    });

    render(
      <AddToPlaylistModal
        isOpen={true}
        target={{ kind: 'item', itemId: 42, title: 'Dune' }}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(api.getPlaylists).toHaveBeenCalledWith(42);
      expect(screen.getByText('Watchlist')).toBeInTheDocument();
      expect(screen.getByText('Weekend Queue')).toBeInTheDocument();
    });

    // Click unchecked playlist -> adds item
    fireEvent.click(screen.getByRole('button', { name: /Weekend Queue/i }));
    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith('pl-2', { item_ids: [42] });
    });

    // Click already-checked playlist -> removes item
    fireEvent.click(screen.getByRole('button', { name: /Watchlist/i }));
    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith('pl-1', 42);
    });

    // 1-step inline Create & Add
    fireEvent.change(screen.getByPlaceholderText(/New playlist name/i), {
      target: { value: 'Dune Double Feature' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Create & Add/i }));
    await waitFor(() => {
      expect(createSpy).toHaveBeenCalledWith({
        name: 'Dune Double Feature',
        is_public: false,
        items: { item_ids: [42] },
      });
    });
  });

  it('supports season and folder batch targets with recursive checkbox', async () => {
    vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [secondPlaylist] });
    const addSpy = vi.spyOn(api, 'addPlaylistItems').mockResolvedValue({
      added_count: 6,
      skipped_duplicate_count: 0,
      summary: secondPlaylist,
    });

    const { rerender } = render(
      <AddToPlaylistModal
        isOpen={true}
        target={{ kind: 'season', showId: 77, seasonNumber: 2, title: 'Severance — Season 2' }}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => expect(screen.getByText('Weekend Queue')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /Weekend Queue/i }));
    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith('pl-2', {
        show: { show_id: 77, season_number: 2 },
      });
    });

    rerender(
      <AddToPlaylistModal
        isOpen={true}
        target={{
          kind: 'folder',
          libraryId: 'lib-1',
          path: 'Sci-Fi/Collection',
          recursive: true,
          title: 'Collection',
        }}
        onClose={vi.fn()}
      />
    );

    const recursiveToggle = await screen.findByLabelText(/Include subfolders/i);
    expect(recursiveToggle).toBeChecked();
    fireEvent.click(recursiveToggle);

    fireEvent.click(screen.getByRole('button', { name: /Weekend Queue/i }));
    await waitFor(() => {
      expect(addSpy).toHaveBeenLastCalledWith('pl-2', {
        folder: { library_id: 'lib-1', path: 'Sci-Fi/Collection', recursive: false },
      });
    });
  });
});
