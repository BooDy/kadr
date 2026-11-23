import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { ItemDetailsModal } from './ItemDetailsModal';
import { describe, it, expect, vi } from 'vitest';
import { api } from '../../api/client';
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

  it('exposes + Playlist, Add Season, and per-episode Add to playlist actions', async () => {
    const getPlaylistsSpy = vi.spyOn(api, 'getPlaylists').mockResolvedValue({ playlists: [] });

    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: /\+ Playlist/i }));
    await waitFor(() => {
      expect(getPlaylistsSpy).toHaveBeenCalledWith(10);
      expect(
        screen.getByRole('dialog', { name: /Add What We Do in the Shadows to playlist/i })
      ).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: /Close add to playlist modal/i }));

    fireEvent.click(screen.getByRole('button', { name: /Add Season/i }));
    await waitFor(() => {
      expect(
        screen.getByRole('dialog', {
          name: /Add What We Do in the Shadows — Season 4 to playlist/i,
        })
      ).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: /Close add to playlist modal/i }));

    fireEvent.click(screen.getByRole('button', { name: /Add Reunited to playlist/i }));
    await waitFor(() => {
      expect(getPlaylistsSpy).toHaveBeenCalledWith(101);
      expect(screen.getByRole('dialog', { name: /Add Reunited to playlist/i })).toBeInTheDocument();
    });
  });

  it('toggles single movie Mark Watched button to Watched via api.setPlaybackState and hides on live_channel', async () => {
    const setSpy = vi.spyOn(api, 'setPlaybackState').mockResolvedValue({
      user_id: 'u1',
      media_item_id: 20,
      playback_position_seconds: 0,
      watch_state: 'completed',
      last_watched_at: 1000,
      play_count: 1,
    });

    const movieDetails: ItemDetailsPayload = {
      card: {
        id: 20,
        title: 'Interstellar',
        media_type: 'movie',
        badge: 'RESUME',
        playback_progress: 0.4,
      },
      overview: 'A team of explorers travel through a wormhole.',
      genres: ['Sci-Fi'],
      stream_url: '/api/v1/stream/20',
      resume_position_seconds: 1200,
    };

    const { rerender } = render(
      <ItemDetailsModal itemId={20} initialDetails={movieDetails} onClose={vi.fn()} />
    );

    expect(screen.getByText('Mark Watched')).toBeInTheDocument();
    const watchBtn = screen.getByRole('button', { name: 'Mark Interstellar as watched' });
    expect(watchBtn).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(watchBtn);

    await waitFor(() => {
      expect(setSpy).toHaveBeenCalledWith(20, true);
    });
    expect(screen.getByText('Watched')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Mark Interstellar as unwatched' })
    ).toHaveAttribute('aria-pressed', 'true');

    // Hides watch controls completely for live_channel
    rerender(
      <ItemDetailsModal
        itemId={30}
        initialDetails={{
          card: { id: 30, title: 'Live Sports', media_type: 'live_channel' },
          genres: [],
          stream_url: '/api/v1/stream/30',
        }}
        onClose={vi.fn()}
      />
    );
    expect(
      screen.queryByRole('button', { name: /Mark Live Sports as/i })
    ).not.toBeInTheDocument();
  });

  it('supports Mark Season Watched, Mark Series Watched batch actions, and per-episode watched toggle', async () => {
    const batchSpy = vi
      .spyOn(api, 'setPlaybackStatesBatch')
      .mockResolvedValue({ updated: 2, watched: true });
    const singleSpy = vi.spyOn(api, 'setPlaybackState').mockResolvedValue({
      user_id: 'u1',
      media_item_id: 201,
      playback_position_seconds: 0,
      watch_state: 'completed',
      last_watched_at: 1000,
      play_count: 1,
    });

    render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={vi.fn()}
      />
    );

    // Selected season defaults to Season 4 (episodes 101, 102)
    const seasonBtn = screen.getByRole('button', { name: 'Mark Season Watched' });
    expect(seasonBtn).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(seasonBtn);

    await waitFor(() => {
      expect(batchSpy).toHaveBeenCalledWith([101, 102], true);
    });
    expect(screen.getByRole('button', { name: 'Season Watched' })).toHaveAttribute(
      'aria-pressed',
      'true'
    );
    // Series still has unwatched Season 5 episode 201
    expect(screen.getByRole('button', { name: 'Mark Series Watched' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );

    // Switch to Season 5 and toggle episode 201 individually via row WatchStateButton
    fireEvent.click(screen.getByRole('button', { name: /Season 5/i }));
    const epBtn = screen.getByRole('button', { name: 'Mark Mall Stories as watched' });
    fireEvent.click(epBtn);

    await waitFor(() => {
      expect(singleSpy).toHaveBeenCalledWith(201, true);
    });

    // Now all episodes across all seasons are watched!
    expect(screen.getByRole('button', { name: 'Series Watched' })).toHaveAttribute(
      'aria-pressed',
      'true'
    );

    // Clicking Series Watched marks entire series unwatched ([101, 102, 201], false)
    batchSpy.mockResolvedValueOnce({ updated: 3, watched: false });
    fireEvent.click(screen.getByRole('button', { name: 'Series Watched' }));

    await waitFor(() => {
      expect(batchSpy).toHaveBeenLastCalledWith([101, 102, 201], false);
    });
    expect(screen.getByRole('button', { name: 'Mark Series Watched' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );

    // Rolls back optimistic batch state if api.setPlaybackStatesBatch rejects
    batchSpy.mockRejectedValueOnce(new Error('batch network failure'));
    fireEvent.click(screen.getByRole('button', { name: 'Mark Series Watched' }));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Mark Series Watched' })).toHaveAttribute(
        'aria-pressed',
        'false'
      );
    });

    // Disables both Series and Season batch buttons while either batch request is in-flight
    let resolveInFlightBatch!: (val: { updated: number; watched: boolean }) => void;
    batchSpy.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveInFlightBatch = resolve;
        })
    );
    fireEvent.click(screen.getByRole('button', { name: 'Mark Season Watched' }));
    expect(screen.getByRole('button', { name: 'Season Watched' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Mark Series Watched' })).toBeDisabled();
    resolveInFlightBatch({ updated: 1, watched: true });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Season Watched' })).not.toBeDisabled();
      expect(screen.getByRole('button', { name: 'Mark Series Watched' })).not.toBeDisabled();
    });
  });

  it('renders technical audio stream badges from details.technical.audio_tracks and falls back to api.getAudioTracks', async () => {
    const movieWithAudioTracks: ItemDetailsPayload = {
      card: {
        id: 50,
        title: 'Spirited Away',
        media_type: 'movie',
      },
      genres: ['Animation', 'Fantasy'],
      stream_url: '/api/v1/stream/50',
      technical: {
        duration_seconds: 7500,
        resolution: '1080p',
        video_codec: 'h264',
        audio_codec: 'eac3',
        audio_channels: 6,
        container: 'mkv',
        audio_tracks: [
          {
            audio_index: 0,
            stream_index: 1,
            codec: 'aac',
            channels: 2,
            language: 'eng',
            is_default: true,
            is_commentary: false,
          },
          {
            audio_index: 1,
            stream_index: 2,
            codec: 'eac3',
            channels: 6,
            language: 'jpn',
            is_default: false,
            is_commentary: false,
          },
        ],
      },
    };

    const { unmount } = render(
      <ItemDetailsModal itemId={50} initialDetails={movieWithAudioTracks} onClose={vi.fn()} />
    );

    expect(screen.getByText('Audio Streams')).toBeInTheDocument();
    expect(screen.getByText('English • AAC Stereo')).toBeInTheDocument();
    expect(screen.getByText('Japanese • EAC3 5.1')).toBeInTheDocument();

    unmount();

    // Fallback to api.getAudioTracks when details.technical.audio_tracks is absent
    const getAudioSpy = vi.spyOn(api, 'getAudioTracks').mockResolvedValue([
      {
        audio_index: 0,
        stream_index: 1,
        codec: 'flac',
        channels: 2,
        language: 'fra',
        is_default: true,
        is_commentary: false,
      },
    ]);

    render(
      <ItemDetailsModal
        itemId={51}
        initialDetails={{
          card: { id: 51, title: 'Amélie', media_type: 'movie' },
          genres: ['Comedy'],
          stream_url: '/api/v1/stream/51',
          technical: { duration_seconds: 7320, container: 'mkv' },
        }}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(getAudioSpy).toHaveBeenCalledWith(51);
      expect(screen.getByText('French • FLAC Stereo')).toBeInTheDocument();
    });
  });

  it('renders FavoriteButton in the modal header (including TV Show and live_channel) and on EpisodeCard rows', async () => {
    const addSpy = vi
      .spyOn(api, 'addFavorite')
      .mockImplementation(async (id: number) => ({ item_id: id, is_favorite: true }));
    const removeSpy = vi
      .spyOn(api, 'removeFavorite')
      .mockImplementation(async (id: number) => ({ item_id: id, is_favorite: false }));
    const onPlayItem = vi.fn();

    const { rerender } = render(
      <ItemDetailsModal
        itemId={10}
        initialDetails={mockShowDetails}
        onClose={vi.fn()}
        onPlayItem={onPlayItem}
      />
    );

    // Header FavoriteButton works on TV Show container items
    const showFavBtn = screen.getByRole('button', {
      name: 'Add What We Do in the Shadows to favorites',
    });
    expect(showFavBtn).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(showFavBtn);

    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith(10);
    });
    expect(
      screen.getByRole('button', {
        name: 'Remove What We Do in the Shadows from favorites',
      })
    ).toHaveAttribute('aria-pressed', 'true');

    // EpisodeCard rows render always-visible FavoriteButton alongside WatchStateButton, + Playlist, and Play
    const epFavBtn = screen.getByRole('button', { name: 'Add Reunited to favorites' });
    expect(epFavBtn.className).toContain('opacity-100');
    fireEvent.click(epFavBtn);

    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith(101);
    });
    expect(onPlayItem).not.toHaveBeenCalled();
    expect(
      screen.getByRole('button', { name: 'Remove Reunited from favorites' })
    ).toHaveAttribute('aria-pressed', 'true');

    // Switch to Season 5 and back to Season 4: episode favorite state persists on details.episodes
    fireEvent.click(screen.getByRole('button', { name: /Season 5/i }));
    fireEvent.click(screen.getByRole('button', { name: /Season 4/i }));
    const persistedEpBtn = screen.getByRole('button', {
      name: 'Remove Reunited from favorites',
    });
    expect(persistedEpBtn).toHaveAttribute('aria-pressed', 'true');

    // Removing favorite from EpisodeCard calls removeFavorite
    fireEvent.click(persistedEpBtn);
    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith(101);
    });

    // Header FavoriteButton also renders on live_channel items
    rerender(
      <ItemDetailsModal
        itemId={30}
        initialDetails={{
          card: { id: 30, title: 'Live Sports', media_type: 'live_channel' },
          genres: [],
          stream_url: '/api/v1/stream/30',
        }}
        onClose={vi.fn()}
      />
    );
    expect(
      screen.getByRole('button', { name: 'Add Live Sports to favorites' })
    ).toHaveAttribute('aria-pressed', 'false');
  });
});


