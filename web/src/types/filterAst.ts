import type { CardViewModel, WidgetQueryBinding } from './index';

export const MAX_FILTER_AST_DEPTH = 5;
export const MAX_FILTER_AST_RULES = 32;
export const VISUAL_MAX_FILTER_DEPTH = 2;

export type LogicalOp = 'and' | 'or';

export type FilterMediaType =
  | 'movie'
  | 'show'
  | 'season'
  | 'episode'
  | 'anime'
  | 'music'
  | 'home_videos'
  | 'audiobook'
  | 'live_channel'
  | 'unknown';

export type WatchStateValue = 'unwatched' | 'in_progress' | 'completed';

export type ScalarTextField =
  | 'title'
  | 'original_title'
  | 'series_title'
  | 'director'
  | 'studio'
  | 'country'
  | 'language'
  | 'resolution'
  | 'video_codec'
  | 'audio_codec'
  | 'container';

export type ArrayTextField = 'genres' | 'tags' | 'actors' | 'writers';

export type IntFilterField =
  | 'release_year'
  | 'duration_seconds'
  | 'audio_channels'
  | 'play_count';

export type TimeFilterField = 'added_at' | 'last_watched_at';

export type FilterField =
  | 'library_id'
  | 'item_type'
  | ScalarTextField
  | ArrayTextField
  | IntFilterField
  | 'rating'
  | TimeFilterField
  | 'watch_state'
  | 'is_favorite';

export type ScalarTextCondition =
  | { field: ScalarTextField; op: 'eq' | 'contains' | 'starts_with'; value: string }
  | { field: ScalarTextField; op: 'in'; values: string[] }
  | { field: ScalarTextField; op: 'is_empty' };

export type ArrayTextCondition =
  | { field: ArrayTextField; op: 'contains_any' | 'contains_all'; values: string[] }
  | { field: ArrayTextField; op: 'is_empty' };

export type IntFilterCondition =
  | { field: IntFilterField; op: 'eq' | 'gte' | 'lte'; value: number }
  | { field: IntFilterField; op: 'between'; min: number; max: number };

export type RatingFilterCondition =
  | { field: 'rating'; op: 'gte' | 'lte'; value: number }
  | { field: 'rating'; op: 'between'; min: number; max: number };

export type TimeFilterCondition =
  | { field: TimeFilterField; op: 'within_last_days' | 'older_than_days'; days: number }
  | { field: TimeFilterField; op: 'after_epoch' | 'before_epoch'; timestamp: number }
  | { field: TimeFilterField; op: 'between_epochs'; min: number; max: number };

export type WatchStateFilterCondition =
  | { field: 'watch_state'; op: 'state_in'; values: WatchStateValue[] }
  | { field: 'watch_state'; op: 'progress_between'; min: number; max: number };

export interface IsFavoriteFilterCondition {
  field: 'is_favorite';
  op: 'eq';
  value: boolean;
}

export type FilterCondition =
  | { field: 'library_id'; op: 'in'; values: string[] }
  | { field: 'item_type'; op: 'in'; values: FilterMediaType[] }
  | ScalarTextCondition
  | ArrayTextCondition
  | IntFilterCondition
  | RatingFilterCondition
  | TimeFilterCondition
  | WatchStateFilterCondition
  | IsFavoriteFilterCondition;

export type FilterNode =
  | {
      kind: 'group';
      op: LogicalOp;
      negated?: boolean;
      children: FilterNode[];
    }
  | {
      kind: 'rule';
      negated?: boolean;
      condition: FilterCondition;
    };

export interface CatalogFacets {
  genres: string[];
  countries: string[];
  languages: string[];
  studios: string[];
  directors: string[];
  tags: string[];
  resolutions: string[];
  video_codecs: string[];
  audio_codecs: string[];
  containers: string[];
  year_min: number | null;
  year_max: number | null;
}

export interface WidgetPreviewRequest {
  binding: WidgetQueryBinding;
  screen_id?: string;
  limit?: number;
}

export interface WidgetPreviewResponse {
  items: CardViewModel[];
  total_count: number;
  execution_ms: number;
}

/**
 * Computes the group nesting depth of a `FilterNode` tree, matching Rust's
 * `FilterNode::validate_inner` depth semantics:
 * - `null` / `undefined` or a bare `rule` node -> `0`
 * - A root `group` containing only `rule` children -> `1`
 * - A root `group` containing a Level-2 `group` of `rule`s -> `2`
 */
