import { describe, it, expect, vi, beforeEach } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { FavoriteButton } from './FavoriteButton';
import { api } from '../../api/client';
import { applyFavoriteToCard, onFavoriteChanged } from '../../utils/favoriteEvents';

describe('FavoriteButton', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('toggles aria-label and aria-pressed optimistically, renders filled Heart with aria-hidden, and stops click/keydown propagation', async () => {
    const addSpy = vi.spyOn(api, 'addFavorite').mockResolvedValue({
      item_id: 42,
      is_favorite: true,
    });
    const removeSpy = vi.spyOn(api, 'removeFavorite').mockResolvedValue({
      item_id: 42,
      is_favorite: false,
    });
    const parentClick = vi.fn();
    const parentKeyDown = vi.fn();
    const onToggled = vi.fn();
    const eventListener = vi.fn();
    const unsubscribe = onFavoriteChanged(eventListener);

    render(
      <div onClick={parentClick} onKeyDown={parentKeyDown}>
        <FavoriteButton
          itemId={42}
          title="Inception"
          mediaType="movie"
          isFavorite={false}
          variant="icon"
          onToggled={onToggled}
        />
      </div>
    );

    const btn = screen.getByRole('button', { name: 'Add Inception to favorites' });
    expect(btn).toHaveAttribute('aria-pressed', 'false');
    expect(btn.className).toContain('min-w-7');
    expect(btn.className).toContain('min-h-7');

    const initialSvg = btn.querySelector('svg');
    expect(initialSvg).toHaveAttribute('aria-hidden', 'true');

    fireEvent.keyDown(btn, { key: 'Enter' });
    fireEvent.keyDown(btn, { key: ' ' });
    expect(parentKeyDown).not.toHaveBeenCalled();

    fireEvent.click(btn);
    expect(parentClick).not.toHaveBeenCalled();

    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith(42);
    });
    expect(onToggled).toHaveBeenCalledWith(true);

    const activeBtn = screen.getByRole('button', { name: 'Remove Inception from favorites' });
    expect(activeBtn).toHaveAttribute('aria-pressed', 'true');
    const activeSvg = activeBtn.querySelector('svg');
    expect(activeSvg).toHaveAttribute('aria-hidden', 'true');
    expect(activeSvg?.getAttribute('class') || '').toContain('fill-cta');
    expect(activeSvg?.getAttribute('class') || '').toContain('text-cta');

    expect(eventListener).toHaveBeenCalledTimes(1);
    expect(eventListener).toHaveBeenCalledWith({ itemId: 42, isFavorite: true });

    // Click again to remove from favorites
    fireEvent.click(activeBtn);
    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith(42);
    });
    expect(screen.getByRole('button', { name: 'Add Inception to favorites' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );
    expect(eventListener).toHaveBeenLastCalledWith({ itemId: 42, isFavorite: false });

    unsubscribe();
  });

  it('rolls back optimistic state when api.addFavorite or api.removeFavorite rejects', async () => {
    vi.spyOn(api, 'addFavorite').mockRejectedValueOnce(new Error('network error'));
    vi.spyOn(api, 'removeFavorite').mockRejectedValueOnce(new Error('network error'));
    const onToggled = vi.fn();
    const eventListener = vi.fn();
    const unsubscribe = onFavoriteChanged(eventListener);

    const { rerender } = render(
      <FavoriteButton
        itemId={42}
        title="Inception"
        isFavorite={false}
        onToggled={onToggled}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Add Inception to favorites' }));
    await waitFor(() => {
      expect(onToggled).toHaveBeenLastCalledWith(false);
    });
    expect(screen.getByRole('button', { name: 'Add Inception to favorites' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );
    expect(eventListener).not.toHaveBeenCalled();

    // Rerender as favorited and verify rollback when removeFavorite rejects
    onToggled.mockClear();
    rerender(
      <FavoriteButton
        itemId={42}
        title="Inception"
        isFavorite={true}
        onToggled={onToggled}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Remove Inception from favorites' }));
    await waitFor(() => {
      expect(onToggled).toHaveBeenLastCalledWith(true);
    });
    expect(screen.getByRole('button', { name: 'Remove Inception from favorites' })).toHaveAttribute(
      'aria-pressed',
      'true'
    );
    expect(eventListener).not.toHaveBeenCalled();

    unsubscribe();
  });

  it('works on all mediaTypes including show and live_channel', async () => {
    const addSpy = vi
      .spyOn(api, 'addFavorite')
      .mockImplementation(async (itemId: number) => ({ item_id: itemId, is_favorite: true }));

    render(
      <div>
        <FavoriteButton
          itemId={77}
          title="Severance"
          mediaType="show"
          isFavorite={false}
          variant="icon"
        />
        <FavoriteButton
          itemId={88}
          title="Live News 24"
          mediaType="live_channel"
          isFavorite={false}
          variant="pill"
        />
      </div>
    );

    const showBtn = screen.getByRole('button', { name: 'Add Severance to favorites' });
    const liveBtn = screen.getByRole('button', { name: 'Add Live News 24 to favorites' });

    fireEvent.click(showBtn);
    fireEvent.click(liveBtn);

    await waitFor(() => {
      expect(addSpy).toHaveBeenCalledWith(77);
      expect(addSpy).toHaveBeenCalledWith(88);
    });

    expect(
      screen.getByRole('button', { name: 'Remove Severance from favorites' })
    ).toHaveAttribute('aria-pressed', 'true');
    expect(
      screen.getByRole('button', { name: 'Remove Live News 24 from favorites' })
    ).toHaveAttribute('aria-pressed', 'true');
  });

  it('supports alwaysVisible on icon variant, syncs when itemId changes, and updates cards via applyFavoriteToCard', async () => {
    vi.spyOn(api, 'addFavorite').mockResolvedValue({ item_id: 10, is_favorite: true });

    const { rerender } = render(
      <FavoriteButton
        itemId={10}
        title="Episode 1"
        isFavorite={false}
        variant="icon"
        alwaysVisible
      />
    );

    const btn = screen.getByRole('button', { name: 'Add Episode 1 to favorites' });
    expect(btn.className).toContain('opacity-100');
    expect(btn.className).not.toContain('opacity-0');

    fireEvent.click(btn);
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: 'Remove Episode 1 from favorites' })
      ).toHaveAttribute('aria-pressed', 'true');
    });

    rerender(
      <FavoriteButton
        itemId={11}
        title="Episode 2"
        isFavorite={false}
        variant="icon"
        alwaysVisible
      />
    );

    expect(screen.getByRole('button', { name: 'Add Episode 2 to favorites' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );

    expect(
      applyFavoriteToCard({ id: 11, title: 'Episode 2', media_type: 'episode' }, true)
    ).toEqual({
      id: 11,
      title: 'Episode 2',
      media_type: 'episode',
      is_favorite: true,
    });
  });
});
