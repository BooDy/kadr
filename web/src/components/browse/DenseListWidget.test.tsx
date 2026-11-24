import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, within, act } from '@testing-library/react';
import { DenseListWidget } from './DenseListWidget';
import { api } from '../../api/client';
import type { CardViewModel } from '../../types';
import { emitFavoriteChanged } from '../../utils/favoriteEvents';
import { emitWatchStateChanged } from '../../utils/watchStateEvents';
import {
  detectClientTargetDevice,
  formatDurationCompact,
  formatLocalTimeHHMM,
  isTimeWindowActive,
} from '../../utils/layoutScope';

describe('layoutScope helpers', () => {
  it('formats compact durations and omits zero/invalid values', () => {
    expect(formatDurationCompact(4620)).toBe('1h 17m');
    expect(formatDurationCompact(2700)).toBe('45m');
    expect(formatDurationCompact(3600)).toBe('1h');
    expect(formatDurationCompact(45)).toBe('1m');
    expect(formatDurationCompact(0)).toBe('');
    expect(formatDurationCompact(-120)).toBe('');
    expect(formatDurationCompact(undefined)).toBe('');
  });

  it('formats local Date as zero-padded 24h HH:MM and evaluates same-day & overnight TimeWindows', () => {
    expect(formatLocalTimeHHMM(new Date(2026, 10, 24, 5, 7))).toBe('05:07');
    expect(formatLocalTimeHHMM(new Date(2026, 10, 24, 23, 45))).toBe('23:45');

    // Same-day window 08:00..12:00
    const morning = { start: '08:00', end: '12:00' };
    expect(isTimeWindowActive(morning, '07:59')).toBe(false);
    expect(isTimeWindowActive(morning, '08:00')).toBe(true);
    expect(isTimeWindowActive(morning, '11:59')).toBe(true);
    expect(isTimeWindowActive(morning, '12:00')).toBe(false);

    // Overnight window 23:00..04:00
    const overnight = { start: '23:00', end: '04:00' };
    expect(isTimeWindowActive(overnight, '22:59')).toBe(false);
    expect(isTimeWindowActive(overnight, '23:00')).toBe(true);
    expect(isTimeWindowActive(overnight, '02:15')).toBe(true);
    expect(isTimeWindowActive(overnight, '03:59')).toBe(true);
    expect(isTimeWindowActive(overnight, '04:00')).toBe(false);

    // Zero-width or malformed inputs return false
    expect(isTimeWindowActive({ start: '10:00', end: '10:00' }, '10:00')).toBe(false);
    expect(isTimeWindowActive({ start: '25:00', end: '04:00' }, '01:00')).toBe(false);
    expect(isTimeWindowActive(overnight, 'bad')).toBe(false);
  });

  it('detects mobile vs web safely even when window.matchMedia is missing', () => {
    const originalMatchMedia = window.matchMedia;
    try {
      // Missing matchMedia in jsdom defaults to web
      Object.defineProperty(window, 'matchMedia', {
        writable: true,
        configurable: true,
        value: undefined,
      });
      expect(detectClientTargetDevice()).toBe('web');

      Object.defineProperty(window, 'matchMedia', {
        writable: true,
        configurable: true,
        value: vi.fn().mockImplementation((query: string) => ({
          matches: query === '(max-width: 767px)',
          media: query,
        })),
      });
      expect(detectClientTargetDevice()).toBe('mobile');
    } finally {
      Object.defineProperty(window, 'matchMedia', {
        writable: true,
        configurable: true,
        value: originalMatchMedia,
      });
    }
  });
});

