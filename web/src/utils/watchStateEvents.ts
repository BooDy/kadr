import type { CardViewModel } from '../types';

const WATCH_STATE_EVENT = 'kadr:watch-state-changed';

export interface WatchStateEventPayload {
  itemIds: number[];
  watched: boolean;
}

export function isCardWatched(
  card?: Pick<CardViewModel, 'playback_progress'> | null
): boolean {
  return (
    typeof card?.playback_progress === 'number' && card.playback_progress >= 1
  );
}

export function applyWatchStateToCard(
  card: CardViewModel,
  watched: boolean
): CardViewModel {
  const nextBadge =
    card.badge === 'RESUME' ? undefined : card.badge;
  return {
    ...card,
    playback_progress: watched ? 1 : undefined,
    badge: nextBadge,
  };
}

export function emitWatchStateChanged(
  itemIds: number[],
  watched: boolean
): void {
  if (typeof window === 'undefined' || itemIds.length === 0) return;
  window.dispatchEvent(
    new CustomEvent<WatchStateEventPayload>(WATCH_STATE_EVENT, {
      detail: { itemIds, watched },
    })
  );
}

export function onWatchStateChanged(
  handler: (detail: { itemIds: Set<number>; watched: boolean }) => void
): () => void {
  if (typeof window === 'undefined') return () => {};
  const listener = (evt: Event) => {
    const custom = evt as CustomEvent<WatchStateEventPayload>;
    if (!custom.detail) return;
    handler({
      itemIds: new Set(custom.detail.itemIds),
      watched: custom.detail.watched,
    });
  };
  window.addEventListener(WATCH_STATE_EVENT, listener);
  return () => window.removeEventListener(WATCH_STATE_EVENT, listener);
}
