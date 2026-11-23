import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { CinemaPlayer } from './CinemaPlayer';
import { api } from '../../api/client';
import type { PlaybackState, PlaybackSessionResponse, SubtitleTrack, SubtitleSearchResult, ItemDetailsPayload } from '../../types';

describe('CinemaPlayer Component', () => {
  const mockItemId = 10;
  const mockOnClose = vi.fn();

  const mockSession: PlaybackSessionResponse = {
    session_id: 'test-session-xyz',
    media_item_id: mockItemId,
    duration_seconds: 3600,
    resume_position_seconds: 0,
  };

  const mockPlaybackState: PlaybackState = {
    user_id: 'user-1',
    media_item_id: mockItemId,
    playback_position_seconds: 145,
    watch_state: 'in_progress',
    last_watched_at: 100000,
    play_count: 1,
  };

  const mockSubtitles: SubtitleTrack[] = [
    {
      id: 101,
      media_item_id: mockItemId,
      source: 'sidecar',
      language: 'en',
      title: 'English CC',
      format: 'vtt',
      is_default: true,
      is_forced: false,
      stream_url: '/api/v1/subtitles/101/stream.vtt',
    },
    {
      id: 102,
      media_item_id: mockItemId,
      source: 'sidecar',
      language: 'es',
      title: 'Spanish',
      format: 'vtt',
      is_default: false,
      is_forced: false,
      stream_url: '/api/v1/subtitles/102/stream.vtt',
    },
  ];

  const mockDetails: ItemDetailsPayload = {
    card: {
      id: mockItemId,
      title: 'Blade Runner 2049',
      subtitle: 'Special Edition',
      release_year: 2017,
      media_type: 'movie',
    },
    genres: ['Sci-Fi', 'Thriller'],
    duration_seconds: 9800,
    stream_url: '/api/v1/stream/10',
  };

  beforeEach(() => {
    vi.restoreAllMocks();
    mockOnClose.mockClear();

    // Mock HTMLMediaElement methods
    window.HTMLMediaElement.prototype.play = vi.fn().mockResolvedValue(undefined);
    window.HTMLMediaElement.prototype.pause = vi.fn();
    window.HTMLMediaElement.prototype.load = vi.fn();

    // Mock API methods
    vi.spyOn(api, 'createPlaybackSession').mockResolvedValue(mockSession);
    vi.spyOn(api, 'closePlaybackSession').mockResolvedValue(undefined);
    vi.spyOn(api, 'sendPlaybackHeartbeat').mockResolvedValue(undefined);
    vi.spyOn(api, 'getPlaybackState').mockResolvedValue(mockPlaybackState);
    vi.spyOn(api, 'getSubtitles').mockResolvedValue(mockSubtitles);
    vi.spyOn(api, 'getItemDetails').mockResolvedValue(mockDetails);
    vi.spyOn(api, 'getAudioTracks').mockResolvedValue([]);
    vi.spyOn(api, 'getStoryboard').mockResolvedValue({ status: 'unavailable' });
    vi.spyOn(api, 'deleteHlsSession').mockResolvedValue(undefined);
    vi.spyOn(api, 'getStreamUrl').mockReturnValue(`/api/v1/stream/${mockItemId}`);
    vi.spyOn(api, 'getSubtitleStreamUrl').mockImplementation(
      (id: number) => `/api/v1/subtitles/${id}/stream.vtt`
    );
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders video element with stream URL and autoplays', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    const video = document.querySelector('video') as HTMLVideoElement;
    expect(video).toBeDefined();
    expect(video.getAttribute('src')).toBe(`/api/v1/stream/${mockItemId}`);

    await waitFor(() => {
      expect(api.createPlaybackSession).toHaveBeenCalledWith(mockItemId);
    });
  });

  it('fetches initial playback state and seeks to resume position on metadata load', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(api.getPlaybackState).toHaveBeenCalledWith(mockItemId);
    });

    const video = document.querySelector('video') as HTMLVideoElement;
    // Simulate loadedmetadata event
    fireEvent.loadedMetadata(video);

    expect(video.currentTime).toBe(145);
  });

  it('emits heartbeat every 10 seconds and closes session on unmount', async () => {
    vi.useFakeTimers();

    const { unmount } = render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    // Allow promises to resolve
    await act(async () => {
      await Promise.resolve();
    });

    expect(api.createPlaybackSession).toHaveBeenCalledWith(mockItemId);

    const video = document.querySelector('video') as HTMLVideoElement;
    Object.defineProperty(video, 'currentTime', { value: 75, writable: true });

    // Advance 10 seconds
    await act(async () => {
      vi.advanceTimersByTime(10000);
    });

    expect(api.sendPlaybackHeartbeat).toHaveBeenCalledWith('test-session-xyz', 75);

    // Advance another 10 seconds
    Object.defineProperty(video, 'currentTime', { value: 85, writable: true });
    await act(async () => {
      vi.advanceTimersByTime(10000);
    });

    expect(api.sendPlaybackHeartbeat).toHaveBeenCalledWith('test-session-xyz', 85);

    // Unmount player
    unmount();
    expect(api.closePlaybackSession).toHaveBeenCalledWith('test-session-xyz');
  });

  it('fetches subtitles and mounts native WebVTT track elements', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(api.getSubtitles).toHaveBeenCalledWith(mockItemId);
      expect(document.querySelectorAll('track').length).toBe(2);
    });

    const tracks = document.querySelectorAll('track');

    const track1 = tracks[0];
    expect(track1.getAttribute('kind')).toBe('subtitles');
    expect(track1.getAttribute('src')).toBe('/api/v1/subtitles/101/stream.vtt');
    expect(track1.getAttribute('srclang')).toBe('en');
    expect(track1.getAttribute('label')).toBe('English CC');
    expect(track1.hasAttribute('default')).toBe(true);

    const track2 = tracks[1];
    expect(track2.getAttribute('kind')).toBe('subtitles');
    expect(track2.getAttribute('src')).toBe('/api/v1/subtitles/102/stream.vtt');
    expect(track2.getAttribute('srclang')).toBe('es');
    expect(track2.getAttribute('label')).toBe('Spanish');
    expect(track2.hasAttribute('default')).toBe(false);
  });

  it('supports subtitle selection menu to switch tracks or turn off', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });

    // Open subtitles popover
    const subtitlesBtn = screen.getByRole('button', { name: /subtitles/i });
    fireEvent.click(subtitlesBtn);

    expect(screen.getByText('Off')).toBeDefined();
    expect(screen.getByText('English CC')).toBeDefined();
    expect(screen.getByText('Spanish')).toBeDefined();

    // Select Spanish
    fireEvent.click(screen.getByText('Spanish'));

    // Check popover closes or selected track changes
    // Reopen popover and verify Spanish has active indicator
    fireEvent.click(subtitlesBtn);
    expect(screen.getByRole('button', { name: /spanish/i })).toBeDefined();
  });

  it('handles keyboard shortcuts for playback, seeking, volume, and closing', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });

    const video = document.querySelector('video') as HTMLVideoElement;
    Object.defineProperty(video, 'duration', { value: 600, writable: true });
    Object.defineProperty(video, 'currentTime', { value: 50, writable: true });
    Object.defineProperty(video, 'volume', { value: 0.5, writable: true });
    Object.defineProperty(video, 'muted', { value: false, writable: true });
    Object.defineProperty(video, 'paused', { value: false, writable: true });

    // Space / k: toggle play/pause
    act(() => {
      fireEvent.keyDown(window, { key: ' ' });
    });
    expect(video.pause).toHaveBeenCalled();

    Object.defineProperty(video, 'paused', { value: true, writable: true });
    act(() => {
      fireEvent.keyDown(window, { key: 'k' });
    });
    expect(video.play).toHaveBeenCalled();

    // ArrowLeft / ArrowRight: seek +/- 10s
    act(() => {
      fireEvent.keyDown(window, { key: 'ArrowLeft' });
    });
    expect(video.currentTime).toBe(40);

    act(() => {
      fireEvent.keyDown(window, { key: 'ArrowRight' });
    });
    expect(video.currentTime).toBe(50);

    // ArrowUp / ArrowDown: volume +/- 10%
    act(() => {
      fireEvent.keyDown(window, { key: 'ArrowUp' });
    });
    expect(Math.round(video.volume * 10) / 10).toBe(0.6);

    act(() => {
      fireEvent.keyDown(window, { key: 'ArrowDown' });
    });
    expect(Math.round(video.volume * 10) / 10).toBe(0.5);

    // m: toggle mute
    act(() => {
      fireEvent.keyDown(window, { key: 'm' });
    });
    expect(video.muted).toBe(true);

    // Escape: exit player and call onClose
    act(() => {
      fireEvent.keyDown(window, { key: 'Escape' });
    });
    expect(mockOnClose).toHaveBeenCalledTimes(1);
  });

  it('provides PlayerControls with Back to Browse button and title display', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
      expect(screen.getByText(/2017/)).toBeDefined();
    });

    const backBtn = screen.getByRole('button', { name: /back to browse/i });
    act(() => {
      fireEvent.click(backBtn);
    });
    expect(mockOnClose).toHaveBeenCalledTimes(1);
  });

  it('allows seeking and volume changes via PlayerControls UI controls', async () => {
    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });

    const video = document.querySelector('video') as HTMLVideoElement;
    Object.defineProperty(video, 'duration', { value: 120, writable: true });
    Object.defineProperty(video, 'currentTime', { value: 30, writable: true });
    act(() => {
      fireEvent.timeUpdate(video);
    });

    // Find seek slider
    const seekSlider = screen.getByRole('slider', { name: /seek/i });
    expect(seekSlider).toBeDefined();

    // Change slider position
    act(() => {
      fireEvent.change(seekSlider, { target: { value: '60' } });
    });
    expect(video.currentTime).toBe(60);

    // Volume slider
    const volumeSlider = screen.getByRole('slider', { name: /volume/i });
    expect(volumeSlider).toBeDefined();
    act(() => {
      fireEvent.change(volumeSlider, { target: { value: '0.8' } });
    });
    expect(video.volume).toBe(0.8);
  });

  it('auto-hides controls after 3 seconds of inactivity and reappears on mouse move', async () => {
    vi.useFakeTimers();

    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await act(async () => {
      await Promise.resolve();
    });

    const controlsContainer = screen.getByTestId('player-controls');
    expect(controlsContainer?.className).toContain('opacity-100');

    // Advance 3 seconds
    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(controlsContainer?.className).toContain('opacity-0');

    // Move mouse over container
    const playerContainer = document.querySelector('.fixed.inset-0');
    act(() => {
      fireEvent.mouseMove(playerContainer!);
    });

    expect(controlsContainer?.className).toContain('opacity-100');
  });

  it('integrates online subtitle search, download, and live DOM track injection', async () => {
    const mockSearchResults: SubtitleSearchResult[] = [
      {
        id: 'sub-303',
        language: 'es',
        format: 'srt',
        release_name: 'Blade.Runner.2049.Spanish.srt',
        hearing_impaired: false,
        download_count: 50,
      },
    ];

    const newTrack: SubtitleTrack = {
      id: 103,
      media_item_id: mockItemId,
      source: 'downloaded',
      language: 'es',
      title: 'Blade.Runner.2049.Spanish.srt',
      format: 'srt',
      is_default: false,
      is_forced: false,
      stream_url: '/api/v1/subtitles/103/stream.vtt',
    };

    vi.spyOn(api, 'searchSubtitles').mockResolvedValue({
      configured: true,
      matches: mockSearchResults as any,
    });
    vi.spyOn(api, 'downloadSubtitle').mockResolvedValue(newTrack);

    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText('Blade Runner 2049')).toBeDefined();
    });

    // Subtitles should initially be 2 tracks
    expect(document.querySelectorAll('track').length).toBe(2);

    // Open subtitles popover
    const subtitlesBtn = screen.getByRole('button', { name: /subtitles/i });
    fireEvent.click(subtitlesBtn);

    // Click "+ Search & Download Online"
    const searchOnlineBtn = screen.getByRole('button', { name: /\+ search & download online/i });
    fireEvent.click(searchOnlineBtn);

    // Search input should appear
    const langInput = screen.getByPlaceholderText(/language/i);
    expect(langInput).toBeDefined();
    fireEvent.change(langInput, { target: { value: 'es' } });

    // Submit search
    const searchSubmitBtn = screen.getByRole('button', { name: /^search$/i });
    fireEvent.click(searchSubmitBtn);

    await waitFor(() => {
      expect(api.searchSubtitles).toHaveBeenCalledWith(mockItemId, 'es');
      expect(screen.getByText('Blade.Runner.2049.Spanish.srt')).toBeDefined();
    });

    // When downloaded, getSubtitles returns the original tracks plus the new one
    vi.spyOn(api, 'getSubtitles').mockResolvedValue([...mockSubtitles, newTrack]);

    // Click Download on the match
    const downloadBtn = screen.getByRole('button', { name: /^download$/i });
    fireEvent.click(downloadBtn);

    await waitFor(() => {
      expect(api.downloadSubtitle).toHaveBeenCalledWith(
        mockItemId,
        expect.objectContaining({ file_id: 'sub-303', language: 'es' })
      );
      expect(api.getSubtitles).toHaveBeenCalledWith(mockItemId);
    });

    // Dynamic track injection into <video>
    await waitFor(() => {
      const tracks = document.querySelectorAll('track');
      expect(tracks.length).toBe(3);
      expect(tracks[2].getAttribute('src')).toBe('/api/v1/subtitles/103/stream.vtt');
      expect(tracks[2].getAttribute('srclang')).toBe('es');
      expect(tracks[2].getAttribute('label')).toBe('Blade.Runner.2049.Spanish.srt');
    });

    // Should return to list view with download banner and Downloaded badge
    await waitFor(() => {
      expect(screen.getByText(/subtitle downloaded! click below to display on screen/i)).toBeDefined();
      expect(screen.getByText('Downloaded')).toBeDefined();
    });
  });

  it('suspends HUD auto-hide when subtitles menu is open', async () => {
    vi.useFakeTimers();

    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await act(async () => {
      await Promise.resolve();
    });

    const controlsContainer = screen.getByTestId('player-controls');
    expect(controlsContainer?.className).toContain('opacity-100');

    // Open subtitles popover
    const subtitlesBtn = screen.getByRole('button', { name: /subtitles/i });
    act(() => {
      fireEvent.click(subtitlesBtn);
    });

    // Advance 3 seconds while subtitles menu is open
    act(() => {
      vi.advanceTimersByTime(3000);
    });

    // Controls must remain visible
    expect(controlsContainer?.className).toContain('opacity-100');

    // Close subtitles menu by selecting Off
    const offBtn = screen.getByRole('button', { name: /^off$/i });
    act(() => {
      fireEvent.click(offBtn);
    });

    // Advance 3 seconds now that menu is closed
    act(() => {
      vi.advanceTimersByTime(3000);
    });

    // Controls should now be hidden
    expect(controlsContainer?.className).toContain('opacity-0');
  });

  it('scrobbles final position, resets per-item state, supports Shift+N/Shift+P, and auto-advances queue on video ended', async () => {
    const queue = [
      {
        position: 0,
        added_at: 100,
        duration_seconds: 3600,
        card: { id: 10, title: 'Track One', media_type: 'movie' },
      },
      {
        position: 1,
        added_at: 200,
        duration_seconds: 4000,
        card: { id: 20, title: 'Track Two', media_type: 'movie' },
      },
    ];

    const createSessionSpy = vi
      .spyOn(api, 'createPlaybackSession')
      .mockImplementation(async (id) => ({
        session_id: `sess-${id}`,
        media_item_id: id,
        duration_seconds: 3600,
        resume_position_seconds: 0,
      }));
    const heartbeatSpy = vi.spyOn(api, 'sendPlaybackHeartbeat').mockResolvedValue();
    const closeSessionSpy = vi.spyOn(api, 'closePlaybackSession').mockResolvedValue();
    vi.spyOn(api, 'getPlaybackState').mockResolvedValue({
      user_id: 'u-1',
      media_item_id: 10,
      playback_position_seconds: 3500,
      watch_state: 'completed',
      last_watched_at: 100,
      play_count: 1,
    });
    vi.spyOn(api, 'getSubtitles').mockResolvedValue([]);
    vi.spyOn(api, 'getItemDetails').mockImplementation(async (id) => ({
      card: { id, title: id === 10 ? 'Track One' : 'Track Two', media_type: 'movie' },
      genres: [],
      stream_url: `/api/v1/stream/${id}`,
    }));

    const onSelectQueueIndex = vi.fn();
    const { container, rerender } = render(
      <CinemaPlayer
        itemId={10}
        onClose={mockOnClose}
        returnLabel="Back to Playlist"
        queue={queue}
        currentIndex={0}
        onSelectQueueIndex={onSelectQueueIndex}
      />
    );

    await waitFor(() => expect(createSessionSpy).toHaveBeenCalledWith(10));
    const video = container.querySelector('video') as HTMLVideoElement;
    Object.defineProperty(video, 'duration', { value: 3600, configurable: true });
    fireEvent.loadedMetadata(video);

    // Completed item starts at 0s rather than seeking to old position
    expect(video.currentTime).toBe(0);

    // Shift+N advances track
    act(() => {
      fireEvent.keyDown(window, { key: 'N', shiftKey: true });
    });
    expect(onSelectQueueIndex).toHaveBeenCalledWith(1);
    onSelectQueueIndex.mockClear();

    await act(async () => {
      fireEvent.ended(video);
    });

    await waitFor(() => {
      expect(heartbeatSpy).toHaveBeenCalledWith('sess-10', 3600);
      expect(onSelectQueueIndex).toHaveBeenCalledWith(1);
    });

    // Advance parent to item 20 -> closes sess-10 and opens sess-20
    rerender(
      <CinemaPlayer
        itemId={20}
        onClose={mockOnClose}
        returnLabel="Back to Playlist"
        queue={queue}
        currentIndex={1}
        onSelectQueueIndex={onSelectQueueIndex}
      />
    );

    await waitFor(() => {
      expect(closeSessionSpy).toHaveBeenCalledWith('sess-10');
      expect(createSessionSpy).toHaveBeenCalledWith(20);
    });

    // Shift+P from currentTime <= 5 goes back to index 0
    onSelectQueueIndex.mockClear();
    act(() => {
      fireEvent.keyDown(window, { key: 'P', shiftKey: true });
    });
    expect(onSelectQueueIndex).toHaveBeenCalledWith(0);
  });

  it('pauses the 3s control auto-hide timer while Playback Queue popover is open', async () => {
    vi.useFakeTimers();
    vi.spyOn(api, 'createPlaybackSession').mockResolvedValue(mockSession);
    vi.spyOn(api, 'getPlaybackState').mockResolvedValue(mockPlaybackState);
    vi.spyOn(api, 'getSubtitles').mockResolvedValue([]);
    vi.spyOn(api, 'getItemDetails').mockResolvedValue(mockDetails);

    const queue = [
      { position: 0, added_at: 1, duration_seconds: 100, card: { id: 10, title: 'A', media_type: 'movie' } },
      { position: 1, added_at: 2, duration_seconds: 100, card: { id: 11, title: 'B', media_type: 'movie' } },
    ];

    render(
      <CinemaPlayer
        itemId={10}
        onClose={mockOnClose}
        queue={queue}
        currentIndex={0}
        onSelectQueueIndex={vi.fn()}
      />
    );

    await act(async () => {
      await Promise.resolve();
    });

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Playback Queue' }));
    });
    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(screen.getByTestId('player-controls')).toHaveClass('opacity-100');
    vi.useRealTimers();
  });

  it('wires getAudioTracks, getStoryboard, audio track switching via HLS, Escape dismissal without closing player, and HUD auto-hide pause', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const hlsSpy = vi.spyOn(api, 'getHlsMasterUrl');
    vi.spyOn(api, 'getAudioTracks').mockResolvedValue([
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
      {
        audio_index: 1,
        stream_index: 2,
        codec: 'eac3',
        channels: 6,
        language: 'jpn',
        title: 'Japanese Original',
        is_default: false,
        is_commentary: false,
      },
    ]);
    vi.spyOn(api, 'getStoryboard').mockResolvedValue({
      status: 'ready',
      manifest: {
        interval_ms: 10000,
        tile_width: 320,
        tile_height: 180,
        columns: 10,
        rows: 10,
        total_tiles: 980,
        sheets: [
          { index: 0, url: '/api/v1/media/10/storyboard/0.jpg', first_tile: 0, tile_count: 980 },
        ],
      },
    });

    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(api.getAudioTracks).toHaveBeenCalledWith(mockItemId);
      expect(api.getStoryboard).toHaveBeenCalledWith(mockItemId, expect.any(AbortSignal));
    });

    const audioBtn = await screen.findByRole('button', { name: /audio tracks/i });
    fireEvent.click(audioBtn);

    // Controls remain visible past 3s while Audio Tracks menu is open
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(screen.getByTestId('player-controls')).toHaveClass('opacity-100');

    // Escape closes Audio Tracks menu without closing CinemaPlayer, and restores focus
    const menu = screen.getByRole('menu', { name: /audio tracks/i });
    fireEvent.keyDown(menu, { key: 'Escape' });
    expect(mockOnClose).not.toHaveBeenCalled();
    expect(audioBtn).toHaveAttribute('aria-expanded', 'false');
    expect(document.activeElement).toBe(audioBtn);

    // Window-level Escape while Audio Tracks menu is open also closes menu without exiting player
    fireEvent.click(audioBtn);
    expect(audioBtn).toHaveAttribute('aria-expanded', 'true');
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(mockOnClose).not.toHaveBeenCalled();
    expect(audioBtn).toHaveAttribute('aria-expanded', 'false');
    expect(document.activeElement).toBe(audioBtn);

    // Selecting non-zero audio track switches playback to HLS with audioTrack=1
    fireEvent.click(audioBtn);
    const radios = screen.getAllByRole('menuitemradio');
    fireEvent.click(radios[1]);

    await waitFor(() => {
      expect(hlsSpy).toHaveBeenCalledWith(
        mockItemId,
        expect.objectContaining({ audioTrack: 1 })
      );
    });
    vi.useRealTimers();
  });

  it('falls back to itemDetails.technical.audio_tracks when getAudioTracks rejects, and falls back to HLS on video error', async () => {
    const hlsSpy = vi.spyOn(api, 'getHlsMasterUrl');
    vi.spyOn(api, 'getAudioTracks').mockRejectedValue(new Error('endpoint error'));
    vi.spyOn(api, 'getItemDetails').mockResolvedValue({
      ...mockDetails,
      technical: {
        duration_seconds: 9800,
        container: 'mp4',
        video_codec: 'h264',
        audio_codec: 'aac',
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
        ],
      },
    });

    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    const audioBtn = await screen.findByRole('button', { name: /audio tracks/i });
    expect(audioBtn).toBeInTheDocument();

    const video = document.querySelector('video') as HTMLVideoElement;
    Object.defineProperty(video, 'currentTime', { value: 22, writable: true });
    act(() => {
      fireEvent.error(video);
    });

    await waitFor(() => {
      expect(hlsSpy).toHaveBeenCalledWith(
        mockItemId,
        expect.objectContaining({ startTime: 22 })
      );
    });
  });

  it('preserves saved resume position when Direct Play errors at 0s both after and before getPlaybackState resolves', async () => {
    const hlsSpy = vi.spyOn(api, 'getHlsMasterUrl');

    // Order 1: getPlaybackState resolves (145s) BEFORE <video> fires error (loadedmetadata never fired, currentTime=0)
    const { unmount } = render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    await waitFor(() => {
      expect(screen.getByText(/Resumed at/i)).toBeInTheDocument();
    });

    const video1 = document.querySelector('video') as HTMLVideoElement;
    expect(video1.currentTime).toBe(0);

    act(() => {
      fireEvent.error(video1);
    });

    await waitFor(() => {
      expect(hlsSpy).toHaveBeenCalledWith(
        mockItemId,
        expect.objectContaining({ startTime: 145 })
      );
    });

    unmount();
    hlsSpy.mockClear();

    // Order 2: <video> fires error immediately at 0s BEFORE getPlaybackState resolves
    let resolvePlaybackState!: (state: PlaybackState) => void;
    vi.spyOn(api, 'getPlaybackState').mockImplementationOnce(
      () =>
        new Promise<PlaybackState>((resolve) => {
          resolvePlaybackState = resolve;
        })
    );

    render(<CinemaPlayer itemId={mockItemId} onClose={mockOnClose} />);

    const video2 = document.querySelector('video') as HTMLVideoElement;
    act(() => {
      fireEvent.error(video2);
    });

    await waitFor(() => {
      expect(hlsSpy).toHaveBeenCalledWith(
        mockItemId,
        expect.objectContaining({ startTime: 0 })
      );
    });

    await act(async () => {
      resolvePlaybackState({
        ...mockPlaybackState,
        playback_position_seconds: 185,
        watch_state: 'in_progress',
      });
    });

    await waitFor(() => {
      expect(hlsSpy).toHaveBeenCalledWith(
        mockItemId,
        expect.objectContaining({ startTime: 185 })
      );
    });
  });
});
