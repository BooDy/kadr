import type { AudioStreamInfo, TechnicalInfo } from '../types';

export interface CompatibilityInput {
  mediaType?: string;
  technical?: TechnicalInfo;
  audioTrackIndex: number;
  forceHls?: boolean;
}

export interface CompatibilityDecision {
  useHls: boolean;
  videoCodec: 'h264' | 'none';
  audioCodec: 'aac';
}

const NATIVE_VIDEO_CONTAINERS = new Set(['mp4', 'm4v', 'mov', 'webm']);
const NATIVE_VIDEO_CODECS = new Set([
  'h264',
  'avc',
  'avc1',
  'vp8',
  'vp9',
  'av1',
  'av01',
]);
const NATIVE_AUDIO_CODECS = new Set([
  'aac',
  'mp4a',
  'mp3',
  'mp2',
  'opus',
  'vorbis',
  'flac',
  'pcm',
  'pcm_s16le',
  'pcm_s24le',
  'wav',
]);
const PURE_AUDIO_CONTAINERS = new Set([
  'mp3',
  'm4a',
  'm4b',
  'aac',
  'ogg',
  'oga',
  'opus',
  'flac',
  'wav',
  'mka',
  'wma',
]);
const NATIVE_AUDIO_CONTAINERS = new Set([
  'mp4',
  'mp3',
  'm4a',
  'm4b',
  'aac',
  'ogg',
  'oga',
  'opus',
  'flac',
  'wav',
  'webm',
]);

const LANGUAGE_NAMES: Record<string, string> = {
  en: 'English',
  eng: 'English',
  ar: 'Arabic',
  ara: 'Arabic',
  fr: 'French',
  fra: 'French',
  fre: 'French',
  es: 'Spanish',
  spa: 'Spanish',
  de: 'German',
  deu: 'German',
  ger: 'German',
  it: 'Italian',
  ita: 'Italian',
  ja: 'Japanese',
  jpn: 'Japanese',
  ko: 'Korean',
  kor: 'Korean',
  zh: 'Chinese',
  zho: 'Chinese',
  chi: 'Chinese',
  ru: 'Russian',
  rus: 'Russian',
  pt: 'Portuguese',
  por: 'Portuguese',
  tr: 'Turkish',
  tur: 'Turkish',
  hi: 'Hindi',
  hin: 'Hindi',
  und: 'Unknown',
};

function norm(value?: string): string {
  return (value ?? '').trim().toLowerCase();
}

export function isAudioOnlyMedia(
  mediaType?: string,
  technical?: TechnicalInfo
): boolean {
  const mt = norm(mediaType);
  if (mt === 'music' || mt === 'audiobook') {
    return true;
  }
  const vCodec = norm(technical?.video_codec);
  const aCodec = norm(technical?.audio_codec);
  const container = norm(technical?.container);
  if (!vCodec && aCodec) {
    return true;
  }
  if (!vCodec && PURE_AUDIO_CONTAINERS.has(container)) {
    return true;
  }
  return false;
}

export function shouldUseHls({
  mediaType,
  technical,
  audioTrackIndex,
  forceHls = false,
}: CompatibilityInput): CompatibilityDecision {
  if (norm(mediaType) === 'live_channel') {
    return {
      useHls: forceHls,
      videoCodec: 'h264',
      audioCodec: 'aac',
    };
  }

  const audioOnly = isAudioOnlyMedia(mediaType, technical);
  const targetVideoCodec: 'h264' | 'none' = audioOnly ? 'none' : 'h264';

  if (forceHls || audioTrackIndex > 0) {
    return {
      useHls: true,
      videoCodec: targetVideoCodec,
      audioCodec: 'aac',
    };
  }

  const container = norm(technical?.container);
  const vCodec = norm(technical?.video_codec);
  const aCodec = norm(technical?.audio_codec);

  if (audioOnly) {
    const containerOk = !container || NATIVE_AUDIO_CONTAINERS.has(container);
    const audioOk = !aCodec || NATIVE_AUDIO_CODECS.has(aCodec);
    return {
      useHls: !(containerOk && audioOk),
      videoCodec: 'none',
      audioCodec: 'aac',
    };
  }

  // Unprobed items try Direct Play first and rely on <video> onError -> forceHls fallback
  if (!container && !vCodec && !aCodec) {
    return { useHls: false, videoCodec: 'h264', audioCodec: 'aac' };
  }

  const containerOk = !container || NATIVE_VIDEO_CONTAINERS.has(container);
  const videoOk = !vCodec || NATIVE_VIDEO_CODECS.has(vCodec);
  const audioOk = !aCodec || NATIVE_AUDIO_CODECS.has(aCodec);

  return {
    useHls: !(containerOk && videoOk && audioOk),
    videoCodec: 'h264',
    audioCodec: 'aac',
  };
}

export function formatChannelLayout(channels?: number): string | null {
  if (!channels || channels <= 0) return null;
  if (channels === 1) return 'Mono';
  if (channels === 2) return 'Stereo';
  if (channels === 6) return '5.1';
  if (channels === 8) return '7.1';
  return `${channels}ch`;
}

export function formatLanguageCode(code?: string): string {
  const key = norm(code);
  if (!key || key === 'und') return '';
  return LANGUAGE_NAMES[key] ?? code!.toUpperCase();
}

export function formatAudioTrackLabel(track: AudioStreamInfo): string {
  const lang = formatLanguageCode(track.language);
  const cleanTitle = track.title?.trim() || '';
  if (lang && cleanTitle && cleanTitle.toLowerCase() !== lang.toLowerCase()) {
    return `${lang} — ${cleanTitle}`;
  }
  if (cleanTitle) return cleanTitle;
  if (lang) return lang;
  return `Track ${track.audio_index + 1}`;
}
