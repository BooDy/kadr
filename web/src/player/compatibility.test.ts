import { describe, it, expect } from 'vitest';
import {
  isAudioOnlyMedia,
  shouldUseHls,
  formatChannelLayout,
  formatAudioTrackLabel,
} from './compatibility';

describe('player compatibility helpers', () => {
  it('identifies audio-only media types and missing video streams', () => {
    expect(isAudioOnlyMedia('music', { duration_seconds: 180, container: 'flac' })).toBe(true);
    expect(isAudioOnlyMedia('audiobook', { duration_seconds: 600 })).toBe(true);
    expect(
      isAudioOnlyMedia('movie', { duration_seconds: 120, audio_codec: 'aac' })
    ).toBe(true);
    expect(
      isAudioOnlyMedia('unknown', { duration_seconds: 200, container: 'mka' })
    ).toBe(true);
    expect(
      isAudioOnlyMedia('unknown', { duration_seconds: 200, container: 'wma' })
    ).toBe(true);
    expect(
      isAudioOnlyMedia('movie', { duration_seconds: 120, container: 'webm' })
    ).toBe(false);
    expect(
      isAudioOnlyMedia('movie', {
        duration_seconds: 120,
        video_codec: 'h264',
        audio_codec: 'aac',
      })
    ).toBe(false);
  });

  it('uses Direct Play for kadr-ingest m4a/m4b audio items probed with container=mp4 and preserves video on unprobed webm fallback', () => {
    expect(
      shouldUseHls({
        mediaType: 'music',
        technical: {
          duration_seconds: 240,
          container: 'mp4',
          audio_codec: 'aac',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'none', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'audiobook',
        technical: {
          duration_seconds: 3600,
          container: 'mp4',
          audio_codec: 'aac',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'none', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'webm',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'h264', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'webm',
        },
        audioTrackIndex: 0,
        forceHls: true,
      })
    ).toEqual({ useHls: true, videoCodec: 'h264', audioCodec: 'aac' });
  });

  it('uses Direct Play for browser-native MP4/WebM at audioTrackIndex 0', () => {
    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'h264', audioCodec: 'aac' });
  });

  it('switches to HLS when audioTrackIndex > 0 or container/codec is non-native', () => {
    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'mp4',
          video_codec: 'h264',
          audio_codec: 'aac',
        },
        audioTrackIndex: 1,
      })
    ).toEqual({ useHls: true, videoCodec: 'h264', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: {
          duration_seconds: 120,
          container: 'mkv',
          video_codec: 'h264',
          audio_codec: 'eac3',
        },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: true, videoCodec: 'h264', audioCodec: 'aac' });
  });

  it('uses video_codec=none for audio-only HLS transcoding (WMA/AC3 or non-zero track)', () => {
    expect(
      shouldUseHls({
        mediaType: 'music',
        technical: { duration_seconds: 210, container: 'mka', audio_codec: 'dts' },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: true, videoCodec: 'none', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'audiobook',
        technical: { duration_seconds: 900, container: 'm4b', audio_codec: 'aac' },
        audioTrackIndex: 1,
      })
    ).toEqual({ useHls: true, videoCodec: 'none', audioCodec: 'aac' });
  });

  it('handles live_channel, forceHls, and unprobed items', () => {
    expect(
      shouldUseHls({
        mediaType: 'live_channel',
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'h264', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'live_channel',
        audioTrackIndex: 0,
        forceHls: true,
      })
    ).toEqual({ useHls: true, videoCodec: 'h264', audioCodec: 'aac' });

    expect(
      shouldUseHls({
        mediaType: 'movie',
        technical: { duration_seconds: 100 },
        audioTrackIndex: 0,
      })
    ).toEqual({ useHls: false, videoCodec: 'h264', audioCodec: 'aac' });
  });

  it('formats channel layouts and human-readable audio track labels', () => {
    expect(formatChannelLayout(1)).toBe('Mono');
    expect(formatChannelLayout(2)).toBe('Stereo');
    expect(formatChannelLayout(6)).toBe('5.1');
    expect(formatChannelLayout(8)).toBe('7.1');
    expect(formatChannelLayout(4)).toBe('4ch');
    expect(formatChannelLayout(0)).toBeNull();
    expect(formatChannelLayout(undefined)).toBeNull();

    expect(
      formatAudioTrackLabel({
        audio_index: 1,
        stream_index: 2,
        codec: 'eac3',
        channels: 6,
        language: 'jpn',
        title: 'Original Japanese',
        is_default: false,
        is_commentary: false,
      })
    ).toBe('Japanese — Original Japanese');

    expect(
      formatAudioTrackLabel({
        audio_index: 0,
        stream_index: 1,
        codec: 'aac',
        language: 'und',
        is_default: true,
        is_commentary: false,
      })
    ).toBe('Track 1');
  });
});
