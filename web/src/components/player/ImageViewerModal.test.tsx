import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ImageViewerModal } from './ImageViewerModal';
import type { FolderImageEntry } from '../../types';

describe('ImageViewerModal Component', () => {
  const mockImages: FolderImageEntry[] = [
    { name: 'photo1.jpg', path: 'photos/photo1.jpg', url: '/api/v1/libraries/lib-1/image?path=photo1.jpg', size_bytes: 1024 },
    { name: 'photo2.png', path: 'photos/photo2.png', url: '/api/v1/libraries/lib-1/image?path=photo2.png', size_bytes: 2048 },
    { name: 'photo3.webp', path: 'photos/photo3.webp', url: '/api/v1/libraries/lib-1/image?path=photo3.webp', size_bytes: 4096 },
  ];

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders current image, filename, and index counter', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();
    expect(screen.getByText('1 / 3')).toBeInTheDocument();
    const img = screen.getByRole('img');
    expect(img).toHaveAttribute('src', mockImages[0].url);
  });

  it('navigates next and previous on button click and wraps around', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    const nextBtn = screen.getByRole('button', { name: /next image/i });
    fireEvent.click(nextBtn);
    expect(screen.getByText('photo2.png')).toBeInTheDocument();
    expect(screen.getByText('2 / 3')).toBeInTheDocument();

    const prevBtn = screen.getByRole('button', { name: /previous image/i });
    fireEvent.click(prevBtn);
    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();

    // Wrap around to last
    fireEvent.click(prevBtn);
    expect(screen.getByText('photo3.webp')).toBeInTheDocument();
  });

  it('handles keyboard navigation: ArrowRight, ArrowLeft, Space to pause/play, Escape to close', () => {
    const onClose = vi.fn();
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={onClose}
      />
    );

    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(screen.getByText('photo2.png')).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'ArrowLeft' });
    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();

    // Escape closes
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('auto-advances every 4 seconds when in slideshow mode and pauses with space', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        autoPlay={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText('photo1.jpg')).toBeInTheDocument();

    // Advance 4 seconds
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(screen.getByText('photo2.png')).toBeInTheDocument();

    // Toggle pause with Space
    fireEvent.keyDown(window, { key: ' ' });

    // Advance 4 seconds: should NOT advance
    act(() => {
      vi.advanceTimersByTime(4000);
    });
    expect(screen.getByText('photo2.png')).toBeInTheDocument();
  });

  it('does not render previous and next buttons when single image is provided', () => {
    render(
      <ImageViewerModal
        images={[mockImages[0]]}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.queryByRole('button', { name: /previous image/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /next image/i })).not.toBeInTheDocument();
  });

  it('syncs fullscreen state on document fullscreenchange event', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByRole('button', { name: /fullscreen/i })).toBeInTheDocument();

    // Mock document.fullscreenElement
    Object.defineProperty(document, 'fullscreenElement', {
      value: document.documentElement,
      configurable: true,
    });
    act(() => {
      document.dispatchEvent(new Event('fullscreenchange'));
    });

    expect(screen.getByRole('button', { name: /exit fullscreen/i })).toBeInTheDocument();

    Object.defineProperty(document, 'fullscreenElement', {
      value: null,
      configurable: true,
    });
    act(() => {
      document.dispatchEvent(new Event('fullscreenchange'));
    });

    expect(screen.getByRole('button', { name: /fullscreen/i })).toBeInTheDocument();
  });

  it('resets HUD auto-hide timer when a key is pressed', () => {
    render(
      <ImageViewerModal
        images={mockImages}
        initialIndex={0}
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    const closeBtn = screen.getByRole('button', { name: /close image viewer/i });
    const headerContainer = closeBtn.closest('div');
    expect(headerContainer).toHaveClass('opacity-100');

    // Advance 2 seconds (not yet hidden)
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(headerContainer).toHaveClass('opacity-100');

    // Press a key to reset timer
    fireEvent.keyDown(window, { key: 'ArrowRight' });

    // Advance 2 more seconds (total 4s from start, but 2s from keypress -> still visible)
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(headerContainer).toHaveClass('opacity-100');

    // Advance 1.5 more seconds (3.5s from keypress -> now hidden)
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(headerContainer).toHaveClass('opacity-0');
  });
});
