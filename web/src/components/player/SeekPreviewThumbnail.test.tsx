import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, act, fireEvent } from '@testing-library/react';
import { computeStoryboardTile, useStoryboard } from '../../player/useStoryboard';
import { SeekPreviewThumbnail } from './SeekPreviewThumbnail';
import { api } from '../../api/client';
import type { StoryboardManifest } from '../../types';

const sampleManifest: StoryboardManifest = {
  interval_ms: 10000,
  tile_width: 320,
  tile_height: 180,
  columns: 10,
  rows: 10,
  total_tiles: 125,
  sheets: [
    { index: 0, url: '/api/v1/media/10/storyboard/0.jpg', first_tile: 0, tile_count: 100 },
    { index: 1, url: '/api/v1/media/10/storyboard/1.jpg', first_tile: 100, tile_count: 25 },
  ],
};

describe('Storyboard math & SeekPreviewThumbnail', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    api.setToken('jwt-sb');
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('computes scaled CSS sprite sheet coordinates across multi-sheet boundaries and rejects non-finite seconds', () => {
    // At 1015s -> tileIndex = floor(1015000 / 10000) = 101 -> sheet 1, localIndex 1 -> row 0, col 1
    // Scaled from 320x180 to displayWidth 160 -> height 90
    const slice = computeStoryboardTile(sampleManifest, 1015, 160);
    expect(slice).toEqual({
      sheetUrl: '/api/v1/media/10/storyboard/1.jpg?token=jwt-sb',
      width: 160,
      height: 90,
      backgroundPosition: '-160px -0px',
      backgroundSize: '1600px 900px',
    });

    expect(computeStoryboardTile(sampleManifest, Number.NaN, 160)).toBeNull();
    expect(computeStoryboardTile(sampleManifest, Number.POSITIVE_INFINITY, 160)).toBeNull();
  });

  it('polls 202 generating until 200 ready in useStoryboard and skips ineligible audio-only media', async () => {
    const spy = vi
      .spyOn(api, 'getStoryboard')
      .mockResolvedValueOnce({ status: 'generating', retryAfterMs: 5000 })
      .mockResolvedValueOnce({ status: 'ready', manifest: sampleManifest });

    const Probe = ({ mediaType = 'movie', durationSeconds = 1250 }: { mediaType?: string; durationSeconds?: number }) => {
      const sb = useStoryboard({
        itemId: 10,
        mediaType,
        durationSeconds,
      });
      return <div data-testid="sb-status">{sb.status}</div>;
    };

    const { rerender } = render(<Probe />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByTestId('sb-status').textContent).toBe('generating');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(spy).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('sb-status').textContent).toBe('ready');

    rerender(<Probe mediaType="music" durationSeconds={200} />);
    expect(screen.getByTestId('sb-status').textContent).toBe('unavailable');
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('renders sprite tile when ready and debounced /frame fallback when generating', async () => {
    const { container, rerender } = render(
      <SeekPreviewThumbnail
        itemId={10}
        seconds={25}
        percent={50}
        status="ready"
        manifest={sampleManifest}
      />
    );
    const wrapper = container.firstElementChild as HTMLElement;
    expect(wrapper).toHaveAttribute('aria-hidden', 'true');
    expect(wrapper.className).toContain('pointer-events-none');

    const sprite = screen.getByTestId('seek-preview-sprite');
    expect(sprite.style.backgroundImage).toContain('/api/v1/media/10/storyboard/0.jpg');
    expect(screen.getByText('00:25')).toBeInTheDocument();

    rerender(
      <SeekPreviewThumbnail
        itemId={10}
        seconds={33}
        percent={50}
        status="generating"
        manifest={null}
      />
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    const img = screen.getByTestId('seek-preview-frame') as HTMLImageElement;
    expect(img.src).toContain('/api/v1/media/10/frame?t=32&w=320');

    // Falls back to timestamp pill if /frame image errors
    fireEvent.error(img);
    expect(screen.queryByTestId('seek-preview-frame')).not.toBeInTheDocument();
    expect(screen.getByText('00:33')).toBeInTheDocument();
  });
});