export function getFilterTreeDepth(node: FilterNode | null | undefined): number {
  if (!node || node.kind === 'rule') {
    return 0;
  }
  let maxChildDepth = 0;
  for (const child of node.children) {
    const childDepth = getFilterTreeDepth(child);
    if (childDepth > maxChildDepth) {
      maxChildDepth = childDepth;
    }
  }
  return 1 + maxChildDepth;
}

export function countFilterTreeRules(node: FilterNode | null | undefined): number {
  if (!node) return 0;
  if (node.kind === 'rule') return 1;
  let total = 0;
  for (const child of node.children) {
    total += countFilterTreeRules(child);
  }
  return total;
}

const SCALAR_TEXT_FIELDS = new Set<string>([
  'title',
  'original_title',
  'series_title',
  'director',
  'studio',
  'country',
  'language',
  'resolution',
  'video_codec',
  'audio_codec',
  'container',
]);

const ARRAY_TEXT_FIELDS = new Set<string>(['genres', 'tags', 'actors', 'writers']);
const INT_FIELDS = new Set<string>([
  'release_year',
  'duration_seconds',
  'audio_channels',
  'play_count',
]);
const TIME_FIELDS = new Set<string>(['added_at', 'last_watched_at']);
const MEDIA_TYPE_VALUES = new Set<string>([
  'movie',
  'show',
  'season',
  'episode',
  'anime',
  'music',
  'home_videos',
  'audiobook',
  'live_channel',
  'unknown',
]);
const WATCH_STATE_VALUES = new Set<string>(['unwatched', 'in_progress', 'completed']);

function isStringArray(val: unknown): val is string[] {
  return Array.isArray(val) && val.every((item) => typeof item === 'string');
}

function isFiniteNumber(val: unknown): val is number {
  return typeof val === 'number' && Number.isFinite(val);
}

function validateConditionObject(cond: unknown): string | null {
  if (!cond || typeof cond !== 'object' || Array.isArray(cond)) {
    return 'Rule condition must be a JSON object.';
  }
  const c = cond as Record<string, unknown>;
  if (typeof c.field !== 'string') {
    return 'Rule condition is missing a string "field".';
  }

  if (c.field === 'is_favorite') {
    if (c.op !== undefined && c.op !== 'eq') {
      return 'Condition "is_favorite" requires op "eq" (or omitted) and a boolean "value".';
    }
    if (typeof c.value !== 'boolean') {
      return 'Condition "is_favorite" requires a boolean "value".';
    }
    c.op = 'eq';
    return null;
  }

  if (typeof c.op !== 'string') {
    return `Condition "${c.field}" is missing a string "op".`;
  }

  if (c.field === 'library_id') {
    if (c.op !== 'in' || !isStringArray(c.values)) {
      return 'Condition "library_id" requires op "in" and string[] "values".';
    }
    return null;
  }

  if (c.field === 'item_type') {
    if (
      c.op !== 'in' ||
      !isStringArray(c.values) ||
      !c.values.every((v) => MEDIA_TYPE_VALUES.has(v))
    ) {
      return 'Condition "item_type" requires op "in" and valid MediaType[] "values".';
    }
    return null;
  }

  if (SCALAR_TEXT_FIELDS.has(c.field)) {
    if (c.op === 'eq' || c.op === 'contains' || c.op === 'starts_with') {
      return typeof c.value === 'string'
        ? null
        : `Condition "${c.field}" with op "${c.op}" requires a string "value".`;
    }
    if (c.op === 'in') {
      return isStringArray(c.values)
        ? null
        : `Condition "${c.field}" with op "in" requires string[] "values".`;
    }
    if (c.op === 'is_empty') return null;
    return `Unsupported operator "${c.op}" for text field "${c.field}".`;
  }

  if (ARRAY_TEXT_FIELDS.has(c.field)) {
    if (c.op === 'contains_any' || c.op === 'contains_all') {
      return isStringArray(c.values)
        ? null
        : `Condition "${c.field}" with op "${c.op}" requires string[] "values".`;
    }
    if (c.op === 'is_empty') return null;
    return `Unsupported operator "${c.op}" for list field "${c.field}".`;
  }

  if (INT_FIELDS.has(c.field)) {
    if (c.op === 'eq' || c.op === 'gte' || c.op === 'lte') {
      return isFiniteNumber(c.value)
        ? null
        : `Condition "${c.field}" with op "${c.op}" requires numeric "value".`;
    }
    if (c.op === 'between') {
      return isFiniteNumber(c.min) && isFiniteNumber(c.max)
        ? null
        : `Condition "${c.field}" with op "between" requires numeric "min" and "max".`;
    }
    return `Unsupported operator "${c.op}" for numeric field "${c.field}".`;
  }

  if (c.field === 'rating') {
    if (c.op === 'gte' || c.op === 'lte') {
      return isFiniteNumber(c.value)
        ? null
        : 'Condition "rating" requires numeric "value".';
    }
    if (c.op === 'between') {
      return isFiniteNumber(c.min) && isFiniteNumber(c.max)
        ? null
        : 'Condition "rating" with op "between" requires numeric "min" and "max".';
    }
    return `Unsupported operator "${c.op}" for field "rating".`;
  }

  if (TIME_FIELDS.has(c.field)) {
    if (c.op === 'within_last_days' || c.op === 'older_than_days') {
      return isFiniteNumber(c.days)
        ? null
        : `Condition "${c.field}" with op "${c.op}" requires numeric "days".`;
    }
    if (c.op === 'after_epoch' || c.op === 'before_epoch') {
      return isFiniteNumber(c.timestamp)
        ? null
        : `Condition "${c.field}" with op "${c.op}" requires numeric "timestamp".`;
    }
    if (c.op === 'between_epochs') {
      return isFiniteNumber(c.min) && isFiniteNumber(c.max)
        ? null
        : `Condition "${c.field}" with op "between_epochs" requires numeric "min" and "max".`;
    }
    return `Unsupported operator "${c.op}" for time field "${c.field}".`;
  }

  if (c.field === 'watch_state') {
    if (c.op === 'state_in') {
      return isStringArray(c.values) && c.values.every((v) => WATCH_STATE_VALUES.has(v))
        ? null
        : 'Condition "watch_state" with op "state_in" requires valid WatchState[] "values".';
    }
    if (c.op === 'progress_between') {
      return isFiniteNumber(c.min) && isFiniteNumber(c.max)
        ? null
        : 'Condition "watch_state" with op "progress_between" requires numeric "min" and "max".';
    }
    return `Unsupported operator "${c.op}" for field "watch_state".`;
  }

  return `Unknown filter field "${c.field}".`;
}

