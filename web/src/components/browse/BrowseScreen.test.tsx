import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
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
});
