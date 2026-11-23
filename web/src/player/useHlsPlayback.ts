import { useState, useEffect, useRef, useCallback, type RefObject } from 'react';
import type Hls from 'hls.js';
import { api } from '../api/client';
import type { TechnicalInfo } from '../types';
import { shouldUseHls } from './compatibility';

export interface UseHlsPlaybackParams {
  itemId: number;
  videoRef: RefObject<HTMLVideoElement | null>;
  mediaType?: string;
  technical?: TechnicalInfo;
  knownDurationSeconds?: number;
  initialResumeSeconds?: number | null;
  defaultAudioTrackIndex?: number;
}

export interface UseHlsPlaybackResult {
  isUsingHls: boolean;
  timeOffset: number;
  activeAudioTrackIndex: number;
  switchAudioTrack: (audioIndex: number, currentEffectiveTime: number) => void;
  seekTo: (targetSeconds: number) => void;
  handleVideoError: (currentEffectiveTime: number) => void;
  getEffectiveCurrentTime: () => number;
  getEffectiveDuration: (rawVideoDuration?: number) => number;
}

interface OffsetAnnotatedCue {
  startTime: number;
  endTime: number;
  __kadrOrigStart?: number;
  __kadrOrigEnd?: number;
}

const HLS_SESSION_RE = /\/hls\/sessions\/([0-9a-fA-F-]{36})\//;

export function extractHlsSessionId(url: string): string | null {
  const m = HLS_SESSION_RE.exec(url);
  return m ? m[1] : null;
}

export function syncCueOffsets(
  video: HTMLVideoElement | null,
  timeOffset: number
): void {
  if (!video || !video.textTracks) return;
  const tracks = video.textTracks;
  for (let i = 0; i < tracks.length; i++) {
    const cues = tracks[i]?.cues;
    if (!cues) continue;
    for (let j = 0; j < cues.length; j++) {
      const cue = cues[j] as unknown as OffsetAnnotatedCue;
      if (cue.__kadrOrigStart === undefined) {
        cue.__kadrOrigStart = cue.startTime;
      }
      if (cue.__kadrOrigEnd === undefined) {
        cue.__kadrOrigEnd = cue.endTime;
      }
      const nextStart =
        timeOffset > 0
          ? Math.max(0, cue.__kadrOrigStart - timeOffset)
          : cue.__kadrOrigStart;
      const nextEnd =
        timeOffset > 0
          ? Math.max(0, cue.__kadrOrigEnd - timeOffset)
          : cue.__kadrOrigEnd;

      // Preserve startTime <= endTime invariant across intermediate mutations on strict VTTCue implementations
      if (nextStart > cue.endTime) {
        cue.endTime = nextEnd;
        cue.startTime = nextStart;
      } else {
        cue.startTime = nextStart;
        cue.endTime = nextEnd;
      }
    }
  }
}

function isWithinBuffered(
  video: HTMLVideoElement,
  relativeSeconds: number
): boolean {
  if (relativeSeconds < 0) return false;
  const buffered = video.buffered;
  if (!buffered) return false;
  for (let i = 0; i < buffered.length; i++) {
    if (
      relativeSeconds >= buffered.start(i) - 0.5 &&
      relativeSeconds <= buffered.end(i) + 0.5
    ) {
      return true;
    }
  }
  return false;
}