function validateFilterNodeObject(node: unknown, groupDepth: number): string | null {
  if (!node || typeof node !== 'object' || Array.isArray(node)) {
    return 'Filter AST node must be a JSON object.';
  }
  const n = node as Record<string, unknown>;
  if (n.negated !== undefined && typeof n.negated !== 'boolean') {
    return 'Property "negated" must be a boolean when provided.';
  }

  if (n.kind === 'group') {
    const nextDepth = groupDepth + 1;
    if (nextDepth > MAX_FILTER_AST_DEPTH) {
      return `Filter tree exceeds maximum nesting depth of ${MAX_FILTER_AST_DEPTH}.`;
    }
    if (n.op !== 'and' && n.op !== 'or') {
      return 'Group "op" must be "and" or "or".';
    }
    if (!Array.isArray(n.children) || n.children.length === 0) {
      return 'Filter group must contain a non-empty "children" array.';
    }
    for (const child of n.children) {
      const err = validateFilterNodeObject(child, nextDepth);
      if (err) return err;
    }
    return null;
  }

  if (n.kind === 'rule') {
    return validateConditionObject(n.condition);
  }

  return 'Filter AST node "kind" must be "group" or "rule".';
}

export function parseFilterNodeJson(raw: string): {
  node: FilterNode | null;
  error: string | null;
} {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { node: null, error: null };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'Malformed JSON';
    return { node: null, error: `Invalid JSON syntax: ${msg}` };
  }

  const structErr = validateFilterNodeObject(parsed, 0);
  if (structErr) {
    return { node: null, error: structErr };
  }

  const candidate = parsed as FilterNode;
  const rules = countFilterTreeRules(candidate);
  if (rules === 0 || rules > MAX_FILTER_AST_RULES) {
    return {
      node: null,
      error: `Filter tree must contain between 1 and ${MAX_FILTER_AST_RULES} rules (found ${rules}).`,
    };
  }

  return { node: candidate, error: null };
}
