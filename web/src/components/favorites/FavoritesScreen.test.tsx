import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { FavoritesScreen } from './FavoritesScreen';
import { api } from '../../api/client';
import type { FavoriteItemView, FavoritesListResponse, PlaylistItemView, User } from '../../types';
import { emitWatchStateChanged } from '../../utils/watchStateEvents';

const currentUser: User = { id: 'u-1', username: 'alex', role: 'standard' };

const movieItem: FavoriteItemView = {
  favorited_at: 1700001000,
  duration_seconds: 8880,
  resume_position_seconds: 1200,
  card: {
    id: 101,
    title: 'Arrival',
    subtitle: '2016 • 4K',
    media_type: 'movie',
    release_year: 2016,
    playback_progress: 0.14,
    badge: 'RESUME',
    is_favorite: true,
  },
};

const showItem: FavoriteItemView = {
  favorited_at: 1700002000,
  duration_seconds: 0,
  card: {
    id: 202,
    title: 'Severance',
    subtitle: '2 Seasons',
    media_type: 'show',
    release_year: 2022,
    is_favorite: true,
  },
};

const episodeItem: FavoriteItemView = {
  favorited_at: 1700003000,
  duration_seconds: 3400,
  card: {
    id: 303,
    title: 'Good News About Hell',
    subtitle: 'S01E01 - Severance',
    media_type: 'episode',
    season: 1,
    episode: 1,
    is_favorite: true,
  },
};

const liveItem: FavoriteItemView = {
  favorited_at: 1700004000,
  duration_seconds: 0,
  card: {
    id: 404,
    title: 'News 24 HD',
    subtitle: '1080p',
    media_type: 'live_channel',
    is_favorite: true,
  },
};