describe('DenseListWidget', () => {
  const sampleItems: CardViewModel[] = [
    {
      id: 1,
      title: 'Cairo Station',
      subtitle: '1958 • Classic Drama',
      backdrop_url: 'https://images.example.com/cairo-backdrop.jpg',
      media_type: 'movie',
      duration_seconds: 4620,
      playback_progress: 0.4,
      rating: 8.1,
      badge: 'RESUME',
      is_favorite: false,
    },
    {
      id: 2,
      title: 'Severance S01E09',
      subtitle: 'The We We Are',
      poster_url: 'https://images.example.com/sev-poster.jpg',
      media_type: 'episode',
      duration_seconds: 2700,
      playback_progress: 1,
      is_favorite: true,
    },
    {
      id: 3,
      title: 'Severance',
      subtitle: '2 Seasons',
      media_type: 'show',
      is_favorite: false,
    },
    {
      id: 4,
      title: 'Severance Season 1',
      subtitle: '9 Episodes',
      media_type: 'season',
      is_favorite: false,
    },
    {
      id: 5,
      title: 'Cinema 24 Live',
      subtitle: 'Now Playing: Metropolis',
      media_type: 'live_channel',
      duration_seconds: 3600,
      is_favorite: false,
    },
  ];

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders semantic <ul role="list"> rows with runtime, progress, watched badge, favorite/watch toggles, and isolated Play vs select actions', () => {
    const onSelectItem = vi.fn();
    const onPlayItem = vi.fn();

    render(
      <DenseListWidget
        title="Late Night Queue"
        widgetId="late_night"
        screenId="home"
        items={sampleItems}
        totalCount={5}
        onSelectItem={onSelectItem}
        onPlayItem={onPlayItem}
      />
    );

    expect(screen.getByRole('heading', { name: 'Late Night Queue' })).toBeInTheDocument();
    const list = screen.getByRole('list');
    expect(list.tagName).toBe('UL');
    const listItems = within(list).getAllByRole('listitem');
    expect(listItems).toHaveLength(5);
    for (const li of listItems) {
      expect(li).not.toHaveAttribute('role', 'button');
    }

    // Runtimes: 4620 -> 1h 17m, 2700 -> 45m, omitted on show/season (undefined) and live_channel (even if duration_seconds > 0)
    expect(within(listItems[0]).getByText('1h 17m')).toBeInTheDocument();
    expect(within(listItems[1]).getByText('45m')).toBeInTheDocument();
    expect(within(listItems[4]).queryByText('1h')).toBeNull();

    // Subtitle & metadata rendered
    expect(within(listItems[0]).getByText('1958 • Classic Drama')).toBeInTheDocument();
    expect(within(listItems[0]).getByText('RESUME')).toBeInTheDocument();
    expect(within(listItems[0]).getByText('8.1')).toBeInTheDocument();

    // Progress bar on in-progress item (row 0), omitted on completed item (row 1)
    expect(
      within(listItems[0]).getByRole('progressbar', { name: /Cairo Station/i })
    ).toHaveAttribute('aria-valuenow', '40');
    expect(within(listItems[1]).queryByRole('progressbar')).toBeNull();

    // Watched badge on playback_progress >= 1 (row 1), omitted on row 0
    expect(within(listItems[1]).getByText('Watched')).toBeInTheDocument();
    expect(within(listItems[0]).queryByText('Watched')).toBeNull();

    // FavoriteButton rendered on ALL items including show, season, and live_channel
    expect(
      screen.getByRole('button', { name: 'Add Cairo Station to favorites' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Remove Severance S01E09 from favorites' })
    ).toHaveAttribute('aria-pressed', 'true');
    expect(
      screen.getByRole('button', { name: 'Add Severance to favorites' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Add Cinema 24 Live to favorites' })
    ).toBeInTheDocument();

    // WatchStateButton rendered on playable VOD items (movie, episode), omitted on show, season, and live_channel
    expect(
      screen.getByRole('button', { name: 'Mark Cairo Station as watched' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Mark Severance S01E09 as unwatched' })
    ).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', { name: /Mark Severance as/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /Mark Severance Season 1 as/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /Mark Cinema 24 Live as/i })).toBeNull();

    // Clicking Play button calls onPlayItem(1) WITHOUT triggering onSelectItem
    const playCairoBtn = screen.getByRole('button', { name: 'Play Cairo Station' });
    expect(playCairoBtn).toHaveTextContent('Play');
    fireEvent.click(playCairoBtn);
    expect(onPlayItem).toHaveBeenCalledTimes(1);
    expect(onPlayItem).toHaveBeenCalledWith(1);
    expect(onSelectItem).not.toHaveBeenCalled();

    // Clicking row title button calls onSelectItem(sampleItems[0]) WITHOUT triggering onPlayItem
    fireEvent.click(screen.getByText('Cairo Station'));
    expect(onSelectItem).toHaveBeenCalledTimes(1);
    expect(onSelectItem).toHaveBeenCalledWith(sampleItems[0]);
    expect(onPlayItem).toHaveBeenCalledTimes(1);
  });

  it('reacts live to emitFavoriteChanged and emitWatchStateChanged and paginates via Load More', async () => {
    const getWidgetDataSpy = vi.spyOn(api, 'getWidgetData').mockResolvedValueOnce([
      {
        id: 1, // duplicate of existing item, must be deduplicated
        title: 'Cairo Station',
        media_type: 'movie',
        duration_seconds: 4620,
      },
      {
        id: 6,
        title: 'Stalker',
        subtitle: '1979 • Andrei Tarkovsky',
        media_type: 'movie',
        duration_seconds: 9720,
        is_favorite: false,
      },
    ]);

    render(
      <DenseListWidget
        title="Curated Picks"
        widgetId="curated_dense"
        screenId="home"
        targetDevice="web"
        items={[sampleItems[0], sampleItems[1]]}
        totalCount={3}
        nextCursor="2"
        onSelectItem={vi.fn()}
        onPlayItem={vi.fn()}
      />
    );

    // Live favorite event updates Cairo Station heart state
    act(() => {
      emitFavoriteChanged(1, true);
    });
    expect(
      screen.getByRole('button', { name: 'Remove Cairo Station from favorites' })
    ).toHaveAttribute('aria-pressed', 'true');

    // Live watch-state event marks Cairo Station watched -> removes RESUME & progress bar, adds Watched badge
    act(() => {
      emitWatchStateChanged([1], true);
    });
    const firstRow = within(screen.getByRole('list')).getAllByRole('listitem')[0];
    expect(within(firstRow).queryByText('RESUME')).toBeNull();
    expect(within(firstRow).queryByRole('progressbar')).toBeNull();
    expect(within(firstRow).getByText('Watched')).toBeInTheDocument();

    // Load More fetches next page with screenId + targetDevice, deduplicates id=1, appends Stalker (2h 42m), and hides Load More once exhausted
    const loadMoreBtn = screen.getByRole('button', { name: 'Load More' });
    fireEvent.click(loadMoreBtn);

    await waitFor(() => {
      expect(getWidgetDataSpy).toHaveBeenCalledWith(
        'curated_dense',
        1,
        2,
        undefined,
        'home',
        'web'
      );
      expect(screen.getByText('Stalker')).toBeInTheDocument();
      expect(screen.getByText('2h 42m')).toBeInTheDocument();
    });

    expect(within(screen.getByRole('list')).getAllByRole('listitem')).toHaveLength(3);
    expect(screen.queryByRole('button', { name: 'Load More' })).toBeNull();
  });
});
