import { describe, it, expect, beforeEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { BrowseScreen } from './BrowseScreen';
import { api } from '../../api/client';
import type {
  ScreenLayout,
  ItemDetailsPayload,
  SubtitleTrack,
  OnlineSubtitleSearchResponse,
  LibraryFolderResponse,
} from '../../types';

describe('BrowseScreen & Declarative Widgets', () => {
  const mockHomeLayout: ScreenLayout = {
    id: 'home',
    title: 'Home Cinema',
    widgets: [
      {
        type: 'hero_banner',
        id: 'spotlight_hero',
        binding: { macro_type: 'spotlight_item', limit: 1 },
        data: {
          id: 101,
          title: 'Blade Runner 2049',
          subtitle: 'A young blade runner unearths a long-buried secret.',
          backdrop_url: 'https://images.example.com/br2049-backdrop.jpg',
          poster_url: 'https://images.example.com/br2049-poster.jpg',
          media_type: 'movie',
          release_year: 2017,
          badge: '4K ULTRA HD',
        },
      },
      {
        type: 'carousel',
        id: 'continue_watching',
        title: 'Continue Watching',
        binding: { macro_type: 'continue_watching', limit: 10 },
        items: [
          {
            id: 102,
            title: 'Dune: Part Two',
            subtitle: 'Paul Atreides unites with the Fremen.',
            poster_url: 'https://images.example.com/dune2-poster.jpg',
            media_type: 'movie',
            release_year: 2024,
            playback_progress: 0.65,
          },
        ],
      },
      {
        type: 'grid',
        id: 'top_movies',
        title: 'Popular Movies',
        columns: 4,
        binding: { macro_type: 'top_rated', limit: 20 },
        items: [
          {
            id: 103,
            title: 'Interstellar',
            subtitle: 'A team of explorers travel through a wormhole.',
            poster_url: '',
            media_type: 'movie',
            release_year: 2014,
          },
        ],
      },
    ],
  };

  const mockItemDetails: ItemDetailsPayload = {
    card: {
      id: 101,
      title: 'Blade Runner 2049',
      subtitle: 'Original Title: Blade Runner 2049',
      media_type: 'movie',
      release_year: 2017,
    },
    overview: 'Thirty years after the events of the first film, a new Blade Runner discovers a dark secret.',
    genres: ['Sci-Fi', 'Mystery', 'Drama'],
    duration_seconds: 9840,
    technical: {
      duration_seconds: 9840,
      resolution: '4K UHD (3840x2160)',
      video_codec: 'HEVC Main 10',
      audio_codec: 'Dolby Atmos',
      audio_channels: 6,
      container: 'mkv',
    },
    stream_url: '/api/v1/stream/101',
  };

  const mockSubtitles: SubtitleTrack[] = [
    {
      id: 501,
      media_item_id: 101,
      source: 'embedded',
      language: 'English',
      title: 'English [CC]',
      format: 'srt',
      is_default: true,
      is_forced: false,
      stream_url: '/api/v1/subtitles/501',
    },
  ];

  const mockOnlineSubtitles: OnlineSubtitleSearchResponse = {
    configured: true,
    matches: [
      {
        id: 'sub-8899',
        language: 'French',
        release_name: 'Blade.Runner.2049.2160p.UHD.BluRay.x265',
        hearing_impaired: false,
        format: 'srt',
        download_count: 1420,
        rating: 4.8,
      },
    ],
  };

  const onPlayItem = vi.fn();

  beforeEach(() => {
    vi.restoreAllMocks();
    onPlayItem.mockClear();
  });

  it('renders screen layout with spotlight hero, carousel rail, and grid', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockHomeLayout);

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    // AST loading and spotlight rendering
    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });

    expect(screen.getByText('4K ULTRA HD')).toBeDefined();
    expect(screen.getByText(/A young blade runner unearths/i)).toBeDefined();
    expect(screen.getByRole('button', { name: /play now/i })).toBeDefined();
    expect(screen.getByRole('button', { name: /more info/i })).toBeDefined();

    // Carousel section
    expect(screen.getByText('Continue Watching')).toBeDefined();
    expect(screen.getByText('Dune: Part Two')).toBeDefined();

    // Grid section
    expect(screen.getByText('Popular Movies')).toBeDefined();
    expect(screen.getByText('Interstellar')).toBeDefined();
  });

  it('triggers onPlayItem when clicking "Play Now" on spotlight', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockHomeLayout);

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /play now/i })).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: /play now/i }));
    expect(onPlayItem).toHaveBeenCalledWith(101);
  });

  it('opens ItemDetailsModal when clicking "More Info" on spotlight', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockHomeLayout);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce(mockItemDetails);
    vi.spyOn(api, 'getSubtitles').mockResolvedValueOnce(mockSubtitles);

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /more info/i })).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: /more info/i }));

    // Modal details
    await waitFor(() => {
      expect(screen.getByText(/Thirty years after the events/i)).toBeDefined();
      expect(screen.getByText(/HEVC Main 10/i)).toBeDefined();
      expect(screen.getByText(/Dolby Atmos/i)).toBeDefined();
    });
  });

  it('opens ItemDetailsModal when clicking a card in carousel or grid', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockHomeLayout);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce({
      ...mockItemDetails,
      card: {
        id: 102,
        title: 'Dune: Part Two',
        media_type: 'movie',
      },
      overview: 'Paul Atreides unites with Chani and the Fremen.',
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValueOnce([]);

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Dune: Part Two')).toBeDefined();
    });

    fireEvent.click(screen.getByText('Dune: Part Two'));

    await waitFor(() => {
      expect(screen.getByText(/Paul Atreides unites with Chani/i)).toBeDefined();
    });
  });

  it('allows searching and downloading subtitles in ItemDetailsModal', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockHomeLayout);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce(mockItemDetails);
    vi.spyOn(api, 'getSubtitles')
      .mockResolvedValueOnce(mockSubtitles) // Initial tracks
      .mockResolvedValueOnce([ // After download
        ...mockSubtitles,
        {
          id: 502,
          media_item_id: 101,
          source: 'downloaded',
          language: 'French',
          title: 'Blade.Runner.2049.2160p.UHD.BluRay.x265',
          format: 'srt',
          is_default: false,
          is_forced: false,
          stream_url: '/api/v1/subtitles/502',
        },
      ]);
    vi.spyOn(api, 'searchSubtitles').mockResolvedValueOnce(mockOnlineSubtitles);
    vi.spyOn(api, 'downloadSubtitle').mockResolvedValueOnce({
      id: 502,
      media_item_id: 101,
      source: 'downloaded',
      language: 'French',
      format: 'srt',
      is_default: false,
      is_forced: false,
      stream_url: '/api/v1/subtitles/502',
    });

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /more info/i })).toBeDefined();
    });
    fireEvent.click(screen.getByRole('button', { name: /more info/i }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /subtitles/i })).toBeDefined();
    });

    // Switch to subtitles tab
    fireEvent.click(screen.getByRole('button', { name: /subtitles/i }));

    // Should list existing English subtitle
    await waitFor(() => {
      expect(screen.getByText('English [CC]')).toBeDefined();
    });

    // Search OpenSubtitles button
    const searchBtn = screen.getByRole('button', { name: /search online/i });
    fireEvent.click(searchBtn);

    // Wait for search result match
    await waitFor(() => {
      expect(screen.getByText(/Blade\.Runner\.2049\.2160p/i)).toBeDefined();
    });

    // Download button
    const downloadBtn = screen.getByRole('button', { name: /download/i });
    fireEvent.click(downloadBtn);

    // Verify downloaded subtitle appears in tracks
    await waitFor(() => {
      expect(screen.getAllByText(/Downloaded/i).length).toBeGreaterThan(0);
    });
  });

  it('fetches widget items if unhydrated in AST', async () => {
    const unhydratedLayout: ScreenLayout = {
      id: 'movies',
      title: 'Movies',
      widgets: [
        {
          type: 'carousel',
          id: 'unhydrated_carousel',
          title: 'Fresh Releases',
          binding: { macro_type: 'recently_added', limit: 10 },
          items: undefined,
        },
      ],
    };

    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(unhydratedLayout);
    vi.spyOn(api, 'getWidgetData').mockResolvedValueOnce([
      {
        id: 201,
        title: 'Oppenheimer',
        media_type: 'movie',
        release_year: 2023,
      },
    ]);

    render(<BrowseScreen screenId="movies" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(api.getWidgetData).toHaveBeenCalledWith('unhydrated_carousel', 0, 20, undefined, 'movies');
      expect(screen.getByText('Oppenheimer')).toBeDefined();
    });
  });

  it('renders error state and allows retry on fetch error', async () => {
    vi.spyOn(api, 'getScreen')
      .mockRejectedValueOnce(new Error('Network failure'))
      .mockResolvedValueOnce(mockHomeLayout);

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText(/failed to load screen/i)).toBeDefined();
    });

    const retryBtn = screen.getByRole('button', { name: /retry/i });
    fireEvent.click(retryBtn);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });
  });

  const mockLibraryLayout: ScreenLayout = {
    id: 'movies',
    title: 'Movies Library',
    widgets: [
      {
        type: 'grid',
        id: 'movies_grid',
        title: 'All Movies',
        columns: 4,
        binding: { macro_type: 'top_rated', limit: 20 },
        items: [
          {
            id: 201,
            title: 'Inception',
            media_type: 'movie',
            release_year: 2010,
          },
        ],
      },
    ],
  };

  const mockFolderResponse: LibraryFolderResponse = {
    library_id: 'movies',
    library_name: 'Movies Library',
    current_path: '',
    parent_path: null,
    breadcrumbs: [{ name: 'Root', path: '' }],
    directories: [{ name: 'Sci-Fi', path: 'Sci-Fi', item_count: 5 }],
    items: [],
  };

  it('renders Catalog and Folders view mode toggle buttons when screenId !== "home"', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockLibraryLayout);

    render(<BrowseScreen screenId="movies" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeDefined();
    });

    const catalogBtn = screen.getByRole('button', { name: /catalog/i });
    const foldersBtn = screen.getByRole('button', { name: /folders/i });
    expect(catalogBtn).toBeDefined();
    expect(foldersBtn).toBeDefined();
  });

  it('clicking "Folders" switches view to FolderBrowser', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockLibraryLayout);
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockFolderResponse);

    render(<BrowseScreen screenId="movies" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeDefined();
    });

    const foldersBtn = screen.getByRole('button', { name: /folders/i });
    fireEvent.click(foldersBtn);

    await waitFor(() => {
      expect(screen.getByText('Sci-Fi')).toBeDefined();
      expect(screen.getByText('5 items')).toBeDefined();
    });

    expect(screen.queryByText('All Movies')).toBeNull();
  });

  it('clicking "Catalog" switches view back to declarative widget layout', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValue(mockLibraryLayout);
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValue(mockFolderResponse);

    render(<BrowseScreen screenId="movies" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeDefined();
    });

    // Switch to folders
    fireEvent.click(screen.getByRole('button', { name: /folders/i }));
    await waitFor(() => {
      expect(screen.getByText('Sci-Fi')).toBeDefined();
    });

    // Switch back to catalog
    fireEvent.click(screen.getByRole('button', { name: /catalog/i }));
    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeDefined();
      expect(screen.getByText('All Movies')).toBeDefined();
    });

    expect(screen.queryByText('Sci-Fi')).toBeNull();
  });

  it('does not display view mode toggle when screenId === "home"', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockHomeLayout);

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });

    expect(screen.queryByRole('button', { name: /catalog/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /folders/i })).toBeNull();
  });

  it('initializes with initialViewMode and initialFolder when provided', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockLibraryLayout);
    const getFoldersSpy = vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce({
      ...mockFolderResponse,
      current_path: 'Sci-Fi/Inception (2010)',
    });

    render(
      <BrowseScreen
        screenId="movies"
        onPlayItem={onPlayItem}
        initialViewMode="folders"
        initialFolder="Sci-Fi/Inception (2010)"
      />
    );

    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenCalledWith('movies', 'Sci-Fi/Inception (2010)');
    });
  });

  it('switches viewMode to folders and sets initialPath when ItemDetailsModal triggers onNavigateToFolder for current library', async () => {
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockLibraryLayout);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce({
      card: { id: 201, title: 'Inception', media_type: 'movie' },
      genres: ['Sci-Fi'],
      stream_url: '/api/v1/stream/201',
      library_id: 'movies',
      folder_path: 'Sci-Fi/Inception (2010)',
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValueOnce([]);
    const getFoldersSpy = vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce({
      ...mockFolderResponse,
      current_path: 'Sci-Fi/Inception (2010)',
    });

    render(<BrowseScreen screenId="movies" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeInTheDocument();
    });

    // Click item to open ItemDetailsModal
    fireEvent.click(screen.getByText('Inception'));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /browse folder/i })).toBeInTheDocument();
    });

    // Click Browse Folder
    fireEvent.click(screen.getByRole('button', { name: /browse folder/i }));

    // Should switch to folders and fetch folder contents
    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenCalledWith('movies', 'Sci-Fi/Inception (2010)');
    });
  });

  it('calls props.onNavigateToFolder when ItemDetailsModal triggers folder nav for a different library', async () => {
    const onNavProp = vi.fn();
    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(mockLibraryLayout);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce({
      card: { id: 201, title: 'Inception', media_type: 'movie' },
      genres: ['Sci-Fi'],
      stream_url: '/api/v1/stream/201',
      library_id: 'other-lib',
      folder_path: 'OtherFolder',
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValueOnce([]);

    render(
      <BrowseScreen
        screenId="movies"
        onPlayItem={onPlayItem}
        onNavigateToFolder={onNavProp}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('Inception'));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /browse folder/i })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole('button', { name: /browse folder/i }));

    expect(onNavProp).toHaveBeenCalledWith('other-lib', 'OtherFolder');
  });

  it('toggles card watched state without opening ItemDetailsModal, omits watch toggle on show/live_channel cards, and silently refreshes home screen', async () => {
    const layoutWithMixedCards: ScreenLayout = {
      id: 'home',
      title: 'Home Cinema',
      widgets: [
        {
          type: 'carousel',
          id: 'continue_watching',
          title: 'Continue Watching',
          binding: { macro_type: 'continue_watching', limit: 10 },
          items: [
            {
              id: 102,
              title: 'Dune: Part Two',
              media_type: 'movie',
              playback_progress: 0.65,
            },
            {
              id: 104,
              title: 'Severance',
              media_type: 'show',
            },
            {
              id: 105,
              title: 'Live News 24',
              media_type: 'live_channel',
            },
          ],
        },
        {
          type: 'grid',
          id: 'top_movies',
          title: 'Popular Movies',
          columns: 4,
          binding: { macro_type: 'top_rated', limit: 20 },
          items: [
            {
              id: 102,
              title: 'Dune: Part Two',
              media_type: 'movie',
              playback_progress: 0.65,
            },
          ],
        },
      ],
    };

    const getScreenSpy = vi
      .spyOn(api, 'getScreen')
      .mockResolvedValueOnce(layoutWithMixedCards)
      .mockResolvedValueOnce({
        ...layoutWithMixedCards,
        widgets: [
          {
            ...layoutWithMixedCards.widgets[1],
            items: [
              {
                id: 102,
                title: 'Dune: Part Two',
                media_type: 'movie',
                playback_progress: 1,
              },
            ],
          } as ScreenLayout['widgets'][number],
        ],
      });
    const setSpy = vi.spyOn(api, 'setPlaybackState').mockResolvedValue({
      user_id: 'u1',
      media_item_id: 102,
      playback_position_seconds: 0,
      watch_state: 'completed',
      last_watched_at: 1000,
      play_count: 1,
    });
    const detailsSpy = vi.spyOn(api, 'getItemDetails');

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getAllByText('Dune: Part Two').length).toBe(2);
    });

    // Omit watch toggle on show container cards and live_channel cards
    expect(screen.queryByRole('button', { name: /Mark Severance as/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /Mark Live News 24 as/i })).toBeNull();

    // Both carousel and grid render WatchStateButton for movie 102
    const watchBtns = screen.getAllByRole('button', {
      name: 'Mark Dune: Part Two as watched',
    });
    expect(watchBtns.length).toBe(2);

    fireEvent.click(watchBtns[0]);

    // Must not open ItemDetailsModal
    expect(detailsSpy).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(setSpy).toHaveBeenCalledWith(102, true);
      expect(getScreenSpy).toHaveBeenCalledTimes(2);
    });

    // Grid card synced to watched via watchStateEvents and Continue Watching refreshed away
    await waitFor(() => {
      expect(screen.queryByText('Continue Watching')).toBeNull();
      expect(
        screen.getByRole('button', { name: 'Mark Dune: Part Two as unwatched' })
      ).toHaveAttribute('aria-pressed', 'true');
    });
  });

  it('shows RESUME badge on in-progress Spotlight item and removes/omits RESUME when Spotlight item is marked watched', async () => {
    const spotlightAndCardLayout: ScreenLayout = {
      id: 'movies',
      title: 'Movies',
      widgets: [
        {
          type: 'hero_banner',
          id: 'spotlight_hero',
          binding: { macro_type: 'spotlight_item', limit: 1 },
          data: {
            id: 101,
            title: 'Blade Runner 2049',
            media_type: 'movie',
            release_year: 2017,
            playback_progress: 0.45,
          },
        },
        {
          type: 'grid',
          id: 'all_movies',
          title: 'All Movies',
          columns: 4,
          binding: { macro_type: 'top_rated', limit: 20 },
          items: [
            {
              id: 101,
              title: 'Blade Runner 2049',
              media_type: 'movie',
              release_year: 2017,
              playback_progress: 0.45,
            },
          ],
        },
      ],
    };

    vi.spyOn(api, 'getScreen').mockResolvedValueOnce(spotlightAndCardLayout);
    vi.spyOn(api, 'setPlaybackState').mockResolvedValue({
      user_id: 'u1',
      media_item_id: 101,
      playback_position_seconds: 0,
      watch_state: 'completed',
      last_watched_at: 1000,
      play_count: 1,
    });

    render(<BrowseScreen screenId="movies" onPlayItem={onPlayItem} />);

    const spotlightSection = await screen.findByRole('region', {
      name: 'Spotlight: Blade Runner 2049',
    });
    expect(spotlightSection).toHaveTextContent('RESUME');

    // Mark Blade Runner 2049 as watched -> HydratedSpotlight updates playback_progress to 1, and RESUME badge must disappear
    fireEvent.click(
      screen.getByRole('button', { name: 'Mark Blade Runner 2049 as watched' })
    );

    await waitFor(() => {
      expect(spotlightSection).not.toHaveTextContent('RESUME');
    });
  });

  it('renders FavoriteButton on Spotlight, Carousel, and Grid cards (including show and live_channel), syncs via onFavoriteChanged, and refreshes home screen silently', async () => {
    const homeWithMixedFavorites: ScreenLayout = {
      id: 'home',
      title: 'Home Cinema',
      widgets: [
        {
          type: 'hero_banner',
          id: 'spotlight_hero',
          binding: { macro_type: 'spotlight_item', limit: 1 },
          data: {
            id: 102,
            title: 'Dune: Part Two',
            media_type: 'movie',
            release_year: 2024,
            is_favorite: false,
          },
        },
        {
          type: 'carousel',
          id: 'mixed_rail',
          title: 'Featured Rail',
          binding: { macro_type: 'recently_added', limit: 10 },
          items: [
            {
              id: 102,
              title: 'Dune: Part Two',
              media_type: 'movie',
              playback_progress: 0.3,
              is_favorite: false,
            },
            {
              id: 104,
              title: 'Severance',
              media_type: 'show',
              is_favorite: false,
            },
            {
              id: 105,
              title: 'Live News 24',
              media_type: 'live_channel',
              is_favorite: true,
            },
          ],
        },
        {
          type: 'grid',
          id: 'popular_grid',
          title: 'Popular Grid',
          columns: 4,
          binding: { macro_type: 'top_rated', limit: 20 },
          items: [
            {
              id: 102,
              title: 'Dune: Part Two',
              media_type: 'movie',
              is_favorite: false,
            },
          ],
        },
      ],
    };

    const refreshedHome: ScreenLayout = {
      ...homeWithMixedFavorites,
      widgets: [
        {
          ...homeWithMixedFavorites.widgets[0],
          data: {
            id: 102,
            title: 'Dune: Part Two',
            media_type: 'movie',
            release_year: 2024,
            is_favorite: true,
          },
        } as ScreenLayout['widgets'][number],
        {
          type: 'carousel',
          id: 'home_favorites',
          title: 'Favorites',
          binding: { macro_type: 'favorites', limit: 20 },
          items: [
            {
              id: 102,
              title: 'Dune: Part Two',
              media_type: 'movie',
              is_favorite: true,
            },
          ],
        },
        ...homeWithMixedFavorites.widgets.slice(1),
      ],
    };

    const getScreenSpy = vi
      .spyOn(api, 'getScreen')
      .mockResolvedValueOnce(homeWithMixedFavorites)
      .mockResolvedValueOnce(refreshedHome);
    const addFavSpy = vi.spyOn(api, 'addFavorite').mockResolvedValue({
      item_id: 102,
      is_favorite: true,
    });
    const detailsSpy = vi.spyOn(api, 'getItemDetails');

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getAllByText('Dune: Part Two').length).toBe(3);
    });

    // FavoriteButton renders on show & live_channel cards while WatchStateButton stays omitted on them
    expect(screen.getByRole('button', { name: 'Add Severance to favorites' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Remove Live News 24 from favorites' })
    ).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', { name: /Mark Severance as/i })).toBeNull();
    expect(screen.queryByRole('button', { name: /Mark Live News 24 as/i })).toBeNull();

    // Spotlight + Carousel + Grid all have FavoriteButton for Dune: Part Two, coexisting with WatchStateButton on movie cards
    const duneFavBtns = screen.getAllByRole('button', {
      name: 'Add Dune: Part Two to favorites',
    });
    expect(duneFavBtns.length).toBe(3);
    expect(
      screen.getAllByRole('button', { name: 'Mark Dune: Part Two as watched' }).length
    ).toBe(2);

    // Click card FavoriteButton (carousel card) -> does not open modal, calls addFavorite(102), syncs Spotlight + Grid, and refreshes home screen
    fireEvent.click(duneFavBtns[1]);
    expect(detailsSpy).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(addFavSpy).toHaveBeenCalledWith(102);
      expect(getScreenSpy).toHaveBeenCalledTimes(2);
    });

    await waitFor(() => {
      expect(screen.getByText('Favorites')).toBeInTheDocument();
      const updatedFavBtns = screen.getAllByRole('button', {
        name: 'Remove Dune: Part Two from favorites',
      });
      expect(updatedFavBtns.length).toBeGreaterThanOrEqual(3);
      for (const btn of updatedFavBtns) {
        expect(btn).toHaveAttribute('aria-pressed', 'true');
      }
    });
  });

  it('ignores out-of-order home screen refresh responses so stale snapshots cannot overwrite newer ones', async () => {
    const { emitFavoriteChanged } = await import('../../utils/favoriteEvents');
    let resolveFirstRefresh!: (value: ScreenLayout) => void;
    let resolveSecondRefresh!: (value: ScreenLayout) => void;

    vi.spyOn(api, 'getScreen')
      .mockResolvedValueOnce(mockHomeLayout)
      .mockImplementationOnce(
        () =>
          new Promise<ScreenLayout>((resolve) => {
            resolveFirstRefresh = resolve;
          })
      )
      .mockImplementationOnce(
        () =>
          new Promise<ScreenLayout>((resolve) => {
            resolveSecondRefresh = resolve;
          })
      );

    render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeInTheDocument();
    });

    // Trigger two rapid home refreshes
    act(() => {
      emitFavoriteChanged(101, true);
      emitFavoriteChanged(102, true);
    });

    // Newer (2nd) refresh resolves first
    resolveSecondRefresh({
      id: 'home',
      title: 'Home Cinema',
      widgets: [
        {
          type: 'carousel',
          id: 'home_favorites',
          title: 'Newest Favorites Snapshot',
          binding: { macro_type: 'favorites', limit: 20 },
          items: [{ id: 102, title: 'Dune: Part Two', media_type: 'movie', is_favorite: true }],
        },
      ],
    });

    await waitFor(() => {
      expect(screen.getByText('Newest Favorites Snapshot')).toBeInTheDocument();
    });

    // Older (1st) refresh resolves later -> must be ignored
    resolveFirstRefresh({
      id: 'home',
      title: 'Home Cinema',
      widgets: [
        {
          type: 'carousel',
          id: 'home_favorites',
          title: 'Stale Favorites Snapshot',
          binding: { macro_type: 'favorites', limit: 20 },
          items: [{ id: 101, title: 'Blade Runner 2049', media_type: 'movie', is_favorite: true }],
        },
      ],
    });

    await waitFor(() => {
      expect(screen.getByText('Newest Favorites Snapshot')).toBeInTheDocument();
      expect(screen.queryByText('Stale Favorites Snapshot')).toBeNull();
    });
  });

  it('clears loading skeleton and renders updated home layout when emitFavoriteChanged fires while initial fetchScreen is still pending', async () => {
    const { emitFavoriteChanged } = await import('../../utils/favoriteEvents');
    let resolveInitialFetch!: (value: ScreenLayout) => void;
    let resolveSecondFetch!: (value: ScreenLayout) => void;

    vi.spyOn(api, 'getScreen')
      .mockImplementationOnce(
        () =>
          new Promise<ScreenLayout>((resolve) => {
            resolveInitialFetch = resolve;
          })
      )
      .mockImplementationOnce(
        () =>
          new Promise<ScreenLayout>((resolve) => {
            resolveSecondFetch = resolve;
          })
      );

    const { container } = render(<BrowseScreen screenId="home" onPlayItem={onPlayItem} />);
    expect(container.querySelector('.animate-pulse')).not.toBeNull();

    // Fire favorite change BEFORE initial getScreen('home') settles
    act(() => {
      emitFavoriteChanged(102, true);
    });

    resolveInitialFetch(mockHomeLayout);
    resolveSecondFetch({
      id: 'home',
      title: 'Home Cinema',
      widgets: [
        {
          type: 'carousel',
          id: 'home_favorites',
          title: 'Preempted Favorites Rail',
          binding: { macro_type: 'favorites', limit: 20 },
          items: [{ id: 102, title: 'Dune: Part Two', media_type: 'movie', is_favorite: true }],
        },
      ],
    });

    await waitFor(() => {
      expect(container.querySelector('.animate-pulse')).toBeNull();
      expect(screen.getByText('Preempted Favorites Rail')).toBeInTheDocument();
    });
  });
});