describe('FavoritesScreen', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders cinema empty state and disables Play All / Shuffle when items is empty', async () => {
    const emptyResponse: FavoritesListResponse = {
      items: [],
      total_count: 0,
      locked_item_count: 0,
    };
    vi.spyOn(api, 'getFavorites').mockResolvedValue(emptyResponse);

    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: /My Favorites/i })).toBeInTheDocument();
      expect(screen.getByText(/No favorites yet/i)).toBeInTheDocument();
    });

    expect(screen.getByRole('button', { name: /Play All/i })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Shuffle/i })).toBeDisabled();
    expect(screen.queryByText(/in locked libraries/i)).not.toBeInTheDocument();
  });

  it('triggers api.getFavorites when clicking type filter pills and changing Sort select', async () => {
    const getFavoritesSpy = vi.spyOn(api, 'getFavorites').mockResolvedValue({
      items: [movieItem],
      total_count: 1,
      locked_item_count: 0,
    });

    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Arrival')).toBeInTheDocument();
    });

    expect(getFavoritesSpy).toHaveBeenLastCalledWith({
      sort: 'favorited_at:desc',
      limit: 60,
      offset: 0,
    });

    // Verify all 6 filter pills render with aria-pressed
    const allPill = screen.getByRole('button', { name: /^All$/i });
    const moviesPill = screen.getByRole('button', { name: /^Movies$/i });
    const showsPill = screen.getByRole('button', { name: /^TV Shows$/i });
    const episodesPill = screen.getByRole('button', { name: /^Episodes$/i });
    const homeVideosPill = screen.getByRole('button', { name: /^Home Videos$/i });
    const liveChannelsPill = screen.getByRole('button', { name: /^Live Channels$/i });

    expect(allPill).toHaveAttribute('aria-pressed', 'true');
    expect(moviesPill).toHaveAttribute('aria-pressed', 'false');
    expect(showsPill).toHaveAttribute('aria-pressed', 'false');
    expect(episodesPill).toHaveAttribute('aria-pressed', 'false');
    expect(homeVideosPill).toHaveAttribute('aria-pressed', 'false');
    expect(liveChannelsPill).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(moviesPill);
    await waitFor(() => {
      expect(moviesPill).toHaveAttribute('aria-pressed', 'true');
      expect(allPill).toHaveAttribute('aria-pressed', 'false');
      expect(getFavoritesSpy).toHaveBeenLastCalledWith({
        item_type: 'movie',
        sort: 'favorited_at:desc',
        limit: 60,
        offset: 0,
      });
    });

    fireEvent.click(liveChannelsPill);
    await waitFor(() => {
      expect(liveChannelsPill).toHaveAttribute('aria-pressed', 'true');
      expect(getFavoritesSpy).toHaveBeenLastCalledWith({
        item_type: 'live_channel',
        sort: 'favorited_at:desc',
        limit: 60,
        offset: 0,
      });
    });

    const sortSelect = screen.getByLabelText(/^Sort$/i) as HTMLSelectElement;
    fireEvent.change(sortSelect, { target: { value: 'rating:desc' } });
    await waitFor(() => {
      expect(getFavoritesSpy).toHaveBeenLastCalledWith({
        item_type: 'live_channel',
        sort: 'rating:desc',
        limit: 60,
        offset: 0,
      });
    });
  });

  it('displays locked private libraries notice badge when locked_item_count > 0', async () => {
    vi.spyOn(api, 'getFavorites').mockResolvedValue({
      items: [movieItem],
      total_count: 1,
      locked_item_count: 2,
    });

    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('2 in locked libraries')).toBeInTheDocument();
    });
  });

  it('removes unfavorited card immediately, decrements visible count, and reacts to watch state updates', async () => {
    vi.spyOn(api, 'getFavorites').mockResolvedValue({
      items: [movieItem, episodeItem],
      total_count: 2,
      locked_item_count: 0,
    });
    const removeSpy = vi.spyOn(api, 'removeFavorite').mockResolvedValue({
      item_id: 101,
      is_favorite: false,
    });
    const detailsSpy = vi.spyOn(api, 'getItemDetails');

    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Arrival')).toBeInTheDocument();
      expect(screen.getByText('Good News About Hell')).toBeInTheDocument();
      expect(screen.getByTestId('favorites-total-count')).toHaveTextContent('2');
    });

    // Click heart button on Arrival -> removes Arrival & decrements count without opening ItemDetailsModal
    fireEvent.click(screen.getByRole('button', { name: 'Remove Arrival from favorites' }));

    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith(101);
      expect(screen.queryByText('Arrival')).not.toBeInTheDocument();
      expect(screen.getByTestId('favorites-total-count')).toHaveTextContent('1');
    });
    expect(detailsSpy).not.toHaveBeenCalled();

    // Emit watch state update on remaining episode card
    act(() => {
      emitWatchStateChanged([303], true);
    });
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: 'Mark Good News About Hell as unwatched' })
      ).toHaveAttribute('aria-pressed', 'true');
    });
  });

  it('builds PlaylistItemView[] queue from playable items (excluding shows/seasons) for Play All and Shuffle', async () => {
    vi.spyOn(api, 'getFavorites').mockResolvedValue({
      items: [movieItem, showItem, episodeItem, liveItem],
      total_count: 4,
      locked_item_count: 0,
    });

    const onPlayQueue = vi.fn();
    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={onPlayQueue}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Arrival')).toBeInTheDocument();
      expect(screen.getByText('Severance')).toBeInTheDocument();
    });

    const playAllBtn = screen.getByRole('button', { name: /Play All/i });
    const shuffleBtn = screen.getByRole('button', { name: /Shuffle/i });
    expect(playAllBtn).not.toBeDisabled();
    expect(shuffleBtn).not.toBeDisabled();

    fireEvent.click(playAllBtn);

    expect(onPlayQueue).toHaveBeenCalledTimes(1);
    const expectedQueue: PlaylistItemView[] = [
      {
        position: 0,
        added_at: 1700001000,
        duration_seconds: 8880,
        resume_position_seconds: 1200,
        card: movieItem.card,
      },
      {
        position: 1,
        added_at: 1700003000,
        duration_seconds: 3400,
        card: episodeItem.card,
      },
      {
        position: 2,
        added_at: 1700004000,
        duration_seconds: 0,
        card: liveItem.card,
      },
    ];
    expect(onPlayQueue).toHaveBeenCalledWith(expectedQueue, 0);

    // Force deterministic Math.random = 0 for Fisher-Yates shuffle verification
    const randomSpy = vi.spyOn(Math, 'random').mockReturnValue(0);
    fireEvent.click(shuffleBtn);
    randomSpy.mockRestore();

    expect(onPlayQueue).toHaveBeenCalledTimes(2);
    const [shuffledQueue, startIdx] = onPlayQueue.mock.calls[1] as [PlaylistItemView[], number];
    expect(startIdx).toBe(0);
    expect(shuffledQueue).toHaveLength(3);
    expect(shuffledQueue.map((q) => q.position)).toEqual([0, 1, 2]);
    expect(shuffledQueue.map((q) => q.card.id).sort((a, b) => a - b)).toEqual([101, 303, 404]);
  });

  it('disables Play All and Shuffle when only non-playable TV Show containers are favorited', async () => {
    vi.spyOn(api, 'getFavorites').mockResolvedValue({
      items: [showItem],
      total_count: 1,
      locked_item_count: 0,
    });

    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Severance')).toBeInTheDocument();
    });

    expect(screen.getByRole('button', { name: /Play All/i })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Shuffle/i })).toBeDisabled();
  });

  it('paginates with Load More when total_count exceeds loaded items, deduplicates by card.id, and omits WatchStateButton on season cards', async () => {
    const seasonItem: FavoriteItemView = {
      favorited_at: 1700001500,
      duration_seconds: 0,
      card: {
        id: 205,
        title: 'Severance Season 1',
        subtitle: '9 Episodes',
        media_type: 'season',
        is_favorite: true,
      },
    };

    const getFavoritesSpy = vi
      .spyOn(api, 'getFavorites')
      .mockResolvedValueOnce({
        items: [movieItem, seasonItem],
        total_count: 3,
        locked_item_count: 0,
      })
      .mockResolvedValueOnce({
        items: [movieItem, episodeItem],
        total_count: 3,
        locked_item_count: 1,
      });

    render(
      <FavoritesScreen
        currentUser={currentUser}
        onPlayItem={vi.fn()}
        onPlayQueue={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Arrival')).toBeInTheDocument();
      expect(screen.getByText('Severance Season 1')).toBeInTheDocument();
    });

    // Season cards must not render WatchStateButton
    expect(
      screen.queryByRole('button', { name: /Mark Severance Season 1 as/i })
    ).not.toBeInTheDocument();

    const loadMoreBtn = screen.getByRole('button', { name: /^Load More$/i });
    expect(loadMoreBtn).toBeInTheDocument();
    expect(loadMoreBtn.className).toContain('min-h-7');

    fireEvent.click(loadMoreBtn);

    await waitFor(() => {
      expect(getFavoritesSpy).toHaveBeenLastCalledWith({
        sort: 'favorited_at:desc',
        limit: 60,
        offset: 2,
      });
      expect(screen.getByText('Good News About Hell')).toBeInTheDocument();
      expect(screen.getByText('1 in locked libraries')).toBeInTheDocument();
    });

    // Duplicate movieItem was deduplicated; total loaded items == 3 == total_count -> Load More hides
    expect(screen.getAllByText('Arrival')).toHaveLength(1);
    expect(screen.queryByRole('button', { name: /^Load More$/i })).not.toBeInTheDocument();
  });
});
