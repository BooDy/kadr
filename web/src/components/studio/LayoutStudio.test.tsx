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

  it('reorders widgets up and down, updates tree order, and marks layout dirty', async () => {
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
      expect(screen.getByText('continue_watching_row')).toBeDefined();
    });

    const moveHeroDown = screen.getByRole('button', { name: /move hero_home down/i });
    const moveHeroUp = screen.getByRole('button', { name: /move hero_home up/i });

    expect((moveHeroUp as HTMLButtonElement).disabled).toBe(true);
    expect((moveHeroDown as HTMLButtonElement).disabled).toBe(false);

    // Save button should be disabled initially
    const saveBtn = screen.getByRole('button', { name: /save layout/i });
    expect((saveBtn as HTMLButtonElement).disabled).toBe(true);

    // Move hero_home down
    fireEvent.click(moveHeroDown);

    // Now continue_watching_row is first (#1) and hero_home is second (#2)
    const widgetCards = screen.getAllByTestId('widget-tree-item');
    expect(within(widgetCards[0]).getByText('continue_watching_row')).toBeDefined();
    expect(within(widgetCards[1]).getByText('hero_home')).toBeDefined();

    // Layout should be dirty
    expect((saveBtn as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByText(/unsaved changes/i)).toBeDefined();

    // Move hero_home back up
    const newMoveHeroUp = screen.getByRole('button', { name: /move hero_home up/i });
    fireEvent.click(newMoveHeroUp);

    const reorderedCards = screen.getAllByTestId('widget-tree-item');
    expect(within(reorderedCards[0]).getByText('hero_home')).toBeDefined();
    expect(within(reorderedCards[1]).getByText('continue_watching_row')).toBeDefined();
  });

  it('adds a new widget via WidgetConfigModal and updates preview', async () => {
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
    });

    const addBtn = screen.getByRole('button', { name: /\+ add widget/i });
    fireEvent.click(addBtn);

    // Modal opens
    expect(screen.getByRole('dialog', { name: /add widget/i })).toBeDefined();

    // Fill in Title
    const titleInput = screen.getByLabelText(/^title$/i);
    fireEvent.change(titleInput, { target: { value: 'Trending Action' } });

    // Submit modal
    const saveWidgetBtn = screen.getByRole('button', { name: /save widget/i });
    fireEvent.click(saveWidgetBtn);

    // Modal closed
    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: /add widget/i })).toBeNull();
    });

    // Check widget tree and preview
    const addMatches = screen.getAllByText('Trending Action');
    expect(addMatches.length).toBeGreaterThanOrEqual(2);

    // Check preview
    const preview = screen.getByTestId('viewport-frame');
    expect(within(preview).getByText('Trending Action')).toBeDefined();

    // Save button enabled
    const saveBtn = screen.getByRole('button', { name: /save layout/i });
    expect((saveBtn as HTMLButtonElement).disabled).toBe(false);
  });

  it('edits an existing widget and updates preview', async () => {
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('continue_watching_row')).toBeDefined();
    });

    const editBtn = screen.getByRole('button', { name: /edit continue_watching_row/i });
    fireEvent.click(editBtn);

    expect(screen.getByRole('dialog', { name: /edit widget/i })).toBeDefined();

    const titleInput = screen.getByLabelText(/^title$/i) as HTMLInputElement;
    expect(titleInput.value).toBe('Continue Watching');

    fireEvent.change(titleInput, { target: { value: 'Watch Next Queue' } });

    const saveWidgetBtn = screen.getByRole('button', { name: /save widget/i });
    fireEvent.click(saveWidgetBtn);

    await waitFor(() => {
      expect(screen.queryByRole('dialog', { name: /edit widget/i })).toBeNull();
    });

    const editMatches = screen.getAllByText('Watch Next Queue');
    expect(editMatches.length).toBeGreaterThanOrEqual(2);
    const preview = screen.getByTestId('viewport-frame');
    expect(within(preview).getByText('Watch Next Queue')).toBeDefined();
  });

  it('deletes a widget from tree and preview', async () => {
    render(<LayoutStudio />);

    const preview = screen.getByTestId('viewport-frame');
    await waitFor(() => {
      expect(within(preview).getByText('Interstellar Cinema')).toBeDefined();
    });

    const deleteBtn = screen.getByRole('button', { name: /delete hero_home/i });
    fireEvent.click(deleteBtn);

    expect(screen.queryByText('hero_home')).toBeNull();
    expect(within(preview).queryByText('Interstellar Cinema')).toBeNull();

    const saveBtn = screen.getByRole('button', { name: /save layout/i });
    expect((saveBtn as HTMLButtonElement).disabled).toBe(false);
  });

  it('saves modified layout via api.saveScreen and clears dirty state', async () => {
    const saveSpy = vi.spyOn(api, 'saveScreen').mockResolvedValue(mockHomeLayout);
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
    });

    const moveHeroDown = screen.getByRole('button', { name: /move hero_home down/i });
    fireEvent.click(moveHeroDown);

    const saveBtn = screen.getByRole('button', { name: /save layout/i });
    expect((saveBtn as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(saveSpy).toHaveBeenCalledWith('home', expect.objectContaining({
        id: 'home',
        widgets: expect.arrayContaining([
          expect.objectContaining({ id: 'continue_watching_row' }),
          expect.objectContaining({ id: 'hero_home' }),
        ]),
      }));
    });

    await waitFor(() => {
      expect((saveBtn as HTMLButtonElement).disabled).toBe(true);
      expect(screen.queryByText(/unsaved changes/i)).toBeNull();
    });
  });

  it('resets layout to default via api.resetScreen', async () => {
    const resetSpy = vi.spyOn(api, 'resetScreen').mockResolvedValue(mockHomeLayout);
    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
    });

    // Delete a widget to make it dirty
    const deleteBtn = screen.getByRole('button', { name: /delete hero_home/i });
    fireEvent.click(deleteBtn);
    expect(screen.queryByText('hero_home')).toBeNull();

    const resetBtn = screen.getByRole('button', { name: /reset to default/i });
    fireEvent.click(resetBtn);

    await waitFor(() => {
      expect(resetSpy).toHaveBeenCalledWith('home');
      expect(screen.getByText('hero_home')).toBeDefined();
    });

    const saveBtn = screen.getByRole('button', { name: /save layout/i });
    expect((saveBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('creates a new screen and deletes a custom screen', async () => {
    vi.spyOn(api, 'getScreen').mockImplementation(async (id: string) => {
      if (id === 'anime') {
        return { id: 'anime', title: 'Anime Channel', widgets: [] };
      }
      if (id === 'movies') return mockMoviesLayout;
      return mockHomeLayout;
    });

    const createSpy = vi.spyOn(api, 'createScreen').mockResolvedValue({
      id: 'anime',
      title: 'Anime Channel',
      widgets: [],
    });
    const deleteSpy = vi.spyOn(api, 'deleteScreen').mockResolvedValue(undefined);

    render(<LayoutStudio />);

    await waitFor(() => {
      expect(screen.getByText('hero_home')).toBeDefined();
    });

    const newScreenBtn = screen.getByRole('button', { name: /\+ new screen/i });
    fireEvent.click(newScreenBtn);

    const dialog = screen.getByRole('dialog', { name: /new screen/i });
    expect(dialog).toBeDefined();

    const idInput = screen.getByLabelText(/screen id/i);
    const titleInput = screen.getByLabelText(/screen title/i);

    fireEvent.change(idInput, { target: { value: 'anime' } });
    fireEvent.change(titleInput, { target: { value: 'Anime Channel' } });

    const submitBtn = screen.getByRole('button', { name: /create screen/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(createSpy).toHaveBeenCalledWith({
        id: 'anime',
        title: 'Anime Channel',
      });
      expect(screen.getByRole('button', { name: /^anime channel$/i })).toBeDefined();
    });

    // For custom screen 'anime', "Delete Screen" button is visible instead of "Reset to Default"
    const deleteScreenBtn = screen.getByRole('button', { name: /delete screen/i });
    expect(deleteScreenBtn).toBeDefined();
    expect(screen.queryByRole('button', { name: /reset to default/i })).toBeNull();

    fireEvent.click(deleteScreenBtn);

    await waitFor(() => {
      expect(deleteSpy).toHaveBeenCalledWith('anime');
      expect(screen.queryByRole('button', { name: /^anime channel$/i })).toBeNull();
    });
  });
});
