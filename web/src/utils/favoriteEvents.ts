import type { CardViewModel } from '../types';

const FAVORITE_EVENT = 'kadr:favorite-changed';

export interface FavoriteEventPayload {
  itemId: number;
  isFavorite: boolean;
}

export function applyFavoriteToCard(
  card: CardViewModel,
  isFavorite: boolean
): CardViewModel {
  return {
    ...card,
    is_favorite: isFavorite,
  };
}

export function emitFavoriteChanged(
  itemId: number,
  isFavorite: boolean
): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(
    new CustomEvent<FavoriteEventPayload>(FAVORITE_EVENT, {
      detail: { itemId, isFavorite },
    })
  );
}

export function onFavoriteChanged(
  handler: (detail: FavoriteEventPayload) => void
): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (evt: Event) => {
    const custom = evt as CustomEvent<FavoriteEventPayload>;
    if (!custom.detail) return;
    handler({
      itemId: custom.detail.itemId,
      isFavorite: custom.detail.isFavorite,
    });
  };
  window.addEventListener(FAVORITE_EVENT, listener);
  return () => window.removeEventListener(FAVORITE_EVENT, listener);
}
