import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import { useRef, type FC } from 'react';
import Hls from 'hls.js';
import {
  extractHlsSessionId,
  syncCueOffsets,
  useHlsPlayback,
  type UseHlsPlaybackParams,
  type UseHlsPlaybackResult,
} from './useHlsPlayback';
import { api } from '../api/client';

const mockHlsInstances: Array<{
  loadedSrc: string | null;
  attachedMedia: HTMLMediaElement | null;
  destroyed: boolean;
  handlers: Record<string, Array<(...args: unknown[]) => void>>;
  trigger: (event: string, data: unknown) => void;
}> = [];

vi.mock('hls.js', () => {
  class MockHls {
    static isSupported = vi.fn(() => true);
    static Events = {
      MANIFEST_PARSED: 'hlsManifestParsed',
      FRAG_LOADING: 'hlsFragLoading',
      LEVEL_LOADED: 'hlsLevelLoaded',
      ERROR: 'hlsError',
    };
    static ErrorTypes = {
      NETWORK_ERROR: 'networkError',
      MEDIA_ERROR: 'mediaError',
    };

    loadedSrc: string | null = null;
    attachedMedia: HTMLMediaElement | null = null;
    destroyed = false;
    handlers: Record<string, Array<(...args: unknown[]) => void>> = {};

    constructor() {
      mockHlsInstances.push(this);
    }

    on(event: string, cb: (...args: unknown[]) => void) {
      this.handlers[event] = this.handlers[event] || [];
      this.handlers[event].push(cb);
    }

    loadSource(src: string) {
      this.loadedSrc = src;
    }

    attachMedia(media: HTMLMediaElement) {
      this.attachedMedia = media;
    }

    destroy() {
      this.destroyed = true;
    }

    trigger(event: string, data: unknown) {
      for (const cb of this.handlers[event] || []) {
        cb(event, data);
      }
    }
  }

  return { default: MockHls };
});

interface HarnessProps extends Omit<UseHlsPlaybackParams, 'videoRef'> {
  onResult?: (hls: UseHlsPlaybackResult) => void;
}

const Harness: FC<HarnessProps> = ({ onResult, ...props }) => {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const hls = useHlsPlayback({ ...props, videoRef });
  onResult?.(hls);
  return (
    <div>
      <video ref={videoRef} data-testid="video" />
      <span data-testid="mode">{hls.isUsingHls ? 'hls' : 'direct'}</span>
      <span data-testid="offset">{hls.timeOffset}</span>
      <span data-testid="track">{hls.activeAudioTrackIndex}</span>
      <button
        type="button"
        onClick={() => hls.switchAudioTrack(1, 45)}
        data-testid="switch-track"
      >
        Switch
      </button>
      <button
        type="button"
        onClick={() => hls.seekTo(300)}
        data-testid="seek-unbuffered"
      >
        Seek 300
      </button>
      <button
        type="button"
        onClick={() => hls.handleVideoError(18)}
        data-testid="trigger-error"
      >
        Error
      </button>
    </div>
  );
};

