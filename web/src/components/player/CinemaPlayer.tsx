import { useState, useEffect, useRef, useCallback, type FC } from 'react';
import { api } from '../../api/client';
import type {
  SubtitleTrack,
  SubtitleSearchResult,
  ItemDetailsPayload,
  PlaylistItemView,
} from '../../types';
import { PlayerControls, formatPlaybackTime } from './PlayerControls';

export interface CinemaPlayerProps {
  itemId: number;
  onClose: () => void;
  returnLabel?: string;
  queue?: PlaylistItemView[];
  currentIndex?: number;
  onSelectQueueIndex?: (index: number) => void;
}

export const CinemaPlayer: FC<CinemaPlayerProps> = ({
  itemId,
  onClose,
  returnLabel,
  queue,
  currentIndex,
  onSelectQueueIndex,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  const [itemDetails, setItemDetails] = useState<ItemDetailsPayload | null>(null);
  const [subtitles, setSubtitles] = useState<SubtitleTrack[]>([]);
  const [activeSubtitleId, setActiveSubtitleId] = useState<number | null>(null);
  const [recentlyDownloadedId, setRecentlyDownloadedId] = useState<number | null>(null);
  const [isSubtitlesMenuOpen, setIsSubtitlesMenuOpen] = useState(false);
  const [isQueueMenuOpen, setIsQueueMenuOpen] = useState(false);

  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [resumePosition, setResumePosition] = useState<number | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [isControlsVisible, setIsControlsVisible] = useState(true);

  const sessionIdRef = useRef<string | null>(null);
  const resumePositionRef = useRef<number | null>(null);
  const resumeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isMetadataLoadedRef = useRef(false);
  const hasSeekedResumeRef = useRef(false);
  const hideTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isSubtitlesMenuOpenRef = useRef(false);
  const isQueueMenuOpenRef = useRef(false);

  const streamUrl = api.getStreamUrl(itemId);

  const showControls = useCallback(() => {
    setIsControlsVisible(true);
    if (hideTimeoutRef.current) {
      clearTimeout(hideTimeoutRef.current);
      hideTimeoutRef.current = null;
    }
    if (!isSubtitlesMenuOpenRef.current && !isQueueMenuOpenRef.current) {
      hideTimeoutRef.current = setTimeout(() => {
        setIsControlsVisible(false);
      }, 3000);
    }
  }, []);

  useEffect(() => {
    isSubtitlesMenuOpenRef.current = isSubtitlesMenuOpen;
    isQueueMenuOpenRef.current = isQueueMenuOpen;
    if (isSubtitlesMenuOpen || isQueueMenuOpen) {
      setIsControlsVisible(true);
      if (hideTimeoutRef.current) {
        clearTimeout(hideTimeoutRef.current);
        hideTimeoutRef.current = null;
      }
    } else {
      showControls();
    }
  }, [isSubtitlesMenuOpen, isQueueMenuOpen, showControls]);

  const handleSearchSubtitles = useCallback(
    async (language: string): Promise<SubtitleSearchResult[]> => {
      const res = await api.searchSubtitles(itemId, language.trim() || undefined);
      return res.matches || [];
    },
    [itemId]
  );

  const handleDownloadSubtitle = useCallback(
    async (result: SubtitleSearchResult): Promise<void> => {
      await api.downloadSubtitle(itemId, {
        file_id: result.id,
        language: result.language,
        title: result.release_name || undefined,
        is_forced: false,
      });
      const updatedTracks = await api.getSubtitles(itemId);
      setSubtitles(updatedTracks);
      const latestDownloaded =
        updatedTracks.find(
          (t) => t.source === 'downloaded' && !subtitles.some((old) => old.id === t.id)
        ) ||
        updatedTracks.find((t) => !subtitles.some((old) => old.id === t.id)) ||
        updatedTracks[updatedTracks.length - 1];
      if (latestDownloaded) {
        setRecentlyDownloadedId(latestDownloaded.id);
      }
    },
    [itemId, subtitles]
  );

  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      if (containerRef.current?.requestFullscreen) {
        containerRef.current.requestFullscreen().catch(() => {});
      }
    } else {
      if (document.exitFullscreen) {
        document.exitFullscreen().catch(() => {});
      }
    }
  }, []);

  // Listen to fullscreen changes
  useEffect(() => {
    const handleFsChange = () => {
      setIsFullscreen(!!document.fullscreenElement);
    };
    document.addEventListener('fullscreenchange', handleFsChange);
    return () => {
      document.removeEventListener('fullscreenchange', handleFsChange);
    };
  }, []);

  // Reset per-item playback refs and state whenever itemId changes
  useEffect(() => {
    isMetadataLoadedRef.current = false;
    hasSeekedResumeRef.current = false;
    resumePositionRef.current = null;
    if (resumeTimerRef.current) {
      clearTimeout(resumeTimerRef.current);
      resumeTimerRef.current = null;
    }
    setResumePosition(null);
    setCurrentTime(0);
    setDuration(0);
    setActiveSubtitleId(null);
    setSubtitles([]);
    setItemDetails(null);
    setRecentlyDownloadedId(null);
    setIsLoading(false);
    if (videoRef.current) {
      videoRef.current.currentTime = 0;
    }
  }, [itemId]);

  // Initialize playback session, heartbeat interval, and close on unmount / itemId change
  useEffect(() => {
    let isDisposed = false;
    let sessionId: string | null = null;
    let heartbeatInterval: ReturnType<typeof setInterval> | null = null;
    sessionIdRef.current = null;

    api.createPlaybackSession(itemId)
      .then((session) => {
        if (isDisposed) {
          api.closePlaybackSession(session.session_id).catch(() => {});
          return;
        }
        sessionId = session.session_id;
        sessionIdRef.current = session.session_id;
        heartbeatInterval = setInterval(() => {
          if (sessionId && videoRef.current) {
            api.sendPlaybackHeartbeat(sessionId, Math.floor(videoRef.current.currentTime)).catch(() => {});
          }
        }, 10000);
      })
      .catch(() => {});

    return () => {
      isDisposed = true;
      if (sessionIdRef.current === sessionId) {
        sessionIdRef.current = null;
      }
      if (heartbeatInterval) {
        clearInterval(heartbeatInterval);
      }
      if (sessionId) {
        api.closePlaybackSession(sessionId).catch(() => {});
      }
    };
  }, [itemId]);

  // Fetch initial resume playback state (only resume in_progress items; completed items start at 0s)
  useEffect(() => {
    let isCancelled = false;
    api.getPlaybackState(itemId)
      .then((state) => {
        if (isCancelled) return;
        if (
          state?.watch_state === 'in_progress' &&
          state.playback_position_seconds &&
          state.playback_position_seconds > 0
        ) {
          resumePositionRef.current = state.playback_position_seconds;
          setResumePosition(state.playback_position_seconds);
          if (resumeTimerRef.current) clearTimeout(resumeTimerRef.current);
          resumeTimerRef.current = setTimeout(() => {
            setResumePosition(null);
          }, 4000);
          if (videoRef.current && isMetadataLoadedRef.current && !hasSeekedResumeRef.current) {
            videoRef.current.currentTime = state.playback_position_seconds;
            hasSeekedResumeRef.current = true;
          }
        }
      })
      .catch(() => {});

    return () => {
      isCancelled = true;
      if (resumeTimerRef.current) clearTimeout(resumeTimerRef.current);
    };
  }, [itemId]);

  // Fetch subtitles and item details
  useEffect(() => {
    let isCancelled = false;
    api.getSubtitles(itemId)
      .then((tracks) => {
        if (isCancelled) return;
        setSubtitles(tracks);
        const def = tracks.find((t) => t.is_default);
        if (def) {
          setActiveSubtitleId(def.id);
        }
      })
      .catch(() => {});

    api.getItemDetails(itemId)
      .then((details) => {
        if (isCancelled) return;
        setItemDetails(details);
      })
      .catch(() => {});

    return () => {
      isCancelled = true;
    };
  }, [itemId]);

  // Sync subtitle tracks mode with activeSubtitleId
  useEffect(() => {
    if (videoRef.current && videoRef.current.textTracks) {
      const tracks = videoRef.current.textTracks;
      for (let i = 0; i < tracks.length; i++) {
        const textTrack = tracks[i];
        const sub = subtitles[i];
        if (sub && sub.id === activeSubtitleId) {
          textTrack.mode = 'showing';
        } else {
          textTrack.mode = 'disabled';
        }
      }
    }
  }, [activeSubtitleId, subtitles]);

  const safePlay = useCallback((video: HTMLVideoElement) => {
    try {
      const res = video.play();
      if (res && typeof res.catch === 'function') {
        res.catch(() => {});
      }
    } catch {
      // Ignore autoplay errors
    }
  }, []);

  const handleSeek = useCallback((seconds: number) => {
    if (!videoRef.current) return;
    videoRef.current.currentTime = seconds;
    setCurrentTime(seconds);
  }, []);

  const handlePrevTrack = useCallback(() => {
    if ((videoRef.current?.currentTime ?? currentTime) > 5) {
      handleSeek(0);
    } else if (queue && currentIndex !== undefined && currentIndex > 0) {
      onSelectQueueIndex?.(currentIndex - 1);
    }
  }, [currentTime, queue, currentIndex, onSelectQueueIndex, handleSeek]);

  const handleNextTrack = useCallback(() => {
    if (queue && currentIndex !== undefined && currentIndex + 1 < queue.length) {
      onSelectQueueIndex?.(currentIndex + 1);
    }
  }, [queue, currentIndex, onSelectQueueIndex]);

  // Keyboard shortcut handler
  useEffect(() => {
    showControls();

    const handleKeyDown = (e: KeyboardEvent) => {
      showControls();
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.tagName === 'SELECT' ||
          target.isContentEditable) &&
        target.getAttribute('type') !== 'range'
      ) {
        return;
      }

      if ((e.shiftKey && (e.key === 'N' || e.key === 'n')) || e.key === 'MediaTrackNext') {
        e.preventDefault();
        handleNextTrack();
        return;
      }

      if ((e.shiftKey && (e.key === 'P' || e.key === 'p')) || e.key === 'MediaTrackPrevious') {
        e.preventDefault();
        handlePrevTrack();
        return;
      }

      const video = videoRef.current;
      if (!video) return;

      switch (e.key) {
        case ' ':
        case 'k':
        case 'K':
          e.preventDefault();
          if (video.paused) {
            safePlay(video);
          } else {
            video.pause();
          }
          break;
        case 'ArrowLeft':
          e.preventDefault();
          video.currentTime = Math.max(0, video.currentTime - 10);
          break;
        case 'ArrowRight':
          e.preventDefault();
          video.currentTime = Math.min(video.duration || Infinity, video.currentTime + 10);
          break;
        case 'ArrowUp':
          e.preventDefault();
          video.volume = Math.min(1, Math.round((video.volume + 0.1) * 10) / 10);
          break;
        case 'ArrowDown':
          e.preventDefault();
          video.volume = Math.max(0, Math.round((video.volume - 0.1) * 10) / 10);
          break;
        case 'm':
        case 'M':
          e.preventDefault();
          video.muted = !video.muted;
          break;
        case 'f':
        case 'F':
          e.preventDefault();
          toggleFullscreen();
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      if (hideTimeoutRef.current) {
        clearTimeout(hideTimeoutRef.current);
      }
    };
  }, [onClose, safePlay, showControls, toggleFullscreen, handleNextTrack, handlePrevTrack]);

  // Video event handlers
  const handleLoadedMetadata = () => {
    isMetadataLoadedRef.current = true;
    if (videoRef.current) {
      setDuration(videoRef.current.duration || 0);
      if (resumePositionRef.current && resumePositionRef.current > 0 && !hasSeekedResumeRef.current) {
        videoRef.current.currentTime = resumePositionRef.current;
        hasSeekedResumeRef.current = true;
      }
      // Autoplay
      safePlay(videoRef.current);
    }
  };

  const handleVideoEnded = async () => {
    const finalSec = Math.floor(videoRef.current?.duration || duration || 0);
    if (sessionIdRef.current && finalSec > 0) {
      await api.sendPlaybackHeartbeat(sessionIdRef.current, finalSec).catch(() => {});
    }
    if (queue && currentIndex !== undefined && currentIndex + 1 < queue.length) {
      onSelectQueueIndex?.(currentIndex + 1);
    } else {
      setIsPlaying(false);
    }
  };

  const handleTimeUpdate = () => {
    if (videoRef.current) {
      setCurrentTime(videoRef.current.currentTime);
    }
  };

  const handlePlay = () => setIsPlaying(true);
  const handlePause = () => setIsPlaying(false);

  const handleVolumeChangeFromVideo = () => {
    if (videoRef.current) {
      setVolume(videoRef.current.volume);
      setIsMuted(videoRef.current.muted);
    }
  };

  const handlePlayPause = () => {
    if (!videoRef.current) return;
    if (videoRef.current.paused) {
      safePlay(videoRef.current);
    } else {
      videoRef.current.pause();
    }
  };

  const handleVolumeChange = (vol: number) => {
    if (!videoRef.current) return;
    videoRef.current.volume = vol;
    if (vol > 0 && videoRef.current.muted) {
      videoRef.current.muted = false;
    }
    setVolume(vol);
  };

  const handleToggleMute = () => {
    if (!videoRef.current) return;
    videoRef.current.muted = !videoRef.current.muted;
    setIsMuted(videoRef.current.muted);
  };

  const activeQueueCard =
    currentIndex !== undefined && queue ? queue[currentIndex]?.card : undefined;
  const displayTitle =
    itemDetails?.card?.title || activeQueueCard?.title || `Now playing item #${itemId}`;
  const displaySubtitle = itemDetails?.card?.subtitle ?? activeQueueCard?.subtitle;
  const displayYear = itemDetails?.card?.release_year ?? activeQueueCard?.release_year;

  return (
    <div
      ref={containerRef}
      onMouseMove={showControls}
      onClick={showControls}
      className={`fixed inset-0 z-50 bg-canvas flex flex-col items-center justify-center select-none overflow-hidden ${
        isControlsVisible ? 'cursor-default' : 'cursor-none'
      }`}
    >
      <span className="sr-only">Cinema Player</span>
      <video
        ref={videoRef}
        src={streamUrl}
        autoPlay
        playsInline
        onLoadedMetadata={handleLoadedMetadata}
        onTimeUpdate={handleTimeUpdate}
        onPlay={handlePlay}
        onPause={handlePause}
        onEnded={handleVideoEnded}
        onWaiting={() => setIsLoading(true)}
        onPlaying={() => setIsLoading(false)}
        onCanPlay={() => setIsLoading(false)}
        onSeeked={() => setIsLoading(false)}
        onVolumeChange={handleVolumeChangeFromVideo}
        className="w-full h-full object-contain"
      >
        {subtitles.map((track) => (
          <track
            key={track.id}
            kind="subtitles"
            src={api.getSubtitleStreamUrl(track.id)}
            srcLang={track.language}
            label={track.title || track.language}
            default={track.is_default}
          />
        ))}
      </video>

      {/* Loading Spinner */}
      {isLoading && (
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none z-20">
          <div className="w-12 h-12 rounded-full border-4 border-accent border-t-transparent animate-spin" />
        </div>
      )}

      {/* Resume banner / prompt */}
      {resumePosition !== null && (
        <div className="absolute top-20 left-1/2 -translate-x-1/2 z-40 bg-panel/95 border border-border-subtle rounded-xl text-text-main px-4 py-2.5 shadow-2xl flex items-center gap-3 backdrop-blur-md pointer-events-auto">
          <span className="text-sm font-medium">
            Resumed at <span className="font-mono text-accent">{formatPlaybackTime(resumePosition)}</span>
          </span>
          <button
            onClick={() => {
              if (resumeTimerRef.current) {
                clearTimeout(resumeTimerRef.current);
                resumeTimerRef.current = null;
              }
              if (videoRef.current) {
                videoRef.current.currentTime = 0;
                setCurrentTime(0);
              }
              setResumePosition(null);
            }}
            className="text-xs text-muted hover:text-text-main underline cursor-pointer ml-1 focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none rounded"
          >
            Start Over
          </button>
        </div>
      )}

      <PlayerControls
        title={displayTitle}
        subtitle={displaySubtitle}
        releaseYear={displayYear}
        returnLabel={returnLabel}
        queue={queue}
        currentIndex={currentIndex}
        onSelectQueueIndex={onSelectQueueIndex}
        onPrevTrack={handlePrevTrack}
        onNextTrack={handleNextTrack}
        onQueueMenuToggle={(isOpen) => {
          setIsQueueMenuOpen(isOpen);
          isQueueMenuOpenRef.current = isOpen;
          if (isOpen && hideTimeoutRef.current) {
            clearTimeout(hideTimeoutRef.current);
            hideTimeoutRef.current = null;
          }
        }}
        isPlaying={isPlaying}
        currentTime={currentTime}
        duration={duration}
        volume={volume}
        isMuted={isMuted}
        isFullscreen={isFullscreen}
        subtitles={subtitles}
        activeSubtitleId={activeSubtitleId}
        isVisible={isControlsVisible}
        onPlayPause={handlePlayPause}
        onSeek={handleSeek}
        onVolumeChange={handleVolumeChange}
        onToggleMute={handleToggleMute}
        onToggleFullscreen={toggleFullscreen}
        onSelectSubtitle={setActiveSubtitleId}
        onClose={onClose}
        onSearchSubtitles={handleSearchSubtitles}
        onDownloadSubtitle={handleDownloadSubtitle}
        recentlyDownloadedId={recentlyDownloadedId}
        onSubtitlesMenuToggle={(isOpen) => {
          setIsSubtitlesMenuOpen(isOpen);
          isSubtitlesMenuOpenRef.current = isOpen;
          if (isOpen && hideTimeoutRef.current) {
            clearTimeout(hideTimeoutRef.current);
            hideTimeoutRef.current = null;
          }
        }}
      />
    </div>
  );
};
