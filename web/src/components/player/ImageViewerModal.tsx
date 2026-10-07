import { useState, useEffect, useCallback, useRef, type FC } from 'react';
import {
  X,
  ChevronLeft,
  ChevronRight,
  Play,
  Pause,
  Maximize,
  Minimize,
} from 'lucide-react';
import type { FolderImageEntry } from '../../types';

export interface ImageViewerModalProps {
  images: FolderImageEntry[];
  initialIndex?: number;
  isOpen: boolean;
  autoPlay?: boolean;
  onClose: () => void;
}

export const ImageViewerModal: FC<ImageViewerModalProps> = ({
  images,
  initialIndex = 0,
  isOpen,
  autoPlay = false,
  onClose,
}) => {
  const [currentIndex, setCurrentIndex] = useState<number>(initialIndex);
  const [isPlaying, setIsPlaying] = useState<boolean>(autoPlay);
  const [isControlsVisible, setIsControlsVisible] = useState<boolean>(true);
  const [isFullscreen, setIsFullscreen] = useState<boolean>(false);
  const controlsTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const onFullscreenChange = () => {
      setIsFullscreen(!!document.fullscreenElement);
    };
    document.addEventListener('fullscreenchange', onFullscreenChange);
    return () => document.removeEventListener('fullscreenchange', onFullscreenChange);
  }, []);

  useEffect(() => {
    if (isOpen) {
      setCurrentIndex(initialIndex);
      setIsPlaying(autoPlay);
      setIsControlsVisible(true);
    }
  }, [isOpen, initialIndex, autoPlay]);

  const handleNext = useCallback(() => {
    if (images.length === 0) return;
    setCurrentIndex((prev) => (prev + 1) % images.length);
  }, [images.length]);

  const handlePrev = useCallback(() => {
    if (images.length === 0) return;
    setCurrentIndex((prev) => (prev - 1 + images.length) % images.length);
  }, [images.length]);

  const togglePlay = useCallback(() => {
    setIsPlaying((prev) => !prev);
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen?.().catch(() => {});
      setIsFullscreen(true);
    } else {
      document.exitFullscreen?.().catch(() => {});
      setIsFullscreen(false);
    }
  }, []);

  // Slideshow auto-advance interval (4 seconds)
  useEffect(() => {
    if (!isOpen || !isPlaying || images.length <= 1) return;

    const timer = setInterval(() => {
      handleNext();
    }, 4000);

    return () => clearInterval(timer);
  }, [isOpen, isPlaying, images.length, handleNext]);

  const resetControlsTimer = useCallback(() => {
    setIsControlsVisible(true);
    if (controlsTimeoutRef.current) {
      clearTimeout(controlsTimeoutRef.current);
    }
    controlsTimeoutRef.current = setTimeout(() => {
      setIsControlsVisible(false);
    }, 3000);
  }, []);

  // Controls auto-hide after 3 seconds of inactivity
  useEffect(() => {
    if (!isOpen) {
      if (controlsTimeoutRef.current) {
        clearTimeout(controlsTimeoutRef.current);
      }
      return;
    }

    resetControlsTimer();
    window.addEventListener('mousemove', resetControlsTimer);
    return () => {
      if (controlsTimeoutRef.current) {
        clearTimeout(controlsTimeoutRef.current);
      }
      window.removeEventListener('mousemove', resetControlsTimer);
    };
  }, [isOpen, resetControlsTimer]);

  // Keyboard navigation
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      resetControlsTimer();
      switch (e.key) {
        case 'ArrowRight':
          e.preventDefault();
          handleNext();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          handlePrev();
          break;
        case ' ':
          e.preventDefault();
          togglePlay();
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
        case 'f':
        case 'F':
          e.preventDefault();
          toggleFullscreen();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, handleNext, handlePrev, togglePlay, onClose, toggleFullscreen, resetControlsTimer]);

  if (!isOpen || images.length === 0) return null;

  const currentImage = images[currentIndex] || images[0];

  return (
    <div
      role="dialog"
      aria-label="Image viewer"
      className="fixed inset-0 z-50 bg-canvas/98 flex flex-col justify-between overflow-hidden select-none"
    >
      {/* Top Header Bar */}
      <div
        className={`w-full p-4 sm:p-6 bg-gradient-to-b from-canvas/90 via-canvas/40 to-transparent flex items-center justify-between gap-4 transition-opacity duration-300 z-10 ${
          isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="flex items-center gap-3 min-w-0">
          <span className="font-semibold text-base sm:text-lg text-text-main truncate">
            {currentImage.name}
          </span>
          <span className="text-xs px-2.5 py-0.5 rounded-full font-mono font-medium bg-panel border border-border-subtle text-muted shrink-0">
            {currentIndex + 1} / {images.length}
          </span>
          {isPlaying && (
            <span className="text-xs px-2.5 py-0.5 rounded-full font-semibold bg-accent/20 text-accent border border-accent/30 shrink-0">
              Slideshow (4s)
            </span>
          )}
        </div>

        <button
          type="button"
          onClick={onClose}
          aria-label="Close image viewer"
          className="p-2 rounded-xl bg-panel/80 hover:bg-panel border border-border-subtle text-muted hover:text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none shrink-0"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      {/* Main Image Display Area */}
      <div className="relative flex-1 flex items-center justify-center p-4">
        {/* Previous Button Overlay */}
        {images.length > 1 && (
          <button
            type="button"
            onClick={handlePrev}
            aria-label="Previous image"
            className={`absolute left-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-panel/80 hover:bg-panel border border-border-subtle text-text-main shadow-lg transition-all duration-300 z-10 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
              isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
            }`}
          >
            <ChevronLeft className="w-6 h-6" />
          </button>
        )}

        {/* The Image */}
        <img
          key={currentImage.url}
          src={currentImage.url}
          alt={currentImage.name}
          className="max-h-[82vh] max-w-[92vw] object-contain shadow-2xl rounded-lg transition-opacity duration-200"
        />

        {/* Next Button Overlay */}
        {images.length > 1 && (
          <button
            type="button"
            onClick={handleNext}
            aria-label="Next image"
            className={`absolute right-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-panel/80 hover:bg-panel border border-border-subtle text-text-main shadow-lg transition-all duration-300 z-10 cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none ${
              isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
            }`}
          >
            <ChevronRight className="w-6 h-6" />
          </button>
        )}
      </div>

      {/* Bottom Controls Bar */}
      <div
        className={`w-full p-4 sm:p-6 bg-gradient-to-t from-canvas/95 via-canvas/50 to-transparent flex items-center justify-between gap-4 transition-opacity duration-300 z-10 ${
          isControlsVisible ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={togglePlay}
            aria-label={isPlaying ? 'Pause slideshow' : 'Play slideshow'}
            className="flex items-center gap-2 px-4 py-2 rounded-xl bg-cta hover:bg-cta-hover text-white text-xs font-semibold shadow-md transition-all cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            {isPlaying ? (
              <>
                <Pause className="w-4 h-4 fill-white text-white" />
                <span>Pause</span>
              </>
            ) : (
              <>
                <Play className="w-4 h-4 fill-white text-white" />
                <span>Play Slideshow</span>
              </>
            )}
          </button>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={toggleFullscreen}
            aria-label={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
            className="p-2 rounded-xl bg-panel/80 hover:bg-panel border border-border-subtle text-muted hover:text-text-main transition-colors cursor-pointer focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none"
          >
            {isFullscreen ? <Minimize className="w-4 h-4" /> : <Maximize className="w-4 h-4" />}
          </button>
        </div>
      </div>
    </div>
  );
};