describe('useHlsPlayback', () => {
  beforeEach(() => {
    mockHlsInstances.length = 0;
    vi.restoreAllMocks();
    vi.mocked(Hls.isSupported).mockReturnValue(true);
    vi.spyOn(api, 'deleteHlsSession').mockResolvedValue(undefined);
    api.setToken('jwt-test');
  });

  it('extracts session UUIDs from rewritten segment URLs', () => {
    expect(
      extractHlsSessionId(
        '/api/v1/media/42/hls/sessions/e8b6238b-d72b-47e1-85b8-50fa973ff52c/segment_0000.ts?token=abc'
      )
    ).toBe('e8b6238b-d72b-47e1-85b8-50fa973ff52c');
    expect(extractHlsSessionId('/api/v1/stream/42')).toBeNull();
  });

  it('starts in Direct Play for MP4 H264/AAC, switches to HLS on audio track change, and cleans up session on unmount', async () => {
    const { unmount } = render(
      <Harness
        itemId={42}
        mediaType="movie"
        technical={{
          duration_seconds: 600,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        }}
        knownDurationSeconds={600}
      />
    );

    const video = screen.getByTestId('video') as HTMLVideoElement;
    expect(screen.getByTestId('mode').textContent).toBe('direct');
    expect(video.getAttribute('src')).toContain('/api/v1/stream/42');

    await act(async () => {
      screen.getByTestId('switch-track').click();
    });

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(1);
    });
    expect(screen.getByTestId('mode').textContent).toBe('hls');
    expect(screen.getByTestId('offset').textContent).toBe('45');
    expect(mockHlsInstances[0].loadedSrc).toContain('audio_track=1');
    expect(mockHlsInstances[0].loadedSrc).toContain('start_time=45');

    // Simulate segment load carrying session ID
    act(() => {
      mockHlsInstances[0].trigger('hlsFragLoading', {
        frag: {
          url: '/api/v1/media/42/hls/sessions/11111111-2222-3333-4444-555555555555/segment_0000.ts',
        },
      });
    });

    // Seek to unbuffered 300s restarts HLS and deletes previous session
    await act(async () => {
      screen.getByTestId('seek-unbuffered').click();
    });

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(2);
    });
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      42,
      '11111111-2222-3333-4444-555555555555'
    );
    expect(mockHlsInstances[1].loadedSrc).toContain('start_time=300');

    act(() => {
      mockHlsInstances[1].trigger('hlsFragLoading', {
        frag: {
          url: '/api/v1/media/42/hls/sessions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/segment_0000.ts',
        },
      });
    });

    unmount();
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      42,
      'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'
    );
  });

  it('seeks in-place within buffered range during HLS without restarting session, and reloads on unbuffered seek to same start_time', async () => {
    let latestHls: UseHlsPlaybackResult | null = null;
    render(
      <Harness
        itemId={77}
        mediaType="movie"
        technical={{
          duration_seconds: 600,
          container: 'mkv',
          video_codec: 'hevc',
          audio_codec: 'eac3',
        }}
        initialResumeSeconds={100}
        onResult={(r) => {
          latestHls = r;
        }}
      />
    );

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(1);
    });
    expect(screen.getByTestId('offset').textContent).toBe('100');
    expect(mockHlsInstances[0].loadedSrc).toContain('start_time=100');

    const video = screen.getByTestId('video') as HTMLVideoElement;
    Object.defineProperty(video, 'buffered', {
      configurable: true,
      get: () => ({
        length: 1,
        start: (i: number) => (i === 0 ? 0 : 0),
        end: (i: number) => (i === 0 ? 30 : 0),
      }),
    });

    // Seek to 120s -> relative 20s is inside [0, 30] buffered range
    act(() => {
      latestHls!.seekTo(120);
    });
    expect(video.currentTime).toBe(20);
    expect(mockHlsInstances).toHaveLength(1);
    expect(api.deleteHlsSession).not.toHaveBeenCalled();

    // Effective current time and fallback duration include timeOffset
    expect(latestHls!.getEffectiveCurrentTime()).toBe(120);
    expect(latestHls!.getEffectiveDuration(500)).toBe(600);

    // Clear buffered ranges and seek back to 100s (same as current hlsStartTime=100) -> forces nonce reload
    Object.defineProperty(video, 'buffered', {
      configurable: true,
      get: () => ({
        length: 0,
        start: () => 0,
        end: () => 0,
      }),
    });

    await act(async () => {
      latestHls!.seekTo(100);
    });
    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(2);
    });
  });

  it('falls back to native Safari HLS when Hls.isSupported() is false and cleans up extracted session', async () => {
    vi.mocked(Hls.isSupported).mockReturnValue(false);
    const playlistBody = [
      '#EXTM3U',
      '#EXTINF:4.000,',
      '/api/v1/media/88/hls/sessions/99999999-8888-7777-6666-555555555555/segment_0000.ts?token=jwt-test',
    ].join('\n');
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      text: async () => playlistBody,
    } as Response);

    const canPlaySpy = vi
      .spyOn(HTMLMediaElement.prototype, 'canPlayType')
      .mockImplementation((mime: string) =>
        mime === 'application/vnd.apple.mpegurl' ? 'maybe' : ''
      );

    const { unmount } = render(
      <Harness
        itemId={88}
        mediaType="movie"
        technical={{
          duration_seconds: 400,
          container: 'mkv',
          video_codec: 'hevc',
          audio_codec: 'dts',
        }}
      />
    );

    const video = screen.getByTestId('video') as HTMLVideoElement;
    await waitFor(() => {
      expect(video.getAttribute('src')).toContain('/api/v1/media/88/hls/master.m3u8');
    });
    expect(mockHlsInstances).toHaveLength(0);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    unmount();
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      88,
      '99999999-8888-7777-6666-555555555555'
    );
    canPlaySpy.mockRestore();
  });

  it('automatically falls back from Direct Play to HLS on <video> error', async () => {
    render(
      <Harness
        itemId={55}
        mediaType="movie"
        technical={{ duration_seconds: 500 }}
        knownDurationSeconds={500}
      />
    );

    expect(screen.getByTestId('mode').textContent).toBe('direct');
    await act(async () => {
      screen.getByTestId('trigger-error').click();
    });

    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(1);
    });
    expect(screen.getByTestId('mode').textContent).toBe('hls');
    expect(mockHlsInstances[0].loadedSrc).toContain('start_time=18');
  });

  it('shifts VTTCue startTime and endTime by -timeOffset idempotently', () => {
    const fakeCue = { startTime: 50, endTime: 54 };
    const fakeVideo = {
      textTracks: [{ cues: [fakeCue] }],
    } as unknown as HTMLVideoElement;

    syncCueOffsets(fakeVideo, 40);
    expect(fakeCue.startTime).toBe(10);
    expect(fakeCue.endTime).toBe(14);

    // Calling again with a new offset uses stored original timestamps!
    syncCueOffsets(fakeVideo, 20);
    expect(fakeCue.startTime).toBe(30);
    expect(fakeCue.endTime).toBe(34);

    // Returning to offset 0 restores original timestamps
    syncCueOffsets(fakeVideo, 0);
    expect(fakeCue.startTime).toBe(50);
    expect(fakeCue.endTime).toBe(54);

    // Enforces startTime <= endTime at every intermediate step for strict DOM VTTCue implementations
    let s = 50;
    let e = 54;
    const strictCue = {
      get startTime() {
        return s;
      },
      set startTime(val: number) {
        if (val > e) {
          throw new DOMException('startTime > endTime', 'IndexSizeError');
        }
        s = val;
      },
      get endTime() {
        return e;
      },
      set endTime(val: number) {
        if (val < s) {
          throw new DOMException('endTime < startTime', 'IndexSizeError');
        }
        e = val;
      },
    };
    const strictVideo = {
      textTracks: [{ cues: [strictCue] }],
    } as unknown as HTMLVideoElement;

    syncCueOffsets(strictVideo, 40);
    expect(strictCue.startTime).toBe(10);
    expect(strictCue.endTime).toBe(14);

    syncCueOffsets(strictVideo, 10);
    expect(strictCue.startTime).toBe(40);
    expect(strictCue.endTime).toBe(44);

    syncCueOffsets(strictVideo, 0);
    expect(strictCue.startTime).toBe(50);
    expect(strictCue.endTime).toBe(54);
  });

  it('switches back to Direct Play on track 0 for MP4 and resets cleanly when itemId changes', async () => {
    let latestHls: UseHlsPlaybackResult | null = null;
    const { rerender } = render(
      <Harness
        itemId={42}
        mediaType="movie"
        technical={{
          duration_seconds: 600,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        }}
        onResult={(r) => {
          latestHls = r;
        }}
      />
    );

    const video = screen.getByTestId('video') as HTMLVideoElement;
    await act(async () => {
      latestHls!.switchAudioTrack(1, 45);
    });
    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(1);
    });
    act(() => {
      mockHlsInstances[0].trigger('hlsLevelLoaded', {
        details: {
          fragments: [
            {
              url: '/api/v1/media/42/hls/sessions/12345678-1234-1234-1234-1234567890ab/segment_0000.ts',
            },
          ],
        },
      });
    });

    // Switch back to track 0 at 90s -> returns to Direct Play and deletes HLS session
    await act(async () => {
      latestHls!.switchAudioTrack(0, 90);
    });
    expect(screen.getByTestId('mode').textContent).toBe('direct');
    expect(screen.getByTestId('offset').textContent).toBe('0');
    expect(video.getAttribute('src')).toContain('/api/v1/stream/42');
    expect(video.currentTime).toBe(90);
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      42,
      '12345678-1234-1234-1234-1234567890ab'
    );

    // Switch back to HLS on item 42, then transition to item 43
    await act(async () => {
      latestHls!.switchAudioTrack(1, 120);
    });
    await waitFor(() => {
      expect(mockHlsInstances).toHaveLength(2);
    });
    act(() => {
      mockHlsInstances[1].trigger('hlsFragLoading', {
        frag: {
          url: '/api/v1/media/42/hls/sessions/bbbbbbbb-cccc-dddd-eeee-ffffffffffff/segment_0000.ts',
        },
      });
    });

    rerender(
      <Harness
        itemId={43}
        mediaType="movie"
        technical={{
          duration_seconds: 300,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        }}
        onResult={(r) => {
          latestHls = r;
        }}
      />
    );

    expect(screen.getByTestId('mode').textContent).toBe('direct');
    expect(screen.getByTestId('offset').textContent).toBe('0');
    expect(screen.getByTestId('track').textContent).toBe('0');
    expect(video.getAttribute('src')).toContain('/api/v1/stream/43');
    expect(api.deleteHlsSession).toHaveBeenCalledWith(
      42,
      'bbbbbbbb-cccc-dddd-eeee-ffffffffffff'
    );
    expect(mockHlsInstances).toHaveLength(2);
  });
});
