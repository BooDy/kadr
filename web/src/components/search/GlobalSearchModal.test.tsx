import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { GlobalSearchModal } from './GlobalSearchModal';
import { api } from '../../api/client';
import type { SearchResultItem } from '../../types';

const SAMPLE_ITEMS: SearchResultItem[] = [
  {
    id: 101,
    library_id: 'lib-movies',
    library_name: '4K Cinema',
    item_type: 'movie',
    title: 'Inception',
    original_title: null,
    release_year: 2010,
    overview: 'A thief who steals corporate secrets through dream-sharing technology.',
    poster_url: '/api/v1/artwork/101/poster',
    backdrop_url: '/api/v1/artwork/101/backdrop',
    badge: 'RESUME',
    rating: 8.8,
    duration_seconds: 8880,
    playback_progress: 0.45,
    series_title: null,
    season: null,
    episode: null,
    genres: ['Action', 'Sci-Fi'],
    director: 'Christopher Nolan',
  },
  {
    id: 202,
    library_id: 'lib-shows',
    library_name: 'TV Series',
    item_type: 'episode',
    title: 'Half Loop',
    original_title: null,
    release_year: 2022,
    overview: 'Mark takes the team out.',
    poster_url: '/api/v1/artwork/202/poster',
    backdrop_url: null,
    badge: null,
    rating: 8.5,
    duration_seconds: 3120,
    playback_progress: null,
    series_title: 'Severance',
    season: 1,
    episode: 2,
    genres: ['Thriller', 'Drama'],
    director: 'Ben Stiller',
  },
];

describe('GlobalSearchModal', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('debounces live search by 150ms and renders rich result metadata', async () => {
    const searchSpy = vi.spyOn(api, 'search').mockResolvedValue({
      query: 'Inception',
      total: 2,
      items: SAMPLE_ITEMS,
    });

    render(
      <GlobalSearchModal
        isOpen={true}
        onClose={vi.fn()}
        onSelectItem={vi.fn()}
      />
    );

    const input = screen.getByLabelText('Global media search');
    fireEvent.change(input, { target: { value: 'In' } });
    await act(async () => {
      vi.advanceTimersByTime(80);
    });
    fireEvent.change(input, { target: { value: 'Incep' } });
    await act(async () => {
      vi.advanceTimersByTime(80);
    });
    fireEvent.change(input, { target: { value: 'Inception' } });

    // Still within 150ms of last keystroke -> not called yet
    expect(searchSpy).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    expect(searchSpy).toHaveBeenCalledTimes(1);
    expect(searchSpy).toHaveBeenCalledWith({
      q: 'Inception',
      item_type: undefined,
      library_id: undefined,
      limit: 30,
    });

    // Verify movie metadata, director/genres subtitle, year, badge, and progress bar
    expect(screen.getByText('Inception')).toBeInTheDocument();
    expect(screen.getByText('2010')).toBeInTheDocument();
    expect(screen.getByText('RESUME')).toBeInTheDocument();
    expect(screen.getByText(/Dir\. Christopher Nolan • Action, Sci-Fi/)).toBeInTheDocument();
    const progressBar = screen.getByRole('progressbar', {
      name: /Playback progress for Inception/i,
    });
    expect(progressBar).toHaveAttribute('aria-valuenow', '45');

    // Verify episode S01E02 context & series title
    expect(screen.getByText('Half Loop')).toBeInTheDocument();
    expect(screen.getByText('S01E02')).toBeInTheDocument();
    expect(screen.getByText('Severance')).toBeInTheDocument();
  });

  it('switches media type filter pills and passes item_type to api.search', async () => {
    const searchSpy = vi.spyOn(api, 'search').mockResolvedValue({
      query: 'Trip',
      total: 0,
      items: [],
    });

    render(
      <GlobalSearchModal
        isOpen={true}
        onClose={vi.fn()}
        onSelectItem={vi.fn()}
      />
    );

    const input = screen.getByLabelText('Global media search');
    fireEvent.change(input, { target: { value: 'Trip' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    for (const [label, expectedType] of [
      ['Movies', 'movie'],
      ['Shows', 'show'],
      ['Episodes', 'episode'],
      ['Home Videos', 'home_videos'],
      ['All', undefined],
    ] as const) {
      fireEvent.click(screen.getByRole('button', { name: label }));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(150);
      });
      expect(searchSpy).toHaveBeenLastCalledWith({
        q: 'Trip',
        item_type: expectedType,
        library_id: undefined,
        limit: 30,
      });
    }
  });

  it('supports ArrowDown, ArrowUp, Enter selection, and Escape close', async () => {
    vi.spyOn(api, 'search').mockResolvedValue({
      query: 'a',
      total: 2,
      items: SAMPLE_ITEMS,
    });
    const onClose = vi.fn();
    const onSelectItem = vi.fn();

    render(
      <GlobalSearchModal
        isOpen={true}
        onClose={onClose}
        onSelectItem={onSelectItem}
      />
    );

    const input = screen.getByLabelText('Global media search');
    fireEvent.change(input, { target: { value: 'a' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    const options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'true');
    expect(options[1]).toHaveAttribute('aria-selected', 'false');

    // ArrowDown -> moves highlight to second item ("Half Loop")
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(options[0]).toHaveAttribute('aria-selected', 'false');
    expect(options[1]).toHaveAttribute('aria-selected', 'true');

    // Enter -> selects second item
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onSelectItem).toHaveBeenCalledTimes(1);
    expect(onSelectItem).toHaveBeenCalledWith(SAMPLE_ITEMS[1]);

    // ArrowUp wraps/moves back to first item
    fireEvent.keyDown(input, { key: 'ArrowUp' });
    expect(options[0]).toHaveAttribute('aria-selected', 'true');

    // Escape closes modal
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