export function useHlsPlayback({
  itemId,
  videoRef,
  mediaType,
  technical,
  knownDurationSeconds,
  initialResumeSeconds,
  defaultAudioTrackIndex = 0,
}: UseHlsPlaybackParams): UseHlsPlaybackResult {
  const initialDecision = shouldUseHls({
    mediaType,
    technical,
    audioTrackIndex: defaultAudioTrackIndex,
    forceHls: false,
  });
  const initialStart =
    initialDecision.useHls && initialResumeSeconds && initialResumeSeconds > 0
      ? Math.floor(initialResumeSeconds)
      : 0;

  const [stateItemId, setStateItemId] = useState(itemId);
  const [activeAudioTrackIndex, setActiveAudioTrackIndex] = useState<number>(
    defaultAudioTrackIndex
  );
  const [forceHls, setForceHls] = useState(false);
  const [hlsStartTime, setHlsStartTime] = useState(initialStart);
  const [hlsReloadNonce, setHlsReloadNonce] = useState(0);

  const hlsInstanceRef = useRef<Hls | null>(null);
  const activeHlsSessionRef = useRef<{ itemId: number; sessionId: string } | null>(
    null
  );
  const hasAppliedDefaultTrackRef = useRef(defaultAudioTrackIndex > 0);
  const hasAppliedInitialResumeRef = useRef(initialStart > 0);
  const pendingDirectSeekRef = useRef<number | null>(null);

  const isItemTransitioning = stateItemId !== itemId;
  const effectiveAudioTrackIndex = isItemTransitioning
    ? defaultAudioTrackIndex
    : activeAudioTrackIndex;
  const effectiveForceHls = isItemTransitioning ? false : forceHls;
  const effectiveReloadNonce = isItemTransitioning ? 0 : hlsReloadNonce;

  const decision = shouldUseHls({
    mediaType,
    technical,
    audioTrackIndex: effectiveAudioTrackIndex,
    forceHls: effectiveForceHls,
  });

  const effectiveHlsStartTime = isItemTransitioning
    ? decision.useHls && initialResumeSeconds && initialResumeSeconds > 0
      ? Math.floor(initialResumeSeconds)
      : 0
    : hlsStartTime;

  // Reset per-item state when itemId changes
  useEffect(() => {
    if (stateItemId !== itemId) {
      const nextInitialStart =
        decision.useHls && initialResumeSeconds && initialResumeSeconds > 0
          ? Math.floor(initialResumeSeconds)
          : 0;
      hasAppliedDefaultTrackRef.current = defaultAudioTrackIndex > 0;
      hasAppliedInitialResumeRef.current = nextInitialStart > 0;
      pendingDirectSeekRef.current = null;
      setStateItemId(itemId);
      setActiveAudioTrackIndex(defaultAudioTrackIndex);
      setForceHls(false);
      setHlsStartTime(nextInitialStart);
      setHlsReloadNonce(0);
    }
  }, [
    stateItemId,
    itemId,
    defaultAudioTrackIndex,
    decision.useHls,
    initialResumeSeconds,
  ]);

  // Pick up non-zero default audio track once discovered
  useEffect(() => {
    if (
      !isItemTransitioning &&
      !hasAppliedDefaultTrackRef.current &&
      defaultAudioTrackIndex > 0
    ) {
      hasAppliedDefaultTrackRef.current = true;
      setActiveAudioTrackIndex(defaultAudioTrackIndex);
    }
  }, [isItemTransitioning, defaultAudioTrackIndex]);

  // Apply initial resume position to HLS start_time before first stream start if HLS is active
  useEffect(() => {
    if (
      !isItemTransitioning &&
      decision.useHls &&
      !hasAppliedInitialResumeRef.current &&
      initialResumeSeconds &&
      initialResumeSeconds > 0 &&
      hlsStartTime === 0
    ) {
      hasAppliedInitialResumeRef.current = true;
      setHlsStartTime(Math.floor(initialResumeSeconds));
    }
  }, [
    isItemTransitioning,
    decision.useHls,
    initialResumeSeconds,
    hlsStartTime,
  ]);

  const timeOffset = decision.useHls ? effectiveHlsStartTime : 0;

  const teardownActiveHls = useCallback(() => {
    if (hlsInstanceRef.current) {
      hlsInstanceRef.current.destroy();
      hlsInstanceRef.current = null;
    }
    if (activeHlsSessionRef.current) {
      const { itemId: prevItemId, sessionId } = activeHlsSessionRef.current;
      activeHlsSessionRef.current = null;
      api.deleteHlsSession(prevItemId, sessionId).catch(() => {});
    }
  }, []);

  // Attach either Direct Play src or HLS pipeline
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    let cancelled = false;
    teardownActiveHls();

    if (!decision.useHls) {
      const directUrl = api.getStreamUrl(itemId);
      if (video.getAttribute('src') !== directUrl) {
        video.src = directUrl;
      }
      let cleanupLoadedMetadata: (() => void) | null = null;
      if (pendingDirectSeekRef.current !== null) {
        const seekTarget = pendingDirectSeekRef.current;
        pendingDirectSeekRef.current = null;
        const applySeek = () => {
          if (!cancelled) {
            video.currentTime = seekTarget;
          }
        };
        if (video.readyState >= 1) {
          applySeek();
        } else {
          video.addEventListener('loadedmetadata', applySeek, { once: true });
          cleanupLoadedMetadata = () =>
            video.removeEventListener('loadedmetadata', applySeek);
          try {
            video.currentTime = seekTarget;
          } catch {
            // Wait for loadedmetadata if element is not yet seekable
          }
        }
      }
      return () => {
        cancelled = true;
        cleanupLoadedMetadata?.();
      };
    }

    const masterUrl = api.getHlsMasterUrl(itemId, {
      videoCodec: decision.videoCodec,
      audioCodec: decision.audioCodec,
      startTime: effectiveHlsStartTime,
      audioTrack: effectiveAudioTrackIndex,
    });

    (async () => {
      try {
        const mod = await import('hls.js');
        if (cancelled) return;
        const HlsCtor = mod.default;

        if (HlsCtor.isSupported()) {
          video.removeAttribute('src');
          const hls = new HlsCtor({
            enableWorker: true,
            backBufferLength: 60,
          });
          hlsInstanceRef.current = hls;

          const recordSessionFromUrl = (candidate?: string) => {
            if (!candidate) return;
            const sid = extractHlsSessionId(candidate);
            if (sid) {
              activeHlsSessionRef.current = { itemId, sessionId: sid };
            }
          };

          hls.on(HlsCtor.Events.FRAG_LOADING, (_evt, data) => {
            recordSessionFromUrl(data?.frag?.url);
          });

          hls.on(HlsCtor.Events.LEVEL_LOADED, (_evt, data) => {
            recordSessionFromUrl(data?.details?.fragments?.[0]?.url);
          });

          hls.on(HlsCtor.Events.MANIFEST_PARSED, () => {
            if (cancelled) return;
            try {
              const p = video.play();
              if (p && typeof p.catch === 'function') p.catch(() => {});
            } catch {
              // Ignore autoplay block in headless/restricted browsers
            }
          });

          hls.loadSource(masterUrl);
          hls.attachMedia(video);
          return;
        }

        // Native Safari HLS fallback
        if (video.canPlayType('application/vnd.apple.mpegurl')) {
          try {
            const resp = await fetch(masterUrl);
            if (!cancelled && resp.ok) {
              const playlistText = await resp.text();
              const sid = extractHlsSessionId(playlistText);
              if (sid) {
                activeHlsSessionRef.current = { itemId, sessionId: sid };
              }
            }
          } catch {
            // Best-effort session extraction for Safari
          }
          if (!cancelled) {
            video.src = masterUrl;
          }
        }
      } catch {
        // Ignore dynamic import cancellation
      }
    })();

    return () => {
      cancelled = true;
      teardownActiveHls();
    };
  }, [
    itemId,
    videoRef,
    decision.useHls,
    decision.videoCodec,
    decision.audioCodec,
    effectiveHlsStartTime,
    effectiveAudioTrackIndex,
    effectiveReloadNonce,
    teardownActiveHls,
  ]);

  // Keep WebVTT cues shifted by -timeOffset
  useEffect(() => {
    syncCueOffsets(videoRef.current, timeOffset);
  }, [videoRef, timeOffset]);

  const getEffectiveCurrentTime = useCallback((): number => {
    const raw = videoRef.current?.currentTime ?? 0;
    return timeOffset + (Number.isFinite(raw) ? raw : 0);
  }, [videoRef, timeOffset]);

  const getEffectiveDuration = useCallback(
    (rawVideoDuration?: number): number => {
      if (knownDurationSeconds && knownDurationSeconds > 0) {
        return knownDurationSeconds;
      }
      const raw = rawVideoDuration ?? videoRef.current?.duration ?? 0;
      if (Number.isFinite(raw) && raw > 0) {
        return decision.useHls ? timeOffset + raw : raw;
      }
      return 0;
    },
    [knownDurationSeconds, videoRef, decision.useHls, timeOffset]
  );

  const switchAudioTrack = useCallback(
    (audioIndex: number, currentEffectiveTime: number) => {
      if (audioIndex === effectiveAudioTrackIndex) {
        return;
      }
      hasAppliedDefaultTrackRef.current = true;
      hasAppliedInitialResumeRef.current = true;
      const safeStart = Math.max(0, Math.floor(currentEffectiveTime));
      const nextDecision = shouldUseHls({
        mediaType,
        technical,
        audioTrackIndex: audioIndex,
        forceHls: effectiveForceHls,
      });
      setActiveAudioTrackIndex(audioIndex);
      if (nextDecision.useHls) {
        pendingDirectSeekRef.current = null;
        setHlsStartTime(safeStart);
      } else {
        pendingDirectSeekRef.current = safeStart > 0 ? safeStart : null;
        setHlsStartTime(0);
      }
    },
    [effectiveAudioTrackIndex, mediaType, technical, effectiveForceHls]
  );

  const seekTo = useCallback(
    (targetSeconds: number) => {
      const clamped = Math.max(0, targetSeconds);
      const video = videoRef.current;
      if (!decision.useHls) {
        if (video) {
          video.currentTime = clamped;
        }
        return;
      }

      if (video) {
        const relative = clamped - timeOffset;
        if (isWithinBuffered(video, relative)) {
          video.currentTime = Math.max(0, relative);
          return;
        }
      }

      hasAppliedInitialResumeRef.current = true;
      const nextStart = Math.floor(clamped);
      if (nextStart === effectiveHlsStartTime) {
        setHlsReloadNonce((n) => n + 1);
      } else {
        setHlsStartTime(nextStart);
      }
    },
    [decision.useHls, timeOffset, effectiveHlsStartTime, videoRef]
  );

  const handleVideoError = useCallback(
    (currentEffectiveTime: number) => {
      if (!decision.useHls && !effectiveForceHls) {
        hasAppliedInitialResumeRef.current = true;
        setHlsStartTime(Math.max(0, Math.floor(currentEffectiveTime)));
        setForceHls(true);
      }
    },
    [decision.useHls, effectiveForceHls]
  );

  return {
    isUsingHls: decision.useHls,
    timeOffset,
    activeAudioTrackIndex: effectiveAudioTrackIndex,
    switchAudioTrack,
    seekTo,
    handleVideoError,
    getEffectiveCurrentTime,
    getEffectiveDuration,
  };
}
