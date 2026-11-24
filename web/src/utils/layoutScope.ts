import type { TimeWindow } from '../types';

/**
 * Detects the active browser viewport tier (`'mobile'` below 768px, `'web'` otherwise),
 * safely guarding `window` and `window.matchMedia` for SSR and jsdom environments.
 */
export function detectClientTargetDevice(): 'mobile' | 'web' {
  if (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(max-width: 767px)').matches
  ) {
    return 'mobile';
  }
  return 'web';
}

/**
 * Formats a `Date` instance as local 24-hour `"HH:MM"` (`00:00`..`23:59`).
 */
export function formatLocalTimeHHMM(date: Date = new Date()): string {
  const hours = String(date.getHours()).padStart(2, '0');
  const minutes = String(date.getMinutes()).padStart(2, '0');
  return `${hours}:${minutes}`;
}

function parseHHMMMinutes(hhmm: string): number | null {
  if (typeof hhmm !== 'string' || !/^\d{2}:\d{2}$/.test(hhmm)) {
    return null;
  }
  const hours = Number(hhmm.slice(0, 2));
  const minutes = Number(hhmm.slice(3, 5));
  if (
    !Number.isInteger(hours) ||
    !Number.isInteger(minutes) ||
    hours < 0 ||
    hours > 23 ||
    minutes < 0 ||
    minutes > 59
  ) {
    return null;
  }
  return hours * 60 + minutes;
}

/**
 * Evaluates whether a 24-hour `TimeWindow` (`start` inclusive, `end` exclusive)
 * is active at `hhmm`, supporting both same-day (`08:00`–`12:00`) and overnight
 * (`23:00`–`04:00`) schedules. Returns `false` if `start === end` or malformed.
 */
export function isTimeWindowActive(tw: TimeWindow, hhmm: string): boolean {
  if (!tw) return false;
  const startMins = parseHHMMMinutes(tw.start);
  const endMins = parseHHMMMinutes(tw.end);
  const currentMins = parseHHMMMinutes(hhmm);
  if (
    startMins === null ||
    endMins === null ||
    currentMins === null ||
    startMins === endMins
  ) {
    return false;
  }
  if (startMins < endMins) {
    return currentMins >= startMins && currentMins < endMins;
  }
  return currentMins >= startMins || currentMins < endMins;
}

/**
 * Formats runtime in seconds as compact cinema text (`1h 17m`, `45m`, `1h`),
 * returning `''` when undefined, zero, or negative.
 */
export function formatDurationCompact(seconds?: number): string {
  if (!seconds || !Number.isFinite(seconds) || seconds <= 0) {
    return '';
  }
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  if (hrs > 0) {
    return mins > 0 ? `${hrs}h ${mins}m` : `${hrs}h`;
  }
  return `${Math.max(1, mins)}m`;
}
