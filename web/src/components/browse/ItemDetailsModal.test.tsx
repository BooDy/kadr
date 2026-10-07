import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { ItemDetailsModal } from './ItemDetailsModal';
import { describe, it, expect, vi } from 'vitest';
import type { ItemDetailsPayload } from '../../types';

describe('ItemDetailsModal Seasons & Episodes', () => {
  const mockShowDetails: ItemDetailsPayload = {
    card: {
      id: 10,
      title: 'What We Do in the Shadows',
      media_type: 'show',
    },
    overview: 'Vampire roommates in Staten Island.',
    genres: ['Comedy'],
    stream_url: '/api/v1/stream/10',
    episodes: [
      {
        id: 101,
        title: 'Reunited',
        subtitle: 'S04E01',
        media_type: 'episode',
        season: 4,
        episode: 1,
      },
      {
        id: 102,
        title: 'The Lamp',
        subtitle: 'S04E02',
        media_type: 'episode',
        season: 4,
        episode: 2,
      },
      {
        id: 201,
        title: 'Mall Stories',
        subtitle: 'S05E01',
        media_type: 'episode',
        season: 5,
        episode: 1,
      },
    ],
  };

  it('renders season selector tabs and filters episodes by selected season', async () => {
    const onPlayItem = vi.fn();
    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={onPlayItem}
      />
    );

    // Verify season tabs appear
    expect(screen.getByRole('button', { name: /Season 4/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Season 5/i })).toBeInTheDocument();

    // Default season 4 episodes visible
    expect(screen.getByText('Reunited')).toBeInTheDocument();
    expect(screen.getByText('The Lamp')).toBeInTheDocument();
    expect(screen.queryByText('Mall Stories')).not.toBeInTheDocument();

    // Click Season 5
    fireEvent.click(screen.getByRole('button', { name: /Season 5/i }));
    expect(screen.getByText('Mall Stories')).toBeInTheDocument();
    expect(screen.queryByText('Reunited')).not.toBeInTheDocument();
  });

  it('clicking an episode play button triggers onPlayItem with episode id', async () => {
    const onPlayItem = vi.fn();
    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={onPlayItem}
      />
    );

    const playBtn = screen.getByRole('button', { name: /Play Reunited/i });
    fireEvent.click(playBtn);
    expect(onPlayItem).toHaveBeenCalledWith(101);
  });

  it('clicking or pressing Enter on the episode thumbnail triggers onPlayItem with episode id', async () => {
    const onPlayItem = vi.fn();
    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={onPlayItem}
      />
    );

    const thumbBtn = screen.getByRole('button', { name: /Play episode Reunited/i });

    // Click thumbnail container
    fireEvent.click(thumbBtn);
    expect(onPlayItem).toHaveBeenCalledWith(101);

    // Press Enter on thumbnail container
    onPlayItem.mockClear();
    fireEvent.keyDown(thumbBtn, { key: 'Enter', code: 'Enter' });
    expect(onPlayItem).toHaveBeenCalledWith(101);
  });

  it('does not render season tabs or episodes section when item has no episodes', () => {
    const mockMovieDetails: ItemDetailsPayload = {
      card: {
        id: 20,
        title: 'Interstellar',
        media_type: 'movie',
      },
      overview: 'A team of explorers travel through a wormhole.',
      genres: ['Sci-Fi', 'Adventure'],
      stream_url: '/api/v1/stream/20',
    };

    render(
      <ItemDetailsModal
        itemId={20}
        initialDetails={mockMovieDetails}
        onClose={vi.fn()}
      />
    );

    expect(screen.queryByText('Episodes')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Season/i })).not.toBeInTheDocument();
  });

  it('falls back to onPlay when onPlayItem is not provided', () => {
    const onPlay = vi.fn();
    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlay={onPlay}
      />
    );

    const playBtn = screen.getByRole('button', { name: /Play The Lamp/i });
    fireEvent.click(playBtn);
    expect(onPlay).toHaveBeenCalledWith(102);
  });

  it('renders episode progress bar and duration when provided', () => {
    const detailsWithProgress: ItemDetailsPayload = {
      ...mockShowDetails,
      episodes: [
        {
          id: 301,
          title: 'Special Episode',
          media_type: 'episode',
          season: 1,
          episode: 1,
          playback_progress: 0.75,
          duration_seconds: 3600,
        },
      ],
    };

    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={detailsWithProgress}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText('Special Episode')).toBeInTheDocument();
    expect(screen.getByText('E01')).toBeInTheDocument();
    // 3600s = 1h 0m
    expect(screen.getAllByText('1h 0m').length).toBeGreaterThan(0);
  });

  it('renders "Browse Folder" button when library_id is provided and calls onNavigateToFolder', () => {
    const onNavigateToFolder = vi.fn();
    const onClose = vi.fn();
    const mockMovieDetails: ItemDetailsPayload = {
      card: {
        id: 101,
        title: 'Inception',
        media_type: 'movie',
      },
      overview: 'A thief who steals corporate secrets...',
      genres: ['Action', 'Sci-Fi'],
      stream_url: '/api/v1/stream/101',
    };
    const detailsWithFolder: ItemDetailsPayload = {
      ...mockMovieDetails,
      library_id: 'lib-movies-1',
      folder_path: 'Sci-Fi/Inception (2010)',
    };

    render(
      <ItemDetailsModal
        isOpen={true}
        onClose={onClose}
        itemId={101}
        initialDetails={detailsWithFolder}
        onPlay={vi.fn()}
        onNavigateToFolder={onNavigateToFolder}
      />
    );

    const browseBtn = screen.getByRole('button', { name: /browse folder/i });
    expect(browseBtn).toBeInTheDocument();
    fireEvent.click(browseBtn);

    expect(onClose).toHaveBeenCalled();
    expect(onNavigateToFolder).toHaveBeenCalledWith('lib-movies-1', 'Sci-Fi/Inception (2010)');
  });

  it('does not render "Browse Folder" button when library_id is omitted or onNavigateToFolder not passed', () => {
    const mockMovieDetails: ItemDetailsPayload = {
      card: {
        id: 101,
        title: 'Inception',
        media_type: 'movie',
      },
      overview: 'A thief who steals corporate secrets...',
      genres: ['Action', 'Sci-Fi'],
      stream_url: '/api/v1/stream/101',
    };
    const detailsWithoutFolder: ItemDetailsPayload = {
      ...mockMovieDetails,
      library_id: undefined,
      folder_path: undefined,
    };

    render(
      <ItemDetailsModal
        isOpen={true}
        onClose={vi.fn()}
        itemId={101}
        initialDetails={detailsWithoutFolder}
        onPlay={vi.fn()}
      />
    );

    expect(screen.queryByRole('button', { name: /browse folder/i })).not.toBeInTheDocument();
  });
});

