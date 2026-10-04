import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
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

    const controlsContainer = document.querySelector('.bg-gradient-to-t');
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
      expect(api.searchSubtitles).toHaveBeenCalledWith(mockItemId, undefined, 'es');
      expect(screen.getByText('Blade.Runner.2049.Spanish.srt')).toBeDefined();
    });

    // When downloaded, getSubtitles returns the original tracks plus the new one
    vi.spyOn(api, 'getSubtitles').mockResolvedValue([...mockSubtitles, newTrack]);

    // Click Download on the match
    const downloadBtn = screen.getByRole('button', { name: /^download$/i });
    fireEvent.click(downloadBtn);

    await waitFor(() => {
      expect(api.downloadSubtitle).toHaveBeenCalledWith(mockItemId, mockSearchResults[0]);
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

    const controlsContainer = document.querySelector('.bg-gradient-to-t');
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
});
