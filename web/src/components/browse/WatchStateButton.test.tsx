import { describe, it, expect, vi, beforeEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WatchStateButton } from './WatchStateButton';
import { api } from '../../api/client';
import { onWatchStateChanged } from '../../utils/watchStateEvents';

describe('WatchStateButton', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('hides completely for live_channel items', () => {
    const { container } = render(
      <WatchStateButton
        itemId={10}
        title="Live News"
        mediaType="live_channel"
        watched={false}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it('toggles watched state optimistically, stops card click and keydown propagation, and disambiguates aria-label', async () => {
    const setSpy = vi.spyOn(api, 'setPlaybackState').mockResolvedValue({
      user_id: 'u1',
      media_item_id: 42,
      playback_position_seconds: 0,
      watch_state: 'completed',
      last_watched_at: 1000,
      play_count: 1,
    });
    const parentClick = vi.fn();
    const parentKeyDown = vi.fn();
    const onToggled = vi.fn();
    const eventListener = vi.fn();
    const unsubscribe = onWatchStateChanged(eventListener);

    render(
      <div onClick={parentClick} onKeyDown={parentKeyDown}>
        <WatchStateButton
          itemId={42}
          title="Inception"
          mediaType="movie"
          watched={false}
          variant="icon"
          onToggled={onToggled}
        />
      </div>
    );

    const btn = screen.getByRole('button', { name: 'Mark Inception as watched' });
    expect(btn).toHaveAttribute('aria-pressed', 'false');

    fireEvent.keyDown(btn, { key: 'Enter' });
    fireEvent.keyDown(btn, { key: ' ' });
    expect(parentKeyDown).not.toHaveBeenCalled();

    fireEvent.click(btn);
    expect(parentClick).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(setSpy).toHaveBeenCalledWith(42, true);
    });
    expect(onToggled).toHaveBeenCalledWith(true);
    expect(screen.getByRole('button', { name: 'Mark Inception as unwatched' })).toHaveAttribute(
      'aria-pressed',
      'true'
    );
    expect(eventListener).toHaveBeenCalledTimes(1);
    expect(eventListener.mock.calls[0][0].watched).toBe(true);
    expect(eventListener.mock.calls[0][0].itemIds.has(42)).toBe(true);

    unsubscribe();
  });

  it('rolls back optimistic state when API call fails', async () => {
    vi.spyOn(api, 'setPlaybackState').mockRejectedValue(new Error('network error'));
    const onToggled = vi.fn();

    render(
      <WatchStateButton
        itemId={42}
        title="Inception"
        mediaType="movie"
        watched={false}
        onToggled={onToggled}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Mark Inception as watched' }));
    await waitFor(() => {
      expect(onToggled).toHaveBeenLastCalledWith(false);
    });
    expect(screen.getByRole('button', { name: 'Mark Inception as watched' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );
  });

  it('supports alwaysVisible prop on icon variant so row buttons remain visible without hover', () => {
    render(
      <WatchStateButton
        itemId={99}
        title="Pilot"
        mediaType="episode"
        watched={false}
        variant="icon"
        alwaysVisible
      />
    );

    const btn = screen.getByRole('button', { name: 'Mark Pilot as watched' });
    expect(btn.className).toContain('opacity-100');
    expect(btn.className).not.toContain('opacity-0');
  });
});
