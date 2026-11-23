import { describe, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PlayerControls, type PlayerControlsProps } from './PlayerControls';
import type { SubtitleTrack, SubtitleSearchResult } from '../../types';

describe('PlayerControls Component', () => {
  const defaultProps: PlayerControlsProps = {
    title: 'Interstellar',
    subtitle: 'Christopher Nolan',
    releaseYear: 2014,
    isPlaying: true,
    currentTime: 120,
    duration: 7200,
    volume: 0.8,
    isMuted: false,
    isFullscreen: false,
    subtitles: [],
    activeSubtitleId: null,
    isVisible: true,
    onPlayPause: vi.fn(),
    onSeek: vi.fn(),
    onVolumeChange: vi.fn(),
    onToggleMute: vi.fn(),
    onToggleFullscreen: vi.fn(),
    onSelectSubtitle: vi.fn(),
    onClose: vi.fn(),
  };

  const sampleTracks: SubtitleTrack[] = [
    {
      id: 1,
      media_item_id: 10,
      source: 'sidecar',
      language: 'en',
      title: 'English CC',
      format: 'srt',
      is_default: true,
      is_forced: false,
      stream_url: '/api/v1/subtitles/1/stream.vtt',
    },
    {
      id: 2,
      media_item_id: 10,
      source: 'downloaded',
      language: 'es',
      title: 'Spanish',
      format: 'srt',
      is_default: false,
      is_forced: false,
      stream_url: '/api/v1/subtitles/2/stream.vtt',
    },
  ];

  it('renders search online button when no subtitles and switches to search view', async () => {
    const onSearch = vi.fn().mockResolvedValue([
      { id: 'sub-1', language: 'en', format: 'srt', release_name: 'Interstellar.1080p' },
    ]);
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(screen.getByRole('button', { name: /Search OpenSubtitles Online/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));
    expect(onSearch).toHaveBeenCalledWith('en');
    expect(await screen.findByText(/Interstellar.1080p/i)).toBeInTheDocument();
  });

  it('allows entering a custom language filter and searching', async () => {
    const onSearch = vi.fn().mockResolvedValue([
      { id: 'sub-fr-1', language: 'fr', format: 'srt', release_name: 'Interstellar.French' },
    ]);
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));

    const input = screen.getByPlaceholderText(/Language/i);
    fireEvent.change(input, { target: { value: 'fr' } });
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    expect(onSearch).toHaveBeenCalledWith('fr');
    expect(await screen.findByText(/Interstellar.French/i)).toBeInTheDocument();
  });

  it('falls back to "en" when search input is empty or whitespace', async () => {
    const onSearch = vi.fn().mockResolvedValue([]);
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));

    const input = screen.getByPlaceholderText(/Language/i);
    fireEvent.change(input, { target: { value: '   ' } });
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    expect(onSearch).toHaveBeenCalledWith('en');
  });

  it('clicking download on a match calls onDownloadSubtitle and shows loading state', async () => {
    const searchMatch: SubtitleSearchResult = {
      id: 'sub-1',
      language: 'en',
      format: 'srt',
      release_name: 'Interstellar.1080p',
    };
    const onSearch = vi.fn().mockResolvedValue([searchMatch]);
    let resolveDownload: () => void = () => {};
    const onDownload = vi.fn().mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveDownload = resolve;
        })
    );

    render(
      <PlayerControls
        {...defaultProps}
        subtitles={[]}
        onSearchSubtitles={onSearch}
        onDownloadSubtitle={onDownload}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    await screen.findByText(/Interstellar.1080p/i);
    const downloadBtn = screen.getByRole('button', { name: /Download/i });
    fireEvent.click(downloadBtn);

    expect(onDownload).toHaveBeenCalledWith(searchMatch);
    expect(screen.getByText(/Downloading/i)).toBeInTheDocument();

    resolveDownload();
    await waitFor(() => {
      expect(screen.queryByText(/Downloading/i)).not.toBeInTheDocument();
    });
  });

  it('displays prompt banner when recentlyDownloadedId is provided', () => {
    render(
      <PlayerControls
        {...defaultProps}
        subtitles={sampleTracks}
        recentlyDownloadedId={2}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(screen.getByText(/Subtitle downloaded! Click below to display on screen/i)).toBeInTheDocument();
  });

  it('renders "+ Search & Download Online" button when subtitles are present and switches to search view', () => {
    render(<PlayerControls {...defaultProps} subtitles={sampleTracks} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));

    expect(screen.getByText('English CC')).toBeInTheDocument();
    expect(screen.getByText('Spanish')).toBeInTheDocument();
    const searchBtn = screen.getByRole('button', { name: /\+ Search & Download Online/i });
    expect(searchBtn).toBeInTheDocument();

    fireEvent.click(searchBtn);
    expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
  });

  it('navigates back to track list when clicking back button in search view', () => {
    render(<PlayerControls {...defaultProps} subtitles={sampleTracks} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /\+ Search & Download Online/i }));

    expect(screen.getByPlaceholderText(/Language/i)).toBeInTheDocument();
    const backBtn = screen.getByRole('button', { name: /Back to Subtitles/i });
    fireEvent.click(backBtn);

    expect(screen.getByText('English CC')).toBeInTheDocument();
  });

  it('notifies parent via onSubtitlesMenuToggle when menu opens and closes', () => {
    const onToggle = vi.fn();
    render(<PlayerControls {...defaultProps} onSubtitlesMenuToggle={onToggle} />);

    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(onToggle).toHaveBeenCalledWith(true);

    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it('displays search error when onSearchSubtitles fails', async () => {
    const onSearch = vi.fn().mockRejectedValue(new Error('OpenSubtitles unconfigured'));
    render(<PlayerControls {...defaultProps} subtitles={[]} onSearchSubtitles={onSearch} />);
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    expect(await screen.findByText(/Failed to search online subtitles/i)).toBeInTheDocument();
  });

  it('displays download error when onDownloadSubtitle fails', async () => {
    const searchMatch: SubtitleSearchResult = {
      id: 'sub-fail',
      language: 'en',
      format: 'srt',
      release_name: 'Failed.Release',
    };
    const onSearch = vi.fn().mockResolvedValue([searchMatch]);
    const onDownload = vi.fn().mockRejectedValue(new Error('Network error'));

    render(
      <PlayerControls
        {...defaultProps}
        subtitles={[]}
        onSearchSubtitles={onSearch}
        onDownloadSubtitle={onDownload}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    fireEvent.click(screen.getByRole('button', { name: /Search OpenSubtitles Online/i }));
    fireEvent.click(screen.getByRole('button', { name: /Search/i }));

    await screen.findByText(/Failed.Release/i);
    fireEvent.click(screen.getByRole('button', { name: /Download/i }));

    expect(await screen.findByText(/Failed to download subtitle/i)).toBeInTheDocument();
  });

  it('renders Previous/Next track controls, track counter, and Queue popover drawer when queue has >1 items', () => {
    const queue = [
      {
        position: 0,
        added_at: 100,
        duration_seconds: 7200,
        card: { id: 10, title: 'Arrival', media_type: 'movie' },
      },
      {
        position: 1,
        added_at: 200,
        duration_seconds: 9800,
        card: { id: 14, title: 'Blade Runner 2049', media_type: 'movie' },
      },
    ];
    const onSelectQueueIndex = vi.fn();
    const onPrevTrack = vi.fn();
    const onNextTrack = vi.fn();
    const onQueueMenuToggle = vi.fn();

    render(
      <PlayerControls
        {...defaultProps}
        returnLabel="Back to Playlist"
        queue={queue}
        currentIndex={0}
        onSelectQueueIndex={onSelectQueueIndex}
        onPrevTrack={onPrevTrack}
        onNextTrack={onNextTrack}
        onQueueMenuToggle={onQueueMenuToggle}
      />
    );

    expect(screen.getByRole('button', { name: 'Back to Playlist' })).toBeInTheDocument();
    expect(screen.getByText('1 / 2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Next track' }));
    expect(onNextTrack).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: 'Previous track' }));
    expect(onPrevTrack).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: 'Playback Queue' }));
    expect(onQueueMenuToggle).toHaveBeenCalledWith(true);
    expect(screen.getByText('Blade Runner 2049')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Blade Runner 2049/i }));
    expect(onSelectQueueIndex).toHaveBeenCalledWith(1);
  });

  it('renders Audio Tracks menu with role="menuitemradio", aria-checked, badges, mutual exclusion, and Escape focus restore', () => {
    const onSelectAudioTrack = vi.fn();
    const onAudioMenuToggle = vi.fn();
    const onSubtitlesMenuToggle = vi.fn();
    render(
      <PlayerControls
        {...defaultProps}
        audioTracks={[
          {
            audio_index: 0,
            stream_index: 1,
            codec: 'aac',
            channels: 2,
            language: 'eng',
            title: 'English Stereo',
            is_default: true,
            is_commentary: false,
          },
          {
            audio_index: 1,
            stream_index: 2,
            codec: 'eac3',
            channels: 6,
            language: 'jpn',
            title: 'Director Commentary',
            is_default: false,
            is_commentary: true,
          },
        ]}
        activeAudioTrackIndex={0}
        onSelectAudioTrack={onSelectAudioTrack}
        onAudioMenuToggle={onAudioMenuToggle}
        onSubtitlesMenuToggle={onSubtitlesMenuToggle}
      />
    );

    const trigger = screen.getByRole('button', { name: /audio tracks/i });
    expect(trigger).toHaveAttribute('aria-haspopup', 'menu');
    expect(trigger).toHaveAttribute('aria-expanded', 'false');

    // Open Subtitles first, then open Audio Tracks -> Subtitles closes mutually exclusively
    fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
    expect(onSubtitlesMenuToggle).toHaveBeenCalledWith(true);

    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(onAudioMenuToggle).toHaveBeenCalledWith(true);
    expect(onSubtitlesMenuToggle).toHaveBeenCalledWith(false);

    const radios = screen.getAllByRole('menuitemradio');
    expect(radios).toHaveLength(2);
    expect(radios[0]).toHaveAttribute('aria-checked', 'true');
    expect(radios[1]).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByText('AAC')).toBeInTheDocument();
    expect(screen.getByText('Stereo')).toBeInTheDocument();
    expect(screen.getByText('Default')).toBeInTheDocument();
    expect(screen.getByText('EAC3')).toBeInTheDocument();
    expect(screen.getByText('5.1')).toBeInTheDocument();
    expect(screen.getByText('Commentary')).toBeInTheDocument();

    // Arrow key navigation moves between menuitemradio options
    radios[0].focus();
    fireEvent.keyDown(screen.getByRole('menu', { name: /audio tracks/i }), {
      key: 'ArrowDown',
    });
    expect(document.activeElement).toBe(radios[1]);

    // Escape closes menu and restores focus to trigger
    fireEvent.keyDown(screen.getByRole('menu', { name: /audio tracks/i }), {
      key: 'Escape',
    });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(onAudioMenuToggle).toHaveBeenCalledWith(false);
    expect(document.activeElement).toBe(trigger);

    fireEvent.click(trigger);
    fireEvent.click(screen.getAllByRole('menuitemradio')[1]);
    expect(onSelectAudioTrack).toHaveBeenCalledWith(1);
  });

  it('renders SeekPreviewThumbnail on pointer hover over seek bar with finite math and hides on pointer leave', () => {
    const manifest = {
      interval_ms: 10000,
      tile_width: 320,
      tile_height: 180,
      columns: 10,
      rows: 10,
      total_tiles: 720,
      sheets: [
        { index: 0, url: '/api/v1/media/10/storyboard/0.jpg', first_tile: 0, tile_count: 720 },
      ],
    };

    render(
      <PlayerControls
        {...defaultProps}
        itemId={10}
        duration={7200}
        storyboard={{ status: 'ready', manifest }}
      />
    );

    const seekInput = screen.getByRole('slider', { name: /seek/i });
    const seekContainer = seekInput.parentElement as HTMLElement;

    // Zero-width rect must not render NaN preview
    vi.spyOn(seekContainer, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      top: 0,
      width: 0,
      height: 10,
      right: 0,
      bottom: 10,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    fireEvent(seekContainer, new MouseEvent('pointermove', { bubbles: true, clientX: 50 }));
    expect(screen.queryByTestId('seek-preview-sprite')).not.toBeInTheDocument();

    // Valid rect renders preview at 25% of 7200s = 1800s (30:00)
    vi.spyOn(seekContainer, 'getBoundingClientRect').mockReturnValue({
      left: 100,
      top: 0,
      width: 400,
      height: 10,
      right: 500,
      bottom: 10,
      x: 100,
      y: 0,
      toJSON: () => ({}),
    });
    fireEvent(seekContainer, new MouseEvent('pointermove', { bubbles: true, clientX: 200 }));
    expect(screen.getByTestId('seek-preview-sprite')).toBeInTheDocument();
    expect(screen.getByText('30:00')).toBeInTheDocument();

    fireEvent.pointerLeave(seekContainer);
    expect(screen.queryByTestId('seek-preview-sprite')).not.toBeInTheDocument();
  });

  it('resets open Audio Tracks menu, hover preview, and capture-phase Escape listener on itemId transition or empty audioTracks', () => {
    const onAudioMenuToggle = vi.fn();
    const manifest = {
      interval_ms: 10000,
      tile_width: 320,
      tile_height: 180,
      columns: 10,
      rows: 10,
      total_tiles: 720,
      sheets: [
        { index: 0, url: '/api/v1/media/10/storyboard/0.jpg', first_tile: 0, tile_count: 720 },
      ],
    };
    const tracks = [
      {
        audio_index: 0,
        stream_index: 1,
        codec: 'aac',
        channels: 2,
        language: 'eng',
        title: 'English',
        is_default: true,
        is_commentary: false,
      },
    ];

    const { rerender } = render(
      <PlayerControls
        {...defaultProps}
        itemId={10}
        audioTracks={tracks}
        activeAudioTrackIndex={0}
        onAudioMenuToggle={onAudioMenuToggle}
        storyboard={{ status: 'ready', manifest }}
      />
    );

    // Activate hover preview and open Audio Tracks menu (single track omits Default pill)
    const seekContainer = screen.getByRole('slider', { name: /seek/i })
      .parentElement as HTMLElement;
    vi.spyOn(seekContainer, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      top: 0,
      width: 400,
      height: 10,
      right: 400,
      bottom: 10,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    fireEvent(seekContainer, new MouseEvent('pointermove', { bubbles: true, clientX: 200 }));
    expect(screen.getByTestId('seek-preview-sprite')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /audio tracks/i }));
    expect(screen.getByRole('menu', { name: /audio tracks/i })).toBeInTheDocument();
    expect(screen.queryByText('Default')).not.toBeInTheDocument();
    onAudioMenuToggle.mockClear();

    // Transition itemId from 10 -> 20 while audioTracks remain present: closes menu & clears hover preview
    rerender(
      <PlayerControls
        {...defaultProps}
        itemId={20}
        audioTracks={tracks}
        activeAudioTrackIndex={0}
        onAudioMenuToggle={onAudioMenuToggle}
        storyboard={{ status: 'ready', manifest }}
      />
    );

    expect(screen.queryByRole('menu', { name: /audio tracks/i })).not.toBeInTheDocument();
    expect(screen.queryByTestId('seek-preview-sprite')).not.toBeInTheDocument();
    expect(onAudioMenuToggle).toHaveBeenCalledWith(false);

    // Escape event is NOT intercepted when menu was auto-closed
    const escAfterItemChange = new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(escAfterItemChange);
    expect(escAfterItemChange.defaultPrevented).toBe(false);

    // Reopen Audio Tracks on item 20, then clear audioTracks -> closes menu and disarms Escape listener
    fireEvent.click(screen.getByRole('button', { name: /audio tracks/i }));
    expect(screen.getByRole('menu', { name: /audio tracks/i })).toBeInTheDocument();
    onAudioMenuToggle.mockClear();

    rerender(
      <PlayerControls
        {...defaultProps}
        itemId={20}
        audioTracks={[]}
        activeAudioTrackIndex={0}
        onAudioMenuToggle={onAudioMenuToggle}
        storyboard={{ status: 'ready', manifest }}
      />
    );

    expect(onAudioMenuToggle).toHaveBeenCalledWith(false);
    const escWithoutTracks = new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(escWithoutTracks);
    expect(escWithoutTracks.defaultPrevented).toBe(false);
  });
});
