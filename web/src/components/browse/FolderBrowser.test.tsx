import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { FolderBrowser } from './FolderBrowser';
import { api } from '../../api/client';
import type { LibraryFolderResponse } from '../../types';

describe('FolderBrowser Component', () => {
  const mockLibraryId = 'lib-movies-1';
  const mockOnPlayItem = vi.fn();

  const mockRootResponse: LibraryFolderResponse = {
    library_id: 'lib-movies-1',
    library_name: 'Movies Library',
    current_path: '',
    parent_path: null,
    breadcrumbs: [
      { name: 'Root', path: '' },
    ],
    directories: [
      { name: 'Action', path: 'Action', item_count: 5 },
      { name: 'Sci-Fi', path: 'Sci-Fi', item_count: 12 },
    ],
    items: [
      {
        id: 101,
        title: 'Inception',
        subtitle: 'Your mind is the scene of the crime',
        poster_url: 'https://images.example.com/inception.jpg',
        media_type: 'movie',
        release_year: 2010,
        rating: 8.8,
        badge: '4K',
        playback_progress: 0.5,
      },
      {
        id: 102,
        title: 'The Matrix',
        subtitle: 'Welcome to the Real World',
        poster_url: '',
        media_type: 'movie',
        release_year: 1999,
        rating: 8.7,
      },
    ],
  };

  const mockSubfolderResponse: LibraryFolderResponse = {
    library_id: 'lib-movies-1',
    library_name: 'Movies Library',
    current_path: 'Sci-Fi',
    parent_path: '',
    breadcrumbs: [
      { name: 'Root', path: '' },
      { name: 'Sci-Fi', path: 'Sci-Fi' },
    ],
    directories: [
      { name: 'Cyberpunk', path: 'Sci-Fi/Cyberpunk', item_count: 3 },
    ],
    items: [
      {
        id: 103,
        title: 'Blade Runner 2049',
        subtitle: 'There are still pages left in your story',
        poster_url: 'https://images.example.com/br2049.jpg',
        media_type: 'movie',
        release_year: 2017,
        rating: 8.0,
        badge: 'HDR',
      },
    ],
  };

  const mockEmptyResponse: LibraryFolderResponse = {
    library_id: 'lib-movies-1',
    library_name: 'Movies Library',
    current_path: 'EmptyFolder',
    parent_path: '',
    breadcrumbs: [
      { name: 'Root', path: '' },
      { name: 'EmptyFolder', path: 'EmptyFolder' },
    ],
    directories: [],
    items: [],
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders breadcrumbs and directory cards when browsing root', async () => {
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockRootResponse);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    // Verify loading indicator is present or api is called
    expect(api.getLibraryFolders).toHaveBeenCalledWith(mockLibraryId, '');

    // Wait for directories and breadcrumbs to load
    await waitFor(() => {
      expect(screen.getByText('Root')).toBeInTheDocument();
      expect(screen.getByText('Action')).toBeInTheDocument();
      expect(screen.getByText('Sci-Fi')).toBeInTheDocument();
    });

    // Check item counts
    expect(screen.getByText('5 items')).toBeInTheDocument();
    expect(screen.getByText('12 items')).toBeInTheDocument();

    // In root, "Up one level" button should not be rendered
    expect(screen.queryByRole('button', { name: /up one level/i })).not.toBeInTheDocument();
  });

  it('clicking a directory card calls api.getLibraryFolders with subpath and updates breadcrumbs', async () => {
    const getFoldersSpy = vi.spyOn(api, 'getLibraryFolders')
      .mockResolvedValueOnce(mockRootResponse)
      .mockResolvedValueOnce(mockSubfolderResponse);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Sci-Fi')).toBeInTheDocument();
    });

    // Click Sci-Fi directory card
    fireEvent.click(screen.getByText('Sci-Fi'));

    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenCalledWith(mockLibraryId, 'Sci-Fi');
      expect(screen.getByText('Cyberpunk')).toBeInTheDocument();
      expect(screen.getByText('Blade Runner 2049')).toBeInTheDocument();
    });

    // Should now show "Up one level" button
    expect(screen.getByRole('button', { name: /up one level/i })).toBeInTheDocument();
  });

  it('clicking "Up one level" navigates back to parent folder', async () => {
    const getFoldersSpy = vi.spyOn(api, 'getLibraryFolders')
      .mockResolvedValueOnce(mockSubfolderResponse)
      .mockResolvedValueOnce(mockRootResponse);

    // Initial render in subfolder
    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Cyberpunk')).toBeInTheDocument();
    });

    const upButton = screen.getByRole('button', { name: /up one level/i });
    expect(upButton).toBeInTheDocument();

    fireEvent.click(upButton);

    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenLastCalledWith(mockLibraryId, '');
      expect(screen.getByText('Action')).toBeInTheDocument();
    });
  });

  it('renders media cards with title, rating/badge, and progress', async () => {
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockRootResponse);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeInTheDocument();
      expect(screen.getByText('The Matrix')).toBeInTheDocument();
    });

    // Check badge and release years
    expect(screen.getByText('4K')).toBeInTheDocument();
    expect(screen.getByText(/2010/)).toBeInTheDocument();
    expect(screen.getByText(/1999/)).toBeInTheDocument();

    // Check rating
    expect(screen.getByText(/8.8/)).toBeInTheDocument();
  });

  it('clicking a media card opens ItemDetailsModal; clicking play calls onPlayItem', async () => {
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockRootResponse);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce({
      card: mockRootResponse.items[0],
      genres: ['Sci-Fi', 'Action'],
      duration_seconds: 8880,
      stream_url: '/api/v1/stream/101',
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValueOnce([]);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('Inception')).toBeInTheDocument();
    });

    // Clicking the play CTA button on the poster calls onPlayItem directly
    const playButtons = screen.getAllByRole('button', { name: /play/i });
    expect(playButtons.length).toBeGreaterThan(0);
    fireEvent.click(playButtons[0]);

    expect(mockOnPlayItem).toHaveBeenCalledWith(101);

    // Clicking the media card opens details modal
    fireEvent.click(screen.getByText('Inception'));

    await waitFor(() => {
      expect(api.getItemDetails).toHaveBeenCalledWith(101);
    });
  });

  it('empty state notice when a directory has no subfolders and no media items', async () => {
    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockEmptyResponse);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText('This folder is empty')).toBeInTheDocument();
    });

    // Verify directories and media cards sections are empty
    expect(screen.queryByText('Action')).not.toBeInTheDocument();
    expect(screen.queryByText('Inception')).not.toBeInTheDocument();
  });

  it('error state handling when API fails with button to return to root', async () => {
    const getFoldersSpy = vi.spyOn(api, 'getLibraryFolders')
      .mockRejectedValueOnce(new Error('Network connection lost'))
      .mockResolvedValueOnce(mockRootResponse);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(screen.getByText(/Network connection lost/i)).toBeInTheDocument();
    });

    // "Back to Root" button should be visible
    const backToRootBtn = screen.getByRole('button', { name: /back to root/i });
    expect(backToRootBtn).toBeInTheDocument();

    // Clicking Back to Root resets path and re-fetches
    fireEvent.click(backToRootBtn);

    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenLastCalledWith(mockLibraryId, '');
      expect(screen.getByText('Action')).toBeInTheDocument();
    });
  });

  it('resets currentPath to root when libraryId prop changes', async () => {
    const getFoldersSpy = vi.spyOn(api, 'getLibraryFolders')
      .mockResolvedValueOnce(mockRootResponse)
      .mockResolvedValueOnce(mockSubfolderResponse)
      .mockResolvedValueOnce({
        ...mockRootResponse,
        library_id: 'lib-shows-2',
        library_name: 'TV Shows',
      });

    const { rerender } = render(
      <FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />
    );

    await waitFor(() => {
      expect(screen.getByText('Sci-Fi')).toBeInTheDocument();
    });

    // Navigate to subfolder
    fireEvent.click(screen.getByText('Sci-Fi'));

    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenCalledWith(mockLibraryId, 'Sci-Fi');
      expect(screen.getByText('Cyberpunk')).toBeInTheDocument();
    });

    // Change libraryId prop
    rerender(<FolderBrowser libraryId="lib-shows-2" onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      expect(getFoldersSpy).toHaveBeenLastCalledWith('lib-shows-2', '');
    });
  });

  it('renders dynamic thumbnail endpoint for unindexed synthetic media card', async () => {
    const mockThumbnailResponse: LibraryFolderResponse = {
      library_id: 'lib-movies-1',
      library_name: 'Movies Library',
      current_path: '',
      parent_path: null,
      breadcrumbs: [{ name: 'Root', path: '' }],
      directories: [],
      items: [
        {
          id: 201,
          title: 'clip',
          poster_url: '/api/v1/libraries/lib-movies-1/thumbnail?path=clip.mp4',
          media_type: 'video',
        },
      ],
    };

    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockThumbnailResponse);

    render(<FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />);

    await waitFor(() => {
      const img = screen.getByRole('img', { name: 'clip' });
      expect(img).toBeInTheDocument();
      expect(img).toHaveAttribute('src', '/api/v1/libraries/lib-movies-1/thumbnail?path=clip.mp4');
      expect(img).toHaveAttribute('alt', 'clip');
    });
  });

  it('gracefully degrades to Film icon placeholder on thumbnail error while preserving interactions and play button', async () => {
    const mockThumbnailResponse: LibraryFolderResponse = {
      library_id: 'lib-movies-1',
      library_name: 'Movies Library',
      current_path: '',
      parent_path: null,
      breadcrumbs: [{ name: 'Root', path: '' }],
      directories: [],
      items: [
        {
          id: 201,
          title: 'clip',
          poster_url: '/api/v1/libraries/lib-movies-1/thumbnail?path=clip.mp4',
          media_type: 'video',
        },
      ],
    };

    vi.spyOn(api, 'getLibraryFolders').mockResolvedValueOnce(mockThumbnailResponse);
    vi.spyOn(api, 'getItemDetails').mockResolvedValueOnce({
      card: mockThumbnailResponse.items[0],
      genres: [],
      stream_url: '/api/v1/stream/201',
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValueOnce([]);

    const { container } = render(
      <FolderBrowser libraryId={mockLibraryId} onPlayItem={mockOnPlayItem} />
    );

    // Initial render displays the thumbnail image
    const img = await screen.findByRole('img', { name: 'clip' });
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute('src', '/api/v1/libraries/lib-movies-1/thumbnail?path=clip.mp4');

    // Trigger image error
    fireEvent.error(img);

    // The image should no longer be present
    expect(screen.queryByRole('img', { name: 'clip' })).not.toBeInTheDocument();

    // Film icon placeholder is displayed
    const filmIcon = container.querySelector('svg.lucide-film');
    expect(filmIcon).toBeInTheDocument();

    // Title is still visible (rendered in fallback placeholder and card footer)
    const titleElements = screen.getAllByText('clip');
    expect(titleElements.length).toBeGreaterThanOrEqual(1);

    // Play action button is preserved and callable
    const playBtn = screen.getByRole('button', { name: 'Play clip' });
    expect(playBtn).toBeInTheDocument();
    fireEvent.click(playBtn);
    expect(mockOnPlayItem).toHaveBeenCalledWith(201);

    // Card click interaction still opens item details modal
    fireEvent.click(titleElements[0]);
    await waitFor(() => {
      expect(api.getItemDetails).toHaveBeenCalledWith(201);
    });
  });
});

