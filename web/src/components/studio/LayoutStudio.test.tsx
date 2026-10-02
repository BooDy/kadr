import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { LayoutStudio } from './LayoutStudio';
import { api } from '../../api/client';
import type { ScreenLayout, ScreenSummary } from '../../types';

describe('LayoutStudio Component', () => {
  const mockScreens: ScreenSummary[] = [
    { id: 'home', title: 'Home' },
    { id: 'movies', title: 'Movies' },
    { id: 'shows', title: 'Shows' },
  ];

  const mockHomeLayout: ScreenLayout = {
    id: 'home',
    title: 'Home',
    widgets: [
      {
        type: 'hero_banner',
        id: 'hero_home',
        display_type: 'spotlight',
        binding: { macro_type: 'spotlight_item', limit: 1 },
        data: {
          id: 101,
          title: 'Interstellar Cinema',
          media_type: 'movie',
          release_year: 2024,
        },
      },
      {
        type: 'carousel',
        id: 'continue_watching_row',
        display_type: 'carousel',
        title: 'Continue Watching',
        binding: { macro_type: 'continue_watching', limit: 10 },
        items: [
          { id: 201, title: 'Inception', media_type: 'movie' },
          { id: 202, title: 'Blade Runner', media_type: 'movie' },
        ],
      },
    ],
  };

  const mockMoviesLayout: ScreenLayout = {
    id: 'movies',
    title: 'Movies',
    widgets: [
      {
        type: 'grid',
        id: 'movies_grid',
        display_type: 'grid',
        title: 'All Feature Films',
        binding: { macro_type: 'top_rated', limit: 20 },
        columns: 6,
        items: [
          { id: 301, title: 'Dunkirk', media_type: 'movie' },
        ],
      },
    ],
  };

  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, 'getScreens').mockResolvedValue(mockScreens);
    vi.spyOn(api, 'getScreen').mockImplementation(async (id: string) => {
      if (id === 'movies') return mockMoviesLayout;
      return mockHomeLayout;
    });
  });

  it('renders layout studio header and screen selector tabs/dropdown', async () => {
    render(<LayoutStudio />);
    expect(screen.getByText('Layout Studio')).toBeDefined();

    await waitFor(() => {
      expect(api.getScreens).toHaveBeenCalled();
    });

    // Screen selector options/buttons
    expect(screen.getByRole('button', { name: /^home$/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /^movies$/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /^shows$/i })).toBeDefined();
  });

  it('loads screen layout and renders widget nodes in inspector and preview', async () => {
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
      expect(screen.getByText('continue_watching_row')).toBeDefined();
    });

    const preview = screen.getByTestId('viewport-frame');
    expect(within(preview).getByText('Interstellar Cinema')).toBeDefined();
    expect(within(preview).getByText('Continue Watching')).toBeDefined();
  });

  it('switches screens when a different screen tab is clicked', async () => {
    render(<LayoutStudio />);

    const preview = screen.getByTestId('viewport-frame');
    await waitFor(() => {
      expect(within(preview).getByText('Interstellar Cinema')).toBeDefined();
    });

    // Click Movies tab
    const moviesTab = screen.getByRole('button', { name: /^movies$/i });
    fireEvent.click(moviesTab);

    await waitFor(() => {
      expect(api.getScreen).toHaveBeenCalledWith('movies');
      expect(screen.getByText('movies_grid')).toBeDefined();
      expect(within(preview).getByText('All Feature Films')).toBeDefined();
    });
  });

  it('supports viewport switching between TV (16:9), Tablet (4:3), and Mobile (9:16)', async () => {
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
    });

    const tvButton = screen.getByRole('button', { name: /tv/i });
    const tabletButton = screen.getByRole('button', { name: /tablet/i });
    const mobileButton = screen.getByRole('button', { name: /mobile/i });

    expect(tvButton).toBeDefined();
    expect(tabletButton).toBeDefined();
    expect(mobileButton).toBeDefined();

    const previewFrame = screen.getByTestId('viewport-frame');
    expect(previewFrame.getAttribute('data-viewport')).toBe('tv');

    // Switch to Tablet
    fireEvent.click(tabletButton);
    expect(previewFrame.getAttribute('data-viewport')).toBe('tablet');

    // Switch to Mobile
    fireEvent.click(mobileButton);
    expect(previewFrame.getAttribute('data-viewport')).toBe('mobile');

    // Switch back to TV
    fireEvent.click(tvButton);
    expect(previewFrame.getAttribute('data-viewport')).toBe('tv');
  });

  it('toggles widgets on and off via the Widget Tree Inspector', async () => {
    render(<LayoutStudio />);

    const preview = screen.getByTestId('viewport-frame');
    await waitFor(() => {
      expect(within(preview).getByText('Interstellar Cinema')).toBeDefined();
      expect(within(preview).getByText('Continue Watching')).toBeDefined();
    });

    // Find toggle for continue_watching_row
    const toggleHero = screen.getByRole('checkbox', { name: /toggle hero_home/i });
    expect(toggleHero).toBeDefined();
    expect((toggleHero as HTMLInputElement).checked).toBe(true);

    // Disable hero_home widget
    fireEvent.click(toggleHero);
    expect((toggleHero as HTMLInputElement).checked).toBe(false);

    // Interstellar Cinema should now be hidden from preview
    expect(within(preview).queryByText('Interstellar Cinema')).toBeNull();
    // Continue watching remains visible
    expect(within(preview).getByText('Continue Watching')).toBeDefined();

    // Re-enable hero_home widget
    fireEvent.click(toggleHero);
    expect((toggleHero as HTMLInputElement).checked).toBe(true);
    expect(within(preview).getByText('Interstellar Cinema')).toBeDefined();
  });

  it('handles item playback from the interactive preview', async () => {
    const onPlayItem = vi.fn();
    render(<LayoutStudio onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Interstellar Cinema')).toBeDefined();
    });

    const playBtn = screen.getByRole('button', { name: /play now/i });
    fireEvent.click(playBtn);

    expect(onPlayItem).toHaveBeenCalledWith(101);
  });
});
